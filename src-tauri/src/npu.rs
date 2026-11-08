//! On-device AI on the AMD Ryzen AI NPU (XDNA 2) via FastFlowLM ("Powered by FastFlowLM",
//! https://github.com/FastFlowLM/FastFlowLM). Hermio downloads the portable Windows runtime into its
//! data folder, pulls models with `flm pull`, and runs `flm serve` in the background, which exposes
//! an OpenAI-compatible API on localhost.

use crate::state::AppState;
use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use tokio::io::AsyncReadExt;

/// Hermio's own port (FastFlowLM's default is 52625), so a separately started FLM is never touched.
pub const DEFAULT_PORT: u16 = 52725;
const RELEASES_API: &str = "https://api.github.com/repos/ROCm/FastFlowLM/releases/latest";

#[derive(Debug, Clone, Serialize)]
pub struct CatalogModel {
    pub tag: &'static str,
    pub name: &'static str,
    pub params: &'static str,
    pub note: &'static str,
    pub vision: bool,
    pub thinking: bool,
    pub recommended: bool,
}

/// Small models that run well on a Ryzen AI 300 NPU, best first.
pub fn catalog() -> Vec<CatalogModel> {
    let m = |tag, name, params, note, vision, thinking, recommended| CatalogModel { tag, name, params, note, vision, thinking, recommended };
    vec![
        m("qwen3.5:4b", "Qwen3.5 4B", "4B", "Best quality for mail: summaries, replies, sorting. 256k context, reads images.", true, true, true),
        m("gemma4-it:e4b", "Gemma 4 E4B", "≈4B effective", "Strong writing and multilingual; reads images and audio.", true, true, false),
        m("qwen3-it:4b", "Qwen3 4B Instruct 2507", "4B", "Fast, no thinking step; good at following instructions.", false, false, false),
        m("phi4-mini-it:4b", "Phi-4-mini", "3.8B", "Good reasoning for its size; English-first.", false, false, false),
        m("gemma4-it:e2b", "Gemma 4 E2B", "≈2B effective", "Light and quick; reads images and audio.", true, true, false),
        m("qwen3.5:2b", "Qwen3.5 2B", "2B", "Lightest capable model; reads images.", true, true, false),
        m("llama3.2:3b", "Llama 3.2 3B", "3B", "Classic small model, English-first.", false, false, false),
    ]
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Status {
    pub installed: bool,
    pub path: String,
    pub version: String,
    pub running: bool,
    pub port: u16,
    pub npu: String,
    pub models: Vec<String>,
    pub pulling: Vec<String>,
}

struct Runtime {
    server: Option<tokio::process::Child>,
    pulls: BTreeMap<String, u32>,
}

static LAST_USE: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);

fn touch() {
    LAST_USE.store(chrono::Utc::now().timestamp(), std::sync::atomic::Ordering::Relaxed);
}

/// Seconds of inactivity before the model is released from the NPU (process exits).
fn idle_limit(state: &AppState) -> i64 {
    let v = state.db.get_setting("npu_idle").ok().flatten().and_then(|v| v.trim_matches('"').parse::<i64>().ok()).unwrap_or(120);
    let v = v.clamp(20, 3600);
    // On battery never keep it loaded longer than a minute.
    if on_battery() { v.min(60) } else { v }
}

/// Hibernation: the runtime only lives while Hermio is in use. It is started on the first AI
/// request, released after a short idle time, when the window goes to the tray, and on exit.
pub fn start_idle_watch(state: std::sync::Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(10)).await;
            let ours = RT.lock().unwrap().server.is_some();
            if !ours || BUSY.load(std::sync::atomic::Ordering::Relaxed) > 0 {
                continue;
            }
            let idle = chrono::Utc::now().timestamp() - LAST_USE.load(std::sync::atomic::Ordering::Relaxed);
            if idle > idle_limit(&state) {
                stop_server(&state).await;
                state.emit("npu-state", json!({ "running": false, "reason": "idle" }));
            }
        }
    });
}

/// Requests currently using the model (the idle timer never fires mid-answer).
static BUSY: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

pub struct UseGuard;
impl Drop for UseGuard {
    fn drop(&mut self) {
        BUSY.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        touch();
    }
}
/// Someone is using the NPU right now.
pub fn busy() -> bool {
    BUSY.load(std::sync::atomic::Ordering::Relaxed) > 0
}

pub fn use_guard() -> UseGuard {
    BUSY.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    touch();
    UseGuard
}

/// True when a laptop runs on battery (Windows); false elsewhere or when unknown.
pub fn on_battery() -> bool {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
        let mut st: SYSTEM_POWER_STATUS = std::mem::zeroed();
        if GetSystemPowerStatus(&mut st) != 0 {
            return st.ACLineStatus == 0;
        }
    }
    false
}

/// NPU power mode: powersaver on battery, balanced on AC, unless the user picked one.
fn pmode(state: &AppState) -> String {
    let v = state.db.get_setting("npu_pmode").ok().flatten().unwrap_or_default().trim_matches('"').to_string();
    match v.as_str() {
        "powersaver" | "balanced" | "performance" | "turbo" => v,
        _ => {
            if on_battery() { "powersaver".into() } else { "balanced".into() }
        }
    }
}

/// Windows job object that kills the runtime if Hermio exits or crashes, so the model is never
/// left loaded on the NPU.
#[cfg(windows)]
mod job {
    use std::sync::OnceLock;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::*;
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE};

    struct Job(HANDLE);
    unsafe impl Send for Job {}
    unsafe impl Sync for Job {}
    static JOB: OnceLock<Option<Job>> = OnceLock::new();

    fn job() -> Option<HANDLE> {
        JOB.get_or_init(|| unsafe {
            let h = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if h.is_null() {
                return None;
            }
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            SetInformationJobObject(
                h,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            Some(Job(h))
        })
        .as_ref()
        .map(|j| j.0)
    }

    pub fn adopt(pid: u32) {
        let Some(j) = job() else { return };
        unsafe {
            let p = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, pid);
            if !p.is_null() {
                AssignProcessToJobObject(j, p);
                CloseHandle(p);
            }
        }
    }
}
#[cfg(not(windows))]
mod job {
    pub fn adopt(_pid: u32) {}
}

static RT: Mutex<Runtime> = Mutex::new(Runtime { server: None, pulls: BTreeMap::new() });

fn setting_str(state: &AppState, k: &str) -> String {
    state.db.get_setting(k).ok().flatten().unwrap_or_default().trim_matches('"').to_string()
}

pub fn default_runtime_base(state: &AppState) -> PathBuf {
    state.db.data_dir.join("runtime")
}

/// Folder holding the NPU runtimes (`flm/`, `ovms/`); changeable in Settings.
pub fn runtime_base(state: &AppState) -> PathBuf {
    let v = setting_str(state, "npu_runtime_dir");
    if v.is_empty() { default_runtime_base(state) } else { PathBuf::from(v) }
}

fn runtime_dir(state: &AppState) -> PathBuf {
    runtime_base(state).join("flm")
}

/// Where models are stored by default for this PC's runtime.
pub fn default_models_dir(state: &AppState) -> PathBuf {
    if cached_info().map(|i| i.vendor == "intel").unwrap_or(false) {
        state.db.data_dir.join("models").join("openvino")
    } else {
        // FastFlowLM's own default: %USERPROFILE%\.flm\models
        std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from).unwrap_or_default().join(".flm").join("models")
    }
}

/// Folder holding downloaded models; changeable in Settings.
pub fn models_dir(state: &AppState) -> PathBuf {
    let v = setting_str(state, "npu_models_dir");
    if v.is_empty() { default_models_dir(state) } else { PathBuf::from(v) }
}

/// The user's models folder, if they chose one. When they pick the folder *above* the models
/// (e.g. `.flm` instead of `.flm\models`) the models subfolder is used.
fn custom_models_dir(state: &AppState) -> Option<PathBuf> {
    let v = setting_str(state, "npu_models_dir");
    if v.is_empty() {
        return None;
    }
    let d = PathBuf::from(v);
    let sub = d.join("models");
    if vendor() != "intel" && !has_model_folders(state, &d) && has_model_folders(state, &sub) {
        return Some(sub);
    }
    Some(d)
}

/// Folder names FastFlowLM uses for its models (model_list.json `name`), with their files.
fn flm_names(state: &AppState) -> Vec<(String, String, Vec<String>)> {
    let Some(list) = read_json(&runtime_dir(state), "model_list.json").or_else(|| flm_path(state).and_then(|p| read_json(p.parent()?, "model_list.json"))) else {
        return vec![];
    };
    let mut out = vec![];
    for (fam, sizes) in list["models"].as_object().cloned().unwrap_or_default() {
        for (sz, m) in sizes.as_object().cloned().unwrap_or_default() {
            let files: Vec<String> = m["files"].as_array().cloned().unwrap_or_default().iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect();
            if let Some(name) = m["name"].as_str() {
                out.push((format!("{fam}:{sz}"), name.to_string(), files));
            }
        }
    }
    out
}

fn has_model_folders(state: &AppState, d: &Path) -> bool {
    d.is_dir() && flm_names(state).iter().any(|(_, n, _)| d.join(n).is_dir())
}

/// Models found on disk where the runtime will look for them (tag list), cached for a few seconds.
static SCAN: Mutex<Option<(std::time::Instant, String, Vec<String>)>> = Mutex::new(None);

fn scan_installed(state: &AppState) -> Vec<String> {
    let bases = model_bases(state);
    let key = bases.iter().map(|b| b.display().to_string()).collect::<Vec<_>>().join("|");
    if let Some((t, k, v)) = SCAN.lock().unwrap().clone() {
        if k == key && t.elapsed() < Duration::from_secs(3) {
            return v;
        }
    }
    let found: Vec<String> = flm_names(state)
        .into_iter()
        .filter(|(_, name, files)| bases.iter().any(|b| {
            let d = b.join(name);
            d.is_dir() && !files.is_empty() && files.iter().all(|f| d.join(f).is_file())
        }))
        .map(|(tag, _, _)| tag)
        .collect();
    *SCAN.lock().unwrap() = Some((std::time::Instant::now(), key, found.clone()));
    found
}

pub fn forget_scan() {
    *SCAN.lock().unwrap() = None;
}

pub fn vendor() -> String {
    cached_info().map(|i| i.vendor).unwrap_or_default()
}

fn find_in(dir: &Path, depth: usize) -> Option<PathBuf> {
    let exe = dir.join(if cfg!(windows) { "flm.exe" } else { "flm" });
    if exe.is_file() {
        return Some(exe);
    }
    if depth == 0 {
        return None;
    }
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        if e.path().is_dir() {
            if let Some(p) = find_in(&e.path(), depth - 1) {
                return Some(p);
            }
        }
    }
    None
}

/// The flm executable: Hermio's own copy first, then a system-wide install.
pub fn flm_path(state: &AppState) -> Option<PathBuf> {
    if let Some(p) = find_in(&runtime_dir(state), 3) {
        return Some(p);
    }
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    for var in ["ProgramFiles", "LOCALAPPDATA", "ProgramW6432"] {
        if let Some(v) = std::env::var_os(var) {
            let base = PathBuf::from(v);
            dirs.push(base.join("flm"));
            dirs.push(base.join("FastFlowLM"));
            dirs.push(base.join("Programs").join("flm"));
        }
    }
    dirs.into_iter().find_map(|d| find_in(&d, 1))
}

pub fn port_of(state: &AppState) -> u16 {
    port(state)
}

fn port(state: &AppState) -> u16 {
    state.db.get_setting("npu_port").ok().flatten().and_then(|v| v.trim_matches('"').parse().ok()).unwrap_or(DEFAULT_PORT)
}

pub fn base_url(state: &AppState) -> String {
    if vendor() == "intel" {
        format!("http://127.0.0.1:{}/v3", port(state))
    } else {
        format!("http://127.0.0.1:{}/v1", port(state))
    }
}

fn command(state: &AppState, exe: &Path) -> tokio::process::Command {
    let mut c = tokio::process::Command::new(exe);
    if let Some(d) = custom_models_dir(state) {
        c.env("FLM_MODEL_PATH", d);
    }
    if let Some(dir) = exe.parent() {
        c.current_dir(dir);
    }
    #[cfg(windows)]
    c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    c.stdin(std::process::Stdio::null());
    c
}

/// Downloaded models. With a chosen models folder, what is on disk there is the truth (so a folder
/// that already holds FastFlowLM models works straight away); with FastFlowLM's default folder,
/// Hermio's own list is merged with what's on disk.
pub fn installed_models(state: &AppState) -> Vec<String> {
    if vendor() == "intel" {
        return crate::ovms::installed(state);
    }
    let on_disk = scan_installed(state);
    if custom_models_dir(state).is_some() {
        return on_disk;
    }
    let mut list: Vec<String> = state
        .db
        .get_setting("npu_models")
        .ok()
        .flatten()
        .and_then(|v| serde_json::from_str::<Vec<String>>(&v).ok())
        .unwrap_or_default();
    for t in on_disk {
        if !list.contains(&t) {
            list.push(t);
        }
    }
    list
}

fn set_models(state: &AppState, list: &[String]) -> Result<()> {
    forget_scan();
    state.db.set_setting("npu_models", &serde_json::to_string(list)?)
}

fn http(timeout: u64) -> reqwest::Client {
    reqwest::Client::builder().timeout(Duration::from_secs(timeout)).user_agent("Hermio-Mail").build().unwrap_or_default()
}

async fn alive_fast(state: &AppState) -> bool {
    http(1).get(format!("{}/models", base_url(state))).send().await.map(|r| r.status().is_success()).unwrap_or(false)
}

/// Reconcile the downloaded-model list with the runtime (slow: runs `flm list`).
pub async fn sync_models(state: &AppState) {
    if vendor() == "intel" {
        return;
    }
    if let Some(p) = flm_path(state) {
        reconcile(state, &p).await;
    }
}

async fn alive(state: &AppState) -> bool {
    http(3).get(format!("{}/models", base_url(state))).send().await.map(|r| r.status().is_success()).unwrap_or(false)
}

/// Sync our list of downloaded models with `flm list --filter installed`.
async fn reconcile(state: &AppState, exe: &Path) {
    let Ok(Ok(o)) = tokio::time::timeout(Duration::from_secs(8), command(state, exe).args(["list", "--filter", "installed", "--quiet"]).output()).await else { return };
    if !o.status.success() {
        return;
    }
    let text = strip_ansi(&String::from_utf8_lossy(&o.stdout));
    let tags: Vec<String> = catalog()
        .iter()
        .filter(|m| text.split(|c: char| c.is_whitespace() || c == ',' || c == '|').any(|w| w.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != ':' && c != '.' && c != '-') == m.tag))
        .map(|m| m.tag.to_string())
        .collect();
    // Only trust a non-empty answer (older runtimes may not support the filter).
    if !tags.is_empty() || text.to_lowercase().contains("no model") {
        let mut list = installed_models(state);
        list.retain(|t| tags.contains(t) || !catalog().iter().any(|m| m.tag == t));
        for t in tags {
            if !list.contains(&t) {
                list.push(t);
            }
        }
        let _ = set_models(state, &list);
    }
}

/// Quick status for the UI (no slow `flm list` call; see [`sync_models`]).
pub async fn status(state: &AppState) -> Status {
    let intel = vendor() == "intel";
    let path = if intel { crate::ovms::exe_path(state) } else { flm_path(state) };
    let running = is_running() && if intel { crate::ovms::alive(state).await } else { alive_fast(state).await };
    let npu = String::new();
    let version = setting_str(state, if intel { "ovms_version" } else { "npu_version" });
    Status {
        installed: path.is_some(),
        path: path.map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
        version,
        running,
        port: port(state),
        npu,
        models: installed_models(state),
        pulling: RT.lock().unwrap().pulls.keys().cloned().collect(),
    }
}

// ---------------- NPU detection ----------------

#[derive(Debug, Clone, Serialize, Default)]
pub struct NpuInfo {
    /// The processor has an NPU (or an NPU device was found).
    pub present: bool,
    /// "amd", "intel" or "" (no supported NPU).
    pub vendor: String,
    pub device: String,
    pub driver: String,
    /// Driver is new enough for the runtime.
    pub driver_ok: bool,
    /// The NPU device is present but Windows has no working driver for it.
    pub driver_missing: bool,
    pub driver_min: String,
    pub cpu: String,
    /// "xdna2" / "xdna1" (AMD), "intel" (Core Ultra), "none" or "unknown".
    pub arch: String,
    /// Runtime used on this PC: "flm" (AMD) or "ovms" (Intel).
    pub engine: String,
    /// FastFlowLM platform id ("aie2p" for XDNA 2).
    pub platform: String,
    pub supported: bool,
    pub ram_gb: f64,
    pub note: String,
}

static INFO: Mutex<Option<NpuInfo>> = Mutex::new(None);

pub fn cached_info() -> Option<NpuInfo> {
    INFO.lock().unwrap().clone()
}

/// Compare dotted driver versions (`a >= b`).
pub fn version_ge(a: &str, b: &str) -> bool {
    let pa: Vec<u32> = a.split('.').map(|x| x.trim().parse().unwrap_or(0)).collect();
    let pb: Vec<u32> = b.split('.').map(|x| x.trim().parse().unwrap_or(0)).collect();
    for i in 0..pb.len().max(pa.len()) {
        let (x, y) = (*pa.get(i).unwrap_or(&0), *pb.get(i).unwrap_or(&0));
        if x != y {
            return x > y;
        }
    }
    true
}

#[allow(dead_code)]
fn driver_ok(v: &str) -> bool {
    version_ge(v, AMD_DRIVER_MIN)
}

pub const AMD_DRIVER_MIN: &str = "32.0.203.311";
pub const INTEL_DRIVER_MIN: &str = "32.0.100.3104";

/// Which NPU generation a CPU name implies.
pub fn arch_of(cpu: &str) -> &'static str {
    let c = cpu.to_lowercase();
    // Ryzen AI 300 (Strix Point / Krackan), AI Max (Strix Halo), AI 400 (Gorgon Point): XDNA 2.
    let re2 = regex_lite::Regex::new(r"ryzen\s+ai\s+(max\+?\s*(pro\s*)?\d|(\d\s+)?(pro\s+)?(hx\s+)?[34]\d\d|z2)").unwrap();
    if re2.is_match(&c) {
        return "xdna2";
    }
    // Ryzen 7040 / 8040 (Phoenix / Hawk Point) and Ryzen 8000G: XDNA 1.
    let re1 = regex_lite::Regex::new(r"ryzen\s+\d\s+(pro\s+)?(7\d40|8\d40|8\d00g|8\d45)").unwrap();
    if re1.is_match(&c) {
        return "xdna1";
    }
    // Intel Core Ultra (Meteor Lake, Lunar Lake, Arrow Lake, Panther Lake) have an "AI Boost" NPU.
    let ri = regex_lite::Regex::new(r"core(\(tm\))?\s+ultra").unwrap();
    if ri.is_match(&c) {
        return "intel";
    }
    "unknown"
}

/// Ask Windows about the CPU and the NPU device (name, driver version, status).
pub async fn detect() -> NpuInfo {
    let mut info = NpuInfo { arch: "unknown".into(), ram_gb: ram_gb(), ..Default::default() };
    let mut dev_status = String::new();
    #[cfg(windows)]
    {
        // Only the Compute Accelerator class, or exact NPU names (case-sensitive: "Input" must not match).
        let script = "$ErrorActionPreference='SilentlyContinue';\
            $c=(Get-CimInstance Win32_Processor | Select-Object -First 1).Name;\
            $d=Get-PnpDevice -Class ComputeAccelerator | Select-Object -First 1;\
            if(-not $d){ $d=Get-PnpDevice | Where-Object { $_.FriendlyName -cmatch '(\\bNPU\\b|AI Boost|\\bIPU\\b|Neural Processing)' } | Select-Object -First 1 };\
            $v=$null; if($d){ $v=(Get-PnpDeviceProperty -InstanceId $d.InstanceId -KeyName DEVPKEY_Device_DriverVersion).Data };\
            [pscustomobject]@{cpu=$c;device=$d.FriendlyName;driver=$v;status=[string]$d.Status} | ConvertTo-Json -Compress";
        let mut c = tokio::process::Command::new("powershell");
        c.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", script]);
        c.creation_flags(0x0800_0000);
        c.stdin(std::process::Stdio::null());
        if let Ok(Ok(o)) = tokio::time::timeout(Duration::from_secs(25), c.output()).await {
            let txt = String::from_utf8_lossy(&o.stdout).to_string();
            if let Ok(v) = serde_json::from_str::<Value>(txt.trim_start_matches('\u{feff}').trim()) {
                info.cpu = v["cpu"].as_str().unwrap_or("").trim().to_string();
                info.device = v["device"].as_str().unwrap_or("").trim().to_string();
                info.driver = v["driver"].as_str().unwrap_or("").trim().to_string();
                dev_status = v["status"].as_str().unwrap_or("").trim().to_string();
            }
        }
    }
    #[cfg(not(windows))]
    {
        if let Ok(s) = std::fs::read_to_string("/proc/cpuinfo") {
            info.cpu = s.lines().find(|l| l.starts_with("model name")).and_then(|l| l.split(':').nth(1)).unwrap_or("").trim().to_string();
        }
        if std::path::Path::new("/dev/accel/accel0").exists() {
            info.device = "NPU".into();
            dev_status = "OK".into();
        }
    }
    let arch = arch_of(&info.cpu);
    let cpu_l = info.cpu.to_lowercase();
    let dev_l = info.device.to_lowercase();
    info.vendor = if arch == "intel" || dev_l.contains("ai boost") || (cpu_l.contains("intel") && !info.device.is_empty()) {
        "intel".into()
    } else if arch.starts_with("xdna") || cpu_l.contains("amd") && !info.device.is_empty() {
        "amd".into()
    } else {
        String::new()
    };
    info.arch = if arch == "unknown" && !info.device.is_empty() {
        if info.vendor == "intel" { "intel".into() } else { "unknown".into() }
    } else {
        arch.into()
    };
    // The processor has an NPU even when Windows lacks its driver.
    info.present = !info.device.is_empty() || matches!(arch, "xdna2" | "xdna1" | "intel");
    if !info.present {
        info.arch = "none".into();
    }
    info.driver_missing = info.present && (info.device.is_empty() || info.driver.is_empty() || (!dev_status.is_empty() && dev_status != "OK"));
    info.driver_min = if info.vendor == "intel" { INTEL_DRIVER_MIN.into() } else { AMD_DRIVER_MIN.into() };
    info.driver_ok = !info.driver_missing && version_ge(&info.driver, &info.driver_min);
    info.engine = match info.vendor.as_str() {
        "intel" => "ovms".into(),
        "amd" if info.arch != "xdna1" => "flm".into(),
        _ => String::new(),
    };
    info.platform = if info.vendor == "amd" && info.arch != "xdna1" { "aie2p".into() } else { String::new() };
    info.supported = info.present && !info.engine.is_empty() && info.driver_ok;
    info.note = if !info.present {
        "No NPU was found. On-device AI needs an AMD Ryzen AI or Intel Core Ultra processor.".into()
    } else if info.arch == "xdna1" {
        "This NPU (XDNA 1, Ryzen 7040/8040) is not supported by FastFlowLM, which needs XDNA 2 (Ryzen AI 300 or newer).".into()
    } else if info.driver_missing {
        "The NPU has no working driver yet. Install the driver to use on-device AI.".into()
    } else if !info.driver_ok {
        "driver-old".into()
    } else {
        String::new()
    };
    *INFO.lock().unwrap() = Some(info.clone());
    info
}

pub async fn info(force: bool) -> NpuInfo {
    if !force {
        if let Some(i) = cached_info() {
            return i;
        }
    }
    detect().await
}

// ---------------- Hugging Face collection (the official XDNA 2 model list) ----------------

const COLLECTION_API: [&str; 2] = [
    "https://huggingface.co/api/collections/FastFlowLM/flm-models-xdna2-6a8f60edd012f24cc58d5898",
    "https://huggingface.co/api/collections/FastFlowLM/flm-models-xdna2",
];
pub const COLLECTION_URL: &str = "https://huggingface.co/collections/FastFlowLM/flm-models-xdna2";

/// Snapshot of the collection (Nov 2026), used when Hugging Face can't be reached.
const COLLECTION_FALLBACK: &[&str] = &[
    "DeepSeek-R1-0528-Qwen3-8B-NPU2", "Deepseek-R1-Distill-Llama-8B-NPU2", "Embedding-Gemma-300M-NPU2", "GPT-OSS-20B-NPU2",
    "GPT-OSS-Safeguard-20b-NPU2", "Gemma3-1B-NPU2", "Gemma3-270M-NPU2", "Gemma3-4B-NPU2", "Gemma3-4B-Text-NPU2", "Gemma4-E2B-IT-NPU2",
    "Gemma4-E4B-IT-NPU2", "LFM2-1.2B-NPU2", "LFM2-1.2B-Tool-NPU2", "LFM2-2.6B-NPU2", "LFM2-2.6B-Transcript-NPU2", "LFM2.5-1.2B-NPU2",
    "LFM2.5-1.2B-Thinking-NPU2", "Llama-3.1-8B-NPU2", "Llama-3.2-1B-NPU2", "Llama-3.2-3B-NPU2", "Nanbeige4.1-3B-NPU2",
    "Phi4-mini-Instruct-NPU2", "Qwen2.5-3B-Instruct-NPU2", "Qwen2.5-VL-3B-Instruct-NPU2", "Qwen3-0.6B-NPU2", "Qwen3-1.7B-NPU2",
    "Qwen3-4B-Instruct-2507-NPU2", "Qwen3-4B-NPU2", "Qwen3-4B-Thinking-2507-NPU2", "Qwen3-8B-NPU2", "Qwen3-VL-4B-Instruct-NPU2",
    "Qwen3.5-0.8B-NPU2", "Qwen3.5-2B-NPU2", "Qwen3.5-4B-NPU2", "Qwen3.5-9B-NPU2", "Qwen3.6-35B-A3B-NPU2",
    "Translategemma-4B-Instruct-NPU2", "Whisper-V3-Turbo-NPU2", "medgemma-1.5-4b-it-NPU2", "medgemma-4b-it-NPU2", "Hy-MT2-1.8B-NPU2",
    "MiniCPM-V-4.7-1B-NPU2", "D3-Lite-NPU2", "D3-Nano-NPU2", "D3-Mini-NPU2", "Nemotron-3.5-Bolt-E4B-NPU2", "Qwen3.8-27B-NPU2",
    "D3-Ultra-NPU2", "D3-Turbo-NPU2", "Gemma4-12B-IT-NPU2", "LFM3-1.7B-NPU2", "Qwen3.8-27B-DFlash2-NPU2", "MiniCPM-OCR-1.0-NPU2",
    "MiniCPM-V-4.7-35B-A3B-NPU2", "D3-Edge-NPU2", "Nemotron-3.5-Bolt-E4B-DFlash-NPU2", "D3-Flash-NPU2",
];

/// Repo names in the FastFlowLM XDNA 2 collection (cached for a day).
pub async fn collection(state: &AppState) -> Vec<String> {
    let now = chrono::Utc::now().timestamp();
    if let Some(raw) = state.db.get_setting("npu_collection").ok().flatten() {
        if let Ok(v) = serde_json::from_str::<Value>(&raw) {
            if now - v["at"].as_i64().unwrap_or(0) < 86400 {
                let list: Vec<String> = v["repos"].as_array().cloned().unwrap_or_default().iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect();
                if !list.is_empty() {
                    return list;
                }
            }
        }
    }
    for url in COLLECTION_API {
        if let Ok(r) = http(10).get(url).send().await {
            if !r.status().is_success() {
                continue;
            }
            if let Ok(v) = r.json::<Value>().await {
                let list: Vec<String> = v["items"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .iter()
                    .filter(|i| i["type"] == "model")
                    .filter_map(|i| i["id"].as_str().and_then(|s| s.strip_prefix("FastFlowLM/")).map(|s| s.to_string()))
                    .collect();
                if !list.is_empty() {
                    let _ = state.db.set_setting("npu_collection", &json!({ "at": now, "repos": list }).to_string());
                    return list;
                }
            }
        }
    }
    COLLECTION_FALLBACK.iter().map(|s| s.to_string()).collect()
}

/// `FastFlowLM/<repo>` from a model_list URL like `https://huggingface.co/FastFlowLM/<repo>/resolve/<rev>`.
fn repo_of(url: &str) -> Option<String> {
    let i = url.find("huggingface.co/FastFlowLM/")? + "huggingface.co/FastFlowLM/".len();
    let rest = &url[i..];
    Some(rest.split('/').next()?.to_string())
}

/// Translation, transcription and safety-classifier models: not general assistants.
fn special_purpose(repo: &str) -> bool {
    let l = repo.to_lowercase();
    ["translategemma", "transcript", "safeguard", "hy-mt", "whisper", "embedding", "d3-"].iter().any(|k| l.contains(k))
}

/// Specialised image models (VL/OCR/medical imaging) — hidden, Hermio needs general chat models.
fn vision_only(repo: &str) -> bool {
    let r = repo.to_lowercase();
    r.contains("-vl-") || r.contains("ocr") || r.starts_with("minicpm-v") || r.starts_with("medgemma")
}

// ---------------- runtime model catalog (model_list.json / model_info.json) ----------------

#[derive(Debug, Clone, Serialize, Default)]
pub struct RuntimeModel {
    pub tag: String,
    pub name: String,
    pub family: String,
    pub params: String,
    /// Download size in bytes.
    pub size: u64,
    /// RAM needed while loaded, GB.
    pub footprint: f64,
    pub vision: bool,
    pub audio: bool,
    pub thinking: bool,
    pub tools: bool,
    pub labels: Vec<String>,
    pub context: u64,
    pub min_version: String,
    pub url: String,
    /// Runs on this kind of NPU with the installed runtime version.
    pub compatible: bool,
    /// A general chat model (not embeddings, speech, decision or single-turn only).
    pub chat: bool,
    pub installed: bool,
    pub recommended: bool,
    pub note: String,
    /// Hugging Face repo (from the FastFlowLM collection) and its page.
    pub repo: String,
    pub page: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Available {
    pub models: Vec<RuntimeModel>,
    pub ram_gb: f64,
    pub version: String,
    pub npu: Option<NpuInfo>,
    pub collection: String,
    /// Models in the collection that the installed runtime doesn't know yet.
    pub newer: Vec<String>,
}

fn read_json(dir: &Path, name: &str) -> Option<Value> {
    let exe = find_in(dir, 3)?;
    let p = exe.parent()?.join(name);
    serde_json::from_slice(&std::fs::read(p).ok()?).ok()
}

/// Total physical memory in GB (0 when unknown).
pub fn ram_gb() -> f64 {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
        let mut m: MEMORYSTATUSEX = std::mem::zeroed();
        m.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        if GlobalMemoryStatusEx(&mut m) != 0 {
            return m.ullTotalPhys as f64 / 1024.0 / 1024.0 / 1024.0;
        }
    }
    #[cfg(not(windows))]
    if let Ok(s) = std::fs::read_to_string("/proc/meminfo") {
        if let Some(kb) = s.lines().find(|l| l.starts_with("MemTotal")).and_then(|l| l.split_whitespace().nth(1)).and_then(|v| v.parse::<f64>().ok()) {
            return kb / 1024.0 / 1024.0;
        }
    }
    0.0
}

fn version_tuple(v: &str) -> (u32, u32, u32) {
    let mut it = v.trim().trim_start_matches('v').split(|c: char| !c.is_ascii_digit()).filter(|x| !x.is_empty()).map(|x| x.parse::<u32>().unwrap_or(0));
    (it.next().unwrap_or(0), it.next().unwrap_or(0), it.next().unwrap_or(0))
}

/// Folders where FastFlowLM keeps downloaded models.
fn model_bases(state: &AppState) -> Vec<PathBuf> {
    let mut out = vec![];
    // A chosen folder is the only place the runtime looks (FLM_MODEL_PATH), so it's the only place
    // that counts.
    if let Some(d) = custom_models_dir(state) {
        out.push(d);
        return out;
    }
    if let Some(p) = std::env::var_os("FLM_MODEL_PATH") {
        out.push(PathBuf::from(p));
    }
    let mut docs = vec![];
    if let Some(h) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) {
        docs.push(PathBuf::from(&h).join("Documents"));
        docs.push(PathBuf::from(&h).join("OneDrive").join("Documents"));
    }
    if let Some(o) = std::env::var_os("OneDrive") {
        docs.push(PathBuf::from(o).join("Documents"));
    }
    for d in docs {
        out.push(d.join("flm").join("models"));
    }
    if let Some(h) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) {
        out.push(PathBuf::from(&h).join(".flm").join("models"));
        out.push(PathBuf::from(&h).join(".flm"));
    }
    out
}

fn dir_size(p: &Path) -> u64 {
    let mut n = 0;
    if let Ok(rd) = std::fs::read_dir(p) {
        for e in rd.flatten() {
            let path = e.path();
            if path.is_dir() {
                n += dir_size(&path);
            } else if let Ok(m) = e.metadata() {
                n += m.len();
            }
        }
    }
    n
}

fn model_dirs_for(state: &AppState, name: &str) -> Vec<PathBuf> {
    model_bases(state).into_iter().map(|b| b.join(name)).collect()
}

fn on_disk(state: &AppState, name: &str, files: &[String]) -> bool {
    model_dirs_for(state, name).iter().any(|d| d.is_dir() && !files.is_empty() && files.iter().all(|f| d.join(f).is_file()))
}

/// Every model the installed runtime knows (unfiltered).
pub fn all_models(state: &AppState) -> Vec<RuntimeModel> {
    let dir = runtime_dir(state);
    let version = state.db.get_setting("npu_version").ok().flatten().unwrap_or_default().trim_matches('"').to_string();
    let Some(list) = read_json(&dir, "model_list.json").or_else(|| flm_path(state).and_then(|p| read_json(p.parent()?, "model_list.json"))) else {
        return vec![];
    };
    let info = read_json(&dir, "model_info.json").unwrap_or(Value::Null);
    let curated = catalog();
    let installed = installed_models(state);
    let mut out = vec![];
    for (fam, sizes) in list["models"].as_object().cloned().unwrap_or_default() {
        for (sz, m) in sizes.as_object().cloned().unwrap_or_default() {
            let tag = format!("{fam}:{sz}");
            let labels: Vec<String> = m["label"].as_array().cloned().unwrap_or_default().iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect();
            let platforms: Vec<String> = m["supported_platforms"].as_array().cloned().unwrap_or_default().iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect();
            let files: Vec<String> = m["files"].as_array().cloned().unwrap_or_default().iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect();
            let size = info[&tag].as_array().map(|a| a.iter().map(|f| f["size"].as_u64().unwrap_or(0)).sum::<u64>()).filter(|s| *s > 0).unwrap_or(m["size"].as_u64().unwrap_or(0));
            let min_version = m["flm_min_version"].as_str().unwrap_or("").to_string();
            let version_ok = version.is_empty() || min_version.is_empty() || version_tuple(&version) >= version_tuple(&min_version);
            let has = |l: &str| labels.iter().any(|x| x == l);
            let non_chat = has("embeddings") || has("decision") || has("single-turn") || has("translation") || (has("transcription") && !has("reasoning") && !has("vision"));
            let name = m["name"].as_str().unwrap_or(&tag).to_string();
            let cur = curated.iter().find(|c| c.tag == tag);
            out.push(RuntimeModel {
                installed: installed.contains(&tag) || on_disk(state, &name, &files),
                tag: tag.clone(),
                name: cur.map(|c| c.name.to_string()).unwrap_or_else(|| name.trim_end_matches("-NPU2").replace('-', " ")),
                family: m.pointer("/details/family").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                params: m.pointer("/details/parameter_size").and_then(|x| x.as_str()).unwrap_or(&sz).to_string(),
                size,
                footprint: m["footprint"].as_f64().unwrap_or(0.0),
                vision: m["vlm"].as_bool().unwrap_or(false),
                audio: has("audio"),
                thinking: m.pointer("/details/think").and_then(|x| x.as_bool()).unwrap_or(false),
                tools: has("tool-calling"),
                context: m["default_context_length"].as_u64().unwrap_or(0),
                url: m["url"].as_str().unwrap_or("").to_string(),
                compatible: platforms.iter().any(|p| p == "aie2p") && version_ok,
                chat: !non_chat,
                recommended: cur.map(|c| c.recommended).unwrap_or(false),
                note: cur.map(|c| c.note.to_string()).unwrap_or_default(),
                repo: repo_of(m["url"].as_str().unwrap_or("")).unwrap_or_default(),
                page: repo_of(m["url"].as_str().unwrap_or("")).map(|r| format!("https://huggingface.co/FastFlowLM/{r}")).unwrap_or_default(),
                labels,
                min_version,
            });
        }
    }
    out.sort_by(|a, b| b.recommended.cmp(&a.recommended).then(a.footprint.partial_cmp(&b.footprint).unwrap_or(std::cmp::Ordering::Equal)));
    out
}

/// Models offered for download: those in the FastFlowLM XDNA 2 collection that run on this
/// machine's NPU, are general chat models and are not vision-only.
pub async fn available(state: &AppState) -> Available {
    let npu = info(false).await;
    if npu.vendor == "intel" {
        let models = if npu.present { crate::ovms::available(state).await } else { vec![] };
        return Available {
            models,
            ram_gb: npu.ram_gb.max(ram_gb()),
            version: setting_str(state, "ovms_version"),
            npu: Some(npu),
            collection: crate::ovms::COLLECTION_URL.into(),
            newer: vec![],
        };
    }
    let coll = collection(state).await;
    let coll_l: Vec<String> = coll.iter().map(|r| r.to_lowercase()).collect();
    let version = state.db.get_setting("npu_version").ok().flatten().unwrap_or_default().trim_matches('"').to_string();
    let all = all_models(state);
    let platform = if npu.platform.is_empty() { "aie2p".to_string() } else { npu.platform.clone() };
    let list = read_json(&runtime_dir(state), "model_list.json").unwrap_or(Value::Null);
    let platforms_of = |tag: &str| -> Vec<String> {
        tag.split_once(':')
            .and_then(|(f, z)| list.pointer(&format!("/models/{f}/{z}/supported_platforms")).and_then(|x| x.as_array()).cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|x| x.as_str().map(|s| s.to_string()))
            .collect()
    };
    let known: Vec<String> = all.iter().map(|m| m.repo.to_lowercase()).collect();
    let models: Vec<RuntimeModel> = if !npu.present || npu.arch == "xdna1" || npu.vendor != "amd" {
        vec![]
    } else {
        all.into_iter()
            .filter(|m| !m.repo.is_empty() && coll_l.contains(&m.repo.to_lowercase()))
            .filter(|m| platforms_of(&m.tag).iter().any(|p| *p == platform))
            .filter(|m| m.chat && !vision_only(&m.repo) && !special_purpose(&m.repo) && !m.labels.iter().any(|l| l == "speculative"))
            .collect()
    };
    let skip = |r: &str| {
        let l = r.to_lowercase();
        vision_only(r) || ["embedding", "d3-", "whisper", "transcript", "hy-mt", "translategemma", "safeguard", "dflash"].iter().any(|k| l.contains(k))
    };
    let newer: Vec<String> = coll.iter().filter(|r| !known.contains(&r.to_lowercase()) && !skip(r)).cloned().collect();
    Available { models, ram_gb: npu.ram_gb.max(ram_gb()), version, npu: Some(npu), collection: COLLECTION_URL.into(), newer }
}

/// Whether an NPU model tag accepts images (from model_list.json, with a name-based fallback).
pub fn is_vlm(state: &AppState, tag: &str) -> bool {
    if vendor() == "intel" {
        return false;
    }
    let dir = runtime_dir(state);
    if let Some(list) = read_json(&dir, "model_list.json") {
        if let Some((fam, sz)) = tag.split_once(':') {
            if let Some(v) = list.pointer(&format!("/models/{fam}/{sz}/vlm")).and_then(|x| x.as_bool()) {
                return v;
            }
        }
    }
    tag.starts_with("qwen3.5") || tag.starts_with("gemma4") || tag.starts_with("gemma3:4b") || tag.contains("vl") || tag.starts_with("minicpm-v")
}

fn model_meta(state: &AppState, tag: &str) -> Option<(String, u64)> {
    // The folder FastFlowLM downloads into is named after model_list.json's `name`.
    let list = read_json(&runtime_dir(state), "model_list.json")?;
    let (f, z) = tag.split_once(':')?;
    let name = list.pointer(&format!("/models/{f}/{z}/name"))?.as_str()?.to_string();
    let size = all_models(state).into_iter().find(|m| m.tag == tag).map(|m| m.size).unwrap_or(0);
    Some((name, size))
}

pub fn progress(state: &AppState, kind: &str, tag: &str, percent: Option<f64>, done: u64, total: u64, line: &str, finished: bool, error: &str) {
    state.emit(
        "npu-progress",
        json!({ "kind": kind, "tag": tag, "percent": percent, "done": done, "total": total, "line": line, "finished": finished, "error": error }),
    );
}

/// Download the portable FastFlowLM runtime for Windows into Hermio's data folder.
/// Download a file with byte progress (`npu-progress`, kind runtime / phase download).
pub async fn download(state: &AppState, url: &str, path: &Path, size_hint: u64) -> Result<u64> {
    let mut resp = http(60 * 30).get(url).send().await?.error_for_status()?;
    let total = resp.content_length().unwrap_or(size_hint);
    let mut file = std::fs::File::create(path)?;
    let mut done: u64 = 0;
    let mut last = std::time::Instant::now();
    while let Some(chunk) = resp.chunk().await? {
        use std::io::Write;
        file.write_all(&chunk)?;
        done += chunk.len() as u64;
        if last.elapsed() > Duration::from_millis(120) {
            last = std::time::Instant::now();
            let pct = if total > 0 { Some(done as f64 * 100.0 / total as f64) } else { None };
            progress(state, "runtime", "download", pct, done, total, "", false, "");
        }
    }
    Ok(done)
}

/// Unpack a zip into `target` (replacing it), reporting file progress.
pub async fn extract_zip(state: &AppState, zip_path: &Path, target: &Path) -> Result<()> {
    progress(state, "runtime", "extract", Some(0.0), 0, 0, "", false, "");
    let zp = zip_path.to_path_buf();
    let target = target.to_path_buf();
    let app = state.app.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<()> {
        use tauri::Emitter;
        if target.exists() {
            let _ = std::fs::remove_dir_all(&target);
        }
        std::fs::create_dir_all(&target)?;
        let mut z = zip::ZipArchive::new(std::fs::File::open(&zp)?)?;
        let n = z.len();
        let mut last = std::time::Instant::now();
        for i in 0..n {
            if last.elapsed() > Duration::from_millis(120) {
                last = std::time::Instant::now();
                let _ = app.emit(
                    "npu-progress",
                    json!({ "kind": "runtime", "tag": "extract", "percent": i as f64 * 100.0 / n as f64, "done": i, "total": n, "line": "", "finished": false, "error": "" }),
                );
            }
            let mut f = z.by_index(i)?;
            let Some(rel) = f.enclosed_name() else { continue };
            let out = target.join(rel);
            if f.is_dir() {
                std::fs::create_dir_all(&out)?;
            } else {
                if let Some(p) = out.parent() {
                    std::fs::create_dir_all(p)?;
                }
                let mut w = std::fs::File::create(&out)?;
                std::io::copy(&mut f, &mut w)?;
            }
        }
        Ok(())
    })
    .await??;
    Ok(())
}

/// Install (or update) the runtime that fits this PC's NPU.
pub async fn install_runtime(state: &AppState) -> Result<String> {
    let npu = info(false).await;
    if !npu.present {
        bail!("No NPU was found on this PC, so on-device AI isn't available.");
    }
    if npu.vendor == "intel" {
        return crate::ovms::install_runtime(state).await;
    }
    if npu.arch == "xdna1" {
        bail!("This NPU (XDNA 1) is not supported by FastFlowLM.");
    }
    let h = http(60 * 30);
    progress(state, "runtime", "check", None, 0, 0, "", false, "");
    let rel: serde_json::Value = h
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
                n.starts_with("fastflowlm_") && n.ends_with("_windows_amd64.zip")
            })
        })
        .ok_or_else(|| anyhow!("No Windows build found in the latest FastFlowLM release"))?;
    let url = asset["browser_download_url"].as_str().unwrap_or("").to_string();
    let name = asset["name"].as_str().unwrap_or("flm.zip").to_string();
    let dir = runtime_dir(state);
    std::fs::create_dir_all(runtime_base(state))?;
    let zip_path = runtime_base(state).join(&name);
    download(state, &url, &zip_path, asset["size"].as_u64().unwrap_or(0)).await?;
    stop_server(state).await;
    extract_zip(state, &zip_path, &dir).await?;
    let _ = std::fs::remove_file(&zip_path);
    if find_in(&dir, 3).is_none() {
        bail!("The download did not contain flm.exe");
    }
    state.db.set_setting("npu_version", &serde_json::to_string(&version)?)?;
    progress(state, "runtime", "done", Some(100.0), 0, 0, "", true, "");
    Ok(version)
}

fn parse_percent(line: &str) -> Option<f64> {
    let re = regex_lite::Regex::new(r"(\d{1,3}(?:\.\d+)?)\s*%").ok()?;
    re.captures_iter(line).last().and_then(|c| c[1].parse::<f64>().ok()).filter(|p| *p <= 100.0)
}

pub fn dir_size_pub(p: &Path) -> u64 {
    dir_size(p)
}

/// Download a model (runtime-specific `pull` process), with byte progress from `size_now`.
pub async fn run_pull(
    state: &AppState,
    tag: &str,
    mut child: tokio::process::Child,
    total: u64,
    size_now: impl Fn() -> u64 + Send + 'static,
) -> Result<()> {
    if let Some(pid) = child.id() {
        job::adopt(pid);
        RT.lock().unwrap().pulls.insert(tag.to_string(), pid);
    }
    progress(state, "model", tag, Some(0.0), 0, total, "Starting download…", false, "");
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let last_line = std::sync::Arc::new(Mutex::new(String::new()));
    let byte_mode = total > 0;
    if byte_mode {
        let (st, ll, app, tag2) = (stop.clone(), last_line.clone(), state.app.clone(), tag.to_string());
        tokio::spawn(async move {
            use tauri::Emitter;
            let started = std::time::Instant::now();
            let mut first: Option<u64> = None;
            while !st.load(std::sync::atomic::Ordering::Relaxed) {
                let n = size_now().min(total);
                let base = *first.get_or_insert(n);
                let secs = started.elapsed().as_secs_f64().max(1.0);
                let rate = (n.saturating_sub(base)) as f64 / secs;
                let line = ll.lock().unwrap().clone();
                let _ = app.emit(
                    "npu-progress",
                    json!({ "kind": "model", "tag": tag2, "percent": n as f64 * 100.0 / total as f64, "done": n, "total": total,
                            "rate": rate, "line": line, "finished": false, "error": "" }),
                );
                tokio::time::sleep(Duration::from_millis(600)).await;
            }
        });
    }
    let mut out = child.stdout.take().unwrap();
    let mut err = child.stderr.take().unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
    let tx2 = tx.clone();
    tokio::spawn(async move {
        let mut b = [0u8; 4096];
        while let Ok(n) = out.read(&mut b).await {
            if n == 0 || tx.send(b[..n].to_vec()).is_err() {
                break;
            }
        }
    });
    tokio::spawn(async move {
        let mut b = [0u8; 4096];
        while let Ok(n) = err.read(&mut b).await {
            if n == 0 || tx2.send(b[..n].to_vec()).is_err() {
                break;
            }
        }
    });
    let mut buf = String::new();
    let mut last_pct: Option<f64> = None;
    let mut tail: Vec<String> = vec![];
    let mut last_emit = std::time::Instant::now();
    while let Some(bytes) = rx.recv().await {
        buf.push_str(&String::from_utf8_lossy(&bytes));
        while let Some(i) = buf.find(|c| c == '\r' || c == '\n') {
            let line: String = buf.drain(..=i).collect();
            let line = strip_ansi(line.trim());
            if line.is_empty() {
                continue;
            }
            if let Some(p) = parse_percent(&line) {
                last_pct = Some(p);
            }
            tail.push(line.clone());
            if tail.len() > 8 {
                tail.remove(0);
            }
            *last_line.lock().unwrap() = line.clone();
            if !byte_mode && last_emit.elapsed() > Duration::from_millis(150) {
                last_emit = std::time::Instant::now();
                progress(state, "model", tag, last_pct, 0, 0, &line, false, "");
            }
        }
    }
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    let st = child.wait().await?;
    let cancelled = RT.lock().unwrap().pulls.remove(tag).is_none();
    if !st.success() {
        let msg = if cancelled { "Cancelled".to_string() } else { tail.join("\n") };
        progress(state, "model", tag, last_pct, 0, 0, "", true, &msg);
        bail!("Download of {tag} failed{}", if msg.is_empty() { String::new() } else { format!(":\n{msg}") });
    }
    Ok(())
}

/// Download a model for this PC's runtime.
pub async fn pull(state: &AppState, tag: &str) -> Result<()> {
    let npu = info(false).await;
    if !npu.present || (!npu.supported && npu.arch == "xdna1") {
        bail!("On-device AI isn't available on this PC.");
    }
    if npu.vendor == "intel" {
        crate::ovms::pull(state, tag).await?;
        progress(state, "model", tag, Some(100.0), 0, 0, "", true, "");
        return Ok(());
    }
    let exe = flm_path(state).ok_or_else(|| anyhow!("Install the NPU runtime first"))?;
    if !tag.chars().all(|c| c.is_ascii_alphanumeric() || ".:-_".contains(c)) {
        bail!("Invalid model tag");
    }
    let child = command(state, &exe)
        .arg("pull")
        .arg(tag)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("Could not start {}", exe.display()))?;
    let (name, total) = model_meta(state, tag).unwrap_or_default();
    let dirs = model_dirs_for(state, &name);
    run_pull(state, tag, child, total, move || dirs.iter().map(|d| dir_size(d)).max().unwrap_or(0)).await?;
    let mut list = installed_models(state);
    if !list.iter().any(|t| t == tag) {
        list.push(tag.to_string());
        set_models(state, &list)?;
    }
    progress(state, "model", tag, Some(100.0), 0, 0, "", true, "");
    Ok(())
}

fn strip_ansi(s: &str) -> String {
    regex_lite::Regex::new(r"\x1b\[[0-9;?]*[A-Za-z]").map(|re| re.replace_all(s, "").to_string()).unwrap_or_else(|_| s.to_string())
}

pub fn cancel_pull(tag: &str) {
    let pid = RT.lock().unwrap().pulls.remove(tag);
    if let Some(pid) = pid {
        kill_pid(pid);
    }
}

fn kill_pid(pid: u32) {
    #[cfg(windows)]
    {
        let mut c = std::process::Command::new("taskkill");
        use std::os::windows::process::CommandExt;
        c.args(["/PID", &pid.to_string(), "/T", "/F"]).creation_flags(0x0800_0000);
        let _ = c.status();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("kill").arg(pid.to_string()).status();
    }
}

pub async fn remove_model(state: &AppState, tag: &str) -> Result<()> {
    if vendor() == "intel" {
        if crate::ovms::serving().as_deref() == Some(tag) {
            stop_server(state).await;
        }
        return crate::ovms::remove(state, tag);
    }
    // Files of a loaded model are locked on Windows.
    if is_running() {
        stop_server(state).await;
    }
    if let Some(exe) = flm_path(state) {
        let mut o = command(state, &exe).arg("remove").arg(tag).output().await?;
        if !o.status.success() {
            o = command(state, &exe).arg("rm").arg(tag).output().await?;
        }
        if !o.status.success() {
            let e = String::from_utf8_lossy(&o.stderr).to_string() + &String::from_utf8_lossy(&o.stdout);
            if !e.to_lowercase().contains("not found") && !e.to_lowercase().contains("not installed") {
                bail!("flm remove {tag}: {}", e.trim().chars().take(300).collect::<String>());
            }
        }
    }
    // FastFlowLM keeps no record of models it didn't download itself: delete the folder too.
    if let Some((_, name, _)) = flm_names(state).into_iter().find(|(t, _, _)| t == tag) {
        if !name.is_empty() && !name.contains("..") && !name.contains('/') && !name.contains('\\') {
            for d in model_dirs_for(state, &name) {
                if d.is_dir() {
                    std::fs::remove_dir_all(&d).with_context(|| format!("Couldn't delete {}", d.display()))?;
                }
            }
        }
    }
    let list: Vec<String> = state
        .db
        .get_setting("npu_models")
        .ok()
        .flatten()
        .and_then(|v| serde_json::from_str::<Vec<String>>(&v).ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|t| t != tag)
        .collect();
    set_models(state, &list)
}

/// Delete a model's files and download it again (e.g. when files are damaged or incomplete).
pub async fn redownload(state: &AppState, tag: &str) -> Result<()> {
    if is_running() {
        stop_server(state).await;
    }
    remove_model(state, tag).await?;
    pull(state, tag).await
}

/// Files in the models folder this PC's runtime can't use (other formats), for a clear message.
pub fn foreign_files(state: &AppState) -> Vec<String> {
    let dir = models_dir(state);
    let intel = vendor() == "intel";
    let mut kinds: BTreeMap<&'static str, Vec<String>> = BTreeMap::new();
    fn walk(p: &Path, depth: u32, intel: bool, kinds: &mut BTreeMap<&'static str, Vec<String>>) {
        let Ok(rd) = std::fs::read_dir(p) else { return };
        for e in rd.flatten() {
            let path = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            let l = name.to_lowercase();
            if path.is_dir() {
                if depth < 3 {
                    walk(&path, depth + 1, intel, kinds);
                }
                continue;
            }
            let kind = if l.ends_with(".gguf") {
                Some("GGUF (llama.cpp / LM Studio / Ollama)")
            } else if l.ends_with(".safetensors") {
                Some("Safetensors (Hugging Face / PyTorch)")
            } else if !intel && l == "openvino_model.xml" {
                Some("OpenVINO (for Intel NPUs)")
            } else if intel && l.ends_with(".q4nx") {
                Some("FastFlowLM (for AMD NPUs)")
            } else {
                None
            };
            if let Some(k) = kind {
                let shown = if l == "openvino_model.xml" { path.parent().and_then(|x| x.file_name()).map(|x| x.to_string_lossy().to_string()).unwrap_or(name) } else { name };
                let v = kinds.entry(k).or_default();
                if !v.contains(&shown) {
                    v.push(shown);
                }
            }
        }
    }
    walk(&dir, 0, intel, &mut kinds);
    kinds
        .into_iter()
        .map(|(k, v)| {
            let more = if v.len() > 3 { format!(" +{}", v.len() - 3) } else { String::new() };
            format!("{k}: {}{more}", v.into_iter().take(3).collect::<Vec<_>>().join(", "))
        })
        .collect()
}

/// The model the built-in NPU provider uses by default.
pub fn default_model(state: &AppState) -> String {
    let pref = setting_str(state, "npu_model");
    let models = installed_models(state);
    if models.contains(&pref) {
        pref
    } else {
        models.first().cloned().unwrap_or_default()
    }
}

/// Make sure the runtime answers on our port with `model` available (starting it if needed).
pub async fn ensure_server(state: &AppState, model: &str) -> Result<()> {
    touch();
    if vendor() == "intel" {
        let m = if model.is_empty() { default_model(state) } else { model.to_string() };
        return crate::ovms::ensure_server(state, &m).await;
    }
    if alive(state).await {
        return Ok(());
    }
    let exe = flm_path(state).ok_or_else(|| anyhow!("The NPU runtime isn't installed. Open Settings → AI assistant → On-device (NPU)."))?;
    {
        let mut rt = RT.lock().unwrap();
        if let Some(c) = rt.server.as_mut() {
            if c.try_wait().ok().flatten().is_none() {
                // still starting
            } else {
                rt.server = None;
            }
        }
        if rt.server.is_none() {
            let ctx = state.db.get_setting("npu_ctx").ok().flatten().and_then(|v| v.trim_matches('"').parse::<u32>().ok()).unwrap_or(32768);
            // FastFlowLM's own messages go to a log file, so a crash can be explained.
            let log = std::fs::File::create(server_log(state)).ok();
            let (out, err) = match log.as_ref().and_then(|f| f.try_clone().ok().map(|g| (f.try_clone().ok(), g))) {
                Some((Some(a), b)) => (std::process::Stdio::from(a), std::process::Stdio::from(b)),
                _ => (std::process::Stdio::null(), std::process::Stdio::null()),
            };
            let child = command(state, &exe)
                .args(["serve", "--port", &port(state).to_string(), "--ctx-len", &ctx.to_string(), "--pmode", &pmode(state)])
                .stdout(out)
                .stderr(err)
                .kill_on_drop(true)
                .spawn()
                .with_context(|| format!("Could not start {}", exe.display()))?;
            if let Some(pid) = child.id() {
                job::adopt(pid);
            }
            rt.server = Some(child);
        }
    }
    for _ in 0..240 {
        tokio::time::sleep(Duration::from_millis(250)).await;
        if alive(state).await {
            state.emit("npu-state", json!({ "running": true }));
            server_started(state);
            return Ok(());
        }
        let exited = {
            let mut rt = RT.lock().unwrap();
            match rt.server.as_mut().map(|c| c.try_wait()) {
                Some(Ok(Some(code))) => {
                    rt.server = None;
                    Some(code)
                }
                _ => None,
            }
        };
        if let Some(code) = exited {
            bail!("The NPU runtime stopped ({code}). Check that the AMD NPU driver (32.0.203.311 or newer) is installed.");
        }
    }
    bail!("The NPU runtime did not start within 60 seconds")
}

fn server_log(state: &AppState) -> PathBuf {
    state.db.data_dir.join("npu-server.log")
}

/// The last lines FastFlowLM printed (for error messages), if it stopped unexpectedly.
pub fn server_log_tail(state: &AppState) -> String {
    let Ok(t) = std::fs::read_to_string(server_log(state)) else { return String::new() };
    let t = strip_ansi(&t);
    let lines: Vec<&str> = t.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
    if lines.is_empty() {
        return String::new();
    }
    let tail = lines[lines.len().saturating_sub(4)..].join(" | ");
    format!(" — FastFlowLM said: {}", tail.chars().take(400).collect::<String>())
}

/// Quick health check of the NPU server.
pub async fn alive_now(state: &AppState) -> bool {
    if vendor() == "intel" {
        return crate::ovms::alive(state).await;
    }
    alive(state).await
}

/// Drop our handle to a server process that has died, so the next start launches a new one.
pub fn forget_server() {
    let mut rt = RT.lock().unwrap();
    if let Some(c) = rt.server.as_mut() {
        if c.try_wait().ok().flatten().is_some() {
            rt.server = None;
        }
    }
}

/// Keep a started runtime process so it can be stopped (and is killed with Hermio).
pub fn adopt_server(child: tokio::process::Child) {
    if let Some(pid) = child.id() {
        job::adopt(pid);
    }
    RT.lock().unwrap().server = Some(child);
}

/// Exit status if the runtime process ended on its own.
pub fn server_exited() -> Option<String> {
    let mut rt = RT.lock().unwrap();
    match rt.server.as_mut().map(|c| c.try_wait()) {
        Some(Ok(Some(code))) => {
            rt.server = None;
            Some(code.to_string())
        }
        _ => None,
    }
}

pub fn is_running() -> bool {
    RT.lock().unwrap().server.is_some()
}

/// The NPU just woke up: handle mail that waited for group suggestions (after current work).
pub fn server_started(state: &AppState) {
    use tauri::Manager;
    let st = state.app.state::<std::sync::Arc<AppState>>().inner().clone();
    tauri::async_runtime::spawn(async move {
        // Wait until the user has left the NPU alone for 15 s (up to 5 minutes), so queued
        // sorting never competes with what they're doing.
        let mut quiet = 0;
        for _ in 0..600 {
            tokio::time::sleep(Duration::from_millis(500)).await;
            if !is_running() {
                return;
            }
            quiet = if BUSY.load(std::sync::atomic::Ordering::Relaxed) == 0 { quiet + 1 } else { 0 };
            if quiet >= 30 {
                break;
            }
        }
        if quiet < 30 {
            return;
        }
        if let Err(e) = crate::ai::process_suggest_queue(&st).await {
            eprintln!("suggest queue: {e:#}");
        }
    });
}

pub async fn stop_server(_state: &AppState) {
    crate::ovms::clear_serving();
    let child = RT.lock().unwrap().server.take();
    if let Some(mut c) = child {
        if let Some(pid) = c.id() {
            kill_pid(pid);
        }
        let _ = c.kill().await;
    }
}

/// Release the model right away (window hidden to the tray). Downloads keep going.
pub fn hibernate() {
    crate::ovms::clear_serving();
    let child = RT.lock().unwrap().server.take();
    if let Some(c) = child {
        if let Some(pid) = c.id() {
            kill_pid(pid);
        }
    }
}

/// Synchronous stop for app exit.
pub fn shutdown() {
    let mut rt = RT.lock().unwrap();
    if let Some(c) = rt.server.take() {
        if let Some(pid) = c.id() {
            kill_pid(pid);
        }
    }
    for (_, pid) in std::mem::take(&mut rt.pulls) {
        kill_pid(pid);
    }
}


// ---------------- storage folders ----------------

#[derive(Debug, Clone, Serialize)]
pub struct Dirs {
    /// Models recognised in the models folder.
    pub found: Vec<String>,
    /// Model files of other formats found there (can't be used by this PC's runtime).
    pub foreign: Vec<String>,
    pub runtime: String,
    pub runtime_default: String,
    pub runtime_size: u64,
    pub models: String,
    pub models_default: String,
    pub models_size: u64,
}

pub async fn dirs(state: &AppState) -> Dirs {
    let (r, m) = (runtime_base(state), custom_models_dir(state).unwrap_or_else(|| models_dir(state)));
    let (r2, m2) = (r.clone(), m.clone());
    let (rs, ms) = tokio::task::spawn_blocking(move || (dir_size(&r2), dir_size(&m2))).await.unwrap_or((0, 0));
    forget_scan();
    let st = state.app.clone();
    let (found, foreign) = tokio::task::spawn_blocking(move || {
        use tauri::Manager;
        let s = st.state::<std::sync::Arc<AppState>>();
        (installed_models(&s), foreign_files(&s))
    })
    .await
    .unwrap_or_default();
    Dirs {
        found,
        foreign,
        runtime: r.display().to_string(),
        runtime_default: default_runtime_base(state).display().to_string(),
        runtime_size: rs,
        models: m.display().to_string(),
        models_default: default_models_dir(state).display().to_string(),
        models_size: ms,
    }
}

fn same_path(a: &Path, b: &Path) -> bool {
    let n = |p: &Path| p.display().to_string().trim_end_matches(['\\', '/']).to_lowercase();
    n(a) == n(b)
}

fn inside(child: &Path, parent: &Path) -> bool {
    let c = child.display().to_string().to_lowercase().replace('\\', "/");
    let p = parent.display().to_string().to_lowercase().replace('\\', "/");
    let p = p.trim_end_matches('/');
    c.starts_with(&format!("{p}/"))
}

fn count_files(p: &Path) -> u64 {
    let Ok(rd) = std::fs::read_dir(p) else { return 0 };
    rd.flatten().map(|e| if e.path().is_dir() { count_files(&e.path()) } else { 1 }).sum()
}

/// Move everything in `from` into `to`. Same drive → rename; otherwise copy and delete.
fn move_tree(state: &AppState, from: &Path, to: &Path, tag: &str) -> Result<()> {
    if !from.is_dir() {
        return Ok(());
    }
    std::fs::create_dir_all(to)?;
    let total = count_files(from).max(1);
    let mut done = 0u64;
    fn walk(state: &AppState, from: &Path, to: &Path, tag: &str, done: &mut u64, total: u64) -> Result<()> {
        for e in std::fs::read_dir(from)?.flatten() {
            let src = e.path();
            let dst = to.join(e.file_name());
            if std::fs::rename(&src, &dst).is_ok() {
                *done += if dst.is_dir() { count_files(&dst) } else { 1 };
            } else if src.is_dir() {
                std::fs::create_dir_all(&dst)?;
                walk(state, &src, &dst, tag, done, total)?;
                let _ = std::fs::remove_dir(&src);
            } else {
                std::fs::copy(&src, &dst).with_context(|| format!("copying {}", src.display()))?;
                let _ = std::fs::remove_file(&src);
                *done += 1;
            }
            state.emit("npu-progress", json!({ "kind": "move", "tag": tag, "percent": (*done as f64 * 100.0 / total as f64).min(100.0), "done": *done, "total": total, "line": "", "finished": false, "error": "" }));
        }
        Ok(())
    }
    walk(state, from, to, tag, &mut done, total)?;
    let _ = std::fs::remove_dir(from);
    Ok(())
}

/// Change where the runtime and/or the models live. An empty string means "back to the default".
pub async fn set_dirs(state: &AppState, runtime: Option<String>, models: Option<String>, move_files: bool) -> Result<Dirs> {
    if !RT.lock().unwrap().pulls.is_empty() {
        bail!("Wait for the model downloads to finish before moving folders.");
    }
    let old_r = runtime_base(state);
    let old_m = models_dir(state);
    let new_r = match &runtime { Some(v) if !v.trim().is_empty() => PathBuf::from(v.trim()), Some(_) => default_runtime_base(state), None => old_r.clone() };
    // Models default depends on vendor, so resolve it after clearing the setting.
    let new_m = match &models { Some(v) if !v.trim().is_empty() => PathBuf::from(v.trim()), Some(_) => default_models_dir(state), None => old_m.clone() };
    for (a, b) in [(&new_r, &old_r), (&new_m, &old_m)] {
        if !same_path(a, b) && (inside(a, b) || inside(b, a)) {
            bail!("The new folder can't be inside the old one (or the other way round).");
        }
    }
    if inside(&new_m, &new_r) || inside(&new_r, &new_m) || (same_path(&new_r, &new_m)) {
        bail!("Keep the runtime and the models in separate folders.");
    }
    for p in [&new_r, &new_m] {
        std::fs::create_dir_all(p).with_context(|| format!("Can't use {}", p.display()))?;
        let probe = p.join(".hermio-write-test");
        std::fs::write(&probe, b"ok").with_context(|| format!("Hermio can't write to {}", p.display()))?;
        let _ = std::fs::remove_file(probe);
    }
    // Nothing may hold files open while they move.
    stop_server(state).await;
    if move_files {
        let st = state.app.clone();
        let jobs: Vec<(PathBuf, PathBuf, &str)> = [(old_r.clone(), new_r.clone(), "runtime"), (old_m.clone(), new_m.clone(), "models")]
            .into_iter()
            .filter(|(a, b, _)| !same_path(a, b))
            .collect();
        let jobs: Vec<(PathBuf, PathBuf, String)> = jobs.into_iter().map(|(a, b, t)| (a, b, t.to_string())).collect();
        let res = tokio::task::spawn_blocking(move || -> Result<()> {
            use tauri::Manager;
            let s = st.state::<std::sync::Arc<AppState>>();
            for (a, b, t) in jobs {
                move_tree(&s, &a, &b, &t)?;
            }
            Ok(())
        })
        .await
        .map_err(|e| anyhow!("{e}"))?;
        if let Err(e) = res {
            state.emit("npu-progress", json!({ "kind": "move", "tag": "", "finished": true, "error": format!("{e:#}") }));
            return Err(e);
        }
    }
    let save = |k: &str, p: &Path, def: &Path| -> Result<()> {
        let v = if same_path(p, def) { String::new() } else { p.display().to_string() };
        state.db.set_setting(k, &serde_json::to_string(&v)?)
    };
    save("npu_runtime_dir", &new_r, &default_runtime_base(state))?;
    save("npu_models_dir", &new_m, &default_models_dir(state))?;
    state.emit("npu-progress", json!({ "kind": "move", "tag": "", "finished": true, "error": "" }));
    Ok(dirs(state).await)
}

#[cfg(test)]
mod tests {
    #[test]
    fn percent() {
        assert_eq!(super::parse_percent("Downloading  45.5% |####   | 1.2GB/2.6GB"), Some(45.5));
        assert_eq!(super::parse_percent("[3/7] model.bin  100%"), Some(100.0));
        assert_eq!(super::parse_percent("no progress"), None);
        assert_eq!(super::strip_ansi("\u{1b}[32m50%\u{1b}[0m"), "50%");
    }
}

#[cfg(test)]
mod detect_tests {
    #[test]
    fn paths() {
        use std::path::Path;
        assert!(super::inside(Path::new("D:\\AI\\models\\x"), Path::new("D:\\AI\\models")));
        assert!(!super::inside(Path::new("D:\\AI\\models2"), Path::new("D:\\AI\\models")));
        assert!(super::same_path(Path::new("D:\\AI\\"), Path::new("d:\\ai")));
    }
    use super::*;
    #[test]
    fn arch_and_driver() {
        assert_eq!(arch_of("AMD Ryzen AI 7 PRO 360 w/ Radeon 880M"), "xdna2");
        assert_eq!(arch_of("AMD Ryzen AI 9 HX 370 w/ Radeon 890M"), "xdna2");
        assert_eq!(arch_of("AMD RYZEN AI MAX+ 395 w/ Radeon 8060S"), "xdna2");
        assert_eq!(arch_of("AMD Ryzen 7 7840U w/ Radeon 780M Graphics"), "xdna1");
        assert_eq!(arch_of("AMD Ryzen 9 8945HS"), "xdna1");
        assert_eq!(arch_of("Intel(R) Core(TM) Ultra 7 155H"), "intel");
        assert_eq!(arch_of("Intel(R) Core(TM) Ultra 5 228V"), "intel");
        assert_eq!(arch_of("Intel(R) Core(TM) i7-1165G7"), "unknown");
        assert!(driver_ok("32.0.203.311"));
        assert!(driver_ok("32.0.203.314"));
        assert!(!driver_ok("32.0.201.204"));
        assert!(!driver_ok("10.1109.8.100"));
        assert_eq!(repo_of("https://huggingface.co/FastFlowLM/Qwen3.5-4B-NPU2/resolve/flm_q4k_high_precision").as_deref(), Some("Qwen3.5-4B-NPU2"));
        assert!(vision_only("Qwen3-VL-4B-Instruct-NPU2") && vision_only("MiniCPM-OCR-1.0-NPU2") && vision_only("medgemma-4b-it-NPU2"));
        assert!(!vision_only("Qwen3.5-4B-NPU2") && !vision_only("Gemma4-E4B-IT-NPU2"));
        assert!(special_purpose("Translategemma-4B-Instruct-NPU2") && special_purpose("LFM2-2.6B-Transcript-NPU2") && !special_purpose("Qwen3-4B-NPU2"));
    }
}
