//! TCP/TLS connection helpers shared by IMAP and POP3.

use anyhow::{anyhow, Context, Result};
use std::pin::Pin;
use std::sync::{Arc, OnceLock};
use std::future::Future;
use std::task::{Context as TaskCx, Poll};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tokio_rustls::TlsConnector;

pub const TIMEOUT: Duration = Duration::from_secs(30);

/// How long a connection may go without any data in either direction while we wait for the
/// server, before it's treated as dead. Without this an IMAP connection that silently dropped
/// (Wi-Fi change, sleep, a NAT timeout, a throttled Gmail session) waited forever: the sync of
/// that mailbox never finished, so it never synced again until Hermio restarted.
pub const STALL: Duration = Duration::from_secs(75);

#[derive(Debug)]
pub enum Stream {
    Plain(TcpStream),
    Tls(Box<TlsStream<TcpStream>>),
}

/// A mail-server connection whose reads and writes fail with `TimedOut` after [`STALL`] without
/// progress.
#[derive(Debug)]
pub struct Conn {
    s: Stream,
    rd: Option<Pin<Box<tokio::time::Sleep>>>,
    wr: Option<Pin<Box<tokio::time::Sleep>>>,
}

/// Root certificates from the Windows certificate store. Antivirus "HTTPS/e-mail scanning", VPNs and
/// company proxies install their own root there; trusting it (like Outlook does) avoids `UnknownIssuer`.
pub fn native_roots() -> &'static [rustls::pki_types::CertificateDer<'static>] {
    static ROOTS: OnceLock<Vec<rustls::pki_types::CertificateDer<'static>>> = OnceLock::new();
    ROOTS.get_or_init(|| rustls_native_certs::load_native_certs().certs)
}

fn tls_config() -> Arc<rustls::ClientConfig> {
    static CFG: OnceLock<Arc<rustls::ClientConfig>> = OnceLock::new();
    CFG.get_or_init(|| {
        let mut roots = rustls::RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let (_added, _ignored) = roots.add_parsable_certificates(native_roots().iter().cloned());
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let cfg = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .expect("tls versions")
            .with_root_certificates(roots)
            .with_no_client_auth();
        Arc::new(cfg)
    })
    .clone()
}

pub async fn tcp(host: &str, port: u16) -> Result<TcpStream> {
    let s = tokio::time::timeout(TIMEOUT, TcpStream::connect((host, port)))
        .await
        .map_err(|_| anyhow!("Timed out connecting to {host}:{port}"))?
        .with_context(|| format!("Could not connect to {host}:{port}"))?;
    s.set_nodelay(true).ok();
    Ok(s)
}

pub async fn upgrade(host: &str, s: TcpStream) -> Result<Conn> {
    let name = rustls::pki_types::ServerName::try_from(host.to_string()).map_err(|_| anyhow!("Invalid server name: {host}"))?;
    let tls = tokio::time::timeout(TIMEOUT, TlsConnector::from(tls_config()).connect(name, s))
        .await
        .map_err(|_| anyhow!("TLS handshake with {host} timed out"))?
        .map_err(|e| {
            let m = e.to_string();
            if m.contains("UnknownIssuer") || m.contains("certificate") {
                anyhow!("TLS error with {host}: {m}. The certificate was issued by other software (usually antivirus HTTPS/e-mail scanning, a VPN or a proxy) \
                    and is not in the Windows certificate store. Turn off encrypted-connection scanning in your antivirus, or exclude Hermio.")
            } else {
                anyhow!("TLS error with {host}: {m}")
            }
        })?;
    Ok(Conn::new(Stream::Tls(Box::new(tls))))
}

/// Connect with implicit TLS ("ssl") or plain ("starttls"/"none"; caller upgrades for STARTTLS).
pub async fn connect(host: &str, port: u16, security: &str) -> Result<Conn> {
    let s = tcp(host, port).await?;
    if security == "ssl" {
        upgrade(host, s).await
    } else {
        Ok(Conn::new(Stream::Plain(s)))
    }
}

impl Conn {
    pub fn new(s: Stream) -> Conn {
        Conn { s, rd: None, wr: None }
    }
    pub fn plain(s: TcpStream) -> Conn {
        Conn::new(Stream::Plain(s))
    }
    pub fn into_tcp(self) -> Option<TcpStream> {
        match self.s {
            Stream::Plain(s) => Some(s),
            Stream::Tls(_) => None,
        }
    }
}

/// Wait for `inner`; if it stays pending for [`STALL`], fail with `TimedOut`.
fn guarded<T>(timer: &mut Option<Pin<Box<tokio::time::Sleep>>>, cx: &mut TaskCx<'_>, inner: Poll<std::io::Result<T>>) -> Poll<std::io::Result<T>> {
    match inner {
        Poll::Ready(r) => {
            *timer = None;
            Poll::Ready(r)
        }
        Poll::Pending => {
            let t = timer.get_or_insert_with(|| Box::pin(tokio::time::sleep(STALL)));
            if t.as_mut().poll(cx).is_ready() {
                *timer = None;
                return Poll::Ready(Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "the mail server stopped responding")));
            }
            Poll::Pending
        }
    }
}

impl AsyncRead for Conn {
    fn poll_read(self: Pin<&mut Self>, cx: &mut TaskCx<'_>, buf: &mut ReadBuf<'_>) -> Poll<std::io::Result<()>> {
        let me = self.get_mut();
        let r = match &mut me.s {
            Stream::Plain(s) => Pin::new(s).poll_read(cx, buf),
            Stream::Tls(s) => Pin::new(s.as_mut()).poll_read(cx, buf),
        };
        guarded(&mut me.rd, cx, r)
    }
}

impl AsyncWrite for Conn {
    fn poll_write(self: Pin<&mut Self>, cx: &mut TaskCx<'_>, buf: &[u8]) -> Poll<std::io::Result<usize>> {
        let me = self.get_mut();
        let r = match &mut me.s {
            Stream::Plain(s) => Pin::new(s).poll_write(cx, buf),
            Stream::Tls(s) => Pin::new(s.as_mut()).poll_write(cx, buf),
        };
        guarded(&mut me.wr, cx, r)
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut TaskCx<'_>) -> Poll<std::io::Result<()>> {
        let me = self.get_mut();
        let r = match &mut me.s {
            Stream::Plain(s) => Pin::new(s).poll_flush(cx),
            Stream::Tls(s) => Pin::new(s.as_mut()).poll_flush(cx),
        };
        guarded(&mut me.wr, cx, r)
    }
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut TaskCx<'_>) -> Poll<std::io::Result<()>> {
        match &mut self.get_mut().s {
            Stream::Plain(s) => Pin::new(s).poll_shutdown(cx),
            Stream::Tls(s) => Pin::new(s.as_mut()).poll_shutdown(cx),
        }
    }
}
