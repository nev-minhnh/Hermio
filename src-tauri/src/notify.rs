//! New-mail notifications (Windows toasts), sent from the back end.
//!
//! They used to be raised by the web page when it heard the "new-mail" event, which loses them
//! whenever the page isn't listening: while it is still loading at start-up (the first sync runs
//! two seconds after launch), or while WebView2 throttles the hidden window in the tray. Now the
//! sync itself shows them, once per mailbox and sync run.

use crate::state::AppState;
use tauri_plugin_notification::NotificationExt;

/// One new message worth telling the user about.
#[derive(Debug, Clone)]
pub struct NewMail {
    pub from: String,
    pub subject: String,
}

/// Mail sorted into a group that is hidden from the inbox is only announced when the user
/// asked for it (Settings → Mail → "Notify for mail sorted into groups").
pub fn include_grouped(state: &AppState) -> bool {
    setting_bool(state, "notify_groups", false)
}

fn setting_bool(state: &AppState, key: &str, default: bool) -> bool {
    state
        .db
        .get_setting(key)
        .ok()
        .flatten()
        .map(|v| match v.trim().trim_matches('"') {
            "false" | "0" => false,
            "true" | "1" => true,
            _ => default,
        })
        .unwrap_or(default)
}

/// Show the toasts for what one sync run of one mailbox brought in: one toast per message for
/// a few messages, a single summary for a burst (Windows queues and drops floods of toasts).
pub fn new_mail(state: &AppState, account: &str, items: &[NewMail]) {
    if items.is_empty() || !setting_bool(state, "notifications", true) {
        return;
    }
    let show = |title: &str, body: &str| {
        if let Err(e) = state.app.notification().builder().title(title).body(body).show() {
            crate::sync::log_plain(state, "warn", format!("Couldn't show a notification: {e}"));
        }
    };
    let subject = |m: &NewMail| if m.subject.trim().is_empty() { "(No subject)".to_string() } else { m.subject.clone() };
    if items.len() <= 3 {
        for m in items {
            let from = if m.from.trim().is_empty() { account } else { m.from.as_str() };
            show(from, &subject(m));
        }
    } else {
        let mut senders: Vec<&str> = vec![];
        for m in items {
            if !m.from.is_empty() && !senders.contains(&m.from.as_str()) {
                senders.push(&m.from);
            }
        }
        let names = if senders.len() > 3 { format!("{} and {} more", senders[..3].join(", "), senders.len() - 3) } else { senders.join(", ") };
        show(&format!("{} new messages · {account}", items.len()), &format!("From {names}"));
    }
}

/// Windows only shows toasts from an app whose AppUserModelID is registered. The installer's
/// Start-menu shortcut does that, but the portable copy (and an install whose shortcut was
/// removed) has none, so the toasts were silently dropped. Registering the ID under
/// HKCU\Software\Classes\AppUserModelId works for both and also gives the toast its name and icon.
#[cfg(windows)]
pub fn register_app_id(app_id: &str, data_dir: &std::path::Path) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let icon = data_dir.join("hermio-notify.png");
    if !icon.exists() {
        let _ = std::fs::create_dir_all(data_dir);
        let _ = std::fs::write(&icon, include_bytes!("../icons/128x128@2x.png"));
    }
    let key = format!(r"HKCU\Software\Classes\AppUserModelId\{app_id}");
    for (name, value) in [("DisplayName", "Hermio".to_string()), ("IconUri", icon.to_string_lossy().to_string())] {
        let _ = std::process::Command::new("reg")
            .args(["add", &key, "/v", name, "/t", "REG_EXPAND_SZ", "/d", &value, "/f"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
    }
}

#[cfg(not(windows))]
pub fn register_app_id(_app_id: &str, _data_dir: &std::path::Path) {}
