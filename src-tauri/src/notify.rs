//! New-mail notifications (Windows toasts), sent from the back end.
//!
//! They used to be raised by the web page when it heard the "new-mail" event, which loses them
//! whenever the page isn't listening: while it is still loading at start-up (the first sync runs
//! two seconds after launch), or while WebView2 throttles the hidden window in the tray. Now the
//! sync itself shows them, once per mailbox and sync run.

use crate::state::AppState;
use tauri_plugin_notification::NotificationExt;

/// One new message worth telling the user about.
#[derive(Debug, Clone, Default)]
pub struct NewMail {
    pub id: i64,
    pub from_name: String,
    pub from_addr: String,
    /// "Name <addr>, addr, …" as stored for the message.
    pub to: String,
    pub cc: String,
    pub subject: String,
}

impl NewMail {
    /// "Lan Phạm <lan@example.com>", or just the address.
    fn from_line(&self) -> String {
        match (self.from_name.trim(), self.from_addr.trim()) {
            ("", a) => a.to_string(),
            (n, "") => n.to_string(),
            (n, a) if n.eq_ignore_ascii_case(a) => a.to_string(),
            (n, a) => format!("{n} <{a}>"),
        }
    }
    fn short_from(&self) -> String {
        if self.from_name.trim().is_empty() { self.from_addr.clone() } else { self.from_name.trim().to_string() }
    }
}

/// Recipients for one line: "me@x.com, Bình +2". `own` is shown when the list is empty
/// (Bcc / undisclosed recipients) and listed first when the user is among them.
fn to_line(to: &str, cc: &str, own: &str) -> String {
    let all: Vec<&str> = to.split(", ").chain(cc.split(", ")).map(str::trim).filter(|s| !s.is_empty()).collect();
    if all.is_empty() {
        return own.to_string();
    }
    let addr = |s: &str| s.rsplit_once('<').map(|(_, a)| a.trim_end_matches('>').trim().to_string()).unwrap_or_else(|| s.to_string());
    let label = |s: &str| match s.rsplit_once('<') {
        Some((n, _)) if !n.trim().is_empty() => n.trim().to_string(),
        _ => addr(s),
    };
    let mut out: Vec<String> = vec![];
    if let Some(me) = all.iter().find(|s| addr(s).eq_ignore_ascii_case(own)) {
        out.push(addr(me));
    }
    for s in &all {
        if out.len() >= 2 {
            break;
        }
        if !addr(s).eq_ignore_ascii_case(own) {
            out.push(label(s));
        }
    }
    let rest = all.len().saturating_sub(out.len());
    if rest > 0 {
        format!("{} +{rest}", out.join(", "))
    } else {
        out.join(", ")
    }
}

/// Windows shows at most four lines of body text; long From/To lines are cut so they keep to
/// one line each and the subject stays visible.
fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max - 1).collect::<String>().trim_end())
    }
}

/// The toast text: "From: …", "To: …", "Subject: …" on their own lines.
fn body_for(m: &NewMail, account: &str) -> String {
    let subject = if m.subject.trim().is_empty() { "(No subject)" } else { m.subject.trim() };
    format!("From: {}\nTo: {}\nSubject: {}", clip(&m.from_line(), 46), clip(&to_line(&m.to, &m.cc, account), 48), subject)
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
pub fn new_mail(state: &AppState, account_id: i64, account: &str, items: &[NewMail]) {
    if items.is_empty() || !setting_bool(state, "notifications", true) {
        return;
    }
    // The message id travels with the toast, so a click can open that email.
    let show_for = |message: i64, title: &str, body: &str| {
        let b = state
            .app
            .notification()
            .builder()
            .id(i32::try_from(message).unwrap_or(0))
            .title(title)
            .body(body)
            .extra("message_id", message)
            .extra("account_id", account_id);
        if let Err(e) = b.show() {
            crate::sync::log_plain(state, "warn", format!("Couldn't show a notification: {e}"));
        }
    };
    if items.len() <= 3 {
        for m in items {
            show_for(m.id, "New email", &body_for(m, account));
        }
    } else {
        let mut senders: Vec<String> = vec![];
        for m in items {
            let f = m.short_from();
            if !f.is_empty() && !senders.contains(&f) {
                senders.push(f);
            }
        }
        let from = if senders.len() > 3 { format!("{} and {} more", senders[..3].join(", "), senders.len() - 3) } else { senders.join(", ") };
        // Newest first (highest id); a click opens it, in its inbox.
        let newest = items.iter().max_by_key(|m| m.id).unwrap();
        let subject = if newest.subject.trim().is_empty() { "(No subject)" } else { newest.subject.trim() };
        let body = format!("From: {}\nTo: {account}\nSubject: {subject} (+{} more)", clip(&from, 46), items.len() - 1);
        show_for(newest.id, &format!("{} new emails", items.len()), &body);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toast_lines() {
        let m = NewMail {
            id: 1,
            from_name: "Lan Phạm".into(),
            from_addr: "lan@uni.edu".into(),
            to: "Team <team@uni.edu>, me@x.com, Bình <binh@x.com>".into(),
            cc: "An <an@x.com>".into(),
            subject: "Họp nhóm".into(),
        };
        assert_eq!(body_for(&m, "me@x.com"), "From: Lan Phạm <lan@uni.edu>\nTo: me@x.com, Team +2\nSubject: Họp nhóm");
        let bcc = NewMail { from_addr: "news@shop.vn".into(), ..Default::default() };
        assert_eq!(body_for(&bcc, "me@x.com"), "From: news@shop.vn\nTo: me@x.com\nSubject: (No subject)");
        assert_eq!(to_line("a@x.com", "", "me@x.com"), "a@x.com");
        assert_eq!(clip("abcdef", 4), "abc…");
    }
}

/// What a click on a toast should open.
#[derive(Debug, Clone, serde::Serialize)]
pub struct OpenRequest {
    pub message_id: i64,
    pub account_id: i64,
}

static PENDING: std::sync::Mutex<Option<OpenRequest>> = std::sync::Mutex::new(None);

/// Called when a toast is clicked: bring the window back and ask the page to open the email.
/// The request is also kept until the page picks it up, in case the page is still loading.
pub fn clicked(app: &tauri::AppHandle, p: &tauri_plugin_notification::ActionPerformed) {
    use tauri::{Emitter, Manager};
    if p.action_id() != "tap" {
        return;
    }
    let num = |k: &str| p.notification().and_then(|n| n.extra().get(k)).and_then(|v| v.as_i64());
    let req = OpenRequest {
        message_id: num("message_id").or_else(|| p.notification().map(|n| n.id() as i64)).unwrap_or(0),
        account_id: num("account_id").unwrap_or(0),
    };
    *PENDING.lock().unwrap() = Some(req.clone());
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(w) = app2.get_webview_window("main") {
            let _ = w.unminimize();
            let _ = w.show();
            let _ = w.set_focus();
        }
        let _ = app2.emit("open-message", &req);
    });
}

/// The click the page hasn't handled yet (taken once).
pub fn take_pending() -> Option<OpenRequest> {
    PENDING.lock().unwrap().take()
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
