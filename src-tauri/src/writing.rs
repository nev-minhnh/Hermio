//! The writing assistant's back end (the checks and rewrites themselves are prompts built in
//! the page): one-shot model completions, Google Translate, and an English dictionary.

use crate::ai::ChatMsg;
use crate::state::AppState;
use anyhow::{anyhow, bail, Result};
use serde::Serialize;
use std::time::Duration;

fn http() -> reqwest::Client {
    reqwest::Client::builder().timeout(Duration::from_secs(30)).user_agent("Hermio").build().unwrap_or_default()
}

/// One completion with the chosen provider/model; `stream_id` lets the Stop button cancel it.
pub async fn complete(state: &AppState, provider_id: &str, model: Option<&str>, thinking: bool, system: &str, prompt: &str, stream_id: Option<String>) -> Result<String> {
    if provider_id.is_empty() {
        bail!("Choose an AI provider in Settings › AI assistant first.");
    }
    let msgs = vec![ChatMsg { role: "user".into(), content: prompt.to_string(), images: vec![] }];
    let opts = crate::llm::Opts { stream_id, web_native: false };
    let out = crate::ai::complete_ex(state, provider_id, model.filter(|m| !m.is_empty()), thinking, system, &msgs, &opts).await?;
    Ok(out.text)
}

#[derive(Debug, Serialize)]
pub struct Translation {
    pub text: String,
    /// Detected source language code ("en", "vi"…).
    pub from: String,
}

/// Google Translate's public endpoint (the one the Translate widget uses).
pub async fn google_translate(text: &str, to: &str) -> Result<Translation> {
    let resp = http()
        .get("https://translate.googleapis.com/translate_a/single")
        .query(&[("client", "gtx"), ("sl", "auto"), ("tl", to), ("dt", "t"), ("q", text)])
        .send()
        .await?;
    let status = resp.status();
    if !status.is_success() {
        bail!("Google Translate: {status}");
    }
    let v: serde_json::Value = resp.json().await?;
    let out: String = v[0].as_array().into_iter().flatten().filter_map(|seg| seg[0].as_str()).collect();
    if out.is_empty() {
        bail!("Google Translate returned nothing");
    }
    Ok(Translation { text: out, from: v[2].as_str().unwrap_or("").to_string() })
}

/// "auto": into Vietnamese, or into English when the text already is Vietnamese.
pub async fn translate_auto(text: &str, to: &str) -> Result<Translation> {
    if to != "auto" {
        return google_translate(text, to).await;
    }
    let first = google_translate(text, "vi").await?;
    if first.from.starts_with("vi") {
        return google_translate(text, "en").await;
    }
    Ok(first)
}

/// An English word from the Free Dictionary API (dictionaryapi.dev), as plain text.
pub async fn define_english(word: &str) -> Result<String> {
    let w = word.trim().to_lowercase();
    if w.is_empty() || w.contains(char::is_whitespace) || !w.chars().all(|c| c.is_ascii_alphabetic() || c == '-' || c == '\'') {
        bail!("not a single English word");
    }
    let resp = http().get(format!("https://api.dictionaryapi.dev/api/v2/entries/en/{w}")).send().await?;
    if !resp.status().is_success() {
        return Err(anyhow!("No dictionary entry for “{w}”"));
    }
    let v: serde_json::Value = resp.json().await?;
    Ok(format_entry(&w, &v))
}

fn format_entry(w: &str, v: &serde_json::Value) -> String {
    let mut out = String::new();
    let phon = v[0]["phonetic"].as_str().or_else(|| v[0]["phonetics"].as_array().into_iter().flatten().find_map(|p| p["text"].as_str()));
    match phon {
        Some(p) => out.push_str(&format!("{w}  {p}\n")),
        None => out.push_str(&format!("{w}\n")),
    }
    for m in v[0]["meanings"].as_array().into_iter().flatten().take(4) {
        out.push_str(&format!("\n{}\n", m["partOfSpeech"].as_str().unwrap_or("")));
        for (i, d) in m["definitions"].as_array().into_iter().flatten().take(3).enumerate() {
            out.push_str(&format!("{}. {}\n", i + 1, d["definition"].as_str().unwrap_or("")));
            if let Some(e) = d["example"].as_str() {
                out.push_str(&format!("   “{e}”\n"));
            }
        }
        let syn: Vec<&str> = m["synonyms"].as_array().into_iter().flatten().filter_map(|s| s.as_str()).take(8).collect();
        if !syn.is_empty() {
            out.push_str(&format!("   ≈ {}\n", syn.join(", ")));
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    #[test]
    fn dictionary_entry_text() {
        let v = serde_json::json!([{ "phonetic": "/ˈkʌɹəntli/", "meanings": [{ "partOfSpeech": "adverb", "definitions": [{ "definition": "At this moment, at present, now." }], "synonyms": ["now"] }] }]);
        let s = super::format_entry("currently", &v);
        assert!(s.starts_with("currently  /ˈkʌɹəntli/"));
        assert!(s.contains("adverb\n1. At this moment, at present, now."));
        assert!(s.contains("≈ now"));
    }
}
