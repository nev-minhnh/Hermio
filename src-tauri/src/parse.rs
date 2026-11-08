//! Message parsing: summaries for the index, bodies for display, attachments.

use crate::models::*;
use base64::{engine::general_purpose::STANDARD, Engine};
use mail_parser::decoders::html::html_to_text as h2t;
use mail_parser::{Address, HeaderValue, Message, MessageParser, MessagePart, MimeHeaders, PartType};

pub fn html_to_text(h: &str) -> String {
    h2t(h)
}

pub fn parse(raw: &[u8]) -> Option<Message<'_>> {
    MessageParser::default().parse(raw)
}

fn fmt_list(list: Option<&Address>) -> String {
    let Some(list) = list else { return String::new() };
    list.iter()
        .filter_map(|m| {
            let addr = m.address.as_deref()?;
            Some(match m.name.as_deref() {
                Some(n) if !n.is_empty() => format!("{} <{}>", n.replace(['<', '>', ','], " ").trim(), addr),
                _ => addr.to_string(),
            })
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn text_list(v: &HeaderValue) -> Vec<String> {
    match v {
        HeaderValue::Text(t) => vec![t.to_string()],
        HeaderValue::TextList(l) => l.iter().map(|s| s.to_string()).collect(),
        _ => vec![],
    }
}

pub fn collapse_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len().min(4096));
    let mut last_space = false;
    for ch in s.chars() {
        if ch.is_whitespace() {
            if !last_space {
                out.push(' ');
                last_space = true;
            }
        } else {
            out.push(ch);
            last_space = false;
        }
    }
    out.trim().to_string()
}

/// Plain text used for search and previews.
pub fn plain_text(msg: &Message) -> String {
    let has_text_part = msg.text_part(0).map(|p| !p.is_text_html()).unwrap_or(false);
    if has_text_part {
        if let Some(t) = msg.body_text(0) {
            if !t.trim().is_empty() {
                return t.into_owned();
            }
        }
    }
    if let Some(h) = msg.body_html(0) {
        return html_to_text(&h);
    }
    msg.body_text(0).map(|t| t.into_owned()).unwrap_or_default()
}

pub fn snippet_of(text: &str) -> String {
    let s = collapse_ws(text);
    s.chars().take(200).collect()
}

fn is_inline_image(p: &MessagePart) -> bool {
    p.content_id().is_some()
        && p.content_type().map(|c| c.ctype().eq_ignore_ascii_case("image")).unwrap_or(false)
        && !p.content_disposition().map(|d| d.is_attachment()).unwrap_or(false)
}

/// Fill the summary fields of a NewMessage from raw bytes. `with_body` decides whether the
/// plain-text body is extracted for the search index.
pub fn summarize(raw: &[u8], with_body: bool) -> NewMessage {
    let mut m = NewMessage { size: raw.len() as i64, ..Default::default() };
    let Some(msg) = parse(raw) else {
        m.subject = "(unreadable message)".into();
        return m;
    };
    m.subject = msg.subject().unwrap_or("").to_string();
    if let Some(f) = msg.from().and_then(|l| l.first()) {
        m.from_name = f.name.as_deref().unwrap_or("").to_string();
        m.from_addr = f.address.as_deref().unwrap_or("").to_string();
    }
    m.to_list = fmt_list(msg.to());
    m.cc_list = fmt_list(msg.cc());
    m.message_id = msg.message_id().unwrap_or("").to_string();
    m.in_reply_to = text_list(msg.in_reply_to()).into_iter().next().unwrap_or_default();
    m.date = msg.date().map(|d| d.to_timestamp()).unwrap_or(0);
    m.has_attachments = msg.attachments().any(|p| !is_inline_image(p));
    if with_body {
        let text = plain_text(&msg);
        m.snippet = snippet_of(&text);
        m.body = Some(text);
    }
    m
}

fn mime_of(part: &MessagePart) -> String {
    match part.content_type() {
        Some(ct) => match ct.subtype() {
            Some(st) => format!("{}/{}", ct.ctype(), st).to_lowercase(),
            None => ct.ctype().to_lowercase(),
        },
        None => "application/octet-stream".into(),
    }
}

pub fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Plain text → HTML with clickable links.
pub fn text_to_html(t: &str) -> String {
    let mut out = String::from("<div style=\"white-space:pre-wrap;font-family:inherit\">");
    for line in t.lines() {
        let mut first = true;
        for word in line.split(' ') {
            if !first {
                out.push(' ');
            }
            first = false;
            if word.starts_with("http://") || word.starts_with("https://") {
                let e = escape_html(word);
                out.push_str(&format!("<a href=\"{e}\">{e}</a>"));
            } else {
                out.push_str(&escape_html(word));
            }
        }
        out.push('\n');
    }
    out.push_str("</div>");
    out
}

fn part_name(p: &MessagePart, i: usize) -> String {
    if let Some(n) = p.attachment_name() {
        return n.to_string();
    }
    if let PartType::Message(m) = &p.body {
        return format!("{}.eml", m.subject().unwrap_or("message"));
    }
    format!("attachment-{}", i + 1)
}

fn part_bytes(p: &MessagePart) -> Vec<u8> {
    match &p.body {
        PartType::Message(m) => m.raw_message().to_vec(),
        _ => p.contents().to_vec(),
    }
}

pub fn body(id: i64, raw: &[u8]) -> MessageBody {
    let Some(msg) = parse(raw) else {
        return MessageBody {
            id, html: text_to_html(&String::from_utf8_lossy(raw)), text: String::new(), has_remote_images: false,
            attachments: vec![], to: String::new(), cc: String::new(), reply_to: String::new(),
            references: String::new(), ics: None,
        };
    };
    let real_html = msg.html_part(0).map(|p| p.is_text_html()).unwrap_or(false);
    let mut html = match msg.body_html(0) {
        Some(h) if real_html && !h.trim().is_empty() => h.into_owned(),
        _ => text_to_html(&msg.body_text(0).unwrap_or_default()),
    };
    let mut attachments = vec![];
    let mut ics = None;
    for (i, part) in msg.attachments().enumerate() {
        let mime = mime_of(part);
        if let Some(cid) = part.content_id() {
            let cid = cid.trim_matches(['<', '>']);
            let needle = format!("cid:{cid}");
            if html.contains(&needle) {
                let uri = format!("data:{};base64,{}", mime, STANDARD.encode(part.contents()));
                html = html.replace(&needle, &uri);
                if is_inline_image(part) {
                    continue;
                }
            }
        }
        if mime == "text/calendar" && ics.is_none() {
            ics = Some(String::from_utf8_lossy(part.contents()).into_owned());
        }
        attachments.push(Attachment {
            index: i,
            name: part_name(part, i),
            mime,
            size: part.len(),
            inline: is_inline_image(part),
        });
    }
    if ics.is_none() {
        for p in &msg.parts {
            if mime_of(p) == "text/calendar" {
                ics = Some(String::from_utf8_lossy(p.contents()).into_owned());
                break;
            }
        }
    }
    let lower = html.to_ascii_lowercase();
    let has_remote_images = lower.contains("src=\"http") || lower.contains("src='http") || lower.contains("src=http")
        || lower.contains("url(http") || lower.contains("url('http") || lower.contains("url(\"http");
    let references = text_list(msg.references()).iter().map(|s| format!("<{s}>")).collect::<Vec<_>>().join(" ");
    MessageBody {
        id,
        text: plain_text(&msg),
        html,
        has_remote_images,
        attachments,
        to: fmt_list(msg.to()),
        cc: fmt_list(msg.cc()),
        reply_to: fmt_list(msg.reply_to()),
        references,
        ics,
    }
}

/// Decoded bytes, file name and MIME type of the n-th attachment.
pub fn attachment(raw: &[u8], index: usize) -> Option<(String, String, Vec<u8>)> {
    let msg = parse(raw)?;
    let part = msg.attachments().nth(index)?;
    Some((part_name(part, index), mime_of(part), part_bytes(part)))
}
