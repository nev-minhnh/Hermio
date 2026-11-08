//! Background sync orchestration.

use crate::models::Account;
use crate::state::AppState;
use crate::{calendar, imap, pop3};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

// ---------------- sync log ----------------

#[derive(Debug, Clone, serde::Serialize)]
pub struct LogLine {
    /// Unix time (ms).
    pub ts: i64,
    pub account_id: i64,
    pub account: String,
    /// "info", "ok", "warn" or "error".
    pub level: String,
    pub message: String,
}

static LOG: std::sync::Mutex<std::collections::VecDeque<LogLine>> = std::sync::Mutex::new(std::collections::VecDeque::new());

pub fn log_file(state: &AppState) -> std::path::PathBuf {
    state.db.data_dir.join("sync.log")
}

/// Record a sync step: kept in memory (last 600 lines, shown in the sync panel), sent to the UI
/// live and appended to `sync.log` in the data folder.
pub fn log(state: &AppState, a: &Account, level: &str, message: impl Into<String>) {
    let line = LogLine {
        ts: chrono::Utc::now().timestamp_millis(),
        account_id: a.id,
        account: a.email.clone(),
        level: level.into(),
        message: message.into(),
    };
    {
        let mut l = LOG.lock().unwrap();
        l.push_back(line.clone());
        while l.len() > 600 {
            l.pop_front();
        }
    }
    let path = log_file(state);
    // Keep the file small: start over past ~2 MB.
    if std::fs::metadata(&path).map(|m| m.len() > 2_000_000).unwrap_or(false) {
        let _ = std::fs::rename(&path, path.with_extension("old.log"));
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        use std::io::Write;
        let t = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
        let _ = writeln!(f, "{t} [{}] {} — {}", line.level, line.account, line.message);
    }
    state.emit("sync-log", &line);
}

pub fn log_lines() -> Vec<LogLine> {
    LOG.lock().unwrap().iter().cloned().collect()
}

pub fn log_clear() {
    LOG.lock().unwrap().clear();
}

pub async fn sync_one(state: Arc<AppState>, account_id: i64) {
    let lock = state.lock_for(account_id);
    let Ok(_guard) = lock.try_lock() else { return }; // already syncing
    let Ok(a) = state.db.account(account_id) else { return };
    if !a.enabled {
        return;
    }
    state.emit("sync-state", json!({ "account_id": a.id, "syncing": true }));
    let started = std::time::Instant::now();
    log(&state, &a, "info", format!("Sync started ({} · {})", a.protocol.to_uppercase(), if a.auth == "oauth_google" { "Google sign-in".to_string() } else { format!("{}:{}", a.in_host, a.in_port) }));
    let res = run(&state, &a).await;
    let err = match &res {
        Ok(n) => {
            log(&state, &a, "ok", format!("Sync finished in {:.1}s · {n} new unread", started.elapsed().as_secs_f32()));
            String::new()
        }
        Err(e) => {
            log(&state, &a, "error", format!("Sync failed after {:.1}s: {e:#}", started.elapsed().as_secs_f32()));
            format!("{e:#}")
        }
    };
    let _ = state.db.set_account_status(a.id, &err);
    if a.auth == "oauth_google" && a.calendar_sync {
        if let Err(e) = calendar::sync_account(&state, &a).await {
            eprintln!("calendar sync: {e:#}");
            state.emit("sync-warning", json!({ "account_id": a.id, "message": format!("Calendar: {e:#}") }));
        }
    }
    state.emit("sync-state", json!({ "account_id": a.id, "syncing": false, "error": err }));
    state.changed(a.id);
}

async fn run(state: &AppState, a: &Account) -> anyhow::Result<usize> {
    match a.protocol.as_str() {
        "pop3" => pop3::sync_account(state, a).await,
        _ => imap::sync_account(state, a).await,
    }
}

pub async fn sync_all(state: Arc<AppState>) {
    let Ok(accounts) = state.db.accounts() else { return };
    let mut handles = vec![];
    for a in accounts.into_iter().filter(|a| a.enabled) {
        let st = state.clone();
        handles.push(tauri::async_runtime::spawn(async move { sync_one(st, a.id).await }));
    }
    for h in handles {
        let _ = h.await;
    }
    calendar::sync_all_ics(&state).await;
    // Optional: let the assistant suggest groups for new mail no rule matched. Runs on its own so
    // it never holds up sync or new-mail notifications.
    let st = state.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = crate::ai::suggest_groups(&st).await {
            eprintln!("ai suggestions: {e:#}");
        }
    });
}

/// Periodic sync loop. Interval comes from the `sync_interval` setting (minutes, 0 = manual).
pub fn start_scheduler(state: Arc<AppState>) {
    // Scheduled sends are checked every 20 seconds, independent of the mail sync interval.
    let st = state.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(20)).await;
            crate::commands::process_outbox(st.clone()).await;
        }
    });
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(2)).await;
        sync_all(state.clone()).await;
        loop {
            let minutes = state
                .db
                .get_setting("sync_interval")
                .ok()
                .flatten()
                .and_then(|v| v.trim_matches('"').parse::<u64>().ok())
                .unwrap_or(5);
            if minutes == 0 {
                tokio::time::sleep(Duration::from_secs(60)).await;
                continue;
            }
            tokio::time::sleep(Duration::from_secs(minutes * 60)).await;
            sync_all(state.clone()).await;
        }
    });
}
