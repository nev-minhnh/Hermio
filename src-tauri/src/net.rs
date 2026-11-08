//! TCP/TLS connection helpers shared by IMAP and POP3.

use anyhow::{anyhow, Context, Result};
use std::pin::Pin;
use std::sync::{Arc, OnceLock};
use std::task::{Context as TaskCx, Poll};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tokio_rustls::TlsConnector;

pub const TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug)]
pub enum Conn {
    Plain(TcpStream),
    Tls(Box<TlsStream<TcpStream>>),
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
    Ok(Conn::Tls(Box::new(tls)))
}

/// Connect with implicit TLS ("ssl") or plain ("starttls"/"none"; caller upgrades for STARTTLS).
pub async fn connect(host: &str, port: u16, security: &str) -> Result<Conn> {
    let s = tcp(host, port).await?;
    if security == "ssl" {
        upgrade(host, s).await
    } else {
        Ok(Conn::Plain(s))
    }
}

impl Conn {
    pub fn into_tcp(self) -> Option<TcpStream> {
        match self {
            Conn::Plain(s) => Some(s),
            Conn::Tls(_) => None,
        }
    }
}

impl AsyncRead for Conn {
    fn poll_read(self: Pin<&mut Self>, cx: &mut TaskCx<'_>, buf: &mut ReadBuf<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            Conn::Plain(s) => Pin::new(s).poll_read(cx, buf),
            Conn::Tls(s) => Pin::new(s.as_mut()).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for Conn {
    fn poll_write(self: Pin<&mut Self>, cx: &mut TaskCx<'_>, buf: &[u8]) -> Poll<std::io::Result<usize>> {
        match self.get_mut() {
            Conn::Plain(s) => Pin::new(s).poll_write(cx, buf),
            Conn::Tls(s) => Pin::new(s.as_mut()).poll_write(cx, buf),
        }
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut TaskCx<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            Conn::Plain(s) => Pin::new(s).poll_flush(cx),
            Conn::Tls(s) => Pin::new(s.as_mut()).poll_flush(cx),
        }
    }
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut TaskCx<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            Conn::Plain(s) => Pin::new(s).poll_shutdown(cx),
            Conn::Tls(s) => Pin::new(s.as_mut()).poll_shutdown(cx),
        }
    }
}
