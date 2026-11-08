//! AI assistant: OpenAI, Gemini (AI Studio), Grok, local OpenAI-compatible servers, Anthropic API
//! and the Claude Code CLI. The assistant only ever sees mailboxes the user allowed.
//!
//! What the model receives for each email: headers, the full plain-text body (downloaded on demand
//! if it wasn't synced yet) and, for small selections, the text of attachments (PDF, Office, text).

use crate::models::*;
use crate::oauth;
use crate::state::AppState;
use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Provider {
    pub id: String,
    /// openai | gemini | grok | local | anthropic | claude_cli
    pub kind: String,
    pub name: String,
    /// API base URL (OpenAI-compatible kinds) or the `claude` executable path (claude_cli)
    pub base_url: String,
    pub model: String,
    #[serde(skip_deserializing)]
    pub has_key: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Image {
    pub mime: String,
    /// base64
    pub data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ChatMsg {
    pub role: String, // user | assistant
    pub content: String,
    pub images: Vec<Image>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct Scope {
    /// all | accounts | space | group | folder | selection
    pub kind: String,
    pub ids: Vec<i64>,
}

/// A file the user attached to the chat.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AiFile {
    pub name: String,
    pub mime: String,
    pub text: Option<String>,
    pub image: Option<String>,
    /// A PDF without a text layer (scanned): sent back so the UI can render pages for OCR.
    pub pdf: Option<String>,
    pub size: usize,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct AiRequest {
    pub provider_id: String,
    /// Overrides the provider's default model for this request.
    pub model: Option<String>,
    pub thinking: bool,
    pub messages: Vec<ChatMsg>,
    pub scope: Scope,
    /// Look back this many days (0 = no limit). Ignored when `from`/`to` are set.
    pub days: i64,
    pub from: Option<i64>,
    pub to: Option<i64>,
    /// chat | summarize | reply | cluster | report | signature
    pub task: String,
    pub files: Vec<AiFile>,
    /// UI language, used as the default reply language
    pub lang: String,
    /// Stream deltas to the UI under this id (`ai-delta` events).
    pub stream_id: Option<String>,
    /// Let the assistant search the web for up-to-date information.
    pub web: bool,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct AiResponse {
    pub text: String,
    pub used: Vec<i64>,
    pub clusters: Vec<Cluster>,
    pub thinking: String,
    pub sources: Vec<crate::llm::Source>,
    pub cancelled: bool,
    /// Received-date span of the emails that were read ("8 Oct – 7 Nov 2026").
    pub span: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct RuleProposal {
    pub match_all: bool,
    pub conditions: Vec<Condition>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Cluster {
    pub title: String,
    pub summary: String,
    pub ids: Vec<i64>,
    pub rule: Option<RuleProposal>,
    /// How many different senders the cluster has.
    #[serde(default)]
    pub senders: usize,
    /// Of the emails in scope, how many the suggested rule catches inside / outside this cluster.
    #[serde(default)]
    pub rule_hits: usize,
    #[serde(default)]
    pub rule_extra: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub content: String,
    pub enabled: bool,
}

fn key_name(id: &str) -> String {
    format!("ai:{id}:key")
}

pub const GEMINI_BUILTIN: &str = "gemini-builtin";

fn builtin_hidden(state: &AppState) -> bool {
    state.db.get_setting("gemini_builtin_hidden").ok().flatten().map(|v| v.trim_matches('"') == "true").unwrap_or(false)
}

/// First run: make the built-in Gemini model the default assistant when nothing is chosen yet.
pub fn init_defaults(state: &AppState) {
    if crate::state::builtin_gemini_key().is_none() || builtin_hidden(state) {
        return;
    }
    let cur = state.db.get_setting("ai_default").ok().flatten().unwrap_or_default();
    if cur.trim_matches('"').is_empty() {
        let _ = state.db.set_setting("ai_default", &serde_json::to_string(GEMINI_BUILTIN).unwrap_or_default());
    }
}

pub fn providers(state: &AppState) -> Result<Vec<Provider>> {
    let raw = state.db.get_setting("ai_providers")?.unwrap_or_else(|| "[]".into());
    let mut list: Vec<Provider> = serde_json::from_str(&raw).unwrap_or_default();
    list.retain(|p| p.kind != "npu");
    let builtin_key = crate::state::builtin_gemini_key().is_some();
    if !builtin_key || builtin_hidden(state) {
        list.retain(|p| p.id != GEMINI_BUILTIN);
    } else if !list.iter().any(|p| p.id == GEMINI_BUILTIN) {
        // A Gemini model that works out of the box (key embedded at build time). Empty model = newest Flash.
        list.insert(0, Provider { id: GEMINI_BUILTIN.into(), kind: "gemini".into(), name: "Gemini (built-in)".into(), base_url: String::new(), model: String::new(), has_key: true });
    }
    for p in list.iter_mut() {
        p.has_key = oauth::get_secret(&key_name(&p.id)).ok().flatten().is_some() || (p.id == GEMINI_BUILTIN && builtin_key);
    }
    // The on-device NPU model appears as a built-in provider once a model is downloaded.
    let models = crate::npu::installed_models(state);
    // Hidden on PCs without an AMD / Intel NPU.
    let npu_ok = crate::npu::cached_info().map(|i| i.present && !i.vendor.is_empty()).unwrap_or(true);
    if !models.is_empty() && npu_ok {
        let pref = state.db.get_setting("npu_model")?.unwrap_or_default().trim_matches('"').to_string();
        let model = if models.contains(&pref) { pref } else { models[0].clone() };
        list.insert(0, Provider { id: "npu".into(), kind: "npu".into(), name: "On-device (NPU)".into(), base_url: crate::npu::base_url(state), model, has_key: true });
    }
    Ok(list)
}

fn rand_id() -> String {
    let mut b = [0u8; 6];
    let _ = getrandom::getrandom(&mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub fn save_provider(state: &AppState, mut p: Provider, api_key: Option<String>) -> Result<Provider> {
    if p.kind == "npu" {
        state.db.set_setting("npu_model", &serde_json::to_string(&p.model)?)?;
        return Ok(p);
    }
    let mut list: Vec<Provider> = providers(state)?.into_iter().filter(|x| x.kind != "npu").collect();
    if p.id.is_empty() {
        p.id = rand_id();
    }
    if p.base_url.trim().is_empty() {
        p.base_url = match p.kind.as_str() {
            "openai" => "https://api.openai.com/v1",
            "grok" => "https://api.x.ai/v1",
            "local" => "http://127.0.0.1:8080/v1",
            _ => "",
        }
        .into();
    }
    if let Some(k) = api_key {
        if !k.trim().is_empty() {
            oauth::set_secret(&key_name(&p.id), k.trim())?;
        }
    }
    match list.iter_mut().find(|x| x.id == p.id) {
        Some(x) => *x = p.clone(),
        None => list.push(p.clone()),
    }
    state.db.set_setting("ai_providers", &serde_json::to_string(&list)?)?;
    Ok(p)
}

pub fn delete_provider(state: &AppState, id: &str) -> Result<()> {
    if id == GEMINI_BUILTIN {
        state.db.set_setting("gemini_builtin_hidden", "true")?;
    }
    let list: Vec<Provider> = providers(state)?.into_iter().filter(|p| p.id != id && p.kind != "npu").collect();
    state.db.set_setting("ai_providers", &serde_json::to_string(&list)?)?;
    oauth::delete_secret(&key_name(id));
    Ok(())
}

fn provider(state: &AppState, id: &str) -> Result<(Provider, String)> {
    let p = providers(state)?
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| anyhow!("No AI model selected. Add a provider in Settings → AI assistant."))?;
    let mut key = oauth::get_secret(&key_name(&p.id))?.unwrap_or_default();
    if key.is_empty() && p.id == GEMINI_BUILTIN {
        key = crate::state::builtin_gemini_key().unwrap_or_default().to_string();
    }
    Ok((p, key))
}

static FLASH: std::sync::Mutex<Option<(String, i64)>> = std::sync::Mutex::new(None);

/// Rank Gemini model names: newest stable Flash first (no lite / image / tts / live variants).
pub fn best_flash(models: &[String]) -> Option<String> {
    let ver = |m: &str| -> f64 {
        let re = regex_lite::Regex::new(r"gemini-(\d+(?:\.\d+)?)").unwrap();
        re.captures(m).and_then(|c| c[1].parse().ok()).unwrap_or(0.0)
    };
    let bad = ["lite", "image", "tts", "live", "audio", "thinking", "embedding", "robotics", "computer"];
    let mut c: Vec<&String> = models
        .iter()
        .filter(|m| m.starts_with("gemini-") && m.contains("flash") && !bad.iter().any(|b| m.contains(b)))
        .collect();
    // Stable before preview/exp, then the highest version, then the shortest (alias) name.
    c.sort_by(|a, b| {
        let pa = a.contains("preview") || a.contains("exp");
        let pb = b.contains("preview") || b.contains("exp");
        pa.cmp(&pb).then(ver(b).partial_cmp(&ver(a)).unwrap_or(std::cmp::Ordering::Equal)).then(a.len().cmp(&b.len()))
    });
    let stable_best = c.iter().find(|m| !(m.contains("preview") || m.contains("exp")));
    let any_best = c.iter().max_by(|a, b| ver(a).partial_cmp(&ver(b)).unwrap_or(std::cmp::Ordering::Equal));
    // Use a preview only when it is a whole generation newer than every stable Flash.
    match (stable_best, any_best) {
        (Some(s), Some(a)) if ver(a).floor() > ver(s).floor() => Some((*a).clone()),
        (Some(s), _) => Some((*s).clone()),
        (None, a) => a.map(|x| (*x).clone()),
    }
}

/// Fill in a Gemini model when none is set (the built-in provider): the newest Flash, cached for a day.
async fn resolve_gemini_model(state: &AppState, p: &mut Provider) {
    if p.kind != "gemini" || !p.model.trim().is_empty() {
        return;
    }
    let now = chrono::Utc::now().timestamp();
    if let Some((m, t)) = FLASH.lock().unwrap().clone() {
        if now - t < 86400 {
            p.model = m;
            return;
        }
    }
    let pick = match list_models(state, &p.id).await {
        Ok(list) => best_flash(&list),
        Err(_) => None,
    };
    p.model = pick.clone().unwrap_or_else(|| "gemini-2.5-flash".into());
    if pick.is_some() {
        *FLASH.lock().unwrap() = Some((p.model.clone(), now));
    }
}

// ---------------- skills ----------------

pub fn skills(state: &AppState) -> Result<Vec<Skill>> {
    let raw = state.db.get_setting("ai_skills")?.unwrap_or_else(|| "[]".into());
    Ok(serde_json::from_str(&raw).unwrap_or_default())
}

pub fn save_skill(state: &AppState, mut s: Skill) -> Result<Skill> {
    let mut list = skills(state)?;
    if s.id.is_empty() {
        s.id = rand_id();
    }
    // Pull name/description from SKILL.md front matter when present.
    if let Some(rest) = s.content.trim_start().strip_prefix("---") {
        if let Some(end) = rest.find("\n---") {
            for line in rest[..end].lines() {
                if let Some(v) = line.strip_prefix("name:") {
                    if s.name.trim().is_empty() { s.name = v.trim().trim_matches('"').to_string(); }
                }
                if let Some(v) = line.strip_prefix("description:") {
                    if s.description.trim().is_empty() { s.description = v.trim().trim_matches('"').to_string(); }
                }
            }
        }
    }
    if s.name.trim().is_empty() {
        s.name = "Skill".into();
    }
    match list.iter_mut().find(|x| x.id == s.id) {
        Some(x) => *x = s.clone(),
        None => list.push(s.clone()),
    }
    state.db.set_setting("ai_skills", &serde_json::to_string(&list)?)?;
    Ok(s)
}

pub fn delete_skill(state: &AppState, id: &str) -> Result<()> {
    let list: Vec<Skill> = skills(state)?.into_iter().filter(|s| s.id != id).collect();
    state.db.set_setting("ai_skills", &serde_json::to_string(&list)?)?;
    Ok(())
}

fn skills_prompt(state: &AppState, cap: usize) -> String {
    let list: Vec<Skill> = skills(state).unwrap_or_default().into_iter().filter(|s| s.enabled).collect();
    if list.is_empty() {
        return String::new();
    }
    let mut out = String::from("\n\n## Installed skills (follow them when relevant)\n");
    for s in list {
        out.push_str(&format!("\n### Skill: {}\n{}\n", s.name, s.content.chars().take(cap).collect::<String>()));
    }
    out
}

// ---------------- HTTP ----------------

fn http() -> reqwest::Client {
    reqwest::Client::builder().timeout(Duration::from_secs(300)).build().unwrap_or_default()
}

fn api_err(service: &str, status: reqwest::StatusCode, body: &str) -> anyhow::Error {
    let msg = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v.pointer("/error/message").and_then(|m| m.as_str()).map(|s| s.to_string()))
        .unwrap_or_else(|| body.chars().take(300).collect());
    anyhow!("{service} returned {status}: {msg}")
}

pub async fn list_models(state: &AppState, id: &str) -> Result<Vec<String>> {
    let (p, key) = provider(state, id)?;
    let h = http();
    let mut out: Vec<String> = match p.kind.as_str() {
        "gemini" => {
            let r = h.get("https://generativelanguage.googleapis.com/v1beta/models?pageSize=200").header("x-goog-api-key", &key).send().await?;
            let st = r.status();
            let t = r.text().await?;
            if !st.is_success() {
                return Err(api_err("Gemini", st, &t));
            }
            let v: Value = serde_json::from_str(&t)?;
            v["models"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .filter(|m| m["supportedGenerationMethods"].as_array().map(|a| a.iter().any(|x| x == "generateContent")).unwrap_or(false))
                .filter_map(|m| m["name"].as_str().map(|s| s.trim_start_matches("models/").to_string()))
                .collect()
        }
        "anthropic" => {
            let r = h
                .get("https://api.anthropic.com/v1/models?limit=100")
                .header("x-api-key", &key)
                .header("anthropic-version", "2023-06-01")
                .send()
                .await?;
            let st = r.status();
            let t = r.text().await?;
            if !st.is_success() {
                return Err(api_err("Anthropic", st, &t));
            }
            let v: Value = serde_json::from_str(&t)?;
            v["data"].as_array().cloned().unwrap_or_default().iter().filter_map(|m| m["id"].as_str().map(|s| s.to_string())).collect()
        }
        "claude_cli" => vec!["sonnet".into(), "opus".into(), "haiku".into()],
        "npu" => return Ok(crate::npu::installed_models(state)),
        _ => {
            let mut req = h.get(format!("{}/models", p.base_url.trim_end_matches('/')));
            if !key.is_empty() {
                req = req.bearer_auth(&key);
            }
            let r = req.send().await.with_context(|| format!("Could not connect to {}", p.base_url))?;
            let st = r.status();
            let t = r.text().await?;
            if !st.is_success() {
                return Err(api_err(&p.name, st, &t));
            }
            let v: Value = serde_json::from_str(&t)?;
            v["data"].as_array().cloned().unwrap_or_default().iter().filter_map(|m| m["id"].as_str().map(|s| s.to_string())).collect()
        }
    };
    out.sort();
    Ok(out)
}

/// Send one conversation to the provider and return the reply text (no streaming to the UI).
pub async fn complete(
    state: &AppState,
    provider_id: &str,
    model: Option<&str>,
    thinking: bool,
    system: &str,
    msgs: &[ChatMsg],
) -> Result<String> {
    Ok(complete_ex(state, provider_id, model, thinking, system, msgs, &crate::llm::Opts::default()).await?.text)
}

/// `complete` as a helper request of `parent` (so the user's Stop also stops it), or as
/// background work when `parent` is None.
async fn complete_sub(
    state: &AppState,
    provider_id: &str,
    model: Option<&str>,
    system: &str,
    msgs: &[ChatMsg],
    sub_id: String,
) -> Result<String> {
    let opts = crate::llm::Opts { stream_id: Some(sub_id), web_native: false };
    let out = complete_ex(state, provider_id, model, false, system, msgs, &opts).await?;
    if out.cancelled {
        bail!("stopped");
    }
    Ok(out.text)
}

static SUB: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
fn sub_id(parent: &Option<String>, what: &str) -> String {
    let n = SUB.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    match parent {
        Some(p) => format!("{p}:{what}:{n}"),
        None => format!("{}:{what}:{n}", crate::llm::BG),
    }
}

pub async fn complete_ex(
    state: &AppState,
    provider_id: &str,
    model: Option<&str>,
    thinking: bool,
    system: &str,
    msgs: &[ChatMsg],
    opts: &crate::llm::Opts,
) -> Result<crate::llm::Output> {
    let (mut p, key) = provider(state, provider_id)?;
    if let Some(m) = model.filter(|m| !m.trim().is_empty()) {
        p.model = m.trim().to_string();
    }
    resolve_gemini_model(state, &mut p).await;
    if p.model.trim().is_empty() && p.kind != "claude_cli" {
        bail!("Choose a model for {} in Settings → AI assistant.", p.name);
    }
    let mut out = crate::llm::complete_ex(state, &p, &key, thinking, system, msgs, opts).await?;
    out.text = out.text.trim().to_string();
    Ok(out)
}

// ---------------- mailbox context ----------------

fn allowed_accounts(state: &AppState) -> Result<Vec<i64>> {
    let all: Vec<Account> = state.db.accounts()?;
    let allowed: Option<Vec<i64>> = state.db.get_setting("ai_allowed_accounts")?.and_then(|v| serde_json::from_str::<Vec<i64>>(&v).ok());
    Ok(all.iter().map(|a| a.id).filter(|id| allowed.as_ref().map(|al| al.contains(id)).unwrap_or(true)).collect())
}

/// Account ids the assistant may read, intersected with the requested scope.
fn scoped_accounts(state: &AppState, scope: &Scope) -> Result<Vec<i64>> {
    let all: Vec<Account> = state.db.accounts()?;
    let mut ids = allowed_accounts(state)?;
    match scope.kind.as_str() {
        "accounts" => ids.retain(|id| scope.ids.contains(id)),
        "space" => {
            let in_space: Vec<i64> = all.iter().filter(|a| a.space_id.map(|s| scope.ids.contains(&s)).unwrap_or(false)).map(|a| a.id).collect();
            ids.retain(|id| in_space.contains(id));
        }
        _ => {}
    }
    Ok(ids)
}

fn trunc(s: &str, n: usize) -> String {
    let s = crate::parse::collapse_ws(s);
    if s.chars().count() <= n {
        s
    } else {
        format!("{}…", s.chars().take(n).collect::<String>())
    }
}

fn fmt_date(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0)
        .map(|d| d.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_default()
}

/// Make sure bodies are downloaded and indexed for these messages (one connection per account).
pub async fn ensure_bodies(state: &AppState, ids: &[i64]) {
    let mut by_acc: HashMap<i64, Vec<MessageRow>> = HashMap::new();
    for id in ids {
        if let Ok(m) = state.db.message(*id) {
            let has_file = !m.raw_path.is_empty() && std::path::Path::new(&m.raw_path).exists();
            if !has_file {
                by_acc.entry(m.account_id).or_default().push(m);
            }
        }
    }
    for (aid, msgs) in by_acc {
        if let Ok(a) = state.db.account(aid) {
            if a.protocol == "imap" {
                if let Err(e) = crate::imap::fetch_bodies(state, &a, &msgs).await {
                    eprintln!("fetch bodies: {e:#}");
                }
            }
        }
    }
}

/// Body text (and optionally attachment text) of one message, from the stored .eml.
fn read_message(m: &MessageRow, with_attachments: bool) -> (String, Vec<(String, String)>) {
    let Ok(raw) = std::fs::read(&m.raw_path) else { return (m.snippet.clone(), vec![]) };
    let Some(msg) = crate::parse::parse(&raw) else { return (m.snippet.clone(), vec![]) };
    let body = crate::parse::plain_text(&msg);
    let mut atts = vec![];
    if with_attachments {
        for (i, _) in msg.attachments().enumerate() {
            if let Some((name, mime, data)) = crate::parse::attachment(&raw, i) {
                if data.len() > 25 * 1024 * 1024 {
                    atts.push((name, "[file too large to read]".into()));
                    continue;
                }
                match crate::docs::extract_text(&name, &mime, &data) {
                    Some(t) => atts.push((name, t)),
                    None if crate::docs::is_image(&name, &mime) => atts.push((name, "[image attached separately]".into())),
                    None => atts.push((name, format!("[{mime}: no readable text — may be encrypted, scanned or binary]"))),
                }
            }
        }
    }
    (body, atts)
}

/// Image attachments (small ones) of a message, for models that can see images.
fn message_images(m: &MessageRow, max: usize) -> Vec<Image> {
    use base64::Engine;
    let Ok(raw) = std::fs::read(&m.raw_path) else { return vec![] };
    let Some(msg) = crate::parse::parse(&raw) else { return vec![] };
    let n = msg.attachments().count();
    let mut out = vec![];
    for i in 0..n {
        if out.len() >= max {
            break;
        }
        if let Some((name, mime, data)) = crate::parse::attachment(&raw, i) {
            let mime = if mime.starts_with("image/") { mime } else { mime_guess::from_path(&name).first_or_octet_stream().to_string() };
            if crate::docs::is_image(&name, &mime) && data.len() <= 4 * 1024 * 1024 && ["image/png", "image/jpeg", "image/webp", "image/gif"].contains(&mime.as_str()) {
                out.push(Image { mime, data: base64::engine::general_purpose::STANDARD.encode(&data) });
            }
        }
    }
    out
}

/// Whether the chosen model accepts images.
pub fn vision(state: &AppState, kind: &str, model: &str) -> bool {
    match kind {
        "openai" | "gemini" | "anthropic" | "grok" | "claude_cli" => true,
        "npu" => crate::npu::is_vlm(state, model),
        // Unknown OpenAI-compatible servers: let the server decide.
        "local" => true,
        _ => false,
    }
}

fn describe(state: &AppState, m: &MessageRow, body_chars: usize, with_attachments: bool) -> String {
    describe_as(state, m, &format!("[#{}]", m.id), body_chars, with_attachments)
}

/// One email for the prompt, introduced by `label` (`[E3]` in answers, `[#id]` where real ids are needed).
fn describe_as(state: &AppState, m: &MessageRow, label: &str, body_chars: usize, with_attachments: bool) -> String {
    let (body, atts) = read_message(m, with_attachments);
    let body = if body.trim().is_empty() { state.db.indexed_body(m.id).unwrap_or_default() } else { body };
    let body = if body.trim().is_empty() { m.snippet.clone() } else { body };
    let account = state.db.account(m.account_id).map(|a| a.email).unwrap_or_default();
    let folder = state.db.folder(m.folder_id).map(|f| f.name).unwrap_or_default();
    let mut out = format!(
        "{} {} | From: {} <{}> | To: {} | Cc: {} | Mailbox: {} / {} | {}{}{}\nSubject: {}\n--- body ---\n{}\n",
        label,
        fmt_date(m.date),
        m.from_name,
        m.from_addr,
        trunc(&m.to_list, 200),
        trunc(&m.cc_list, 160),
        account,
        folder,
        if m.seen { "read" } else { "UNREAD" },
        if m.flagged { ", flagged" } else { "" },
        if m.has_attachments { ", has attachments" } else { "" },
        m.subject,
        trunc(&body, body_chars)
    );
    for (name, text) in atts {
        out.push_str(&format!("--- attachment: {name} ---\n{}\n", trunc(&text, body_chars.max(4000))));
    }
    out
}

fn scope_sql(scope: &Scope, accounts: &[i64], from: Option<i64>, to: Option<i64>) -> String {
    let acc = if accounts.is_empty() { "-1".into() } else { accounts.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(",") };
    let mut w = format!("account_id IN ({acc}) AND folder_id NOT IN (SELECT id FROM folders WHERE role IN ('trash','junk','all'))");
    let list = |ids: &[i64]| ids.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(",");
    match scope.kind.as_str() {
        "group" => w.push_str(&format!(" AND id IN (SELECT message_id FROM message_groups WHERE group_id IN ({}))", list(&scope.ids))),
        "folder" => w.push_str(&format!(" AND folder_id IN ({})", list(&scope.ids))),
        _ => {}
    }
    if let Some(f) = from {
        w.push_str(&format!(" AND date >= {f}"));
    }
    if let Some(t) = to {
        w.push_str(&format!(" AND date < {t}"));
    }
    w
}

fn prefixed(where_: &str) -> String {
    format!("m.{}", where_.replace(" AND ", " AND m."))
}

fn stats(state: &AppState, where_: &str) -> Result<String> {
    state.db.with(|c| {
        let mut out = String::new();
        let total: i64 = c.query_row(&format!("SELECT COUNT(*) FROM messages WHERE {where_}"), [], |r| r.get(0))?;
        let unread: i64 = c.query_row(&format!("SELECT COUNT(*) FROM messages WHERE {where_} AND seen=0"), [], |r| r.get(0))?;
        out.push_str(&format!("Total messages: {total}, unread: {unread}\n\nBy mailbox:\n"));
        let mut st = c.prepare(&format!(
            "SELECT a.email, COUNT(*), SUM(CASE WHEN m.seen=0 THEN 1 ELSE 0 END) FROM messages m JOIN accounts a ON a.id=m.account_id
             WHERE {} GROUP BY a.email ORDER BY 2 DESC",
            prefixed(where_)
        ))?;
        for r in st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?)))? {
            let (e, n, u) = r?;
            out.push_str(&format!("- {e}: {n} messages ({u} unread)\n"));
        }
        out.push_str("\nTop senders:\n");
        let mut st = c.prepare(&format!(
            "SELECT from_addr, MAX(from_name), COUNT(*) FROM messages WHERE {where_} GROUP BY from_addr ORDER BY 3 DESC LIMIT 15"
        ))?;
        for r in st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)))? {
            let (a, n, k) = r?;
            out.push_str(&format!("- {n} <{a}>: {k}\n"));
        }
        out.push_str("\nBy group:\n");
        let mut st = c.prepare(&format!(
            "SELECT g.name, COUNT(*) FROM message_groups mg JOIN groups g ON g.id=mg.group_id JOIN messages m ON m.id=mg.message_id
             WHERE {} GROUP BY g.name ORDER BY 2 DESC",
            prefixed(where_)
        ))?;
        for r in st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))? {
            let (g, k) = r?;
            out.push_str(&format!("- {g}: {k}\n"));
        }
        out.push_str("\nPer day (most recent 30 days with mail):\n");
        let mut st = c.prepare(&format!(
            "SELECT date(date,'unixepoch','localtime') d, COUNT(*) FROM messages WHERE {where_} GROUP BY d ORDER BY d DESC LIMIT 30"
        ))?;
        for r in st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))? {
            let (d, k) = r?;
            out.push_str(&format!("{d}: {k}; "));
        }
        Ok(out)
    })
}

fn lang_line(lang: &str) -> &'static str {
    if lang == "vi" {
        "Reply in Vietnamese unless the user writes in another language."
    } else {
        "Reply in English unless the user writes in another language."
    }
}

const SYSTEM: &str = "You are the email assistant built into Hermio, a desktop email client. You only see the emails provided below \
(the user decides which mailboxes you may access). Each email includes its headers, full body text and, when available, the text of its attachments. \
Be concise and accurate. Each email below starts with a reference such as [E1]. When you refer to an email, cite its reference exactly (e.g. [E2]); \
use only references that appear below and never make one up. Never invent content that is not in the emails. \
You never send mail yourself: every reply draft is reviewed and sent by the user.";

/// System prompt for small (1–5B) on-device models: short sentences, explicit steps, no ambiguity.
const SMALL_SYSTEM: &str = "You are Hermio's email assistant running on the user's own computer.
RULES:
1. Use ONLY the emails and files given below. If the answer is not there, say so in one sentence.
2. Read the BODY of each email, not just the subject.
3. Be short and concrete: names, dates, amounts, deadlines, what to do next.
4. Each email starts with a reference like [E1]. When you mention an email, copy its reference exactly, e.g. [E2]. Only use references that exist below.
5. Never invent people, dates or numbers. Never say you sent an email.
6. Use simple Markdown: short headings and bullet points.";


/// A worked example per task — small models copy structure much more reliably than they follow abstract instructions.
fn small_examples(task: &str) -> &'static str {
    // Placeholders only: tiny models copy concrete example content into their answers.
    match task {
        "summarize" => "ANSWER FORMAT (a template — replace every <…> with facts from the emails below; never copy the template text):
### Summary
- **<topic of one email>** [E<n>]: <key fact: who / what / amount / date>. **Action:** <what to do, if anything>
- **<topic of another email>** [E<n>]: <key fact>
### Needs your reply
- <who> asks <what> [E<n>]
(Leave out a section when nothing belongs in it.)",
        "reply" => "ANSWER FORMAT: write only the body of the reply — greeting, 1-4 short sentences that do what the user asked, sign-off. \
Match the language and politeness of the original email. No subject line, no notes, no placeholders like [Name].",
        "cluster" => "Output ONLY JSON in the format given above. Use only ids that appear in the emails below.",
        "report" => "ANSWER FORMAT (a template — fill it with facts from the statistics and emails below; never copy the template text):
## Overview
- <total emails>, <unread>. Busiest sender: <name> (<count>).
## Important
- <item> [E<n>]
## Waiting for your reply
- <who> — <what> [E<n>]
## Suggestions
- <one practical suggestion>
Use the exact numbers from the statistics section.",
        _ => "ANSWER FORMAT: answer the question directly in 1-6 short bullet points, each with the reference of the email it comes from, \
like: - **<who or what>** <the fact> [E<n>]. If the emails don't contain the answer, say so in one sentence.",
    }
}

// ---------------- reading a whole scope ----------------

/// "8 Oct – 7 Nov 2026": when the emails that were read arrived.
fn span_of(state: &AppState, ids: &[i64]) -> String {
    if ids.is_empty() {
        return String::new();
    }
    let list = ids.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(",");
    let r: Option<(i64, i64)> = state.db.with(|c| c.query_row(&format!("SELECT MIN(date), MAX(date) FROM messages WHERE id IN ({list})"), [], |r| Ok((r.get(0)?, r.get(1)?)))).ok();
    let Some((a, b)) = r else { return String::new() };
    let d = |t: i64| chrono::DateTime::from_timestamp(t, 0).map(|x| x.with_timezone(&chrono::Local).format("%-d %b %Y").to_string()).unwrap_or_default();
    if d(a) == d(b) { d(a) } else { format!("{} – {}", d(a), d(b)) }
}

/// Safety cap on how many emails one summary/report reads.
const SCOPE_CAP: i64 = 3000;

/// Roughly how many characters of email text one request to this kind of model can take
/// (leaving room for instructions and the answer).
fn context_budget(kind: &str) -> usize {
    match kind {
        "npu" => {
            if crate::npu::vendor() == "intel" {
                (crate::ovms::MAX_PROMPT as usize) * 2
            } else {
                14_000
            }
        }
        "local" => 24_000,
        "gemini" => 600_000,
        "anthropic" => 400_000,
        "claude_cli" => 300_000,
        _ => 240_000, // OpenAI, Grok
    }
}

/// Map-reduce over many emails: one short note per email (several emails per request), then —
/// if the notes are still too long — condensed notes. Returns None when the user pressed Stop.
async fn digest(state: &AppState, req: &AiRequest, ids: &[i64], budget: usize, compact: bool) -> Result<Option<String>> {
    let body = if compact { 600 } else { 1200 };
    let max_batch = if compact { 6 } else { 40 };
    let total = ids.len();
    let note_sys = format!(
        "You read emails for a summary. For EVERY email below write exactly ONE line, starting with the email's reference exactly as given:\n\
         [E<n>] <sender>: <what it is about — key facts: names, dates, amounts, decisions>. ACTION: <what the user must do and by when>\n\
         Leave out \"ACTION:\" when nothing is needed. No other text, no headings. Use only facts from the emails. {}",
        lang_line(&req.lang)
    );
    let line_re = regex_lite::Regex::new(r"^\s*[-*•]?\s*\[E(\d+)\]").unwrap();
    let mut notes: Vec<(usize, String)> = vec![];

    // Small models are slow per request: bulk mail (notifications, newsletters, the same sender
    // many times) is summarised from its subjects without the model; personal mail is read in full.
    let mut skip = vec![false; total];
    if compact {
        let auto_re = regex_lite::Regex::new(r"(?i)(no-?reply|do-?not-?reply|notification|newsletter|mailer|digest|updates?@|alerts?@|news@|info@|marketing|bounce)").unwrap();
        let mut by_sender: Vec<(String, Vec<usize>)> = vec![];
        for (k, id) in ids.iter().enumerate() {
            if let Ok(m) = state.db.message(*id) {
                let key = m.from_addr.to_lowercase();
                match by_sender.iter_mut().find(|x| x.0 == key) {
                    Some(x) => x.1.push(k),
                    None => by_sender.push((key, vec![k])),
                }
            }
        }
        for (addr, ks) in by_sender {
            if !(ks.len() >= 6 || (ks.len() >= 3 && auto_re.is_match(&addr))) {
                continue;
            }
            let rows: Vec<MessageRow> = ks.iter().filter_map(|k| state.db.message(ids[*k]).ok()).collect();
            let who = rows.iter().map(|m| m.from_name.trim()).find(|n| !n.is_empty()).unwrap_or(&addr).to_string();
            let refs: String = ks.iter().take(3).map(|k| format!("[E{}]", k + 1)).collect();
            let mut subjects: Vec<String> = vec![];
            for m in &rows {
                let s = trunc(m.subject.trim(), 70);
                if !s.is_empty() && !subjects.contains(&s) {
                    subjects.push(s);
                }
                if subjects.len() >= 5 {
                    break;
                }
            }
            notes.push((ks[0] + 1, format!("{refs} {who} <{addr}> — {} emails (bulk). Subjects: {}", ks.len(), subjects.join("; "))));
            for k in ks {
                skip[k] = true;
            }
        }
    }
    let read_total = skip.iter().filter(|x| !**x).count();
    let mut read_done = 0usize;
    let mut i = 0;
    while i < total {
        if skip[i] {
            i += 1;
            continue;
        }
        if crate::llm::stop_requested(&req.stream_id) {
            return Ok(None);
        }
        state.emit("ai-progress", json!({ "done": read_done, "total": read_total, "bulk": total - read_total }));
        // A batch: as many emails as fit comfortably.
        let mut batch = String::new();
        let mut members: Vec<(usize, MessageRow)> = vec![];
        while i < total && members.len() < max_batch {
            if skip[i] {
                i += 1;
                continue;
            }
            let Ok(m) = state.db.message(ids[i]) else {
                i += 1;
                continue;
            };
            let one = describe_as(state, &m, &format!("[E{}]", i + 1), body, false);
            if !members.is_empty() && batch.len() + one.len() > budget * 11 / 20 {
                break;
            }
            batch.push_str(&one);
            batch.push('\n');
            members.push((i + 1, m));
            i += 1;
            read_done += 1;
        }
        let text = complete_sub(
            state,
            &req.provider_id,
            req.model.as_deref(),
            &note_sys,
            &[ChatMsg { role: "user".into(), content: batch, images: vec![] }],
            sub_id(&req.stream_id, "map"),
        )
        .await
        .unwrap_or_default();
        if crate::llm::stop_requested(&req.stream_id) {
            return Ok(None);
        }
        let mut got: Vec<usize> = vec![];
        for line in text.lines() {
            if let Some(c) = line_re.captures(line) {
                let n: usize = c[1].parse().unwrap_or(0);
                if members.iter().any(|(k, _)| *k == n) && !got.contains(&n) {
                    got.push(n);
                    notes.push((n, trunc(line.trim().trim_start_matches(['-', '*', '•']).trim(), 400)));
                }
            }
        }
        // Emails the model skipped still appear, with their subject.
        for (n, m) in &members {
            if !got.contains(n) {
                let who = if m.from_name.is_empty() { &m.from_addr } else { &m.from_name };
                notes.push((*n, format!("[E{n}] {who}: {}", trunc(&m.subject, 160))));
            }
        }
    }
    state.emit("ai-progress", json!({ "done": read_total, "total": read_total }));
    notes.sort_by_key(|x| x.0);
    let mut lines: Vec<String> = notes.into_iter().map(|x| x.1).collect();

    // Reduce: condense the notes until they fit in the final request.
    let reduce_sys = format!(
        "Condense these notes about emails into a shorter list. Merge notes on the same topic or sender into one line, \
         keep every ACTION and deadline, and keep the [E<n>] references (a line may carry several, like [E3][E7]). \
         Drop newsletters and automated notices that need no action into one line per sender. One item per line, no headings. {}",
        lang_line(&req.lang)
    );
    let target = budget * 3 / 5;
    for _round in 0..3 {
        if lines.iter().map(|l| l.len() + 1).sum::<usize>() <= target {
            break;
        }
        let mut out: Vec<String> = vec![];
        let mut chunk = String::new();
        let mut chunks: Vec<String> = vec![];
        for l in &lines {
            if !chunk.is_empty() && chunk.len() + l.len() > budget / 2 {
                chunks.push(std::mem::take(&mut chunk));
            }
            chunk.push_str(l);
            chunk.push('\n');
        }
        if !chunk.is_empty() {
            chunks.push(chunk);
        }
        let n_chunks = chunks.len();
        for (k, c) in chunks.into_iter().enumerate() {
            if crate::llm::stop_requested(&req.stream_id) {
                return Ok(None);
            }
            state.emit("ai-progress", json!({ "done": k, "total": n_chunks, "stage": "condense" }));
            let text = complete_sub(state, &req.provider_id, req.model.as_deref(), &reduce_sys, &[ChatMsg { role: "user".into(), content: c.clone(), images: vec![] }], sub_id(&req.stream_id, "reduce"))
                .await
                .unwrap_or_default();
            if crate::llm::stop_requested(&req.stream_id) {
                return Ok(None);
            }
            let kept: Vec<String> = text.lines().map(|l| l.trim().trim_start_matches(['-', '*', '•']).trim().to_string()).filter(|l| l.len() > 3).collect();
            // Never lose a chunk to a bad answer.
            if kept.is_empty() || kept.iter().map(|l| l.len()).sum::<usize>() >= c.len() {
                out.extend(c.lines().map(|l| trunc(l, 200)));
            } else {
                out.extend(kept);
            }
        }
        lines = out;
    }
    let mut joined = lines.join("\n");
    if joined.len() > budget * 3 / 4 {
        joined = joined.chars().take(budget * 3 / 4).collect();
    }
    Ok(Some(joined))
}

// ---------------- topic clustering ----------------

/// Topics the model prefers (it may also name its own).
const TOPICS: &[&str] = &[
    "Marketing", "Promotions", "Newsletters", "Notifications", "Security", "Receipts & finance", "Shopping & orders",
    "Travel", "Social", "Work", "University", "Thesis & research", "Jobs & careers", "Events", "Personal", "Spam",
];
/// Labels that aren't worth a group.
const NO_GROUP: &[&str] = &["personal", "other", "others", "misc", "unknown", "none", "keep", "inbox"];

const STOPWORDS: &[&str] = &[
    "your", "with", "from", "this", "that", "have", "will", "about", "into", "more", "here", "there", "what", "when", "where", "which",
    "their", "they", "them", "been", "were", "would", "could", "should", "just", "only", "also", "than", "then", "some", "most",
    "re:", "fwd:", "dear", "hello", "thanks", "thank", "please", "today", "week", "new", "update", "updates", "email", "mail",
    "cho", "của", "các", "những", "được", "trong", "với", "này", "đã", "một", "không", "bạn", "và", "là",
];

fn norm_topic(label: &str, known: &[String]) -> String {
    let l = label.trim().trim_matches(['*', '"', '.', '`']).trim();
    let key = |x: &str| x.to_lowercase().trim_end_matches('s').replace(['&', '-', '_'], " ").split_whitespace().collect::<Vec<_>>().join(" ");
    let k = key(l);
    for t in known.iter().map(|s| s.as_str()).chain(TOPICS.iter().copied()) {
        if key(t) == k {
            return t.to_string();
        }
    }
    let mut c = l.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

fn subject_words(s: &str) -> Vec<String> {
    let mut out: Vec<String> = s
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 4 && !w.chars().all(|c| c.is_ascii_digit()) && !STOPWORDS.contains(w))
        .map(|w| w.to_string())
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Content clustering for every model: the model gives each email a topic (Marketing,
/// Promotions, Spam, Thesis…, or its own), Hermio groups by topic — any number of senders per
/// group — and builds a rule from what the group's mail has in common: sender domains/addresses
/// that send almost only this kind of mail, and words typical of its subjects. Each rule is
/// checked against the whole scope before it's suggested.
async fn cluster_topics(state: &AppState, req: &AiRequest, ids: &[i64], compact: bool) -> Result<AiResponse> {
    let groups = state.db.groups()?;
    let known: Vec<String> = groups.iter().map(|g| g.name.clone()).collect();
    let own: Vec<String> = state
        .db
        .accounts()?
        .iter()
        .filter_map(|a| a.email.rsplit_once('@').map(|(_, d)| base_domain(&d.to_lowercase())))
        .filter(|d| !FREEMAIL.contains(&d.as_str()))
        .collect();
    let rows: Vec<MessageRow> = ids.iter().filter_map(|id| state.db.message(*id).ok()).collect();
    // Mail that is already in a group isn't clustered again (but still counts when rules are checked).
    let todo: Vec<usize> = (0..rows.len()).filter(|&k| state.db.message_group_ids(rows[k].id).unwrap_or_default().is_empty()).collect();
    if todo.len() < 2 {
        bail!("Nothing to group in this range: the mail here is already in groups. Try a longer time range or another mailbox.");
    }

    // 1) A topic per email.
    let batch = if compact { 8 } else { 30 };
    let body_chars = if compact { 220 } else { 450 };
    let topic_list = TOPICS.iter().copied().chain(known.iter().map(|s| s.as_str())).collect::<Vec<_>>().join(", ");
    let system = format!(
        "You sort emails by what they are about. Give EACH email exactly one topic.\n\
         Prefer one of these topics: {topic_list}.\n\
         If none fits, write a short new topic of 1-3 words naming the subject (a project, course, service or organisation), e.g. \"FIT5037 assignments\".\n\
         Use \"Personal\" for private conversations. Judge by the content, not only the sender.\n\
         Answer with one line per email and nothing else:\nE1 | Topic\nE2 | Topic"
    );
    let line = regex_lite::Regex::new(r"^\W*[Ee]?\s*(\d+)[\s*]*[|:.)\]\-]+\s*(.+)$").unwrap();
    let mut label: HashMap<usize, String> = HashMap::new();
    let total = todo.len();
    for (bi, chunk) in todo.chunks(batch).enumerate() {
        if crate::llm::stop_requested(&req.stream_id) {
            crate::llm::clear_stop(&req.stream_id);
            return Ok(AiResponse { cancelled: true, used: ids.to_vec(), ..Default::default() });
        }
        state.emit("ai-progress", json!({ "done": bi * batch, "total": total }));
        let mut list = String::new();
        for (j, &k) in chunk.iter().enumerate() {
            let m = &rows[k];
            let body = state.db.indexed_body(m.id).unwrap_or_default();
            let text = if body.trim().is_empty() { m.snippet.clone() } else { body };
            list.push_str(&format!("E{}: From {} <{}>\n    Subject: {}\n    Text: {}\n", j + 1, m.from_name, m.from_addr, trunc(&m.subject, 140), trunc(&text, body_chars)));
        }
        let text = complete_sub(state, &req.provider_id, req.model.as_deref(), &system, &[ChatMsg { role: "user".into(), content: list, images: vec![] }], sub_id(&req.stream_id, "topic"))
            .await
            .unwrap_or_default();
        for l in text.lines() {
            if let Some(c) = line.captures(l.trim()) {
                let n: usize = c[1].parse().unwrap_or(0);
                if let Some(&k) = n.checked_sub(1).and_then(|i| chunk.get(i)) {
                    let raw = c[2].split('|').next().unwrap_or("").trim();
                    let t = norm_topic(raw, &known);
                    if !t.is_empty() && t.chars().count() <= 40 {
                        label.entry(k).or_insert(t);
                    }
                }
            }
        }
    }
    state.emit("ai-progress", json!({ "done": total, "total": total }));
    if label.is_empty() {
        bail!("The model didn't return topics for these emails. Try again or pick another model.");
    }

    // 2) Group by topic.
    let mut by: Vec<(String, Vec<usize>)> = vec![];
    for &k in &todo {
        let Some(t) = label.get(&k) else { continue };
        if NO_GROUP.contains(&t.to_lowercase().as_str()) {
            continue;
        }
        match by.iter_mut().find(|x| x.0 == *t) {
            Some(x) => x.1.push(k),
            None => by.push((t.clone(), vec![k])),
        }
    }
    by.retain(|x| x.1.len() >= 2);
    by.sort_by(|a, b| b.1.len().cmp(&a.1.len()));
    by.truncate(12);
    if by.is_empty() {
        bail!("No topic has more than one email here. Try a longer time range.");
    }

    // 3) A rule per group, checked against everything in scope.
    let key_of = |m: &MessageRow| sender_key(&m.from_addr, &own);
    let mut per_sender: HashMap<String, usize> = HashMap::new();
    for m in &rows {
        if let Some((k, _)) = key_of(m) {
            *per_sender.entry(k).or_default() += 1;
        }
    }
    let words: Vec<Vec<String>> = rows.iter().map(|m| subject_words(&m.subject)).collect();
    let acc_email: HashMap<i64, String> = state.db.accounts()?.into_iter().map(|a| (a.id, a.email)).collect();
    let evaluate = |conds: &[Condition], members: &[usize]| -> (usize, usize) {
        if conds.is_empty() {
            return (0, 0);
        }
        let r = Rule { enabled: true, match_all: false, conditions: conds.to_vec(), ..Default::default() };
        let (mut hin, mut hout) = (0, 0);
        for (k, m) in rows.iter().enumerate() {
            if crate::rules::matches(&r, m, acc_email.get(&m.account_id).map(|s| s.as_str()).unwrap_or(""), "") {
                if members.contains(&k) { hin += 1 } else { hout += 1 }
            }
        }
        (hin, hout)
    };
    let mut clusters = vec![];
    for (title, members) in by {
        // Senders that (almost) only send this kind of mail.
        let mut senders: Vec<(String, bool, usize)> = vec![];
        for &k in &members {
            if let Some((key, is_dom)) = key_of(&rows[k]) {
                match senders.iter_mut().find(|s| s.0 == key) {
                    Some(s) => s.2 += 1,
                    None => senders.push((key, is_dom, 1)),
                }
            }
        }
        let n_senders = senders.len();
        let loyal: Vec<&(String, bool, usize)> = senders.iter().filter(|(k, _, c)| *c as f64 >= 0.7 * *per_sender.get(k).unwrap_or(&1) as f64).collect();
        let mut conds: Vec<Condition> = vec![];
        let doms: Vec<String> = loyal.iter().filter(|s| s.1).map(|s| s.0.clone()).take(12).collect();
        let addrs: Vec<String> = loyal.iter().filter(|s| !s.1).map(|s| s.0.clone()).take(12).collect();
        if !doms.is_empty() {
            conds.push(Condition { field: "from".into(), op: "domain".into(), value: doms.join(", ") });
        }
        if !addrs.is_empty() {
            conds.push(Condition { field: "from".into(), op: "contains".into(), value: addrs.join(", ") });
        }
        // Words typical of this group's subjects and rare elsewhere.
        let others = rows.len() - members.len();
        let mut cand: HashMap<&str, (usize, usize)> = HashMap::new();
        for (k, ws) in words.iter().enumerate() {
            let inside = members.contains(&k);
            for w in ws {
                let e = cand.entry(w.as_str()).or_default();
                if inside { e.0 += 1 } else { e.1 += 1 }
            }
        }
        let mut kw: Vec<(&str, usize)> = cand
            .into_iter()
            .filter(|(_, (i, o))| *i >= 2 && *i as f64 >= 0.3 * members.len() as f64 && (*o as f64) <= (0.02 * others as f64).max(1.0))
            .map(|(w, (i, _))| (w, i))
            .collect();
        kw.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        if !kw.is_empty() {
            conds.push(Condition { field: "subject".into(), op: "contains".into(), value: kw.iter().take(3).map(|x| x.0).collect::<Vec<_>>().join(", ") });
        }
        // Drop conditions that catch more unrelated mail than mail of this group.
        let mut keep: Vec<Condition> = vec![];
        for c in conds {
            let (i, o) = evaluate(std::slice::from_ref(&c), &members);
            if i > 0 && o * 3 <= i {
                keep.push(c);
            }
        }
        let (hits, extra) = evaluate(&keep, &members);
        let names: Vec<String> = {
            let mut v: Vec<String> = vec![];
            for &k in &members {
                let m = &rows[k];
                let n = if m.from_name.trim().is_empty() { m.from_addr.clone() } else { m.from_name.trim().to_string() };
                if !v.contains(&n) {
                    v.push(n);
                }
            }
            v
        };
        let shown = names.iter().take(4).cloned().collect::<Vec<_>>().join(", ");
        let more = if names.len() > 4 { format!(" +{}", names.len() - 4) } else { String::new() };
        clusters.push(Cluster {
            summary: format!("{} emails from {n_senders} sender(s): {shown}{more}", members.len()),
            ids: members.iter().map(|&k| rows[k].id).collect(),
            rule: if keep.is_empty() { None } else { Some(RuleProposal { match_all: false, conditions: keep }) },
            senders: n_senders,
            rule_hits: hits,
            rule_extra: extra,
            title,
        });
    }
    Ok(AiResponse { text: format!("{} clusters", clusters.len()), used: ids.to_vec(), clusters, span: span_of(state, ids), ..Default::default() })
}

// ---------------- citations ----------------

/// Normalise email citations to `[E<n>]` (n = position in `ids`, 1-based) and drop the ones that
/// point nowhere — small models often write `[#3]`, `[e3]`, `[E3, E5]` or invent numbers.
/// Plain `[1]` web-source citations are left alone.
pub fn fix_citations(text: &str, ids: &[i64]) -> String {
    let group = regex_lite::Regex::new(r"\[\s*((?:[Ee#]\s*\d+\s*[,;]?\s*)+)\]").unwrap();
    let item = regex_lite::Regex::new(r"([Ee#])\s*(\d+)").unwrap();
    let out = group.replace_all(text, |c: &regex_lite::Captures| {
        let mut refs: Vec<String> = vec![];
        for it in item.captures_iter(&c[1]) {
            let Ok(n) = it[2].parse::<i64>() else { continue };
            let k = if it[1].eq_ignore_ascii_case("e") {
                (n >= 1 && (n as usize) <= ids.len()).then_some(n as usize)
            } else if let Some(p) = ids.iter().position(|x| *x == n) {
                Some(p + 1) // a real message id
            } else {
                (n >= 1 && (n as usize) <= ids.len()).then_some(n as usize)
            };
            if let Some(k) = k {
                let r = format!("[E{k}]");
                if !refs.contains(&r) {
                    refs.push(r);
                }
            }
        }
        refs.join("")
    });
    // Tidy spaces left by removed citations ("text  ." → "text.").
    let sp = regex_lite::Regex::new(r"[ \t]+([.,;:)])").unwrap();
    sp.replace_all(&out, "$1").to_string()
}

// ---------------- clustering helpers ----------------

const FREEMAIL: &[&str] = &[
    "gmail.com", "googlemail.com", "outlook.com", "hotmail.com", "live.com", "msn.com", "yahoo.com", "yahoo.com.vn",
    "icloud.com", "me.com", "mac.com", "proton.me", "protonmail.com", "gmx.com", "aol.com", "yandex.com", "zoho.com",
];

/// "notifications.github.com" → "github.com"; "mail.monash.edu.au" → "monash.edu.au".
pub fn base_domain(d: &str) -> String {
    let labels: Vec<&str> = d.trim().trim_end_matches('.').split('.').filter(|x| !x.is_empty()).collect();
    if labels.len() <= 2 {
        return labels.join(".");
    }
    let n = labels.len();
    let second = labels[n - 2];
    let take = if labels[n - 1].len() == 2 && ["com", "co", "edu", "gov", "org", "net", "ac", "or", "ne", "gob"].contains(&second) { 3 } else { 2 };
    labels[n - take..].join(".")
}

/// How mail from this sender is grouped: by organisation domain, or by the full address for
/// free-mail providers and the user's own organisation (colleagues aren't a "group").
fn sender_key(addr: &str, own_domains: &[String]) -> Option<(String, bool)> {
    let a = addr.trim().to_lowercase();
    let (_, dom) = a.rsplit_once('@')?;
    if FREEMAIL.contains(&dom) {
        return Some((a.clone(), false));
    }
    let base = base_domain(dom);
    if own_domains.contains(&base) {
        return Some((a.clone(), false));
    }
    Some((base, true))
}

/// Lenient reading of `{"clusters":[…]}`: tolerates code fences, ids as strings ("#12"), and
/// `emails` / `messages` instead of `ids`.
fn parse_clusters(text: &str) -> Option<Vec<Cluster>> {
    let t = text.replace("```json", "").replace("```", "");
    let (s, e) = (t.find('{')?, t.rfind('}')?);
    let v: Value = serde_json::from_str(&t[s..=e]).ok().or_else(|| {
        // Trailing commas are the most common small-model mistake.
        let re = regex_lite::Regex::new(r",\s*([\]}])").unwrap();
        serde_json::from_str(&re.replace_all(&t[s..=e], "$1")).ok()
    })?;
    let arr = v.get("clusters").or_else(|| v.get("groups")).and_then(|x| x.as_array())?;
    let id_of = |x: &Value| -> Option<i64> {
        x.as_i64().or_else(|| x.as_str().and_then(|s| s.trim().trim_start_matches(['#', '[']).trim_end_matches(']').parse().ok()))
    };
    let out: Vec<Cluster> = arr
        .iter()
        .map(|c| {
            let ids = ["ids", "emails", "messages", "message_ids"]
                .iter()
                .find_map(|k| c.get(*k).and_then(|x| x.as_array()))
                .map(|a| a.iter().filter_map(id_of).collect())
                .unwrap_or_default();
            let rule = c.get("rule").and_then(|r| serde_json::from_value::<RuleProposal>(r.clone()).ok());
            Cluster {
                title: c.get("title").or_else(|| c.get("name")).and_then(|x| x.as_str()).unwrap_or("").to_string(),
                summary: c.get("summary").or_else(|| c.get("description")).and_then(|x| x.as_str()).unwrap_or("").to_string(),
                ids,
                rule,
                ..Default::default()
            }
        })
        .collect();
    Some(out)
}



fn range(req: &AiRequest) -> (Option<i64>, Option<i64>) {
    if req.from.is_some() || req.to.is_some() {
        return (req.from, req.to);
    }
    if req.days > 0 {
        (Some(chrono::Utc::now().timestamp() - req.days * 86400), None)
    } else {
        (None, None)
    }
}

pub async fn run(state: &AppState, req: AiRequest) -> Result<AiResponse> {
    let question = req.messages.iter().rev().find(|m| m.role == "user").map(|m| m.content.clone()).unwrap_or_default();

    // Signature drafting doesn't need any mail.
    if req.task == "signature" {
        let system = format!(
            "You design professional HTML email signatures. Return ONLY an HTML fragment (no <html>/<body>, no markdown fences), \
             using inline styles, web-safe fonts, at most ~6 short lines, no images unless a URL is given. {}",
            lang_line(&req.lang)
        );
        let text = complete(state, &req.provider_id, req.model.as_deref(), req.thinking, &system, &req.messages).await?;
        let html = text.trim().trim_start_matches("```html").trim_start_matches("```").trim_end_matches("```").trim().to_string();
        return Ok(AiResponse { text: html, ..Default::default() });
    }

    let accounts = scoped_accounts(state, &req.scope)?;
    let (from, to) = range(&req);
    // Small on-device models get a tighter context, a more explicit prompt with examples,
    // map-reduce for long summaries and a JSON repair pass.
    let kind = providers(state)?.into_iter().find(|p| p.id == req.provider_id).map(|p| p.kind).unwrap_or_default();
    let compact = kind == "npu" || kind == "local";
    let mut ids: Vec<i64> = vec![];
    let mut context = String::new();
    let mut body_chars = 1500;
    let mut with_attachments = false;

    if req.scope.kind == "selection" {
        for id in &req.scope.ids {
            if let Ok(m) = state.db.message(*id) {
                if accounts.contains(&m.account_id) {
                    ids.push(m.id);
                }
            }
        }
        body_chars = if compact {
            if ids.len() == 1 { 12_000 } else if ids.len() <= 3 { 5000 } else if ids.len() <= 10 { 1500 } else { 700 }
        } else if ids.len() <= 3 { 30_000 } else if ids.len() <= 10 { 6000 } else { 2000 };
        with_attachments = ids.len() <= 10;
    } else if req.task != "chat" || !req.messages.is_empty() || !req.files.is_empty() {
        if accounts.is_empty() {
            bail!("The assistant isn't allowed to read any mailbox in this scope (Settings → AI assistant → Mailbox access).");
        }
        let mut where_ = scope_sql(&req.scope, &accounts, from, to);
        // Summaries, reports and clustering are about mail you received.
        if matches!(req.task.as_str(), "summarize" | "report" | "cluster") && req.scope.kind != "folder" {
            where_.push_str(" AND folder_id NOT IN (SELECT id FROM folders WHERE role IN ('sent','drafts','outbox'))");
        }
        if req.task == "report" {
            context.push_str("## Statistics (exact, computed from the database)\n");
            context.push_str(&stats(state, &where_)?);
            context.push_str("\n\n");
        }
        let terms: Vec<String> = crate::db::search_terms(&question).into_iter().map(|(t, _)| t).filter(|t| t.chars().count() >= 3).take(10).collect();
        if !terms.is_empty() && req.task == "chat" {
            let fts = terms.iter().map(|t| format!("\"{}\"", t.replace('"', ""))).collect::<Vec<_>>().join(" OR ");
            let hits: Vec<i64> = state
                .db
                .with(|c| {
                    let mut st = c.prepare(&format!(
                        "SELECT m.id FROM messages m JOIN messages_fts f ON f.rowid=m.id WHERE messages_fts MATCH ?1 AND {} ORDER BY rank LIMIT 30",
                        prefixed(&where_)
                    ))?;
                    let r = st.query_map([fts], |r| r.get(0))?;
                    r.collect()
                })
                .unwrap_or_default();
            ids.extend(hits);
        }
        // Summaries and reports read every email in the chosen scope (mailbox + time range);
        // when that doesn't fit in one request they are read in batches (see `digest`).
        let whole = matches!(req.task.as_str(), "summarize" | "report");
        let recent_n = match (req.task.as_str(), compact) {
            (_, _) if whole => SCOPE_CAP,
            ("cluster", false) => 150,
            (_, false) => 40,
            ("cluster", true) => 60,
            (_, true) => 15,
        };
        if compact && !whole && ids.len() > 15 {
            ids.truncate(15);
        }
        for id in state.db.message_ids_where(&where_, recent_n)? {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
        body_chars = match (req.task.as_str(), compact) {
            ("cluster", false) => 500,
            ("report", false) => 600,
            (_, false) => 1500,
            ("cluster", true) => 260,
            ("report", true) => 350,
            (_, true) => 700,
        };
    }

    // Bodies of messages that haven't been downloaded yet (IMAP syncs bodies gradually).
    ensure_bodies(state, &ids).await;

    // Small models can't reliably return a big JSON over dozens of emails: cluster by sender in
    // Hermio and let the model only name the groups.
    if req.task == "cluster" {
        return cluster_topics(state, &req, &ids, compact).await;
    }

    // The UI learns which email each [E<n>] stands for before the answer starts streaming.
    if let Some(sid) = &req.stream_id {
        state.emit("ai-delta", json!({ "id": sid, "kind": "refs", "ids": ids }));
    }
    // Too many emails for one request: read them in batches first.
    let budget = context_budget(&kind);
    let whole = matches!(req.task.as_str(), "summarize" | "report") && req.scope.kind != "selection";
    let fits = |n: usize, per: usize| n * (per + 260) <= budget * 3 / 4;
    if whole && ids.len() > 3 && !fits(ids.len(), if compact { 250 } else { 300 }) {
        match digest(state, &req, &ids, budget, compact).await? {
            Some(notes) => {
                context.push_str(&format!(
                    "## Notes on all {} emails in scope (one line per email, written from the full text; [E<n>] is the email's reference)\n{notes}\n",
                    ids.len()
                ));
            }
            None => {
                // Stopped by the user while reading.
                crate::llm::clear_stop(&req.stream_id);
                return Ok(AiResponse { text: String::new(), used: ids, cancelled: true, ..Default::default() });
            }
        }
    } else if whole && !ids.is_empty() {
        // Everything fits: give each email as much of its text as the budget allows.
        let per = (budget * 3 / 4 / ids.len()).saturating_sub(260).clamp(250, if compact { 4000 } else { 30_000 });
        body_chars = body_chars.max(per);
        with_attachments = with_attachments || ids.len() <= 10;
        context.push_str(&format!("## Emails in scope ({} messages)\n", ids.len()));
        for (i, id) in ids.iter().enumerate() {
            if let Ok(m) = state.db.message(*id) {
                context.push_str(&describe_as(state, &m, &format!("[E{}]", i + 1), body_chars, with_attachments));
                context.push('\n');
            }
        }
    } else if !ids.is_empty() {
        context.push_str(&format!("## Emails in scope ({} messages)\n", ids.len()));
        for (i, id) in ids.iter().enumerate() {
            if let Ok(m) = state.db.message(*id) {
                // Clustering returns real ids; answers cite short numbers ([E1], [E2]…) that small models get right.
                let label = if req.task == "cluster" { format!("[#{}]", m.id) } else { format!("[E{}]", i + 1) };
                context.push_str(&describe_as(state, &m, &label, body_chars, with_attachments));
                context.push('\n');
            }
        }
    }

    // Files the user attached to the chat.
    let mut images: Vec<Image> = vec![];
    let pmodel = req
        .model
        .clone()
        .filter(|m| !m.is_empty())
        .or_else(|| providers(state).ok().and_then(|l| l.into_iter().find(|p| p.id == req.provider_id)).map(|p| p.model))
        .unwrap_or_default();
    let sees = vision(state, &kind, &pmodel);
    let user_images = req.files.iter().any(|f| f.text.is_none() && f.image.is_some()) || req.messages.iter().any(|m| !m.images.is_empty());
    if user_images && !sees {
        bail!("{pmodel} can't read images. Choose a model that can (for example Qwen3.5, Gemma 4, Gemini, ChatGPT or Claude) or remove the images.");
    }
    // Pictures attached to the selected emails, for models that can see them.
    if req.scope.kind == "selection" && ids.len() <= 3 {
        if sees {
            for id in &ids {
                if let Ok(m) = state.db.message(*id) {
                    images.extend(message_images(&m, 4));
                }
            }
            images.truncate(6);
        }
    }
    if !req.files.is_empty() {
        context.push_str("\n## Files attached by the user\n");
        for f in &req.files {
            match (&f.text, &f.image) {
                (Some(t), _) => context.push_str(&format!("--- file: {} ---\n{}\n", f.name, trunc(t, 60_000))),
                (None, Some(img)) => {
                    context.push_str(&format!("--- image: {} (attached below) ---\n", f.name));
                    images.push(Image { mime: f.mime.clone(), data: img.clone() });
                }
                _ => context.push_str(&format!("--- file: {} (no readable text) ---\n", f.name)),
            }
        }
    }

    // ----- web search -----
    let mut web_sources: Vec<crate::llm::Source> = vec![];
    let web_native = req.web && matches!(kind.as_str(), "gemini" | "anthropic" | "claude_cli");
    let mut web_note = String::new();
    if req.web && matches!(req.task.as_str(), "chat" | "report" | "summarize") {
        if web_native {
            web_note = "Web search is available: use it when the question needs current or outside information, and cite the pages you used.".into();
        } else {
            let q = if question.trim().is_empty() { req.task.clone() } else { question.clone() };
            let emit = |kind: &str, text: String| {
                if let Some(id) = &req.stream_id {
                    state.emit("ai-delta", json!({ "id": id, "kind": kind, "text": text }));
                }
            };
            emit("status", format!("web:{}", q.chars().take(120).collect::<String>()));
            match crate::web::search(state, &q).await {
                Ok(mut hits) => {
                    crate::web::enrich(&mut hits, if compact { 3 } else { 4 }).await;
                    if compact {
                        for h in hits.iter_mut() {
                            h.content = h.content.chars().take(1800).collect();
                        }
                        hits.truncate(5);
                    }
                    for h in &hits {
                        web_sources.push(crate::llm::Source { title: h.title.clone(), url: h.url.clone() });
                        if let Some(id) = &req.stream_id {
                            state.emit("ai-delta", json!({ "id": id, "kind": "source", "title": h.title, "url": h.url }));
                        }
                    }
                    context.push_str("\n");
                    context.push_str(&crate::web::context(&hits));
                    web_note = "Web search results are included below; use them for current information and cite them as [1], [2]…".into();
                }
                Err(e) => {
                    emit("status", format!("web-error:{e}"));
                    web_note = format!("Web search failed ({e}); answer from the emails and your own knowledge and say that live data was unavailable.");
                }
            }
        }
    }

    let groups = state.db.groups()?;
    let task_hint = match req.task.as_str() {
        "summarize" => "Task: summarise the emails above (key points, action items, deadlines, who is waiting for a reply). Use the body and attachment text, not just subjects.".to_string(),
        "reply" => "Task: write the BODY of a reply to the email above, following the user's instructions. Return only the body text (no subject line, no explanations), \
                    in the same language as the original email unless the user asks otherwise. The user will review and edit it before sending.".to_string(),
        "cluster" => format!(
            "Task: group the emails above into clusters by what they are actually about — read the body text, not only the subject. \
             For each cluster propose a sorting rule that would catch similar future emails. Rule fields: from | to | subject | body | any; \
             ops: contains | domain | equals | starts_with; several values may be comma-separated. Prefer sender domains/addresses when they identify the cluster well. \
             Existing groups (avoid duplicates): {}. \
             Return ONLY JSON: {{\"clusters\":[{{\"title\":\"…\",\"summary\":\"…\",\"ids\":[123,456],\"rule\":{{\"match_all\":false,\"conditions\":[{{\"field\":\"from\",\"op\":\"domain\",\"value\":\"github.com\"}}]}}}}]}}. \
             Skip one-off emails that don't belong to any cluster.",
            groups.iter().map(|g| g.name.clone()).collect::<Vec<_>>().join(", ")
        ),
        "report" => "Task: write a summary report of the mailbox using the statistics (use the exact numbers) and the email contents: \
                     trends, important items, unanswered mail, recommendations. Use Markdown headings and bullets.".to_string(),
        _ => "Answer the user's question using the emails and files above.".to_string(),
    };
    let now = format!("Current date and time: {}.", chrono::Local::now().format("%A %Y-%m-%d %H:%M"));
    // OpenVINO on Intel NPUs compiles the model for a fixed prompt length (ovms::MAX_PROMPT
    // tokens): keep the whole prompt well inside it.
    let intel_npu = kind == "npu" && crate::npu::vendor() == "intel";
    if intel_npu {
        let budget = (crate::ovms::MAX_PROMPT as usize) * 2; // ≈ chars for half the token budget
        if context.chars().count() > budget {
            context = format!("{}\n[…more emails left out to fit the on-device model…]", context.chars().take(budget).collect::<String>());
        }
    }
    let system = if compact {
        format!(
            "{SMALL_SYSTEM}\n{now}\n{}\n{web_note}\n\n{}\n\n{task_hint}{}\n\n{context}\n\nRemember: {}",
            lang_line(&req.lang),
            small_examples(&req.task),
            skills_prompt(state, if intel_npu { 2_000 } else { 12_000 }),
            lang_line(&req.lang)
        )
    } else {
        format!("{SYSTEM}\n{now}\n{}\n{web_note}\n\n{task_hint}{}\n\n{context}", lang_line(&req.lang), skills_prompt(state, 60_000))
    };
    let mut msgs = req.messages.clone();
    if msgs.is_empty() {
        msgs.push(ChatMsg { role: "user".into(), content: task_hint.clone(), images: vec![] });
    }
    if !images.is_empty() {
        if let Some(last) = msgs.iter_mut().rev().find(|m| m.role == "user") {
            last.images.extend(images);
        }
    }
    let opts = crate::llm::Opts { stream_id: req.stream_id.clone(), web_native };
    let out = complete_ex(state, &req.provider_id, req.model.as_deref(), req.thinking, &system, &msgs, &opts).await?;
    let mut text = out.text.clone();
    let mut sources = web_sources;
    for s in out.sources {
        if !sources.iter().any(|x| x.url == s.url) {
            sources.push(s);
        }
    }
    if out.cancelled {
        return Ok(AiResponse { text, used: ids, thinking: out.thinking, sources, cancelled: true, ..Default::default() });
    }

    if req.task == "cluster" && parse_clusters(&text).map(|c| c.is_empty()).unwrap_or(true) {
        // One repair pass: small models often wrap JSON in prose or break it.
        let mut retry = msgs.clone();
        retry.push(ChatMsg { role: "assistant".into(), content: text.chars().take(6000).collect(), images: vec![] });
        retry.push(ChatMsg {
            role: "user".into(),
            content: "That was not valid JSON. Reply again with ONLY the JSON object in the exact format requested — no explanations, no markdown fences.".into(),
            images: vec![],
        });
        if let Ok(t2) = complete(state, &req.provider_id, req.model.as_deref(), false, &system, &retry).await {
            text = t2;
        }
    }

    if req.task != "cluster" {
        if compact {
            // A tiny model sometimes echoes template lines ("<topic> [E<n>]") — drop those.
            let ph = regex_lite::Regex::new(r"<(topic|key fact|who|what|item|total|unread|name|count|one practical|the fact)[^>]*>|E<n>").unwrap();
            text = text.lines().filter(|l| !ph.is_match(l)).collect::<Vec<_>>().join("\n");
        }
        text = fix_citations(&text, &ids);
    }
    let mut resp = AiResponse { text: text.clone(), used: ids.clone(), clusters: vec![], thinking: out.thinking, sources, cancelled: false, span: span_of(state, &ids) };
    if req.task == "cluster" {
        if let Some(c) = parse_clusters(&text) {
            resp.clusters = c
                .into_iter()
                .map(|mut cl| {
                    cl.ids.retain(|i| ids.contains(i));
                    cl
                })
                .filter(|cl| !cl.ids.is_empty())
                .collect();
        }
        if resp.clusters.is_empty() {
            bail!("The model didn't return a valid grouping. Try again or pick another model.");
        }
        resp.text = format!("{} clusters", resp.clusters.len());
    }
    Ok(resp)
}

// ---------------- group suggestions for new mail ----------------

#[derive(Debug, Clone, Serialize)]
pub struct Suggestion {
    pub message_id: i64,
    pub group_id: Option<i64>,
    pub new_group: String,
    pub reason: String,
}

fn suggest_enabled(state: &AppState) -> Result<Option<String>> {
    let enabled = state.db.get_setting("ai_autosuggest")?.map(|v| v.trim_matches('"') == "true").unwrap_or(false);
    if !enabled {
        return Ok(None);
    }
    let provider_id = state.db.get_setting("ai_default")?.unwrap_or_default().trim_matches('"').to_string();
    Ok(if provider_id.is_empty() { None } else { Some(provider_id) })
}

/// New, unsorted inbox mail that hasn't been looked at yet.
fn suggest_candidates(state: &AppState, limit: i64) -> Result<Vec<i64>> {
    let allowed = allowed_accounts(state)?;
    if allowed.is_empty() {
        return Ok(vec![]);
    }
    let since = chrono::Utc::now().timestamp() - 3 * 86400;
    let acc = allowed.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(",");
    state.db.message_ids_where(
        &format!(
            "account_id IN ({acc}) AND date >= {since} AND folder_id IN (SELECT id FROM folders WHERE role='inbox')
             AND id NOT IN (SELECT message_id FROM message_groups) AND id NOT IN (SELECT message_id FROM ai_suggestions)"
        ),
        limit,
    )
}

const QUEUE_CAP: usize = 200;
const QUEUE_CAP_I: i64 = 200;

/// Mail waiting for the NPU to wake up before it gets group suggestions.
pub fn suggest_queue(state: &AppState) -> Vec<i64> {
    state.db.get_setting("ai_suggest_queue").ok().flatten().and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default()
}

fn set_suggest_queue(state: &AppState, q: &[i64]) -> Result<()> {
    state.db.set_setting("ai_suggest_queue", &serde_json::to_string(q)?)
}

fn enqueue_suggestions(state: &AppState, ids: &[i64]) -> Result<usize> {
    let mut q = suggest_queue(state);
    for id in ids {
        if !q.contains(id) {
            q.push(*id);
        }
    }
    // Keep the newest mail when the queue overflows.
    if q.len() > QUEUE_CAP {
        q.drain(..q.len() - QUEUE_CAP);
    }
    set_suggest_queue(state, &q)?;
    state.emit("ai-suggest-queue", json!({ "count": q.len() }));
    Ok(q.len())
}

static SUGGESTING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Ask the default model where new, unsorted inbox mail should go. Results are stored as
/// pending suggestions the user can accept or dismiss.
///
/// With the on-device model, the NPU is never woken up for this: while it sleeps (or Hermio is in
/// the tray, or the PC runs on battery) new mail is put in a queue that is processed — with a
/// popup — as soon as the NPU starts again.
pub async fn suggest_groups(state: &AppState) -> Result<usize> {
    let Some(provider_id) = suggest_enabled(state)? else { return Ok(0) };
    if provider_id == "npu" {
        use tauri::Manager;
        let visible = state.app.get_webview_window("main").and_then(|w| w.is_visible().ok()).unwrap_or(false);
        if !visible || crate::npu::on_battery() || !crate::npu::is_running() || crate::npu::busy() {
            let ids = suggest_candidates(state, QUEUE_CAP_I)?;
            let queued = suggest_queue(state);
            let fresh: Vec<i64> = ids.into_iter().filter(|i| !queued.contains(i)).collect();
            if !fresh.is_empty() {
                enqueue_suggestions(state, &fresh)?;
            }
            return Ok(0);
        }
    }
    let _g = SUGGESTING.lock().await;
    let ids = suggest_candidates(state, 25)?;
    if ids.is_empty() {
        return Ok(0);
    }
    let _status = if provider_id == "npu" {
        crate::llm::clear_stop(&Some(crate::llm::BG.to_string()));
        Some(BgStatus::new(state, ids.len()))
    } else {
        None
    };
    let n = match classify(state, &provider_id, &ids).await {
        Ok(n) => n,
        Err(e) if e.to_string() == "stopped" => {
            // Interrupted by the user: try again later.
            enqueue_suggestions(state, &ids)?;
            return Ok(0);
        }
        Err(e) => return Err(e),
    };
    if n > 0 {
        state.emit("ai-suggestions", json!({ "count": n }));
    }
    Ok(n)
}

/// Tells the UI that the NPU is busy with background sorting (and how much is left), so it never
/// looks like the NPU works "for nothing"; the status clears when dropped.
struct BgStatus<'a> {
    state: &'a AppState,
}
impl<'a> BgStatus<'a> {
    fn new(state: &'a AppState, count: usize) -> Self {
        state.emit("npu-bg", json!({ "active": true, "task": "suggest", "count": count }));
        BgStatus { state }
    }
}
impl Drop for BgStatus<'_> {
    fn drop(&mut self) {
        self.state.emit("npu-bg", json!({ "active": false }));
    }
}

/// Called when the NPU server has started: classify the queued mail and show a popup.
pub async fn process_suggest_queue(state: &AppState) -> Result<usize> {
    let Some(provider_id) = suggest_enabled(state)? else { return Ok(0) };
    if provider_id != "npu" {
        return Ok(0);
    }
    let _g = SUGGESTING.lock().await;
    let bg = Some(crate::llm::BG.to_string());
    crate::llm::clear_stop(&bg);
    let _status = BgStatus::new(state, suggest_queue(state).len());
    let mut total = 0;
    loop {
        let q = suggest_queue(state);
        if q.is_empty() {
            break;
        }
        // The user started something, or pressed Stop: the rest waits.
        if crate::llm::stop_requested(&bg) || crate::npu::busy() {
            break;
        }
        // Still unsorted and not looked at? (the user may have moved or deleted some meanwhile)
        let still: Vec<i64> = {
            let list = q.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(",");
            state.db.message_ids_where(
                &format!("id IN ({list}) AND id NOT IN (SELECT message_id FROM message_groups) AND id NOT IN (SELECT message_id FROM ai_suggestions)"),
                QUEUE_CAP_I,
            )?
        };
        let batch: Vec<i64> = still.iter().copied().take(25).collect();
        if batch.is_empty() {
            set_suggest_queue(state, &[])?;
            break;
        }
        match classify(state, &provider_id, &batch).await {
            Ok(n) => total += n,
            Err(e) => {
                // Leave the rest queued for the next start.
                set_suggest_queue(state, &still)?;
                if total > 0 {
                    state.emit("ai-suggestions", json!({ "count": total, "popup": true }));
                }
                return Err(e);
            }
        }
        let rest: Vec<i64> = still.into_iter().filter(|i| !batch.contains(i)).collect();
        set_suggest_queue(state, &rest)?;
        // Stop when the NPU went back to sleep; the rest waits for the next start.
        if !crate::npu::is_running() {
            break;
        }
    }
    state.emit("ai-suggest-queue", json!({ "count": suggest_queue(state).len() }));
    if total > 0 {
        state.emit("ai-suggestions", json!({ "count": total, "popup": true }));
    }
    Ok(total)
}

/// "Suggest now" from the UI: queued mail first, then anything new. Wakes the NPU if needed.
pub async fn suggest_now(state: &AppState) -> Result<usize> {
    let Some(provider_id) = suggest_enabled(state)? else { return Ok(0) };
    let _g = SUGGESTING.lock().await;
    let q = suggest_queue(state);
    let mut ids: Vec<i64> = if q.is_empty() {
        vec![]
    } else {
        let list = q.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(",");
        state.db.message_ids_where(
            &format!("id IN ({list}) AND id NOT IN (SELECT message_id FROM message_groups) AND id NOT IN (SELECT message_id FROM ai_suggestions)"),
            QUEUE_CAP_I,
        )?
    };
    for id in suggest_candidates(state, 25)? {
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids.truncate(25);
    let n = classify(state, &provider_id, &ids).await?;
    let rest: Vec<i64> = suggest_queue(state).into_iter().filter(|i| !ids.contains(i)).collect();
    set_suggest_queue(state, &rest)?;
    state.emit("ai-suggest-queue", json!({ "count": rest.len() }));
    if n > 0 {
        state.emit("ai-suggestions", json!({ "count": n }));
    }
    Ok(n)
}

/// The group most of this sender's earlier mail is in (≥ 2 messages), respecting mailbox scope.
fn sender_group(state: &AppState, key: &str, is_domain: bool, account_id: i64, groups: &[Group]) -> Option<(i64, i64)> {
    let cond = if is_domain {
        "(lower(m.from_addr) LIKE '%@' || ?1 OR lower(m.from_addr) LIKE '%.' || ?1)"
    } else {
        "lower(m.from_addr) = ?1"
    };
    let rows: Vec<(i64, i64)> = state
        .db
        .with(|c| {
            let mut st = c.prepare(&format!(
                "SELECT mg.group_id, COUNT(*) FROM message_groups mg JOIN messages m ON m.id=mg.message_id WHERE {cond} GROUP BY mg.group_id ORDER BY 2 DESC"
            ))?;
            let r = st.query_map([key], |r| Ok((r.get(0)?, r.get(1)?)))?;
            r.collect()
        })
        .unwrap_or_default();
    rows.into_iter()
        .filter(|(g, n)| *n >= 2 && groups.iter().any(|x| x.id == *g && x.account_id.map(|a| a == account_id).unwrap_or(true)))
        .next()
}

struct Item {
    id: i64,
    group_id: Option<i64>,
    new_group: String,
    reason: String,
}

fn line_re() -> regex_lite::Regex {
    regex_lite::Regex::new(r"(?i)^\W*e?\s*(\d+)[\s*]*[|:.)\]\-]+\s*(g\s*\d+|new\b[:\s]*[^|]*|keep)\s*(?:\|\s*(.*))?$").unwrap()
}

/// Small models: one short line per email instead of JSON, a few emails per request.
async fn classify_lines(state: &AppState, provider_id: &str, ids: &[i64], groups: &[Group]) -> Result<Vec<Item>> {
    let mut out = vec![];
    let re = line_re();
    let mut glist = String::new();
    for (i, g) in groups.iter().enumerate() {
        let scope = match g.account_id {
            Some(a) => format!("only for {}", state.db.account(a).map(|x| x.email).unwrap_or_default()),
            None => "all mailboxes".into(),
        };
        glist.push_str(&format!("G{}: {} ({scope})\n", i + 1, g.name));
    }
    if glist.is_empty() {
        glist.push_str("(no groups yet)\n");
    }
    let mut any_ok = false;
    for batch in ids.chunks(8) {
        let mut elist = String::new();
        for (i, id) in batch.iter().enumerate() {
            let Ok(m) = state.db.message(*id) else { continue };
            let mailbox = state.db.account(m.account_id).map(|a| a.email).unwrap_or_default();
            let body = state.db.indexed_body(m.id).unwrap_or_default();
            let text = if body.trim().is_empty() { m.snippet.clone() } else { body };
            elist.push_str(&format!(
                "E{}: From {} <{}> · mailbox {mailbox}\n    Subject: {}\n    Text: {}\n",
                i + 1,
                m.from_name,
                m.from_addr,
                trunc(&m.subject, 120),
                trunc(&text, 300)
            ));
        }
        let system = "You sort emails into groups for a mail app. For each email choose ONE: an existing group (Gk), NEW with a short group name \
             when it is a kind of automated mail worth a group (newsletters, receipts, notifications, a project), or KEEP for personal or important mail \
             that should stay in the inbox. Respect each group's mailbox scope.\n\
             Answer with one line per email, nothing else:\nE1 | G2 | reason (max 10 words)\nE2 | NEW Shopee orders | reason\nE3 | KEEP | reason";
        let user = format!("Groups:\n{glist}\nEmails:\n{elist}");
        if crate::llm::stop_requested(&Some(crate::llm::BG.to_string())) {
            break;
        }
        let text = match complete_sub(state, provider_id, None, system, &[ChatMsg { role: "user".into(), content: user, images: vec![] }], sub_id(&None, "suggest")).await {
            Ok(t) => t,
            Err(e) => {
                if any_ok {
                    break;
                }
                return Err(e);
            }
        };
        for line in text.lines() {
            let Some(c) = re.captures(line.trim()) else { continue };
            let n: usize = c[1].parse().unwrap_or(0);
            let Some(id) = n.checked_sub(1).and_then(|k| batch.get(k)) else { continue };
            any_ok = true;
            let choice = c[2].trim().to_string();
            let reason = c.get(3).map(|x| x.as_str().trim().to_string()).unwrap_or_default();
            let lc = choice.to_lowercase();
            let (group_id, new_group) = if lc.starts_with('g') {
                let k: usize = lc.trim_start_matches('g').trim().parse().unwrap_or(0);
                (k.checked_sub(1).and_then(|k| groups.get(k)).map(|g| g.id), String::new())
            } else if lc.starts_with("new") {
                (None, choice[3..].trim_start_matches([':', ' ']).trim().chars().take(40).collect())
            } else {
                (None, String::new())
            };
            if !out.iter().any(|x: &Item| x.id == *id) {
                out.push(Item { id: *id, group_id, new_group, reason: reason.chars().take(120).collect() });
            }
        }
    }
    if crate::llm::stop_requested(&Some(crate::llm::BG.to_string())) && out.is_empty() {
        bail!("stopped");
    }
    if !any_ok {
        bail!("The model returned no valid suggestions");
    }
    Ok(out)
}

/// Lenient `{"items":[…]}` reader (ids may be strings, group ids may be "4" or "G4").
fn parse_items(text: &str) -> Option<Vec<Item>> {
    let t = text.replace("```json", "").replace("```", "");
    let (s, e) = (t.find('{')?, t.rfind('}')?);
    let v: Value = serde_json::from_str(&t[s..=e]).ok().or_else(|| {
        let re = regex_lite::Regex::new(r",\s*([\]}])").unwrap();
        serde_json::from_str(&re.replace_all(&t[s..=e], "$1")).ok()
    })?;
    let num = |x: &Value| -> Option<i64> {
        x.as_i64().or_else(|| x.as_str().and_then(|s| s.trim().trim_start_matches(['#', 'G', 'g']).parse().ok()))
    };
    let arr = v.get("items").or_else(|| v.get("suggestions")).and_then(|x| x.as_array())?;
    Some(
        arr.iter()
            .filter_map(|it| {
                Some(Item {
                    id: it.get("id").and_then(num)?,
                    group_id: it.get("group_id").and_then(num),
                    new_group: it.get("new_group").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                    reason: it.get("reason").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                })
            })
            .collect(),
    )
}

async fn classify(state: &AppState, provider_id: &str, ids: &[i64]) -> Result<usize> {
    if ids.is_empty() {
        return Ok(0);
    }
    let ids = ids.to_vec();
    ensure_bodies(state, &ids).await;
    let groups = state.db.groups()?;
    let kind = providers(state)?.into_iter().find(|p| p.id == provider_id).map(|p| p.kind).unwrap_or_default();
    let compact = kind == "npu" || kind == "local";

    // Exact and free: mail from a sender whose earlier mail is already in a group.
    let own: Vec<String> = state
        .db
        .accounts()?
        .iter()
        .filter_map(|a| a.email.rsplit_once('@').map(|(_, d)| base_domain(&d.to_lowercase())))
        .filter(|d| !FREEMAIL.contains(&d.as_str()))
        .collect();
    let mut items: Vec<Item> = vec![];
    let mut rest: Vec<i64> = vec![];
    for id in &ids {
        let Ok(m) = state.db.message(*id) else { continue };
        let hit = sender_key(&m.from_addr, &own).and_then(|(k, d)| sender_group(state, &k, d, m.account_id, &groups));
        match hit {
            Some((gid, n)) => {
                let name = groups.iter().find(|g| g.id == gid).map(|g| g.name.clone()).unwrap_or_default();
                items.push(Item { id: *id, group_id: Some(gid), new_group: String::new(), reason: format!("{n} earlier emails from this sender are in “{name}”") });
            }
            None => rest.push(*id),
        }
    }

    if !rest.is_empty() {
        if compact {
            items.extend(classify_lines(state, provider_id, &rest, &groups).await?);
        } else {
            let mut ctx = String::from("## Existing groups\n");
            for g in &groups {
                let scope = match g.account_id {
                    Some(a) => format!("only for mailbox {}", state.db.account(a).map(|x| x.email).unwrap_or_default()),
                    None => "all mailboxes".into(),
                };
                ctx.push_str(&format!("- id {}: {} ({scope})\n", g.id, g.name));
            }
            ctx.push_str("\n## New emails\n");
            for id in &rest {
                if let Ok(m) = state.db.message(*id) {
                    ctx.push_str(&describe(state, &m, 600, false));
                    ctx.push('\n');
                }
            }
            let system = format!(
                "{SYSTEM}\n\nTask: for each new email decide whether it belongs in one of the existing groups (respect mailbox scope), \
                 or would fit a NEW group worth creating (newsletters, receipts, notifications, a project…), or should stay in the inbox (personal or important mail). \
                 Read the body, not just the subject. Return ONLY JSON: {{\"items\":[{{\"id\":123,\"group_id\":4,\"new_group\":\"\",\"reason\":\"…\"}}]}}. \
                 Use group_id null and new_group \"\" for mail that should stay in the inbox (you may omit those). Keep reasons under 12 words.\n\n{ctx}"
            );
            let msgs = vec![ChatMsg { role: "user".into(), content: "Classify the new emails.".into(), images: vec![] }];
            let text = complete_sub(state, provider_id, None, &system, &msgs, sub_id(&None, "suggest")).await?;
            items.extend(parse_items(&text).ok_or_else(|| anyhow!("The model returned no valid suggestions"))?);
        }
    }
    struct Out2 {
        items: Vec<Item>,
    }
    let out = Out2 { items };
    let now = chrono::Utc::now().timestamp();
    let mut n = 0;
    for it in out.items {
        if !ids.contains(&it.id) {
            continue;
        }
        let gid = it.group_id.filter(|g| groups.iter().any(|x| x.id == *g));
        let status = if gid.is_none() && it.new_group.trim().is_empty() { "keep" } else { "pending" };
        state.db.with(|c| {
            c.execute(
                "INSERT OR REPLACE INTO ai_suggestions(message_id,group_id,new_group,reason,status,created) VALUES(?1,?2,?3,?4,?5,?6)",
                rusqlite::params![it.id, gid, it.new_group.trim(), it.reason, status, now],
            )
        })?;
        if status == "pending" {
            n += 1;
        }
    }
    // Messages the model skipped are "keep" so they aren't asked about again.
    for id in &ids {
        state.db.with(|c| {
            c.execute(
                "INSERT OR IGNORE INTO ai_suggestions(message_id,group_id,new_group,reason,status,created) VALUES(?1,NULL,'','','keep',?2)",
                rusqlite::params![id, now],
            )
        })?;
    }
    Ok(n)
}

pub fn pending_suggestions(state: &AppState) -> Result<Vec<Suggestion>> {
    state.db.with(|c| {
        let mut st = c.prepare(
            "SELECT s.message_id, s.group_id, s.new_group, s.reason FROM ai_suggestions s
             JOIN messages m ON m.id=s.message_id
             WHERE s.status='pending' AND s.message_id NOT IN (SELECT message_id FROM message_groups)
             ORDER BY m.date DESC",
        )?;
        let r = st.query_map([], |r| Ok(Suggestion { message_id: r.get(0)?, group_id: r.get(1)?, new_group: r.get(2)?, reason: r.get(3)? }))?;
        r.collect()
    })
}

pub fn resolve_suggestion(state: &AppState, message_id: i64, accept: bool) -> Result<()> {
    if accept {
        let (gid, new_group): (Option<i64>, String) = state.db.with(|c| {
            c.query_row("SELECT group_id, new_group FROM ai_suggestions WHERE message_id=?1", [message_id], |r| Ok((r.get(0)?, r.get(1)?)))
        })?;
        let gid = match gid {
            Some(g) => g,
            None if !new_group.trim().is_empty() => {
                let existing = state.db.groups()?.into_iter().find(|g| g.name.eq_ignore_ascii_case(new_group.trim()));
                match existing {
                    Some(g) => g.id,
                    None => state.db.save_group(&Group { name: new_group.trim().into(), color: "#8764b8".into(), hide_from_inbox: true, ..Default::default() })?,
                }
            }
            None => bail!("Nothing to apply"),
        };
        state.db.add_to_group(&[message_id], gid)?;
    }
    state.db.with(|c| {
        c.execute(
            "UPDATE ai_suggestions SET status=?2 WHERE message_id=?1",
            rusqlite::params![message_id, if accept { "accepted" } else { "dismissed" }],
        )
    })?;
    Ok(())
}

// ---------------- saved chats ----------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Chat {
    pub id: i64,
    pub title: String,
    pub entries: Value,
    pub updated: i64,
}

pub fn chats(state: &AppState) -> Result<Vec<Chat>> {
    state.db.with(|c| {
        let mut st = c.prepare("SELECT id,title,updated FROM ai_chats ORDER BY updated DESC LIMIT 200")?;
        let r = st.query_map([], |r| Ok(Chat { id: r.get(0)?, title: r.get(1)?, entries: Value::Null, updated: r.get(2)? }))?;
        r.collect()
    })
}

pub fn chat(state: &AppState, id: i64) -> Result<Chat> {
    state.db.with(|c| {
        c.query_row("SELECT id,title,entries,updated FROM ai_chats WHERE id=?1", [id], |r| {
            let e: String = r.get(2)?;
            Ok(Chat { id: r.get(0)?, title: r.get(1)?, entries: serde_json::from_str(&e).unwrap_or(Value::Array(vec![])), updated: r.get(3)? })
        })
    })
}

pub fn save_chat(state: &AppState, ch: &Chat) -> Result<i64> {
    let now = chrono::Utc::now().timestamp();
    let entries = serde_json::to_string(&ch.entries)?;
    state.db.with(|c| {
        if ch.id > 0 {
            c.execute("UPDATE ai_chats SET title=?2, entries=?3, updated=?4 WHERE id=?1", rusqlite::params![ch.id, ch.title, entries, now])?;
            Ok(ch.id)
        } else {
            c.execute("INSERT INTO ai_chats(title,entries,created,updated) VALUES(?1,?2,?3,?3)", rusqlite::params![ch.title, entries, now])?;
            Ok(c.last_insert_rowid())
        }
    })
}

pub fn delete_chat(state: &AppState, id: i64) -> Result<()> {
    state.db.with(|c| c.execute("DELETE FROM ai_chats WHERE id=?1", [id]))?;
    Ok(())
}

/// Read a file the user wants to give the assistant: text when possible, else an image.
pub fn read_file(path: &str) -> Result<AiFile> {
    let p = std::path::Path::new(path);
    let name = p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "file".into());
    let meta = std::fs::metadata(p)?;
    if meta.len() > 40 * 1024 * 1024 {
        bail!("{name} is larger than 40 MB");
    }
    read_bytes(&name, std::fs::read(p)?)
}

/// Same as [`read_file`] for bytes the UI already has (drag & drop, paste).
pub fn read_bytes(name: &str, bytes: Vec<u8>) -> Result<AiFile> {
    use base64::Engine;
    if bytes.len() > 40 * 1024 * 1024 {
        bail!("{name} is larger than 40 MB");
    }
    let mime = mime_guess::from_path(name).first_or_octet_stream().to_string();
    let mut f = AiFile { name: name.to_string(), mime: mime.clone(), size: bytes.len(), ..Default::default() };
    let is_pdf = name.to_lowercase().ends_with(".pdf") || mime == "application/pdf";
    match crate::docs::extract_text(name, &mime, &bytes) {
        // A PDF with almost no text is a scan: hand it back for page rendering + OCR.
        Some(t) if is_pdf && t.split_whitespace().count() < 25 && bytes.len() <= 30 * 1024 * 1024 => {
            f.pdf = Some(base64::engine::general_purpose::STANDARD.encode(&bytes));
        }
        Some(t) => f.text = Some(t),
        None if is_pdf && bytes.len() <= 30 * 1024 * 1024 => f.pdf = Some(base64::engine::general_purpose::STANDARD.encode(&bytes)),
        None if crate::docs::is_image(name, &mime) && bytes.len() <= 8 * 1024 * 1024 => {
            f.image = Some(base64::engine::general_purpose::STANDARD.encode(&bytes));
        }
        None => {}
    }
    Ok(f)
}

/// Install a skill from a folder: SKILL.md plus the Markdown/text references next to it.
/// Scripts are listed by name only — they are never run.
pub fn install_skill_dir(state: &AppState, dir: &str) -> Result<Skill> {
    let root = std::path::Path::new(dir);
    if !root.is_dir() {
        bail!("{dir} is not a folder");
    }
    let mut files: Vec<std::path::PathBuf> = vec![];
    fn walk(d: &std::path::Path, depth: usize, out: &mut Vec<std::path::PathBuf>) {
        if depth > 4 || out.len() > 400 {
            return;
        }
        if let Ok(rd) = std::fs::read_dir(d) {
            let mut v: Vec<_> = rd.flatten().map(|e| e.path()).collect();
            v.sort();
            for p in v {
                let n = p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                if n.starts_with('.') || n == "node_modules" || n == "__pycache__" {
                    continue;
                }
                if p.is_dir() {
                    walk(&p, depth + 1, out);
                } else {
                    out.push(p);
                }
            }
        }
    }
    walk(root, 0, &mut files);
    let rel = |p: &std::path::Path| p.strip_prefix(root).map(|x| x.to_string_lossy().replace('\\', "/")).unwrap_or_default();
    let main = files
        .iter()
        .find(|p| p.file_name().map(|n| n.to_string_lossy().eq_ignore_ascii_case("SKILL.md")).unwrap_or(false))
        .cloned();
    let mut content = String::new();
    if let Some(m) = &main {
        content.push_str(&std::fs::read_to_string(m).unwrap_or_default());
    }
    let mut budget: usize = 120_000;
    let mut scripts = vec![];
    for p in &files {
        if Some(p) == main.as_ref() {
            continue;
        }
        let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        match ext.as_str() {
            "md" | "txt" | "markdown" | "csv" | "json" | "yaml" | "yml" => {
                let t = std::fs::read_to_string(p).unwrap_or_default();
                if t.is_empty() || budget == 0 {
                    continue;
                }
                let t: String = t.chars().take(budget.min(30_000)).collect();
                budget = budget.saturating_sub(t.len());
                content.push_str(&format!("\n\n---\n### Reference file: {}\n\n{}", rel(p), t));
            }
            "py" | "js" | "ts" | "sh" | "ps1" | "bat" | "cmd" | "rb" | "exe" => scripts.push(rel(p)),
            _ => {}
        }
    }
    if !scripts.is_empty() {
        content.push_str(&format!(
            "\n\n---\nThis skill also ships scripts that Hermio does not run: {}. Describe what they would do instead of claiming to run them.",
            scripts.join(", ")
        ));
    }
    if content.trim().is_empty() {
        bail!("No SKILL.md or Markdown files found in this folder");
    }
    let folder = root.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let mut sk = Skill { id: String::new(), name: String::new(), description: String::new(), content, enabled: true };
    // Keep the front matter parse in save_skill; fall back to the folder name.
    let saved = save_skill(state, sk.clone())?;
    if saved.name == "Skill" {
        sk = saved.clone();
        sk.name = folder;
        return save_skill(state, sk);
    }
    Ok(saved)
}

#[cfg(test)]
mod topic_tests {
    #[test]
    fn topics_and_words() {
        let known = vec!["Thesis".to_string()];
        assert_eq!(super::norm_topic("promotion", &known), "Promotions");
        assert_eq!(super::norm_topic("**thesis**", &known), "Thesis");
        assert_eq!(super::norm_topic("receipts and finance", &known), "Receipts and finance");
        assert_eq!(super::norm_topic("fit5037 assignments", &known), "Fit5037 assignments");
        assert_eq!(super::subject_words("Re: 50% OFF Flash Sale – Today only!"), vec!["flash".to_string(), "sale".to_string()]);
    }
}

#[cfg(test)]
mod cite_tests {
    #[test]
    fn citations() {
        let ids = vec![501, 502, 503];
        assert_eq!(super::fix_citations("A [E1] B [e3].", &ids), "A [E1] B [E3].");
        assert_eq!(super::fix_citations("Real id [#502], index [#2], bad [E9].", &ids), "Real id [E2], index [E2], bad.");
        assert_eq!(super::fix_citations("Both [E1, E3] and web [2]", &ids), "Both [E1][E3] and web [2]");
        assert_eq!(super::fix_citations("Invoice [#41]: due", &ids), "Invoice: due");
    }
}

#[cfg(test)]
mod cluster_tests {
    #[test]
    fn domains_and_json() {
        assert_eq!(super::base_domain("notifications.github.com"), "github.com");
        assert_eq!(super::base_domain("mail.monash.edu.au"), "monash.edu.au");
        assert_eq!(super::base_domain("shopee.vn"), "shopee.vn");
        assert_eq!(super::base_domain("news.vnexpress.net"), "vnexpress.net");
        let own = vec!["monash.edu".to_string()];
        assert_eq!(super::sender_key("Lan@student.monash.edu", &own), Some(("lan@student.monash.edu".into(), false)));
        assert_eq!(super::sender_key("a@gmail.com", &own), Some(("a@gmail.com".into(), false)));
        assert_eq!(super::sender_key("noreply@github.com", &own), Some(("github.com".into(), true)));
        let t = "Sure! ```json\n{\"clusters\":[{\"title\":\"GitHub\",\"ids\":[\"#12\", 15, \"19\"],},]}\n```";
        let c = super::parse_clusters(t).unwrap();
        assert_eq!(c[0].ids, vec![12, 15, 19]);
        let re = super::line_re();
        let c = re.captures("E1 | G2 | CI notification").unwrap();
        assert_eq!((&c[1], &c[2], &c[3]), ("1", "G2", "CI notification"));
        let c = re.captures("**E2** | NEW Shopee orders | order receipt").unwrap();
        assert_eq!((&c[1], c[2].trim()), ("2", "NEW Shopee orders"));
        let c = re.captures("3. KEEP | from a friend").unwrap();
        assert_eq!(&c[2], "KEEP");
    }
}

#[cfg(test)]
mod flash_tests {
    #[test]
    fn picks_flash() {
        let v = |x: &[&str]| x.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(super::best_flash(&v(&["gemini-2.0-flash", "gemini-2.5-flash", "gemini-2.5-flash-lite", "gemini-2.5-pro", "gemini-2.5-flash-image"])).unwrap(), "gemini-2.5-flash");
        assert_eq!(super::best_flash(&v(&["gemini-2.5-flash", "gemini-3-flash-preview"])).unwrap(), "gemini-3-flash-preview");
        assert_eq!(super::best_flash(&v(&["gemini-3-flash", "gemini-3.5-flash-preview", "gemini-3-flash-preview"])).unwrap(), "gemini-3-flash");
    }
}
