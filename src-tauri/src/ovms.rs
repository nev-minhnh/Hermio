//! On-device AI on Intel NPUs (Core Ultra "AI Boost") via OpenVINO Model Server (OVMS).
//!
//! Hermio downloads the portable Windows build of OVMS into its runtime folder, pulls NPU-optimised
//! models from the OpenVINO "LLMs optimized for NPU" collection on Hugging Face, and serves one
//! model at a time on localhost with an OpenAI-compatible API (`/v3/chat/completions`).
//! The same power rules as the AMD runtime apply (start on demand, idle release, killed on exit).

use crate::npu::{self, RuntimeModel};
use crate::state::AppState;
use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

const RELEASES_API: &str = "https://api.github.com/repos/openvinotoolkit/model_server/releases/latest";
const COLLECTION_API: &str = "https://huggingface.co/api/collections/OpenVINO/llms-optimized-for-npu-686e7f0bf7bc184bd71f8ba0";
pub const COLLECTION_URL: &str = "https://huggingface.co/collections/OpenVINO/llms-optimized-for-npu-686e7f0bf7bc184bd71f8ba0";
/// Longest prompt the NPU pipeline is compiled for (tokens). Larger values compile slower.
pub const MAX_PROMPT: u32 = 4096;

/// Snapshot of the collection (Nov 2026) for when Hugging Face can't be reached.
const FALLBACK: &[&str] = &[
    "OpenVINO/Qwen3-8B-int4-cw-ov",
    "OpenVINO/Phi-3.5-mini-instruct-int4-cw-ov",
    "OpenVINO/Mistral-7B-Instruct-v0.2-int4-cw-ov",
    "OpenVINO/Phi-3-mini-4k-instruct-int4-cw-ov",
    "OpenVINO/Mistral-7B-Instruct-v0.3-int4-cw-ov",
    "OpenVINO/DeepSeek-R1-Distill-Qwen-7B-nf4-ov",
    "OpenVINO/DeepSeek-R1-Distill-Qwen-1.5B-int4-gq-ov",
    "OpenVINO/Phi-3.5-mini-instruct-int4-gq-ov",
    "OpenVINO/Phi-3-mini-4k-instruct-int4-gq-ov",
];

/// Model currently served (OVMS serves one model per process here).
static SERVING: Mutex<Option<String>> = Mutex::new(None);

pub fn runtime_dir(state: &AppState) -> PathBuf {
    npu::runtime_base(state).join("ovms")
}

pub fn exe_path(state: &AppState) -> Option<PathBuf> {
    find(&runtime_dir(state), 3)
}

fn find(dir: &Path, depth: usize) -> Option<PathBuf> {
    let exe = dir.join(if cfg!(windows) { "ovms.exe" } else { "ovms" });
    if exe.is_file() {
        return Some(exe);
    }
    if depth == 0 {
        return None;
    }
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        if e.path().is_dir() {
            if let Some(p) = find(&e.path(), depth - 1) {
                return Some(p);
            }
        }
    }
    None
}

fn command(exe: &Path) -> tokio::process::Command {
    let mut c = tokio::process::Command::new(exe);
    if let Some(dir) = exe.parent() {
        c.current_dir(dir);
        // What setupvars.bat does: the OVMS folder (and its python dir) on PATH.
        let mut paths = vec![dir.to_path_buf(), dir.join("python")];
        if let Some(p) = std::env::var_os("PATH") {
            paths.extend(std::env::split_paths(&p));
        }
        if let Ok(joined) = std::env::join_paths(paths) {
            c.env("PATH", joined);
        }
    }
    #[cfg(windows)]
    c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    c.stdin(std::process::Stdio::null());
    c
}

/// Folder of one model inside the repository (`OpenVINO/X` → `<models>/OpenVINO/X`).
/// Where a model lives: `<models>/OpenVINO/<name>` (as OVMS downloads it) or, for models the
/// user downloaded themselves, `<models>/<name>`.
fn model_dir(state: &AppState, repo: &str) -> PathBuf {
    let base = npu::models_dir(state);
    let mut p = base.clone();
    for part in repo.split('/') {
        p = p.join(part);
    }
    if !is_model(&p) {
        if let Some(name) = repo.rsplit('/').next() {
            let flat = base.join(name);
            if is_model(&flat) {
                return flat;
            }
        }
    }
    p
}

fn is_model(d: &Path) -> bool {
    d.join("openvino_model.xml").is_file() && d.join("openvino_model.bin").is_file()
}

/// Downloaded in the OVMS layout (served with --source_model) rather than a plain folder.
fn ovms_layout(state: &AppState, repo: &str) -> bool {
    let mut p = npu::models_dir(state);
    for part in repo.split('/') {
        p = p.join(part);
    }
    is_model(&p)
}

pub fn on_disk(state: &AppState, repo: &str) -> bool {
    is_model(&model_dir(state, repo))
}

/// OpenVINO models already in the models folder (also ones outside the NPU collection).
pub fn local_repos(state: &AppState) -> Vec<String> {
    let base = npu::models_dir(state);
    let mut out = vec![];
    for (dir, prefixed) in [(base.join("OpenVINO"), true), (base.clone(), false)] {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if !prefixed && e.file_name() == "OpenVINO" {
                continue;
            }
            if is_model(&p) {
                let r = format!("OpenVINO/{}", e.file_name().to_string_lossy());
                if !out.contains(&r) {
                    out.push(r);
                }
            }
        }
    }
    out
}

pub fn installed(state: &AppState) -> Vec<String> {
    let mut list: Vec<String> = collection_cached(state).into_iter().filter(|r| on_disk(state, r)).collect();
    for r in local_repos(state) {
        if !list.contains(&r) {
            list.push(r);
        }
    }
    list
}

fn http(secs: u64) -> reqwest::Client {
    reqwest::Client::builder().timeout(Duration::from_secs(secs)).user_agent("Hermio-Mail").build().unwrap_or_default()
}

pub async fn alive(state: &AppState) -> bool {
    http(2)
        .get(format!("http://127.0.0.1:{}/v2/health/ready", npu::port_of(state)))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

// ---------------- install ----------------

pub async fn install_runtime(state: &AppState) -> Result<String> {
    let h = http(60 * 30);
    npu::progress(state, "runtime", "check", None, 0, 0, "", false, "");
    let rel: Value = h
        .get(RELEASES_API)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .context("Could not reach GitHub")?
        .error_for_status()?
        .json()
        .await?;
    let version = rel["tag_name"].as_str().unwrap_or("").to_string();
    let asset = rel["assets"]
        .as_array()
        .and_then(|a| {
            a.iter().find(|x| {
                let n = x["name"].as_str().unwrap_or("");
                n.starts_with("ovms_windows") && n.ends_with("python_off.zip")
            })
        })
        .ok_or_else(|| anyhow!("No Windows build found in the latest OpenVINO Model Server release"))?;
    let url = asset["browser_download_url"].as_str().unwrap_or("").to_string();
    let name = asset["name"].as_str().unwrap_or("ovms.zip").to_string();
    let dir = runtime_dir(state);
    let zip_path = npu::runtime_base(state).join(&name);
    std::fs::create_dir_all(npu::runtime_base(state))?;
    npu::download(state, &url, &zip_path, asset["size"].as_u64().unwrap_or(0)).await?;
    npu::stop_server(state).await;
    npu::extract_zip(state, &zip_path, &dir).await?;
    let _ = std::fs::remove_file(&zip_path);
    if find(&dir, 3).is_none() {
        bail!("The download did not contain ovms.exe");
    }
    state.db.set_setting("ovms_version", &serde_json::to_string(&version)?)?;
    npu::progress(state, "runtime", "done", Some(100.0), 0, 0, "", true, "");
    Ok(version)
}

// ---------------- model list ----------------

fn collection_cached(state: &AppState) -> Vec<String> {
    state
        .db
        .get_setting("ovms_collection")
        .ok()
        .flatten()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .and_then(|v| v["repos"].as_array().cloned())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect::<Vec<_>>())
        .filter(|l| !l.is_empty())
        .unwrap_or_else(|| FALLBACK.iter().map(|s| s.to_string()).collect())
}

/// Text models of the OpenVINO NPU collection (vision models are skipped).
pub async fn collection(state: &AppState) -> Vec<String> {
    let now = chrono::Utc::now().timestamp();
    if let Some(raw) = state.db.get_setting("ovms_collection").ok().flatten() {
        if let Ok(v) = serde_json::from_str::<Value>(&raw) {
            if now - v["at"].as_i64().unwrap_or(0) < 86400 {
                return collection_cached(state);
            }
        }
    }
    if let Ok(r) = http(10).get(COLLECTION_API).send().await {
        if r.status().is_success() {
            if let Ok(v) = r.json::<Value>().await {
                let list: Vec<String> = v["items"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .iter()
                    .filter(|i| i["type"] == "model")
                    .filter(|i| {
                        let tag = i["pipeline_tag"].as_str().unwrap_or("");
                        tag != "image-text-to-text" && tag != "image-to-text"
                    })
                    .filter_map(|i| i["id"].as_str().map(|s| s.to_string()))
                    .filter(|id| !vision_repo(id))
                    .collect();
                if !list.is_empty() {
                    let _ = state.db.set_setting("ovms_collection", &json!({ "at": now, "repos": list }).to_string());
                    return list;
                }
            }
        }
    }
    collection_cached(state)
}

fn vision_repo(id: &str) -> bool {
    let l = id.to_lowercase();
    l.contains("gemma-3-") || l.contains("-vl") || l.contains("llava") || l.contains("minicpm-v") || l.contains("ocr")
}

/// Download size of a repo (sum of its files), cached for a week.
async fn repo_size(state: &AppState, repo: &str) -> u64 {
    let key = format!("ovms_size:{repo}");
    let now = chrono::Utc::now().timestamp();
    if let Some(v) = state.db.get_setting(&key).ok().flatten().and_then(|s| serde_json::from_str::<Value>(&s).ok()) {
        if now - v["at"].as_i64().unwrap_or(0) < 7 * 86400 {
            return v["size"].as_u64().unwrap_or(0);
        }
    }
    let url = format!("https://huggingface.co/api/models/{repo}/tree/main?recursive=1");
    let size = match http(8).get(url).send().await {
        Ok(r) if r.status().is_success() => r
            .json::<Value>()
            .await
            .ok()
            .and_then(|v| v.as_array().cloned())
            .map(|a| a.iter().filter(|f| f["type"] == "file").map(|f| f["lfs"]["size"].as_u64().or(f["size"].as_u64()).unwrap_or(0)).sum())
            .unwrap_or(0),
        _ => 0,
    };
    if size > 0 {
        let _ = state.db.set_setting(&key, &json!({ "at": now, "size": size }).to_string());
    }
    size
}

fn params_of(repo: &str) -> f64 {
    let re = regex_lite::Regex::new(r"(?i)-(\d+(?:\.\d+)?)b-").unwrap();
    if let Some(v) = re.captures(repo).and_then(|c| c[1].parse().ok()) {
        return v;
    }
    // Names without a size in them.
    let l = repo.to_lowercase();
    if l.contains("phi-3.5-mini") || l.contains("phi-3-mini") || l.contains("phi-4-mini") {
        return 3.8;
    }
    if l.contains("phi-3-medium") {
        return 14.0;
    }
    0.0
}

pub fn display_name(repo: &str) -> String {
    let n = repo.trim_start_matches("OpenVINO/");
    let re = regex_lite::Regex::new(r"(?i)-(int4|int8|nf4|fp16)(-(cw|gq))?-ov$").unwrap();
    re.replace(n, "").replace('-', " ")
}

fn quant(repo: &str) -> &'static str {
    let l = repo.to_lowercase();
    if l.contains("-cw-") {
        "INT4 channel-wise"
    } else if l.contains("-gq-") {
        "INT4 group-wise"
    } else if l.contains("nf4") {
        "NF4"
    } else {
        ""
    }
}

pub async fn available(state: &AppState) -> Vec<RuntimeModel> {
    let repos = collection(state).await;
    let sizes = futures::future::join_all(repos.iter().map(|r| repo_size(state, r))).await;
    let recommended = ["OpenVINO/Phi-3.5-mini-instruct-int4-cw-ov", "OpenVINO/Qwen3-8B-int4-cw-ov", "OpenVINO/DeepSeek-R1-Distill-Qwen-1.5B-int4-gq-ov"];
    let mut out: Vec<RuntimeModel> = repos
        .iter()
        .zip(sizes)
        .map(|(r, size)| {
            let p = params_of(r);
            let l = r.to_lowercase();
            let thinking = l.contains("qwen3") || l.contains("deepseek-r1");
            RuntimeModel {
                tag: r.clone(),
                name: display_name(r),
                family: l.trim_start_matches("openvino/").split('-').next().unwrap_or("").to_string(),
                params: if p > 0.0 { format!("{p}B") } else { String::new() },
                size,
                // INT4 weights plus KV cache and NPU buffers.
                footprint: if p > 0.0 { (p * 0.75 + 1.0).round() } else { 0.0 },
                vision: false,
                audio: false,
                thinking,
                tools: l.contains("qwen3") || l.contains("mistral-7b-instruct-v0.3"),
                labels: [quant(r)].iter().filter(|s| !s.is_empty()).map(|s| s.to_string()).collect(),
                context: MAX_PROMPT as u64,
                min_version: String::new(),
                url: format!("https://huggingface.co/{r}"),
                compatible: true,
                chat: true,
                installed: on_disk(state, r),
                recommended: recommended.contains(&r.as_str()),
                note: String::new(),
                repo: r.clone(),
                page: format!("https://huggingface.co/{r}"),
            }
        })
        .collect();
    // Models already in the folder that aren't in the NPU collection.
    for r in local_repos(state) {
        if out.iter().any(|m| m.tag == r) {
            continue;
        }
        let p = params_of(&r);
        let size = dir_size(&model_dir(state, &r));
        out.push(RuntimeModel {
            tag: r.clone(),
            name: display_name(&r),
            family: String::new(),
            params: if p > 0.0 { format!("{p}B") } else { String::new() },
            size,
            footprint: if p > 0.0 { (p * 0.75 + 1.0).round() } else { 0.0 },
            vision: false,
            audio: false,
            thinking: r.to_lowercase().contains("qwen3") || r.to_lowercase().contains("deepseek-r1"),
            tools: false,
            labels: vec!["local".into()],
            context: MAX_PROMPT as u64,
            min_version: String::new(),
            url: String::new(),
            compatible: true,
            chat: true,
            installed: true,
            recommended: false,
            note: "Found in your models folder — not from the NPU-optimised collection, so it may not run on the NPU.".into(),
            repo: r.clone(),
            page: String::new(),
        });
    }
    out.sort_by(|a, b| b.recommended.cmp(&a.recommended).then(a.footprint.partial_cmp(&b.footprint).unwrap_or(std::cmp::Ordering::Equal)));
    out
}

fn dir_size(p: &Path) -> u64 {
    let Ok(rd) = std::fs::read_dir(p) else { return 0 };
    rd.flatten().map(|e| if e.path().is_dir() { dir_size(&e.path()) } else { e.metadata().map(|m| m.len()).unwrap_or(0) }).sum()
}

// ---------------- pull / remove ----------------

fn npu_args(state: &AppState) -> Vec<String> {
    let cache = npu::models_dir(state).join(".ov_cache");
    vec![
        "--target_device".into(),
        "NPU".into(),
        "--task".into(),
        "text_generation".into(),
        "--max_prompt_len".into(),
        MAX_PROMPT.to_string(),
        "--cache_dir".into(),
        cache.to_string_lossy().to_string(),
    ]
}

pub async fn pull(state: &AppState, repo: &str) -> Result<()> {
    let exe = exe_path(state).ok_or_else(|| anyhow!("Install the NPU runtime first"))?;
    if !repo.starts_with("OpenVINO/") || repo.contains("..") {
        bail!("Only models from the OpenVINO organisation can be installed");
    }
    let models = npu::models_dir(state);
    std::fs::create_dir_all(&models)?;
    let total = repo_size(state, repo).await;
    let mut args: Vec<String> = vec![
        "--pull".into(),
        "--source_model".into(),
        repo.into(),
        "--model_repository_path".into(),
        models.to_string_lossy().to_string(),
    ];
    args.extend(npu_args(state));
    let child = command(&exe)
        .args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("Could not start {}", exe.display()))?;
    let dir = model_dir(state, repo);
    npu::run_pull(state, repo, child, total, move || npu::dir_size_pub(&dir)).await?;
    if !on_disk(state, repo) {
        bail!("The model files were not found after the download");
    }
    Ok(())
}

pub fn remove(state: &AppState, repo: &str) -> Result<()> {
    if !repo.starts_with("OpenVINO/") || repo.contains("..") || repo.matches('/').count() != 1 {
        bail!("Invalid model");
    }
    let d = model_dir(state, repo);
    if d.exists() {
        std::fs::remove_dir_all(&d)?;
    }
    Ok(())
}

// ---------------- serving ----------------

pub fn serving() -> Option<String> {
    SERVING.lock().unwrap().clone()
}

/// Start OVMS for `repo` (restarting it if another model is loaded). The first start compiles the
/// model for the NPU, which can take a few minutes; later starts use the compile cache.
pub async fn ensure_server(state: &AppState, repo: &str) -> Result<()> {
    if serving().as_deref() == Some(repo) && alive(state).await {
        return Ok(());
    }
    let exe = exe_path(state).ok_or_else(|| anyhow!("The NPU runtime isn't installed. Open Settings → AI assistant → On-device AI."))?;
    if !on_disk(state, repo) {
        bail!("{repo} isn't downloaded yet. Download it in Settings → AI assistant → On-device AI.");
    }
    npu::stop_server(state).await;
    let mut args: Vec<String> = if ovms_layout(state, repo) {
        vec![
            "--rest_port".into(),
            npu::port_of(state).to_string(),
            "--source_model".into(),
            repo.into(),
            "--model_repository_path".into(),
            npu::models_dir(state).to_string_lossy().to_string(),
            "--model_name".into(),
            repo.into(),
        ]
    } else {
        // A model folder the user downloaded themselves.
        vec![
            "--rest_port".into(),
            npu::port_of(state).to_string(),
            "--model_path".into(),
            model_dir(state, repo).to_string_lossy().to_string(),
            "--model_name".into(),
            repo.into(),
        ]
    };
    args.extend(npu_args(state));
    let child = command(&exe)
        .args(&args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("Could not start {}", exe.display()))?;
    npu::adopt_server(child);
    *SERVING.lock().unwrap() = Some(repo.to_string());
    for _ in 0..(6 * 60 * 4) {
        tokio::time::sleep(Duration::from_millis(250)).await;
        if alive(state).await {
            state.emit("npu-state", json!({ "running": true }));
            npu::server_started(state);
            return Ok(());
        }
        if let Some(code) = npu::server_exited() {
            *SERVING.lock().unwrap() = None;
            bail!("The Intel NPU runtime stopped ({code}). Update the Intel NPU driver and try again.");
        }
    }
    bail!("The Intel NPU runtime did not become ready within 6 minutes")
}

pub fn clear_serving() {
    *SERVING.lock().unwrap() = None;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names() {
        assert_eq!(display_name("OpenVINO/Phi-3.5-mini-instruct-int4-cw-ov"), "Phi 3.5 mini instruct");
        assert_eq!(params_of("OpenVINO/Qwen3-8B-int4-cw-ov"), 8.0);
        assert_eq!(params_of("OpenVINO/DeepSeek-R1-Distill-Qwen-1.5B-int4-gq-ov"), 1.5);
        assert_eq!(params_of("OpenVINO/Phi-3.5-mini-instruct-int4-cw-ov"), 3.8);
        assert!(vision_repo("OpenVINO/gemma-3-4b-it-int4-cw-ov"));
        assert!(!vision_repo("OpenVINO/Qwen3-8B-int4-cw-ov"));
    }
}
