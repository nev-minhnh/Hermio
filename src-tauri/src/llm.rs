//! Talking to the models: one streaming implementation per provider family.
//!
//! Every request is streamed. When a `stream_id` is given, text and "thinking" deltas are sent to
//! the UI as `ai-delta` events (batched every ~40 ms) so answers appear while they are written and
//! the reasoning can be watched live. Requests can be cancelled with [`cancel`].

use crate::ai::{ChatMsg, Provider};
use crate::state::AppState;
use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct Source {
    pub title: String,
    pub url: String,
}

#[derive(Debug, Clone, Default)]
pub struct Opts {
    pub stream_id: Option<String>,
    /// Let the provider search the web itself (Gemini, Anthropic, Claude Code).
    pub web_native: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Output {
    pub text: String,
    pub thinking: String,
    pub sources: Vec<Source>,
    pub cancelled: bool,
}

static CANCELLED: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

pub fn cancel(id: &str) {
    CANCELLED.lock().unwrap().insert(id.to_string());
}
/// Ids are hierarchical: stopping "s1" also stops its helper requests "s1:map:3", and stopping
/// "bg" stops all background work ("bg:suggest:…").
fn is_cancelled(id: &Option<String>) -> bool {
    let Some(i) = id else { return false };
    let set = CANCELLED.lock().unwrap();
    let mut end = 0;
    loop {
        let next = i[end..].find(':').map(|k| end + k);
        let prefix = &i[..next.unwrap_or(i.len())];
        if set.contains(prefix) {
            return true;
        }
        match next {
            Some(k) => end = k + 1,
            None => return false,
        }
    }
}

/// Resolves as soon as the request is stopped (never, without an id).
async fn stopped(id: &Option<String>) {
    if id.is_none() {
        return std::future::pending().await;
    }
    loop {
        if is_cancelled(id) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(80)).await;
    }
}

/// Background NPU work (group suggestions) uses ids under this prefix and yields to the user.
pub const BG: &str = "bg";
/// Whether the user pressed Stop for this stream (for work done before the final model call).
pub fn stop_requested(id: &Option<String>) -> bool {
    is_cancelled(id)
}
pub fn clear_stop(id: &Option<String>) {
    clear_cancel(id)
}
fn clear_cancel(id: &Option<String>) {
    if let Some(i) = id {
        CANCELLED.lock().unwrap().remove(i);
    }
}

/// Collects deltas and forwards them to the UI in small batches.
pub struct Sink<'a> {
    state: &'a AppState,
    id: Option<String>,
    out: Output,
    buf_text: String,
    buf_think: String,
    last: Instant,
}

impl<'a> Sink<'a> {
    pub fn new(state: &'a AppState, id: Option<String>) -> Self {
        Sink { state, id, out: Output::default(), buf_text: String::new(), buf_think: String::new(), last: Instant::now() }
    }
    pub fn text(&mut self, s: &str) {
        if s.is_empty() {
            return;
        }
        self.out.text.push_str(s);
        self.buf_text.push_str(s);
        self.maybe_flush();
    }
    pub fn think(&mut self, s: &str) {
        if s.is_empty() {
            return;
        }
        self.out.thinking.push_str(s);
        self.buf_think.push_str(s);
        self.maybe_flush();
    }
    /// A short status line ("Searching the web…") shown with the thinking.
    pub fn status(&mut self, s: &str) {
        self.flush();
        if let Some(id) = self.id.as_ref().filter(|i| !i.contains(':')) {
            self.state.emit("ai-delta", json!({ "id": id, "kind": "status", "text": s }));
        }
    }
    pub fn source(&mut self, title: &str, url: &str) {
        if url.is_empty() || self.out.sources.iter().any(|s| s.url == url) {
            return;
        }
        self.out.sources.push(Source { title: title.to_string(), url: url.to_string() });
        if let Some(id) = self.id.as_ref().filter(|i| !i.contains(':')) {
            self.state.emit("ai-delta", json!({ "id": id, "kind": "source", "title": title, "url": url }));
        }
    }
    fn maybe_flush(&mut self) {
        if self.last.elapsed() >= Duration::from_millis(40) {
            self.flush();
        }
    }
    pub fn flush(&mut self) {
        self.last = Instant::now();
        // Helper and background requests ("s1:map:2", "bg:…") aren't shown in the chat.
        let Some(id) = self.id.as_ref().filter(|i| !i.contains(':')) else {
            self.buf_text.clear();
            self.buf_think.clear();
            return;
        };
        if !self.buf_think.is_empty() {
            self.state.emit("ai-delta", json!({ "id": id, "kind": "thinking", "text": std::mem::take(&mut self.buf_think) }));
        }
        if !self.buf_text.is_empty() {
            self.state.emit("ai-delta", json!({ "id": id, "kind": "text", "text": std::mem::take(&mut self.buf_text) }));
        }
    }
    pub fn cancelled(&self) -> bool {
        is_cancelled(&self.id)
    }
    pub fn finish(mut self) -> Output {
        self.flush();
        self.out.cancelled = is_cancelled(&self.id);
        clear_cancel(&self.id);
        self.out
    }
}

/// Splits `<think>…</think>` (and `<thinking>`) blocks out of a content stream, even when the tags
/// are cut across chunks.
#[derive(Default)]
pub struct ThinkSplit {
    inside: bool,
    pending: String,
}

impl ThinkSplit {
    pub fn feed(&mut self, chunk: &str, sink: &mut Sink) {
        self.pending.push_str(chunk);
        loop {
            let (open, close) = ("<think>", "</think>");
            if !self.inside {
                if let Some(i) = self.pending.find(open).or_else(|| self.pending.find("<thinking>")) {
                    let tag_len = if self.pending[i..].starts_with(open) { open.len() } else { "<thinking>".len() };
                    sink.text(&self.pending[..i]);
                    self.pending.drain(..i + tag_len);
                    self.inside = true;
                    continue;
                }
            } else if let Some(i) = self.pending.find(close).or_else(|| self.pending.find("</thinking>")) {
                let tag_len = if self.pending[i..].starts_with(close) { close.len() } else { "</thinking>".len() };
                sink.think(&self.pending[..i]);
                self.pending.drain(..i + tag_len);
                self.inside = false;
                continue;
            }
            // Keep a possible partial tag at the end for the next chunk.
            let keep = partial_tag_suffix(&self.pending);
            let emit_to = self.pending.len() - keep;
            let part: String = self.pending.drain(..emit_to).collect();
            if self.inside {
                sink.think(&part);
            } else {
                sink.text(&part);
            }
            break;
        }
    }
    pub fn end(&mut self, sink: &mut Sink) {
        let rest = std::mem::take(&mut self.pending);
        if self.inside {
            sink.think(&rest);
        } else {
            sink.text(&rest);
        }
    }
}

fn partial_tag_suffix(s: &str) -> usize {
    let tags = ["<think>", "</think>", "<thinking>", "</thinking>"];
    let mut best = 0;
    for t in tags {
        for n in (1..t.len()).rev() {
            if n <= s.len() && s.is_char_boundary(s.len() - n) && t.starts_with(&s[s.len() - n..]) {
                best = best.max(n);
                break;
            }
        }
    }
    best
}

fn http() -> reqwest::Client {
    reqwest::Client::builder().connect_timeout(Duration::from_secs(20)).timeout(Duration::from_secs(600)).build().unwrap_or_default()
}

pub fn api_err(service: &str, status: reqwest::StatusCode, body: &str) -> anyhow::Error {
    let msg = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| {
            v.pointer("/error/message")
                .or_else(|| v.pointer("/error"))
                .and_then(|m| m.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| body.chars().take(300).collect());
    anyhow!("{service} returned {status}: {msg}")
}

/// Send a request; on HTTP 400 retry once with `plain` (without thinking options).
/// HTTP client for servers on this PC: no connection reuse, generous timeouts for slow first tokens.
fn local_http() -> reqwest::Client {
    reqwest::Client::builder()
        .pool_max_idle_per_host(0)
        .connect_timeout(Duration::from_secs(10))
        .read_timeout(Duration::from_secs(600))
        .build()
        .unwrap_or_default()
}

/// The connection failed (refused, reset, aborted) rather than the server answering with an error.
fn is_connection_error(e: &anyhow::Error) -> bool {
    e.chain().any(|c| {
        if let Some(r) = c.downcast_ref::<reqwest::Error>() {
            return r.is_connect() || r.is_request() || r.is_timeout();
        }
        let t = c.to_string().to_lowercase();
        t.contains("connection") && (t.contains("abort") || t.contains("reset") || t.contains("refused") || t.contains("closed"))
    })
}

async fn send(
    req: impl Fn(&Value) -> reqwest::RequestBuilder,
    body: Value,
    plain: Option<Value>,
    service: &str,
) -> Result<reqwest::Response> {
    let r = req(&body).send().await.with_context(|| format!("Could not reach {service}"))?;
    if r.status().is_success() {
        return Ok(r);
    }
    let st = r.status();
    let t = r.text().await.unwrap_or_default();
    if st == reqwest::StatusCode::BAD_REQUEST {
        if let Some(p) = plain {
            let r2 = req(&p).send().await?;
            if r2.status().is_success() {
                return Ok(r2);
            }
            let st2 = r2.status();
            let t2 = r2.text().await.unwrap_or_default();
            return Err(api_err(service, st2, &t2));
        }
    }
    Err(api_err(service, st, &t))
}

/// Feed every SSE `data:` payload to `on_data` until the stream ends or is cancelled.
async fn read_sse(mut resp: reqwest::Response, sink: &mut Sink<'_>, mut on_data: impl FnMut(&str, &mut Sink) -> Result<()>) -> Result<()> {
    let mut buf: Vec<u8> = vec![];
    loop {
        if sink.cancelled() {
            break;
        }
        let chunk = match tokio::time::timeout(Duration::from_secs(180), resp.chunk()).await {
            Err(_) => bail!("The model stopped responding (no data for 3 minutes)"),
            Ok(r) => r?,
        };
        let Some(chunk) = chunk else { break };
        buf.extend_from_slice(&chunk);
        while let Some(i) = buf.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = buf.drain(..=i).collect();
            let line = String::from_utf8_lossy(&line);
            let line = line.trim_end_matches(['\r', '\n']);
            if let Some(data) = line.strip_prefix("data:") {
                let data = data.trim();
                if data.is_empty() || data == "[DONE]" {
                    continue;
                }
                on_data(data, sink)?;
            }
        }
    }
    let rest = String::from_utf8_lossy(&buf).to_string();
    if let Some(data) = rest.trim().strip_prefix("data:") {
        let data = data.trim();
        if !data.is_empty() && data != "[DONE]" {
            on_data(data, sink)?;
        }
    }
    Ok(())
}

/// Stream one conversation to the provider.
pub async fn complete_ex(
    state: &AppState,
    p: &Provider,
    key: &str,
    thinking: bool,
    system: &str,
    msgs: &[ChatMsg],
    opts: &Opts,
) -> Result<Output> {
    let mut sink = Sink::new(state, opts.stream_id.clone());
    let background = opts.stream_id.as_deref().map(|i| i == BG || i.starts_with("bg:")).unwrap_or(false);
    if p.kind == "npu" && !background {
        // The user comes first: background sorting on the NPU stops for this request.
        cancel(BG);
    }
    let id = opts.stream_id.clone();
    // Stop works at any moment — also while the model loads or reads a long prompt, before the
    // first token: the request is dropped, which closes the connection.
    let res = tokio::select! {
        r = async {
            match p.kind.as_str() {
                "gemini" => gemini(p, key, thinking, system, msgs, opts, &mut sink).await,
                "anthropic" => anthropic(p, key, thinking, system, msgs, opts, &mut sink).await,
                "claude_cli" => claude_cli(state, p, thinking, system, msgs, opts, &mut sink).await,
                _ => openai(state, p, key, thinking, system, msgs, &mut sink).await,
            }
        } => r,
        _ = stopped(&id) => Ok(()),
    };
    // After the user's Stop make sure the NPU stops generating. Background work is only
    // dropped: an explicit cancel could hit the user's own request queued right behind it.
    if sink.cancelled() && !background && (p.kind == "npu" || p.kind == "local") {
        release_local(state, p, true).await;
    }
    if let Err(e) = res {
        // A cancel can surface as a dropped connection: report what we have.
        if !sink.cancelled() {
            clear_cancel(&opts.stream_id);
            return Err(e);
        }
    }
    Ok(sink.finish())
}

async fn gemini(p: &Provider, key: &str, thinking: bool, system: &str, msgs: &[ChatMsg], opts: &Opts, sink: &mut Sink<'_>) -> Result<()> {
    let contents: Vec<Value> = msgs
        .iter()
        .map(|m| {
            let mut parts = vec![json!({ "text": m.content })];
            for img in &m.images {
                parts.push(json!({ "inline_data": { "mime_type": img.mime, "data": img.data } }));
            }
            json!({ "role": if m.role == "assistant" { "model" } else { "user" }, "parts": parts })
        })
        .collect();
    let mut plain = json!({ "system_instruction": { "parts": [{ "text": system }] }, "contents": contents });
    if opts.web_native {
        plain["tools"] = json!([{ "google_search": {} }]);
    }
    let mut body = plain.clone();
    body["generationConfig"] = json!({ "thinkingConfig": { "thinkingBudget": if thinking { -1 } else { 0 }, "includeThoughts": thinking } });
    let url = format!("https://generativelanguage.googleapis.com/v1beta/models/{}:streamGenerateContent?alt=sse", p.model);
    let h = http();
    let resp = send(|b| h.post(&url).header("x-goog-api-key", key).json(b), body, Some(plain), "Gemini").await?;
    read_sse(resp, sink, |data, sink| {
        let v: Value = serde_json::from_str(data).unwrap_or(Value::Null);
        if let Some(e) = v.pointer("/error/message").and_then(|x| x.as_str()) {
            bail!("Gemini: {e}");
        }
        for part in v.pointer("/candidates/0/content/parts").and_then(|x| x.as_array()).cloned().unwrap_or_default() {
            let t = part["text"].as_str().unwrap_or("");
            if part["thought"].as_bool().unwrap_or(false) {
                sink.think(t);
            } else {
                sink.text(t);
            }
        }
        for c in v.pointer("/candidates/0/groundingMetadata/groundingChunks").and_then(|x| x.as_array()).cloned().unwrap_or_default() {
            sink.source(c.pointer("/web/title").and_then(|x| x.as_str()).unwrap_or(""), c.pointer("/web/uri").and_then(|x| x.as_str()).unwrap_or(""));
        }
        Ok(())
    })
    .await
}

async fn anthropic(p: &Provider, key: &str, thinking: bool, system: &str, msgs: &[ChatMsg], opts: &Opts, sink: &mut Sink<'_>) -> Result<()> {
    let messages: Vec<Value> = msgs
        .iter()
        .map(|m| {
            if m.images.is_empty() {
                json!({ "role": m.role, "content": m.content })
            } else {
                let mut parts: Vec<Value> = m
                    .images
                    .iter()
                    .map(|i| json!({ "type": "image", "source": { "type": "base64", "media_type": i.mime, "data": i.data } }))
                    .collect();
                parts.push(json!({ "type": "text", "text": m.content }));
                json!({ "role": m.role, "content": parts })
            }
        })
        .collect();
    let mut plain = json!({ "model": p.model, "max_tokens": 8192, "system": system, "messages": messages, "stream": true });
    if opts.web_native {
        plain["tools"] = json!([{ "type": "web_search_20250305", "name": "web_search", "max_uses": 5 }]);
    }
    let mut body = plain.clone();
    if thinking {
        body["max_tokens"] = json!(16000);
        body["thinking"] = json!({ "type": "enabled", "budget_tokens": 6000 });
    }
    let h = http();
    let resp = send(
        |b| h.post("https://api.anthropic.com/v1/messages").header("x-api-key", key).header("anthropic-version", "2023-06-01").json(b),
        body,
        if thinking { Some(plain) } else { None },
        "Anthropic",
    )
    .await?;
    let mut tool_json = String::new();
    read_sse(resp, sink, |data, sink| {
        let v: Value = serde_json::from_str(data).unwrap_or(Value::Null);
        match v["type"].as_str().unwrap_or("") {
            "content_block_delta" => match v.pointer("/delta/type").and_then(|x| x.as_str()).unwrap_or("") {
                "text_delta" => sink.text(v.pointer("/delta/text").and_then(|x| x.as_str()).unwrap_or("")),
                "thinking_delta" => sink.think(v.pointer("/delta/thinking").and_then(|x| x.as_str()).unwrap_or("")),
                "input_json_delta" => tool_json.push_str(v.pointer("/delta/partial_json").and_then(|x| x.as_str()).unwrap_or("")),
                _ => {}
            },
            "content_block_start" => {
                let b = &v["content_block"];
                match b["type"].as_str().unwrap_or("") {
                    "server_tool_use" => {
                        tool_json.clear();
                        sink.status("web");
                    }
                    "web_search_tool_result" => {
                        for r in b["content"].as_array().cloned().unwrap_or_default() {
                            sink.source(r["title"].as_str().unwrap_or(""), r["url"].as_str().unwrap_or(""));
                        }
                    }
                    _ => {}
                }
            }
            "content_block_stop" => {
                if !tool_json.is_empty() {
                    if let Ok(q) = serde_json::from_str::<Value>(&tool_json) {
                        if let Some(q) = q["query"].as_str() {
                            sink.status(&format!("web:{q}"));
                        }
                    }
                    tool_json.clear();
                }
            }
            "error" => bail!("Anthropic: {}", v.pointer("/error/message").and_then(|x| x.as_str()).unwrap_or("error")),
            _ => {}
        }
        Ok(())
    })
    .await
}

async fn openai(state: &AppState, p: &Provider, key: &str, thinking: bool, system: &str, msgs: &[ChatMsg], sink: &mut Sink<'_>) -> Result<()> {
    let mut msgs = msgs.to_vec();
    let mut base = p.base_url.clone();
    let _busy = if p.kind == "npu" { Some(crate::npu::use_guard()) } else { None };
    if p.kind == "npu" {
        sink.status("loading");
        crate::npu::ensure_server(state, &p.model).await?;
        base = crate::npu::base_url(state);
        // Qwen-family soft switch for the thinking step.
        let ml = p.model.to_lowercase();
        if ml.contains("qwen3") && !ml.contains("-it") && !ml.contains("instruct") {
            if let Some(last) = msgs.iter_mut().rev().find(|m| m.role == "user") {
                last.content.push_str(if thinking { " /think" } else { " /no_think" });
            }
        }
    }
    let mut messages = vec![json!({ "role": "system", "content": system })];
    for m in &msgs {
        if m.images.is_empty() {
            messages.push(json!({ "role": m.role, "content": m.content }));
        } else {
            let mut parts = vec![json!({ "type": "text", "text": m.content })];
            for i in &m.images {
                parts.push(json!({ "type": "image_url", "image_url": { "url": format!("data:{};base64,{}", i.mime, i.data) } }));
            }
            messages.push(json!({ "role": m.role, "content": parts }));
        }
    }
    let mut plain = json!({ "model": p.model, "messages": messages, "stream": true });
    if p.kind == "npu" || p.kind == "local" {
        plain["temperature"] = json!(0.3);
    }
    let mut body = plain.clone();
    match p.kind.as_str() {
        "local" | "npu" => body["chat_template_kwargs"] = json!({ "enable_thinking": thinking }),
        _ => body["reasoning_effort"] = json!(if thinking { "high" } else { "low" }),
    }
    let url = format!("{}/chat/completions", base.trim_end_matches('/'));
    let local = p.kind == "npu" || p.kind == "local";
    // Local servers restart (idle sleep, Stop, crashes): never reuse a kept-alive connection to
    // them — a dead pooled socket is what gives "connection aborted (os error 10053)".
    let h = if local { local_http() } else { http() };
    let mut attempt = 0;
    let resp = loop {
        attempt += 1;
        let r = send(
            |b| {
                let r = h.post(&url).json(b);
                if key.is_empty() { r } else { r.bearer_auth(key) }
            },
            body.clone(),
            Some(plain.clone()),
            &p.name,
        )
        .await;
        match r {
            Ok(r) => break r,
            Err(e) if local && attempt == 1 && is_connection_error(&e) => {
                // The runtime went away (or was restarting): bring it back and try once more.
                if p.kind == "npu" {
                    sink.status("loading");
                    if !crate::npu::alive_now(state).await {
                        crate::npu::forget_server();
                    }
                    crate::npu::ensure_server(state, &p.model).await?;
                } else {
                    tokio::time::sleep(Duration::from_millis(600)).await;
                }
            }
            Err(e) => {
                let hint = if p.kind == "npu" { crate::npu::server_log_tail(state) } else { String::new() };
                return Err(e.context(format!("{} ({}){hint}", p.name, base)));
            }
        }
    };
    let mut split = ThinkSplit::default();
    read_sse(resp, sink, |data, sink| {
        let v: Value = serde_json::from_str(data).unwrap_or(Value::Null);
        if let Some(e) = v.pointer("/error/message").and_then(|x| x.as_str()) {
            bail!("{e}");
        }
        let d = v.pointer("/choices/0/delta").cloned().unwrap_or(Value::Null);
        for k in ["reasoning_content", "reasoning"] {
            if let Some(t) = d[k].as_str() {
                sink.think(t);
            }
        }
        if let Some(t) = d["content"].as_str() {
            split.feed(t, sink);
        }
        // Some servers ignore `stream` and send one full message.
        if let Some(t) = v.pointer("/choices/0/message/content").and_then(|x| x.as_str()) {
            split.feed(t, sink);
        }
        Ok(())
    })
    .await?;
    split.end(sink);
    Ok(())
}

/// After Stop on a local model: make sure the hardware stops generating. Closing the connection
/// is enough for llama.cpp and OpenVINO Model Server; FastFlowLM gets an explicit cancel, and if
/// it doesn't confirm, its server is stopped (the model is loaded again on the next question).
async fn release_local(state: &AppState, p: &Provider, may_restart: bool) {
    if p.kind != "npu" || crate::npu::vendor() == "intel" {
        return;
    }
    let url = format!("http://127.0.0.1:{}/api/cancel", crate::npu::port_of(state));
    let ok = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap_or_default()
        .post(&url)
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false);
    if !ok && may_restart && crate::npu::is_running() {
        crate::npu::stop_server(state).await;
        state.emit("npu-state", json!({ "running": false, "reason": "cancelled" }));
    }
}

fn claude_exe(p: &Provider) -> String {
    if p.base_url.trim().is_empty() { "claude".to_string() } else { p.base_url.trim().to_string() }
}

fn claude_command(exe: &str, args: &[String]) -> tokio::process::Command {
    #[cfg(windows)]
    {
        // `claude` may be a .cmd shim (npm) or an .exe (native installer); cmd /C finds both.
        let mut c = tokio::process::Command::new("cmd");
        c.arg("/C").arg(exe).args(args);
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        c.kill_on_drop(true); // Stop ends the CLI too
        c
    }
    #[cfg(not(windows))]
    {
        let mut c = tokio::process::Command::new(exe);
        c.args(args);
        c.kill_on_drop(true);
        c
    }
}

/// Run the locally installed, signed-in Claude Code CLI in print mode with streamed JSON output.
async fn claude_cli(state: &AppState, p: &Provider, thinking: bool, system: &str, msgs: &[ChatMsg], opts: &Opts, sink: &mut Sink<'_>) -> Result<()> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
    let mut prompt = format!("{system}\n\n");
    if thinking {
        prompt.push_str("Think hard before answering.\n\n");
    }
    // Images: Claude Code reads them from disk with its Read tool.
    let img_dir = state.db.data_dir.join("tmp").join("ai-images");
    let _ = std::fs::remove_dir_all(&img_dir);
    let mut paths = vec![];
    for (mi, m) in msgs.iter().enumerate() {
        for (ii, img) in m.images.iter().enumerate() {
            use base64::Engine;
            if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(&img.data) {
                let ext = img.mime.split('/').nth(1).unwrap_or("png").replace("jpeg", "jpg");
                std::fs::create_dir_all(&img_dir)?;
                let path = img_dir.join(format!("{mi}-{ii}.{ext}"));
                std::fs::write(&path, bytes)?;
                paths.push(path.to_string_lossy().to_string());
            }
        }
    }
    for m in msgs {
        prompt.push_str(if m.role == "assistant" { "### Assistant\n" } else { "### User\n" });
        prompt.push_str(&m.content);
        prompt.push_str("\n\n");
    }
    if !paths.is_empty() {
        prompt.push_str("Attached images (open them with the Read tool before answering):\n");
        for p in &paths {
            prompt.push_str(&format!("- {p}\n"));
        }
        prompt.push('\n');
    }
    prompt.push_str("### Assistant\n");
    let mut tools: Vec<&str> = vec![];
    if !paths.is_empty() {
        tools.push("Read");
    }
    if opts.web_native {
        tools.extend(["WebSearch", "WebFetch"]);
    }
    let exe = claude_exe(p);
    let mut args: Vec<String> = vec!["-p".into(), "--output-format".into(), "stream-json".into(), "--verbose".into(), "--include-partial-messages".into()];
    if !p.model.trim().is_empty() {
        args.push("--model".into());
        args.push(p.model.trim().into());
    }
    if !tools.is_empty() {
        args.push("--allowedTools".into());
        args.push(tools.join(","));
    }
    let mut cmd = claude_command(&exe, &args);
    cmd.stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).kill_on_drop(true);
    let mut child = cmd
        .spawn()
        .map_err(|e| anyhow!("Could not run Claude Code ({exe}): {e}. Install Claude Code and sign in by running `claude` once."))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(prompt.as_bytes()).await?;
        drop(stdin);
    }
    let mut stderr = child.stderr.take().unwrap();
    let err_task = tokio::spawn(async move {
        use tokio::io::AsyncReadExt;
        let mut s = String::new();
        let _ = stderr.read_to_string(&mut s).await;
        s
    });
    let mut lines = tokio::io::BufReader::new(child.stdout.take().unwrap()).lines();
    let mut streamed_text = false;
    let mut final_text: Option<String> = None;
    let mut error: Option<String> = None;
    loop {
        if sink.cancelled() {
            let _ = child.kill().await;
            break;
        }
        let line = match tokio::time::timeout(Duration::from_secs(300), lines.next_line()).await {
            Err(_) => {
                let _ = child.kill().await;
                bail!("Claude Code did not respond within 5 minutes");
            }
            Ok(l) => l?,
        };
        let Some(line) = line else { break };
        let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
        match v["type"].as_str().unwrap_or("") {
            "stream_event" => {
                let e = &v["event"];
                if e["type"] == "content_block_delta" {
                    match e.pointer("/delta/type").and_then(|x| x.as_str()).unwrap_or("") {
                        "text_delta" => {
                            streamed_text = true;
                            sink.text(e.pointer("/delta/text").and_then(|x| x.as_str()).unwrap_or(""));
                        }
                        "thinking_delta" => sink.think(e.pointer("/delta/thinking").and_then(|x| x.as_str()).unwrap_or("")),
                        _ => {}
                    }
                }
            }
            "assistant" => {
                for c in v.pointer("/message/content").and_then(|x| x.as_array()).cloned().unwrap_or_default() {
                    if c["type"] == "tool_use" {
                        let name = c["name"].as_str().unwrap_or("");
                        match name {
                            "WebSearch" => sink.status(&format!("web:{}", c.pointer("/input/query").and_then(|x| x.as_str()).unwrap_or(""))),
                            "WebFetch" => {
                                let u = c.pointer("/input/url").and_then(|x| x.as_str()).unwrap_or("");
                                sink.source(u, u);
                            }
                            "Read" => sink.status("image"),
                            _ => {}
                        }
                    }
                }
            }
            "result" => {
                if v["is_error"].as_bool().unwrap_or(false) {
                    error = Some(v["result"].as_str().unwrap_or("error").to_string());
                } else {
                    final_text = v["result"].as_str().map(|s| s.to_string());
                }
            }
            _ => {}
        }
    }
    let status = child.wait().await?;
    let err = err_task.await.unwrap_or_default();
    if sink.cancelled() {
        return Ok(());
    }
    if let Some(e) = error {
        bail!("Claude Code: {}", e.chars().take(400).collect::<String>());
    }
    if !status.success() && final_text.is_none() && !streamed_text {
        // Older Claude Code versions don't know the streaming flags: fall back to plain text.
        if err.contains("unknown option") || err.contains("include-partial-messages") {
            return claude_cli_plain(&exe, p, &prompt, &tools, sink).await;
        }
        let msg = if err.trim().is_empty() { "Claude Code exited with an error".to_string() } else { err };
        bail!("Claude Code error: {}", msg.trim().chars().take(400).collect::<String>());
    }
    if !streamed_text {
        if let Some(t) = final_text {
            sink.text(&t);
        }
    }
    Ok(())
}

async fn claude_cli_plain(exe: &str, p: &Provider, prompt: &str, tools: &[&str], sink: &mut Sink<'_>) -> Result<()> {
    use tokio::io::AsyncWriteExt;
    let mut args: Vec<String> = vec!["-p".into(), "--output-format".into(), "text".into()];
    if !p.model.trim().is_empty() {
        args.push("--model".into());
        args.push(p.model.trim().into());
    }
    if !tools.is_empty() {
        args.push("--allowedTools".into());
        args.push(tools.join(","));
    }
    let mut cmd = claude_command(exe, &args);
    cmd.stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).kill_on_drop(true);
    let mut child = cmd.spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(prompt.as_bytes()).await?;
    }
    let out = tokio::time::timeout(Duration::from_secs(300), child.wait_with_output())
        .await
        .map_err(|_| anyhow!("Claude Code did not respond within 5 minutes"))??;
    if !out.status.success() {
        bail!("Claude Code error: {}", String::from_utf8_lossy(&out.stderr).trim().chars().take(400).collect::<String>());
    }
    sink.text(String::from_utf8_lossy(&out.stdout).trim());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake {
        text: String,
        think: String,
    }
    fn run(chunks: &[&str]) -> Fake {
        // Exercise ThinkSplit without an AppState by mirroring the sink logic.
        let mut split = ThinkSplit::default();
        let mut f = Fake { text: String::new(), think: String::new() };
        let mut out = vec![];
        for c in chunks {
            split.pending.push_str(c);
            loop {
                if !split.inside {
                    if let Some(i) = split.pending.find("<think>") {
                        out.push((false, split.pending[..i].to_string()));
                        split.pending.drain(..i + 7);
                        split.inside = true;
                        continue;
                    }
                } else if let Some(i) = split.pending.find("</think>") {
                    out.push((true, split.pending[..i].to_string()));
                    split.pending.drain(..i + 8);
                    split.inside = false;
                    continue;
                }
                let keep = partial_tag_suffix(&split.pending);
                let to = split.pending.len() - keep;
                let part: String = split.pending.drain(..to).collect();
                out.push((split.inside, part));
                break;
            }
        }
        out.push((split.inside, split.pending.clone()));
        for (t, s) in out {
            if t { f.think.push_str(&s) } else { f.text.push_str(&s) }
        }
        f
    }

    #[test]
    fn think_tags_across_chunks() {
        let f = run(&["Hel", "lo <thi", "nk>plan a", "nd go</th", "ink> world"]);
        assert_eq!(f.think, "plan and go");
        assert_eq!(f.text, "Hello  world");
        assert_eq!(partial_tag_suffix("abc</thi"), 5);
        assert_eq!(partial_tag_suffix("a < b"), 0);
    }
}
