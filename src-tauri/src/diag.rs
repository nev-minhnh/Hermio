//! Resource use (Settings → About): memory and CPU time of Hermio and its WebView2 processes, so
//! "Hermio is heavy" can be measured instead of guessed. Plus WebView2 memory trimming while the
//! window is hidden in the tray.

use serde::Serialize;

#[derive(Serialize, Clone, Default)]
pub struct Proc {
    pub pid: u32,
    pub name: String,
    /// Memory in use right now (working set), bytes.
    pub working_set: u64,
    /// Memory committed to this process alone, bytes.
    pub private: u64,
    /// CPU time used since it started, milliseconds.
    pub cpu_ms: u64,
}

#[derive(Serialize, Default)]
pub struct Usage {
    pub processes: Vec<Proc>,
    pub total_working_set: u64,
    pub total_private: u64,
    pub total_cpu_ms: u64,
}

#[cfg(windows)]
pub fn usage() -> Usage {
    use std::collections::{HashMap, HashSet};
    use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS};
    use windows_sys::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX};
    use windows_sys::Win32::System::Threading::{GetCurrentProcessId, GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

    let me = unsafe { GetCurrentProcessId() };
    // pid → (parent, name)
    let mut all: HashMap<u32, (u32, String)> = HashMap::new();
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap != INVALID_HANDLE_VALUE {
            let mut e: PROCESSENTRY32W = std::mem::zeroed();
            e.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
            let mut ok = Process32FirstW(snap, &mut e) != 0;
            while ok {
                let len = e.szExeFile.iter().position(|c| *c == 0).unwrap_or(e.szExeFile.len());
                all.insert(e.th32ProcessID, (e.th32ParentProcessID, String::from_utf16_lossy(&e.szExeFile[..len])));
                ok = Process32NextW(snap, &mut e) != 0;
            }
            CloseHandle(snap);
        }
    }
    // Hermio and everything it started (the WebView2 browser, renderers, GPU…), recursively.
    let mut ours: HashSet<u32> = HashSet::from([me]);
    loop {
        let before = ours.len();
        for (pid, (parent, _)) in &all {
            if ours.contains(parent) && *pid != *parent {
                ours.insert(*pid);
            }
        }
        if ours.len() == before {
            break;
        }
    }
    let ft = |f: FILETIME| ((f.dwHighDateTime as u64) << 32 | f.dwLowDateTime as u64) / 10_000;
    let mut out = Usage::default();
    for pid in ours {
        let name = all.get(&pid).map(|x| x.1.clone()).unwrap_or_else(|| "Hermio".into());
        let mut p = Proc { pid, name, ..Default::default() };
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if h.is_null() {
                continue;
            }
            let mut m: PROCESS_MEMORY_COUNTERS_EX = std::mem::zeroed();
            m.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
            if GetProcessMemoryInfo(h, &mut m as *mut _ as *mut PROCESS_MEMORY_COUNTERS, m.cb) != 0 {
                p.working_set = m.WorkingSetSize as u64;
                p.private = m.PrivateUsage as u64;
            }
            let (mut c, mut x, mut k, mut u): (FILETIME, FILETIME, FILETIME, FILETIME) = std::mem::zeroed();
            if GetProcessTimes(h, &mut c, &mut x, &mut k, &mut u) != 0 {
                p.cpu_ms = ft(k) + ft(u);
            }
            CloseHandle(h);
        }
        out.total_working_set += p.working_set;
        out.total_private += p.private;
        out.total_cpu_ms += p.cpu_ms;
        out.processes.push(p);
    }
    out.processes.sort_by(|a, b| b.private.cmp(&a.private));
    out
}

#[cfg(not(windows))]
pub fn usage() -> Usage {
    Usage::default()
}

#[tauri::command]
pub fn resource_usage() -> Usage {
    usage()
}

/// Hidden in the tray: ask WebView2 to keep as little memory as it can (caches are dropped; the
/// page keeps running, so the unread badge and new mail still update). Back to normal when shown.
#[cfg(windows)]
pub fn low_memory(w: &tauri::WebviewWindow, low: bool) {
    let _ = w.with_webview(move |wv| unsafe {
        use webview2_com::Microsoft::Web::WebView2::Win32::{
            ICoreWebView2_19, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL,
        };
        use windows::core::Interface;
        if let Ok(core) = wv.controller().CoreWebView2() {
            if let Ok(c19) = core.cast::<ICoreWebView2_19>() {
                let _ = c19.SetMemoryUsageTargetLevel(if low { COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW } else { COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL });
            }
        }
    });
}
#[cfg(not(windows))]
pub fn low_memory(_w: &tauri::WebviewWindow, _low: bool) {}
