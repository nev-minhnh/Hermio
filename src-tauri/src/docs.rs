//! Text extraction from attachments and uploaded files, so the AI can read them.

use std::io::Read;

pub const MAX_CHARS: usize = 60_000;

fn clip(s: String) -> String {
    let s = s.replace('\u{0}', "");
    if s.chars().count() > MAX_CHARS {
        let mut t: String = s.chars().take(MAX_CHARS).collect();
        t.push_str("\n…[truncated]");
        t
    } else {
        s
    }
}

fn is_texty(name: &str, mime: &str) -> bool {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    mime.starts_with("text/")
        || mime.contains("json")
        || mime.contains("xml")
        || mime.contains("javascript")
        || ["txt", "md", "csv", "tsv", "json", "xml", "yaml", "yml", "log", "ics", "vcf", "py", "rs", "ts", "js", "tex", "bib", "ini", "toml", "sql"]
            .contains(&ext.as_str())
}

fn docx_text(bytes: &[u8]) -> Option<String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).ok()?;
    let mut xml = String::new();
    zip.by_name("word/document.xml").ok()?.read_to_string(&mut xml).ok()?;
    let xml = xml.replace("</w:p>", "\n").replace("<w:tab/>", "\t").replace("<w:br/>", "\n");
    Some(strip_tags(&xml))
}

fn pptx_text(bytes: &[u8]) -> Option<String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).ok()?;
    let mut names: Vec<String> = zip.file_names().filter(|n| n.starts_with("ppt/slides/slide") && n.ends_with(".xml")).map(|s| s.to_string()).collect();
    names.sort_by_key(|n| n.trim_start_matches("ppt/slides/slide").trim_end_matches(".xml").parse::<u32>().unwrap_or(0));
    let mut out = String::new();
    for (i, n) in names.iter().enumerate() {
        let mut xml = String::new();
        zip.by_name(n).ok()?.read_to_string(&mut xml).ok()?;
        out.push_str(&format!("--- Slide {} ---\n{}\n", i + 1, strip_tags(&xml.replace("</a:p>", "\n"))));
    }
    Some(out)
}

fn xlsx_text(bytes: &[u8]) -> Option<String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).ok()?;
    let mut xml = String::new();
    zip.by_name("xl/sharedStrings.xml").ok()?.read_to_string(&mut xml).ok()?;
    Some(strip_tags(&xml.replace("</si>", "\n")))
}

pub fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len() / 2);
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&apos;", "'")
}

/// Best-effort plain text of a file. None when the format isn't readable as text
/// (images, archives, encrypted PDFs…).
pub fn extract_text(name: &str, mime: &str, bytes: &[u8]) -> Option<String> {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    let text = if mime == "application/pdf" || ext == "pdf" {
        let b = bytes.to_vec();
        std::panic::catch_unwind(move || pdf_extract::extract_text_from_mem(&b).ok()).ok().flatten()?
    } else if ext == "docx" || mime.contains("wordprocessingml") {
        docx_text(bytes)?
    } else if ext == "pptx" || mime.contains("presentationml") {
        pptx_text(bytes)?
    } else if ext == "xlsx" || mime.contains("spreadsheetml") {
        xlsx_text(bytes)?
    } else if mime == "text/html" || ext == "html" || ext == "htm" {
        crate::parse::html_to_text(&String::from_utf8_lossy(bytes))
    } else if is_texty(name, mime) {
        String::from_utf8_lossy(bytes).into_owned()
    } else {
        return None;
    };
    let t = text.trim();
    if t.is_empty() { None } else { Some(clip(t.to_string())) }
}

pub fn is_image(name: &str, mime: &str) -> bool {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    mime.starts_with("image/") || ["png", "jpg", "jpeg", "gif", "webp", "bmp"].contains(&ext.as_str())
}
