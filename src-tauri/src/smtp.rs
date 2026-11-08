//! Building and sending outgoing mail.

use crate::models::*;
use crate::state::{AppState, Creds};
use anyhow::{anyhow, Context, Result};
use lettre::message::header::{ContentType, HeaderName, HeaderValue};
use lettre::message::{Attachment, Mailbox, MultiPart, SinglePart};
use lettre::transport::smtp::authentication::{Credentials, Mechanism};
use lettre::transport::smtp::client::{Certificate, CertificateStore, Tls, TlsParameters};
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

fn mailbox(s: &str) -> Result<Mailbox> {
    let s = s.trim();
    s.parse::<Mailbox>()
        .or_else(|_| format!("<{s}>").parse::<Mailbox>())
        .map_err(|_| anyhow!("Invalid email address: {s}"))
}

/// Build the RFC 5322 message. Returns (lettre message, raw bytes).
pub fn build(a: &Account, o: &Outgoing) -> Result<(Message, Vec<u8>)> {
    let from_name = if a.display_name.is_empty() { a.name.clone() } else { a.display_name.clone() };
    let from: Mailbox = format!("{} <{}>", from_name.replace(['<', '>', '"'], ""), a.email)
        .parse()
        .or_else(|_| a.email.parse())
        .map_err(|_| anyhow!("Invalid sender address"))?;
    let mut b = Message::builder().from(from).subject(o.subject.clone()).date_now();
    for t in &o.to {
        b = b.to(mailbox(t)?);
    }
    for t in &o.cc {
        b = b.cc(mailbox(t)?);
    }
    for t in &o.bcc {
        b = b.bcc(mailbox(t)?);
    }
    if !o.in_reply_to.is_empty() {
        b = b.in_reply_to(o.in_reply_to.clone());
    }
    if !o.references.is_empty() {
        b = b.references(o.references.clone());
    }
    let html = format!(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\"></head><body style=\"font-family:'Segoe UI',Arial,sans-serif;font-size:14px\">{}</body></html>",
        o.html
    );
    let text = crate::parse::html_to_text(&o.html);
    let alt = MultiPart::alternative()
        .singlepart(SinglePart::plain(text))
        .singlepart(SinglePart::html(html));

    let mut parts: Vec<SinglePart> = vec![];
    for att in &o.attachments {
        let Some(path) = &att.path else { continue };
        let data = std::fs::read(path).with_context(|| format!("Could not read attachment {path}"))?;
        let mime = mime_guess::from_path(if att.name.is_empty() { path } else { &att.name }).first_or_octet_stream().to_string();
        let name = if att.name.is_empty() {
            std::path::Path::new(path).file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_else(|| "attachment".into())
        } else {
            att.name.clone()
        };
        let ct = ContentType::parse(&mime).unwrap_or(ContentType::parse("application/octet-stream").unwrap());
        parts.push(Attachment::new(name).body(data, ct));
    }
    let msg = if parts.is_empty() {
        b.multipart(alt)?
    } else {
        let mut mixed = MultiPart::mixed().multipart(alt);
        for p in parts {
            mixed = mixed.singlepart(p);
        }
        b.multipart(mixed)?
    };
    let mut msg = msg;
    msg.headers_mut().insert_raw(HeaderValue::new(HeaderName::new_from_ascii_str("X-Mailer"), "Hermio".into()));
    let raw = msg.formatted();
    Ok((msg, raw))
}

/// TLS settings trusting both the bundled Mozilla roots and the Windows certificate store.
fn tls_params(host: &str) -> Result<TlsParameters> {
    let mut b = TlsParameters::builder(host.to_string()).certificate_store(CertificateStore::WebpkiRoots);
    for c in crate::net::native_roots() {
        if let Ok(cert) = Certificate::from_der(c.to_vec()) {
            b = b.add_root_certificate(cert);
        }
    }
    Ok(b.build_rustls()?)
}

fn transport_builder(a: &Account) -> Result<lettre::transport::smtp::AsyncSmtpTransportBuilder> {
    let b = AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&a.out_host).port(a.out_port);
    Ok(match a.out_security.as_str() {
        "ssl" => b.tls(Tls::Wrapper(tls_params(&a.out_host)?)),
        "starttls" => b.tls(Tls::Required(tls_params(&a.out_host)?)),
        _ => b,
    })
}

pub async fn send(state: &AppState, a: &Account, msg: Message) -> Result<()> {
    let creds = state.smtp_creds(a).await?;
    let builder = transport_builder(a)?.timeout(Some(std::time::Duration::from_secs(60)));
    let transport = match creds {
        Creds::Password { user, pass } => builder
            .credentials(Credentials::new(user, pass))
            .authentication(vec![Mechanism::Plain, Mechanism::Login])
            .build(),
        Creds::OAuth { user, token } => builder
            .credentials(Credentials::new(user, token))
            .authentication(vec![Mechanism::Xoauth2])
            .build(),
    };
    transport.send(msg).await.map_err(|e| anyhow!("Sending failed: {e}"))?;
    Ok(())
}

pub async fn test(state: &AppState, a: &Account) -> Result<()> {
    let creds = state.smtp_creds(a).await?;
    test_with(a, creds).await
}

pub async fn test_with(a: &Account, creds: Creds) -> Result<()> {
    let builder = transport_builder(a)?.timeout(Some(std::time::Duration::from_secs(30)));
    let t: AsyncSmtpTransport<Tokio1Executor> = match creds {
        Creds::Password { user, pass } => builder.credentials(Credentials::new(user, pass)).build(),
        Creds::OAuth { user, token } => builder
            .credentials(Credentials::new(user, token))
            .authentication(vec![Mechanism::Xoauth2])
            .build(),
    };
    let ok = t.test_connection().await.map_err(|e| anyhow!("SMTP: {e}"))?;
    if !ok {
        return Err(anyhow!("Could not connect to the SMTP server"));
    }
    Ok(())
}
