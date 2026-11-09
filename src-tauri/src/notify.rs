//! Windows notifications (toasts): new mail and calendar reminders.
//!
//! They are raised by the back end itself (not by the web page, which may be loading or
//! throttled in the tray) and built as toast XML, which gives them the layout
//!
//! ```text
//!  Subject: …                                    10:04 AM
//!  From: …
//!  To: …
//! ```
//!
//! and buttons. Clicks come back through [`activated`].

use crate::models::Event;
use crate::state::AppState;
use std::sync::Arc;
use tauri::{Emitter, Manager};

/// One new message worth telling the user about.
#[derive(Debug, Clone, Default)]
pub struct NewMail {
    pub id: i64,
    pub from_name: String,
    pub from_addr: String,
    /// Received (Date header), Unix seconds.
    pub date: i64,
    pub subject: String,
}

impl NewMail {
    /// The sender's address ("Name" only when there is no address).
    fn from_line(&self) -> String {
        if self.from_addr.trim().is_empty() { self.from_name.trim().to_string() } else { self.from_addr.trim().to_string() }
    }
    fn short_from(&self) -> String {
        if self.from_name.trim().is_empty() { self.from_addr.clone() } else { self.from_name.trim().to_string() }
    }
}

/// Mail sorted into a group that is hidden from the inbox is only announced when the user
/// asked for it (Settings → Mail → "Also notify for mail sorted into groups").
pub fn include_grouped(state: &AppState) -> bool {
    setting_bool(state, "notify_groups", false)
}

pub fn setting_bool(state: &AppState, key: &str, default: bool) -> bool {
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

/// The interface language (toasts are built here, not in the page).
fn vi(state: &AppState) -> bool {
    state.db.get_setting("lang").ok().flatten().map(|v| v.trim_matches('"').starts_with("vi")).unwrap_or(false)
}

/// Escape text for toast XML.
pub fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&apos;")
}

/// Windows shows a limited width; keep each line to one line.
fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max - 1).collect::<String>().trim_end())
    }
}

/// "10:04 AM" today, "08/11 10:04 AM" on another day.
fn when(ts: i64, vi: bool) -> String {
    use chrono::{Local, TimeZone};
    let Some(t) = Local.timestamp_opt(ts, 0).single() else { return String::new() };
    let time = t.format(if vi { "%H:%M" } else { "%-I:%M %p" }).to_string();
    if t.date_naive() == Local::now().date_naive() { time } else { format!("{} {time}", t.format("%d/%m")) }
}

/// Labels of the toast rows, in the interface language.
struct Words {
    from: &'static str,
    to: &'static str,
    no_subject: &'static str,
}
fn words(vi: bool) -> Words {
    if vi {
        Words { from: "Từ:", to: "Đến:", no_subject: "(Không có tiêu đề)" }
    } else {
        Words { from: "From:", to: "To:", no_subject: "(No subject)" }
    }
}

/// The toast XML for new mail:
///
/// ```text
///  The subject, in bold
///  From:  sender@example.com                 10:04 AM
///  To:    me@example.com
/// ```
///
/// Windows 11 draws a toast's title (its first top-level `<text>`) in regular weight and puts
/// a fixed gap under it, so the subject is drawn as bold content text instead, and the From /
/// To rows are stacked in one group (no gap between them; a small one under the subject).
/// Windows still insists on a top-level text ("<app> / New notification" otherwise):
/// layout 1 uses an invisible one (a zero-width space — Windows keeps an empty line for it, a
/// big gap at the top), layout 2 a small attribution line at the bottom, layout 3 makes the
/// subject itself the title (no gap, but Windows draws it in regular weight). Labels are bold (`base`), addresses regular (`body`). `launch` is what a click
/// reports.
fn mail_xml(subject: &str, time: &str, from: &str, to: &str, launch: &str, w: &Words, layout: u8) -> String {
    let (top, head) = match layout {
        2 => (r#"<text placement="attribution">Hermio</text>"#.to_string(), format!(r#"<group><subgroup><text hint-style="base" hint-maxLines="2">{}</text></subgroup></group>"#, esc(subject))),
        // The subject is the toast's own title: nothing above it, no empty line.
        _ => (format!(r#"<text hint-maxLines="2">{}</text>"#, esc(subject)), String::new()),
    };
    format!(
        r#"<toast launch="{launch}" activationType="foreground"><visual><binding template="ToastGeneric">{top}{head}<group><subgroup hint-weight="15"><text hint-style="base">{lf}</text><text hint-style="base">{lt}</text></subgroup><subgroup hint-weight="62"><text hint-style="body" hint-maxLines="1">{from}</text><text hint-style="body" hint-maxLines="1">{to}</text></subgroup><subgroup hint-weight="23"><text hint-style="captionSubtle" hint-align="right">{time}</text></subgroup></group></binding></visual><audio src="ms-winsoundevent:Notification.Mail"/></toast>"#,
        launch = esc(launch),
        time = esc(time),
        from = esc(from),
        to = esc(to),
        lf = esc(w.from),
        lt = esc(w.to),
    )
}

/// Which way to satisfy Windows' title requirement (see [`mail_xml`]): 3 (subject as the title)
/// unless 2 was chosen. The old layout 1 (an invisible title line) left a big empty band on top.
fn layout(state: &AppState) -> u8 {
    match state.db.get_setting("toast_layout").ok().flatten().and_then(|v| v.trim_matches('"').parse::<u8>().ok()) {
        Some(2) => 2,
        _ => 3,
    }
}

/// Show the toasts for what one sync run of one mailbox brought in: one toast per message for
/// a few messages, a single summary for a burst (Windows queues and drops floods of toasts).
/// "To" is always the mailbox's own address.
pub fn new_mail(state: &AppState, account_id: i64, account: &str, items: &[NewMail]) {
    if items.is_empty() || !setting_bool(state, "notifications", true) {
        return;
    }
    let vi = vi(state);
    let w = words(vi);
    let lay = layout(state);
    let subj = |m: &NewMail| if m.subject.trim().is_empty() { w.no_subject.to_string() } else { m.subject.trim().to_string() };
    let plain = |from: &str| format!("{} {from}\n{} {account}", w.from, w.to);
    if items.len() <= 3 {
        for m in items {
            let subject = subj(m);
            let from = clip(&m.from_line(), 60);
            show(state, &mail_xml(&subject, &when(m.date, vi), &from, account, &format!("msg:{}:{account_id}", m.id), &w, lay), &subject, &plain(&from));
        }
    } else {
        let mut senders: Vec<String> = vec![];
        for m in items {
            let f = m.short_from();
            if !f.is_empty() && !senders.contains(&f) {
                senders.push(f);
            }
        }
        let names = if senders.len() > 3 { format!("{} +{}", senders[..3].join(", "), senders.len() - 3) } else { senders.join(", ") };
        // A click opens the newest of them, in its inbox.
        let newest = items.iter().max_by_key(|m| (m.date, m.id)).unwrap();
        let more = if vi { format!("(+{} thư khác)", items.len() - 1) } else { format!("(+{} more)", items.len() - 1) };
        let subject = format!("{} {more}", subj(newest));
        let from = clip(&names, 60);
        show(state, &mail_xml(&subject, &when(newest.date, vi), &from, account, &format!("msg:{}:{account_id}", newest.id), &w, lay), &subject, &plain(&from));
    }
}

/// Sample new-mail toasts (Settings → "Send a test notification"): one per layout, so the
/// better-looking one can be chosen.
pub fn test(state: &AppState) {
    let vi = vi(state);
    let w = words(vi);
    for (n, lay) in [(1u8, 3u8), (2, 2)] {
        let subject = if vi { format!("Mẫu {n} — thông báo thử của Hermio") } else { format!("Layout {n} — Hermio notification test") };
        let xml = mail_xml(&subject, &when(chrono::Utc::now().timestamp(), vi), "hermio@localhost", "you@example.com", "test", &w, lay);
        show(state, &xml, &subject, &format!("{} hermio@localhost\n{} you@example.com", w.from, w.to));
    }
}

/// "2 h", "1 h 30 min", "15 min" (or Vietnamese).
pub fn lead_text(secs: i64, vi: bool) -> String {
    let mins = ((secs + 30) / 60).max(1);
    let (h, m) = (mins / 60, mins % 60);
    let (hu, mu) = if vi { ("giờ", "phút") } else { ("h", "min") };
    match (h, m) {
        (0, m) => format!("{m} {mu}"),
        (h, 0) => format!("{h} {hu}"),
        (h, m) => format!("{h} {hu} {m} {mu}"),
    }
}

/// A calendar reminder: title and "in 2 h" on top, the time and place below, and the buttons
/// Skip remaining reminders / Open / Dismiss.
pub fn reminder(state: &AppState, e: &Event, left: i64, key: &str, last: bool) {
    use chrono::{Local, TimeZone};
    let vi = vi(state);
    let fmt = |ts: i64| Local.timestamp_opt(ts, 0).single().map(|t| t.format("%-I:%M %p").to_string()).unwrap_or_default();
    let day = Local.timestamp_opt(e.start_ts, 0).single().map(|t| {
        if t.date_naive() == Local::now().date_naive() { String::new() } else { format!("{} · ", t.format("%a %d/%m")) }
    });
    let span = format!("{}{} – {}", day.unwrap_or_default(), fmt(e.start_ts), fmt(e.end_ts));
    let place = if !e.location.trim().is_empty() { e.location.trim().to_string() } else if !e.meet_link.is_empty() { "Google Meet".into() } else { String::new() };
    let soon = if vi { format!("còn {}", lead_text(left, true)) } else { format!("in {}", lead_text(left, false)) };
    let title = if e.title.trim().is_empty() { if vi { "(Không tiêu đề)" } else { "(No title)" }.to_string() } else { e.title.trim().to_string() };
    let (skip, open, dismiss) = if vi { ("Bỏ qua các lần nhắc sau", "Mở", "Đóng") } else { ("Skip remaining reminders", "Open", "Dismiss") };
    let skip_btn = if last { String::new() } else { format!(r#"<action content="{}" arguments="ev:skip:{}"/>"#, esc(skip), esc(key)) };
    let xml = format!(
        r#"<toast scenario="reminder" launch="ev:open:{id}" activationType="foreground"><visual><binding template="ToastGeneric"><text hint-maxLines="2">{title}</text><group><subgroup hint-weight="72"><text hint-style="body" hint-maxLines="1">{span}</text><text hint-style="captionSubtle" hint-maxLines="1">{place}</text></subgroup><subgroup hint-weight="28" hint-textStacking="top"><text hint-style="captionSubtle" hint-align="right">{soon}</text></subgroup></group></binding></visual><actions>{skip_btn}<action content="{open}" arguments="ev:open:{id}"/><action content="{dismiss}" arguments="dismiss" activationType="system"/></actions><audio src="ms-winsoundevent:Notification.Reminder"/></toast>"#,
        id = e.id,
        title = esc(&title),
        soon = esc(&soon),
        span = esc(&span),
        place = esc(&place),
        open = esc(open),
        dismiss = esc(dismiss),
    );
    show(state, &xml, &format!("{title} · {soon}"), &format!("{span}\n{place}"));
}

/// A short confirmation toast (title + one line).
pub fn simple(state: &AppState, title: &str, line: &str) {
    let xml = format!(
        r#"<toast launch="none"><visual><binding template="ToastGeneric"><text>{}</text><text>{}</text></binding></visual></toast>"#,
        esc(title),
        esc(line)
    );
    show(state, &xml, title, line);
}

fn show(state: &AppState, xml: &str, plain_title: &str, plain_body: &str) {
    crate::toast::show(&state.app, xml.to_string(), plain_title.to_string(), plain_body.to_string());
}

// ---------------- clicks ----------------

/// What a click on a new-mail toast should open.
#[derive(Debug, Clone, serde::Serialize)]
pub struct OpenRequest {
    pub message_id: i64,
    pub account_id: i64,
}

static PENDING: std::sync::Mutex<Option<OpenRequest>> = std::sync::Mutex::new(None);

/// The click the page hasn't handled yet (taken once).
pub fn take_pending() -> Option<OpenRequest> {
    PENDING.lock().unwrap().take()
}

#[cfg_attr(not(windows), allow(dead_code))]
fn bring_up(app: &tauri::AppHandle, then: impl FnOnce(&tauri::AppHandle) + Send + 'static) {
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(w) = app2.get_webview_window("main") {
            let _ = w.unminimize();
            let _ = w.show();
            let _ = w.set_focus();
        }
        then(&app2);
    });
}

/// A toast (or one of its buttons) was clicked. `args`:
/// `msg:<message>:<account>` · `ev:open:<event>` · `ev:skip:<reminder key>` · `dismiss`.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn activated(app: &tauri::AppHandle, args: &str) {
    let parts: Vec<&str> = args.splitn(3, ':').collect();
    match parts.as_slice() {
        ["msg", id, acc] => {
            let req = OpenRequest { message_id: id.parse().unwrap_or(0), account_id: acc.parse().unwrap_or(0) };
            *PENDING.lock().unwrap() = Some(req.clone());
            bring_up(app, move |a| {
                let _ = a.emit("open-message", &req);
            });
        }
        ["ev", "open", id] => {
            let id: i64 = id.parse().unwrap_or(0);
            let start = app.try_state::<Arc<AppState>>().and_then(|s| s.db.event(id).ok()).map(|e| e.start_ts).unwrap_or(0);
            bring_up(app, move |a| {
                let _ = a.emit("open-event", serde_json::json!({ "id": id, "start_ts": start }));
            });
        }
        ["ev", "skip", key] => {
            if let Some(state) = app.try_state::<Arc<AppState>>() {
                skip_reminders(&state, key);
            }
        }
        _ => {}
    }
}

/// "Skip remaining reminders": no more reminders for this event (this occurrence).
pub fn skip_reminders(state: &AppState, key: &str) {
    let start = key.rsplit_once('@').and_then(|(_, s)| s.parse().ok()).unwrap_or(0);
    let ok = state.db.set_reminder(key, start, None, Some(true));
    let vi = vi(state);
    match ok {
        Ok(()) => {
            state.emit("reminder-skipped", &serde_json::json!({ "key": key }));
            let (t, l) = if vi {
                ("Đã tắt nhắc lịch", "Bạn sẽ không nhận thêm lời nhắc nào cho sự kiện này.")
            } else {
                ("Reminders skipped", "You won't get more reminders for this event.")
            };
            simple(state, t, l);
        }
        Err(e) => simple(state, if vi { "Không tắt được lời nhắc" } else { "Couldn't skip the reminders" }, &format!("{e:#}")),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mail_toast_xml() {
        let x = mail_xml("A & B", "10:04 AM", "a@x.com", "me@x.com", "msg:1:2", &words(false), 2);
        // Layout 2: the subject is bold content, first; "Hermio" as the attribution line.
        assert!(x.contains(r#"placement="attribution""#) && !x.contains("\u{200B}"));
        assert!(x.find(r#"<text hint-style="base" hint-maxLines="2">A &amp; B</text>"#).unwrap() < x.find("From:").unwrap());
        // From and To in the same group (no gap between them).
        assert_eq!(x.matches("<group>").count(), 2);
        // The old layout 1 now falls back to the title layout: no invisible top line.
        assert!(!mail_xml("s", "t", "f", "t", "l", &words(false), 1).contains("\u{200B}"));
        assert!(!x.contains("Subject"));
        // Bold labels, regular addresses, time on the right of the From row.
        assert!(x.contains(r#"<text hint-style="base">From:</text>"#));
        assert!(x.contains(r#"<text hint-style="body" hint-maxLines="1">a@x.com</text>"#));
        assert!(x.contains(r#"hint-align="right">10:04 AM<"#));
        assert!(x.find("From:").unwrap() < x.find("To:").unwrap());
        assert!(mail_xml("s", "t", "f", "t", "l", &words(true), 1).contains(">Đến:<"));
        // Layout 3: the subject is the title itself — no invisible line, no separate subject row.
        let y = mail_xml("Hello", "t", "f", "t", "l", &words(false), 3);
        assert!(y.contains(r#"<binding template="ToastGeneric"><text hint-maxLines="2">Hello</text><group>"#));
        assert!(!y.contains("\u{200B}") && y.matches("<group>").count() == 1);
        assert_eq!(clip("abcdef", 4), "abc…");
    }

    #[test]
    fn leads() {
        assert_eq!(lead_text(7200, false), "2 h");
        assert_eq!(lead_text(5400, false), "1 h 30 min");
        assert_eq!(lead_text(600, true), "10 phút");
    }
}
