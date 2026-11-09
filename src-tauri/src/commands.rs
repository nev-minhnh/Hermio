//! Tauri commands exposed to the UI.

use crate::models::*;
use crate::oauth;
use crate::state::AppState;
use crate::{calendar, imap, parse, pop3, smtp, sync};
use anyhow::{anyhow, Result};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tauri::State;

type S<'a> = State<'a, Arc<AppState>>;
type R<T> = std::result::Result<T, String>;

fn e<E: std::fmt::Display>(err: E) -> String {
    format!("{err:#}")
}
fn ea(err: anyhow::Error) -> String {
    format!("{err:#}")
}

// ---------------- settings ----------------

#[tauri::command]
pub fn get_settings(state: S) -> R<Value> {
    Ok(Value::Object(state.db.all_settings().map_err(ea)?))
}

#[tauri::command]
pub fn set_setting(state: S, key: String, value: Value) -> R<()> {
    state.db.set_setting(&key, &value.to_string()).map_err(ea)
}

#[tauri::command]
pub fn google_config(state: S) -> R<Value> {
    let id = state.db.get_setting("google_client_id").map_err(ea)?.unwrap_or_default();
    let has_secret = oauth::get_secret(oauth::CLIENT_SECRET_KEY).map_err(ea)?.is_some();
    let builtin = crate::state::builtin_google().is_some();
    Ok(json!({ "client_id": id.trim_matches('"'), "has_secret": has_secret || builtin, "builtin": builtin }))
}

#[tauri::command]
pub fn set_google_config(state: S, client_id: String, client_secret: Option<String>) -> R<()> {
    state.db.set_setting("google_client_id", &Value::String(client_id.trim().into()).to_string()).map_err(ea)?;
    if let Some(s) = client_secret {
        if !s.trim().is_empty() {
            oauth::set_secret(oauth::CLIENT_SECRET_KEY, s.trim()).map_err(ea)?;
        }
    }
    Ok(())
}

// ---------------- accounts ----------------

#[derive(Serialize)]
pub struct Preset {
    provider: String,
    imap: (String, u16, String),
    pop3: Option<(String, u16, String)>,
    smtp: (String, u16, String),
    note: String,
}

#[tauri::command]
pub fn guess_settings(email: String) -> Preset {
    let domain = email.rsplit('@').next().unwrap_or("").trim().to_lowercase();
    let t = |h: &str, p: u16, s: &str| (h.to_string(), p, s.to_string());
    match domain.as_str() {
        "gmail.com" | "googlemail.com" => Preset {
            provider: "google".into(),
            imap: t("imap.gmail.com", 993, "ssl"),
            pop3: Some(t("pop.gmail.com", 995, "ssl")),
            smtp: t("smtp.gmail.com", 465, "ssl"),
            note: "Gmail: use Sign in with Google (OAuth) or an App Password.".into(),
        },
        "outlook.com" | "hotmail.com" | "live.com" | "msn.com" => Preset {
            provider: "microsoft".into(),
            imap: t("outlook.office365.com", 993, "ssl"),
            pop3: Some(t("outlook.office365.com", 995, "ssl")),
            smtp: t("smtp-mail.outlook.com", 587, "starttls"),
            note: "Microsoft may block password sign-in; use an App Password then.".into(),
        },
        "yahoo.com" | "ymail.com" => Preset {
            provider: "other".into(),
            imap: t("imap.mail.yahoo.com", 993, "ssl"),
            pop3: Some(t("pop.mail.yahoo.com", 995, "ssl")),
            smtp: t("smtp.mail.yahoo.com", 465, "ssl"),
            note: "Yahoo requires an App Password.".into(),
        },
        "icloud.com" | "me.com" | "mac.com" => Preset {
            provider: "other".into(),
            imap: t("imap.mail.me.com", 993, "ssl"),
            pop3: None,
            smtp: t("smtp.mail.me.com", 587, "starttls"),
            note: "iCloud requires an app-specific password.".into(),
        },
        d => Preset {
            provider: "other".into(),
            imap: t(&format!("imap.{d}"), 993, "ssl"),
            pop3: Some(t(&format!("pop.{d}"), 995, "ssl")),
            smtp: t(&format!("smtp.{d}"), 465, "ssl"),
            note: "Settings were guessed; check them with your email provider.".into(),
        },
    }
}

#[tauri::command]
pub fn list_accounts(state: S) -> R<Vec<Account>> {
    state.db.accounts().map_err(ea)
}

#[tauri::command]
pub fn default_pop3_dir(state: S, account_id: i64) -> String {
    state.db.data_dir.join("pop3").join(account_id.to_string()).to_string_lossy().to_string()
}

#[tauri::command]
pub async fn save_account(state: S<'_>, account: Account, password: Option<String>, smtp_password: Option<String>) -> R<i64> {
    let mut a = account;
    if a.username.trim().is_empty() {
        a.username = a.email.clone();
    }
    if a.name.trim().is_empty() {
        a.name = a.email.clone();
    }
    let is_new = a.id == 0;
    let id = state.db.save_account(&a).map_err(ea)?;
    if let Some(p) = password {
        if !p.is_empty() {
            oauth::set_secret(&oauth::password_key(id), &p).map_err(ea)?;
        }
    }
    if let Some(p) = smtp_password {
        if !p.is_empty() {
            oauth::set_secret(&format!("account:{id}:smtp_password"), &p).map_err(ea)?;
        }
    }
    if a.protocol == "pop3" {
        pop3::ensure_local_folders(&state, id).map_err(ea)?;
    }
    state.changed(id);
    if is_new {
        let st = state.inner().clone();
        tauri::async_runtime::spawn(async move { sync::sync_one(st, id).await });
    }
    Ok(id)
}

/// Sign in with Google and create (or refresh) a Gmail / Google Workspace account.
/// By default the Google sign-in page opens in a small popup window inside Hermio; `popup = false`
/// uses the system browser instead (fallback if Google refuses the embedded window).
#[tauri::command]
pub async fn google_sign_in(
    state: S<'_>,
    app: tauri::AppHandle,
    account_id: Option<i64>,
    protocol: Option<String>,
    login_hint: Option<String>,
    pop3_dir: Option<String>,
    popup: Option<bool>,
    recent: Option<bool>,
) -> R<i64> {
    use tauri::Manager;
    use tauri_plugin_opener::OpenerExt;
    const LABEL: &str = "google-login";
    let client = state.google_client().map_err(ea)?;
    let use_popup = popup.unwrap_or(true);
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.destroy();
    }
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let tx = std::sync::Mutex::new(Some(tx));
    let app2 = app.clone();
    let open = move |url: &str| -> Result<()> {
        if !use_popup {
            return app2.opener().open_url(url, None::<&str>).map_err(|e| anyhow!("{e}"));
        }
        let parsed: tauri::Url = url.parse().map_err(|e| anyhow!("{e}"))?;
        let w = tauri::WebviewWindowBuilder::new(&app2, LABEL, tauri::WebviewUrl::External(parsed))
            .title("Sign in with Google — Hermio")
            .inner_size(500.0, 720.0)
            .min_inner_size(400.0, 560.0)
            .center()
            .focused(true)
            .build()
            .map_err(|e| anyhow!("Could not open the sign-in window: {e}"))?;
        let tx = std::sync::Mutex::new(tx.lock().unwrap().take());
        w.on_window_event(move |ev| {
            if let tauri::WindowEvent::Destroyed = ev {
                if let Some(t) = tx.lock().unwrap().take() {
                    let _ = t.send(());
                }
            }
        });
        Ok(())
    };
    let result = oauth::google_sign_in(&client, login_hint.as_deref().unwrap_or(""), open, use_popup.then_some(rx)).await;
    if let Some(w) = app.get_webview_window(LABEL) {
        tokio::time::sleep(std::time::Duration::from_millis(700)).await;
        let _ = w.destroy();
    }
    let res = result.map_err(ea)?;
    let id = match account_id.filter(|x| *x > 0) {
        Some(id) => id,
        None => {
            let proto = protocol.unwrap_or_else(|| "imap".into());
            let a = Account {
                name: res.email.clone(),
                email: res.email.clone(),
                display_name: res.name.clone(),
                protocol: proto.clone(),
                auth: "oauth_google".into(),
                provider: "google".into(),
                in_host: if proto == "pop3" { "pop.gmail.com".into() } else { "imap.gmail.com".into() },
                in_port: if proto == "pop3" { 995 } else { 993 },
                in_security: "ssl".into(),
                out_host: "smtp.gmail.com".into(),
                out_port: 465,
                out_security: "ssl".into(),
                username: res.email.clone(),
                pop3_dir: pop3_dir.unwrap_or_default(),
                pop3_leave_on_server: true,
                pop3_recent_mode: recent.unwrap_or(false),
                color: "#0f6cbd".into(),
                calendar_sync: true,
                enabled: true,
                ..Default::default()
            };
            let id = state.db.save_account(&a).map_err(ea)?;
            if proto == "pop3" {
                pop3::ensure_local_folders(&state, id).map_err(ea)?;
            }
            id
        }
    };
    oauth::set_secret(&oauth::refresh_key(id), &res.refresh_token).map_err(ea)?;
    state.tokens.put(id, &res.access_token, res.expires_in);
    state.changed(id);
    let st = state.inner().clone();
    tauri::async_runtime::spawn(async move { sync::sync_one(st, id).await });
    Ok(id)
}

#[tauri::command]
pub async fn test_account(state: S<'_>, account_id: i64) -> R<String> {
    let a = state.db.account(account_id).map_err(ea)?;
    let incoming = if a.protocol == "pop3" { pop3::test(&state, &a).await } else { imap::test(&state, &a).await };
    incoming.map_err(|e| format!("Incoming mail: {e:#}"))?;
    smtp::test(&state, &a).await.map_err(|e| format!("Outgoing mail: {e:#}"))?;
    Ok("Incoming and outgoing mail both work.".into())
}

#[tauri::command]
pub fn delete_account(state: S, account_id: i64) -> R<()> {
    state.db.delete_account(account_id).map_err(ea)?;
    oauth::delete_secret(&oauth::password_key(account_id));
    oauth::delete_secret(&oauth::refresh_key(account_id));
    state.tokens.forget(account_id);
    let _ = std::fs::remove_dir_all(state.db.cache_dir(account_id));
    state.changed(account_id);
    Ok(())
}

#[tauri::command]
pub fn reorder_accounts(state: S, ids: Vec<i64>) -> R<()> {
    state.db.reorder_accounts(&ids).map_err(ea)?;
    state.changed(0);
    Ok(())
}

#[tauri::command]
pub fn relocate_pop3(state: S, account_id: i64, new_dir: String, move_files: bool) -> R<usize> {
    let mut a = state.db.account(account_id).map_err(ea)?;
    let moved = if move_files { pop3::relocate(&state, &a, std::path::Path::new(&new_dir)).map_err(ea)? } else { 0 };
    a.pop3_dir = new_dir;
    state.db.save_account(&a).map_err(ea)?;
    Ok(moved)
}

#[tauri::command]
pub async fn sync_now(state: S<'_>, account_id: Option<i64>) -> R<()> {
    let st = state.inner().clone();
    match account_id {
        Some(id) => {
            tauri::async_runtime::spawn(async move { sync::sync_one(st, id).await });
        }
        None => {
            tauri::async_runtime::spawn(async move { sync::sync_all(st).await });
        }
    }
    Ok(())
}

// ---------------- mail ----------------

#[tauri::command]
pub fn list_folders(state: S) -> R<Vec<Folder>> {
    state.db.folders().map_err(ea)
}

#[tauri::command]
pub fn list_messages(state: S, query: ListQuery) -> R<Vec<MessageRow>> {
    state.db.list_messages(&query).map_err(ea)
}

#[tauri::command]
pub fn unread_counts(state: S, space: Option<i64>) -> R<Value> {
    state.db.unread_counts(space.unwrap_or(0)).map_err(ea)
}

/// Whether a message sits in a group that is hidden from the inbox.
pub fn hidden_from_inbox(state: &AppState, id: i64) -> bool {
    let Ok(gs) = state.db.message_group_ids(id) else { return false };
    if gs.is_empty() {
        return false;
    }
    state.db.groups().map(|all| all.iter().any(|g| g.hide_from_inbox && gs.contains(&g.id))).unwrap_or(false)
}

pub async fn raw_bytes(state: &AppState, id: i64) -> Result<Vec<u8>> {
    let m = state.db.message(id)?;
    if !m.raw_path.is_empty() {
        if let Ok(b) = std::fs::read(&m.raw_path) {
            return Ok(b);
        }
    }
    let a = state.db.account(m.account_id)?;
    if a.protocol == "imap" {
        imap::fetch_raw(state, &a, &m).await
    } else {
        Err(anyhow!("Message file not found: {}", m.raw_path))
    }
}

#[tauri::command]
pub async fn get_message(state: S<'_>, id: i64) -> R<MessageBody> {
    let raw = raw_bytes(&state, id).await.map_err(ea)?;
    Ok(parse::body(id, &raw))
}

#[tauri::command]
pub async fn message_source(state: S<'_>, id: i64) -> R<String> {
    let raw = raw_bytes(&state, id).await.map_err(ea)?;
    Ok(String::from_utf8_lossy(&raw).into_owned())
}

fn group_by_account(state: &AppState, ids: &[i64]) -> Result<HashMap<i64, Vec<MessageRow>>> {
    let mut map: HashMap<i64, Vec<MessageRow>> = HashMap::new();
    for m in state.db.messages_by_ids(ids)? {
        map.entry(m.account_id).or_default().push(m);
    }
    Ok(map)
}

/// Run a server-side IMAP operation in the background; on failure tell the UI and resync.
fn background<F>(state: Arc<AppState>, account_id: i64, fut: F)
where
    F: std::future::Future<Output = Result<()>> + Send + 'static,
{
    tauri::async_runtime::spawn(async move {
        if let Err(err) = fut.await {
            state.emit("op-error", json!({ "account_id": account_id, "message": format!("{err:#}") }));
        }
        sync::sync_one(state, account_id).await;
    });
}

#[tauri::command]
pub fn set_seen(state: S, ids: Vec<i64>, seen: bool) -> R<()> {
    set_flag(state, ids, "\\Seen", seen)
}

#[tauri::command]
pub fn set_flagged(state: S, ids: Vec<i64>, flagged: bool) -> R<()> {
    set_flag(state, ids, "\\Flagged", flagged)
}

fn set_flag(state: S, ids: Vec<i64>, flag: &'static str, on: bool) -> R<()> {
    for (aid, msgs) in group_by_account(&state, &ids).map_err(ea)? {
        for m in &msgs {
            match flag {
                "\\Seen" => state.db.set_flags(m.id, Some(on), None, None),
                _ => state.db.set_flags(m.id, None, Some(on), None),
            }
            .map_err(ea)?;
        }
        let a = state.db.account(aid).map_err(ea)?;
        if a.protocol == "imap" {
            let st = state.inner().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(err) = imap::store_flags(&st, &a, &msgs, flag, on).await {
                    st.emit("op-error", json!({ "account_id": a.id, "message": format!("{err:#}") }));
                }
            });
        }
        state.changed(aid);
    }
    Ok(())
}

/// Move messages to `folder_id` (same account) or to the folder with `role` in each account.
fn move_impl(state: &Arc<AppState>, ids: &[i64], folder_id: Option<i64>, role: Option<&str>) -> Result<()> {
    for (aid, msgs) in group_by_account(state, ids)? {
        let a = state.db.account(aid)?;
        let dest = match (folder_id, role) {
            (Some(fid), _) => state.db.folder(fid)?,
            (None, Some(r)) => match state.db.folder_by_role(aid, r)? {
                Some(f) => f,
                None if r == "archive" && a.provider == "google" => state
                    .db
                    .folder_by_role(aid, "all")?
                    .ok_or_else(|| anyhow!("Archive folder not found"))?,
                None => return Err(anyhow!("Account {} has no '{}' folder", a.email, r)),
            },
            _ => return Err(anyhow!("missing destination folder")),
        };
        if dest.account_id != aid {
            return Err(anyhow!("Messages can only be moved within the same account"));
        }
        if a.protocol == "pop3" {
            for m in &msgs {
                state.db.move_local(m.id, dest.id)?;
            }
        } else {
            let ids: Vec<i64> = msgs.iter().map(|m| m.id).collect();
            state.db.delete_messages(&ids)?;
            let st = state.clone();
            let msgs2 = msgs.clone();
            background(state.clone(), aid, async move { imap::move_to(&st, &a, &msgs2, &dest).await });
        }
        state.changed(aid);
    }
    Ok(())
}

#[tauri::command]
pub fn move_messages(state: S, ids: Vec<i64>, folder_id: i64) -> R<()> {
    move_impl(state.inner(), &ids, Some(folder_id), None).map_err(ea)
}

#[tauri::command]
pub fn archive_messages(state: S, ids: Vec<i64>) -> R<()> {
    move_impl(state.inner(), &ids, None, Some("archive")).map_err(ea)
}

#[tauri::command]
pub fn spam_messages(state: S, ids: Vec<i64>) -> R<()> {
    move_impl(state.inner(), &ids, None, Some("junk")).map_err(ea)
}

/// Delete: move to Trash, or remove permanently if already in Trash/Junk (or `permanent`).
#[tauri::command]
pub fn delete_messages(state: S, ids: Vec<i64>, permanent: Option<bool>) -> R<()> {
    let st = state.inner().clone();
    for (aid, msgs) in group_by_account(&st, &ids).map_err(ea)? {
        let a = st.db.account(aid).map_err(ea)?;
        let trash = st.db.folder_by_role(aid, "trash").map_err(ea)?;
        let mut to_trash = vec![];
        let mut to_kill = vec![];
        for m in msgs {
            let in_trash = trash.as_ref().map(|t| t.id == m.folder_id).unwrap_or(false);
            let in_junk = st.db.folder(m.folder_id).map(|f| f.role == "junk").unwrap_or(false);
            if permanent.unwrap_or(false) || in_trash || in_junk || trash.is_none() {
                to_kill.push(m);
            } else {
                to_trash.push(m);
            }
        }
        if !to_trash.is_empty() {
            let ids: Vec<i64> = to_trash.iter().map(|m| m.id).collect();
            move_impl(&st, &ids, trash.as_ref().map(|t| t.id), None).map_err(ea)?;
        }
        if !to_kill.is_empty() {
            let ids: Vec<i64> = to_kill.iter().map(|m| m.id).collect();
            st.db.delete_messages(&ids).map_err(ea)?;
            if a.protocol == "pop3" {
                for m in &to_kill {
                    if !m.raw_path.is_empty() {
                        let _ = std::fs::remove_file(&m.raw_path);
                    }
                }
            } else {
                for m in &to_kill {
                    if !m.raw_path.is_empty() {
                        let _ = std::fs::remove_file(&m.raw_path);
                    }
                }
                let st2 = st.clone();
                background(st.clone(), aid, async move { imap::expunge(&st2, &a, &to_kill).await });
            }
        }
        st.changed(aid);
    }
    Ok(())
}

#[tauri::command]
pub async fn create_folder(state: S<'_>, account_id: i64, name: String) -> R<()> {
    let a = state.db.account(account_id).map_err(ea)?;
    if a.protocol == "pop3" {
        state.db.upsert_folder(account_id, &name, &name, "/", "", true).map_err(ea)?;
    } else {
        imap::create_folder(&state, &a, &name).await.map_err(ea)?;
    }
    state.changed(account_id);
    Ok(())
}

#[tauri::command]
pub async fn save_attachment(state: S<'_>, id: i64, index: usize, path: String) -> R<()> {
    let raw = raw_bytes(&state, id).await.map_err(ea)?;
    let (_, _, data) = parse::attachment(&raw, index).ok_or("Attachment not found")?;
    std::fs::write(&path, data).map_err(e)?;
    Ok(())
}

fn temp_attachment(id: i64, name: &str, data: &[u8]) -> Result<std::path::PathBuf> {
    let dir = std::env::temp_dir().join("Hermio").join(id.to_string());
    std::fs::create_dir_all(&dir)?;
    let safe: String = name.chars().map(|c| if r#"\/:*?"<>|"#.contains(c) { '_' } else { c }).collect();
    let path = dir.join(if safe.is_empty() { "attachment".into() } else { safe });
    std::fs::write(&path, data)?;
    Ok(path)
}

#[tauri::command]
pub async fn open_attachment(state: S<'_>, app: tauri::AppHandle, id: i64, index: usize) -> R<()> {
    use tauri_plugin_opener::OpenerExt;
    let raw = raw_bytes(&state, id).await.map_err(ea)?;
    let (name, _, data) = parse::attachment(&raw, index).ok_or("Attachment not found")?;
    let path = temp_attachment(id, &name, &data).map_err(ea)?;
    app.opener().open_path(path.to_string_lossy(), None::<&str>).map_err(e)?;
    Ok(())
}

#[tauri::command]
pub fn suggest_contacts(state: S, prefix: String) -> R<Vec<Contact>> {
    state.db.suggest_contacts(&prefix).map_err(ea)
}

/// Turn "forward this attachment" references into temp files the builder can read.
async fn resolve_attachments(state: &AppState, o: &mut Outgoing) -> Result<()> {
    for att in o.attachments.iter_mut() {
        if att.path.is_none() {
            if let Some((mid, idx)) = att.from_message {
                let raw = raw_bytes(state, mid).await?;
                let (name, _, data) = parse::attachment(&raw, idx).ok_or_else(|| anyhow!("Attachment not found"))?;
                let p = temp_attachment(mid, &name, &data)?;
                att.path = Some(p.to_string_lossy().to_string());
                if att.name.is_empty() {
                    att.name = name;
                }
            }
        }
    }
    Ok(())
}

async fn remove_draft(state: &Arc<AppState>, draft_id: i64) {
    if let Ok(m) = state.db.message(draft_id) {
        if let Ok(a) = state.db.account(m.account_id) {
            let _ = state.db.delete_messages(&[draft_id]);
            if a.protocol == "imap" {
                let _ = imap::expunge(state, &a, &[m]).await;
            } else if !m.raw_path.is_empty() {
                let _ = std::fs::remove_file(&m.raw_path);
            }
        }
    }
}

#[tauri::command]
pub async fn send_mail(state: S<'_>, message: Outgoing) -> R<()> {
    send_outgoing(state.inner(), message).await.map_err(ea)
}

/// Send one message: SMTP, contacts, answered flag, Sent copy, draft cleanup, quick resync.
pub async fn send_outgoing(st: &Arc<AppState>, mut message: Outgoing) -> Result<()> {
    let st = st.clone();
    let a = st.db.account(message.account_id)?;
    resolve_attachments(&st, &mut message).await?;
    let (msg, raw) = smtp::build(&a, &message)?;
    smtp::send(&st, &a, msg).await?;
    for r in message.to.iter().chain(message.cc.iter()).chain(message.bcc.iter()) {
        if let Ok(mb) = r.parse::<lettre::message::Mailbox>() {
            let _ = st.db.touch_contact(&mb.email.to_string(), mb.name.as_deref().unwrap_or(""));
        } else {
            let _ = st.db.touch_contact(r, "");
        }
    }
    if let Some(rid) = message.reply_of {
        let _ = st.db.set_flags(rid, None, None, Some(true));
        if a.protocol == "imap" {
            if let Ok(m) = st.db.message(rid) {
                let _ = imap::store_flags(&st, &a, &[m], "\\Answered", true).await;
            }
        }
    }
    // Keep a copy in Sent (Gmail does this itself on the server).
    if a.protocol == "pop3" {
        pop3::store_local(&st, &a, "sent", &raw, true)?;
    } else if a.provider != "google" {
        if let Ok(Some(sent)) = st.db.folder_by_role(a.id, "sent") {
            if let Err(err) = imap::append(&st, &a, &sent, &raw, "(\\Seen)").await {
                st.emit("op-error", json!({ "account_id": a.id, "message": format!("Sent, but could not save a copy to Sent: {err:#}") }));
            }
        }
    }
    if let Some(d) = message.draft_id {
        remove_draft(&st, d).await;
    }
    st.changed(a.id);
    let st2 = st.clone();
    let aid = a.id;
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
        sync::sync_one(st2, aid).await
    });
    Ok(())
}

// ---------------- scheduled sending (outbox) ----------------

#[derive(Serialize)]
pub struct OutboxItem {
    id: i64,
    account_id: i64,
    subject: String,
    recipients: String,
    send_at: i64,
    status: String,
    error: String,
}

/// Queue a message to be sent at `send_at` (unix seconds). Attachments are copied into the app's
/// data folder so the scheduled message doesn't depend on files that may move.
#[tauri::command]
pub async fn schedule_send(state: S<'_>, mut message: Outgoing, send_at: i64) -> R<i64> {
    let st = state.inner().clone();
    resolve_attachments(&st, &mut message).await.map_err(ea)?;
    let mut rnd = [0u8; 6];
    getrandom::getrandom(&mut rnd).map_err(e)?;
    let dir = st.db.data_dir.join("outbox").join(rnd.iter().map(|b| format!("{b:02x}")).collect::<String>());
    for att in message.attachments.iter_mut() {
        if let Some(p) = &att.path {
            std::fs::create_dir_all(&dir).map_err(e)?;
            let name = std::path::Path::new(p).file_name().map(|x| x.to_owned()).unwrap_or_default();
            let dst = dir.join(name);
            std::fs::copy(p, &dst).map_err(e)?;
            att.path = Some(dst.to_string_lossy().to_string());
        }
    }
    let payload = serde_json::to_string(&message).map_err(e)?;
    let recipients = message.to.iter().chain(message.cc.iter()).cloned().collect::<Vec<_>>().join(", ");
    let now = chrono::Utc::now().timestamp();
    let id = st
        .db
        .with(|c| {
            c.execute(
                "INSERT INTO outbox(account_id,payload,subject,recipients,send_at,created) VALUES(?1,?2,?3,?4,?5,?6)",
                rusqlite::params![message.account_id, payload, message.subject, recipients, send_at, now],
            )?;
            Ok(c.last_insert_rowid())
        })
        .map_err(ea)?;
    // A draft that got scheduled is no longer a draft.
    if let Some(d) = message.draft_id {
        remove_draft(&st, d).await;
    }
    st.emit("outbox-changed", json!({}));
    Ok(id)
}

#[tauri::command]
pub fn list_outbox(state: S) -> R<Vec<OutboxItem>> {
    state
        .db
        .with(|c| {
            let mut q = c.prepare("SELECT id,account_id,subject,recipients,send_at,status,error FROM outbox ORDER BY send_at")?;
            let r = q.query_map([], |r| {
                Ok(OutboxItem {
                    id: r.get(0)?, account_id: r.get(1)?, subject: r.get(2)?, recipients: r.get(3)?,
                    send_at: r.get(4)?, status: r.get(5)?, error: r.get(6)?,
                })
            })?;
            r.collect()
        })
        .map_err(ea)
}

#[tauri::command]
pub fn cancel_outbox(state: S, id: i64) -> R<Outgoing> {
    let payload: String = state
        .db
        .with(|c| c.query_row("SELECT payload FROM outbox WHERE id=?1", [id], |r| r.get(0)))
        .map_err(ea)?;
    state.db.with(|c| c.execute("DELETE FROM outbox WHERE id=?1", [id])).map_err(ea)?;
    state.emit("outbox-changed", json!({}));
    serde_json::from_str(&payload).map_err(e)
}

#[tauri::command]
pub fn reschedule_outbox(state: S, id: i64, send_at: i64) -> R<()> {
    state
        .db
        .with(|c| c.execute("UPDATE outbox SET send_at=?2, status='scheduled', error='' WHERE id=?1", rusqlite::params![id, send_at]))
        .map_err(ea)?;
    state.emit("outbox-changed", json!({}));
    Ok(())
}

/// Send every scheduled message that is due. Called by the background scheduler.
pub async fn process_outbox(st: Arc<AppState>) {
    let now = chrono::Utc::now().timestamp();
    let due: Vec<(i64, String)> = st
        .db
        .with(|c| {
            let mut q = c.prepare("SELECT id, payload FROM outbox WHERE status='scheduled' AND send_at <= ?1 ORDER BY send_at")?;
            let r = q.query_map([now], |r| Ok((r.get(0)?, r.get(1)?)))?;
            r.collect()
        })
        .unwrap_or_default();
    for (id, payload) in due {
        let _ = st.db.with(|c| c.execute("UPDATE outbox SET status='sending' WHERE id=?1", [id]));
        let parsed = serde_json::from_str::<Outgoing>(&payload);
        let subject = parsed.as_ref().map(|m| m.subject.clone()).unwrap_or_default();
        let res = match parsed {
            Ok(m) => send_outgoing(&st, m).await,
            Err(err) => Err(anyhow!("{err}")),
        };
        match res {
            Ok(()) => {
                let _ = st.db.with(|c| c.execute("DELETE FROM outbox WHERE id=?1", [id]));
                st.emit("scheduled-sent", json!({ "id": id, "subject": subject }));
            }
            Err(err) => {
                let msg = format!("{err:#}");
                let _ = st.db.with(|c| c.execute("UPDATE outbox SET status='failed', error=?2 WHERE id=?1", rusqlite::params![id, msg]));
                st.emit("op-error", json!({ "message": format!("Scheduled message failed: {msg}") }));
            }
        }
        st.emit("outbox-changed", json!({}));
    }
}

#[tauri::command]
pub async fn send_outbox_now(state: S<'_>, id: i64) -> R<()> {
    state
        .db
        .with(|c| c.execute("UPDATE outbox SET send_at=0, status='scheduled' WHERE id=?1", [id]))
        .map_err(ea)?;
    process_outbox(state.inner().clone()).await;
    Ok(())
}

// ---------------- attachments preview ----------------

#[derive(Serialize)]
pub struct AttachmentData {
    name: String,
    mime: String,
    size: usize,
    data: String,
    /// Extracted text for documents the webview can't render (Word, PowerPoint, Excel).
    text: Option<String>,
}

/// Raw attachment bytes (base64) for in-app preview.
#[tauri::command]
pub async fn attachment_data(state: S<'_>, id: i64, index: usize) -> R<AttachmentData> {
    use base64::Engine;
    let raw = raw_bytes(&state, id).await.map_err(ea)?;
    let (name, mime, data) = parse::attachment(&raw, index).ok_or("Attachment not found")?;
    let mime = if mime == "application/octet-stream" {
        mime_guess::from_path(&name).first_or_octet_stream().to_string()
    } else {
        mime
    };
    let lower = name.to_lowercase();
    let text = if [".docx", ".pptx", ".xlsx", ".odt"].iter().any(|e| lower.ends_with(e)) {
        let (n, m, d) = (name.clone(), mime.clone(), data.clone());
        tauri::async_runtime::spawn_blocking(move || crate::docs::extract_text(&n, &m, &d)).await.ok().flatten()
    } else {
        None
    };
    Ok(AttachmentData { size: data.len(), data: base64::engine::general_purpose::STANDARD.encode(&data), name, mime, text })
}

#[tauri::command]
pub async fn save_draft(state: S<'_>, mut message: Outgoing) -> R<()> {
    let st = state.inner().clone();
    let a = st.db.account(message.account_id).map_err(ea)?;
    resolve_attachments(&st, &mut message).await.map_err(ea)?;
    // Drafts may have no recipient yet.
    let (_, raw) = smtp::build(&a, &message).map_err(ea)?;
    if a.protocol == "pop3" {
        pop3::store_local(&st, &a, "drafts", &raw, true).map_err(ea)?;
    } else {
        let drafts = st.db.folder_by_role(a.id, "drafts").map_err(ea)?.ok_or("This account has no Drafts folder")?;
        imap::append(&st, &a, &drafts, &raw, "(\\Seen \\Draft)").await.map_err(ea)?;
    }
    if let Some(d) = message.draft_id {
        remove_draft(&st, d).await;
    }
    st.changed(a.id);
    let aid = a.id;
    tauri::async_runtime::spawn(async move { sync::sync_one(st, aid).await });
    Ok(())
}

// ---------------- signatures ----------------

#[tauri::command]
pub fn list_signatures(state: S) -> R<Vec<Signature>> {
    state.db.signatures().map_err(ea)
}

#[tauri::command]
pub fn save_signature(state: S, signature: Signature) -> R<i64> {
    state.db.save_signature(&signature).map_err(ea)
}

#[tauri::command]
pub fn delete_signature(state: S, id: i64) -> R<()> {
    state.db.delete_signature(id).map_err(ea)
}

// ---------------- calendar ----------------

#[tauri::command]
pub fn list_calendars(state: S) -> R<Vec<Calendar>> {
    state.db.calendars().map_err(ea)
}

#[tauri::command]
pub fn set_calendar_visible(state: S, id: i64, visible: bool) -> R<()> {
    state.db.set_calendar_visible(id, visible).map_err(ea)
}

#[tauri::command]
pub fn list_events(state: S, from: i64, to: i64) -> R<Vec<Event>> {
    state.db.events(from, to).map_err(ea)
}

#[tauri::command]
pub async fn save_event(state: S<'_>, event: Event) -> R<Event> {
    calendar::save_event(&state, event).await.map_err(ea)
}

#[tauri::command]
pub async fn delete_event(state: S<'_>, id: i64) -> R<()> {
    calendar::delete_event(&state, id).await.map_err(ea)
}

#[tauri::command]
pub async fn sync_calendars(state: S<'_>) -> R<()> {
    for a in state.db.accounts().map_err(ea)? {
        if a.auth == "oauth_google" && a.calendar_sync && a.enabled {
            calendar::sync_account(&state, &a).await.map_err(ea)?;
        }
    }
    calendar::sync_all_ics(&state).await;
    Ok(())
}

// ---------------- misc ----------------

#[tauri::command]
pub fn data_dir(state: S) -> String {
    state.db.data_dir.to_string_lossy().to_string()
}

#[tauri::command]
pub fn open_url(app: tauri::AppHandle, url: String) -> R<()> {
    use tauri_plugin_opener::OpenerExt;
    if !(url.starts_with("http://") || url.starts_with("https://") || url.starts_with("mailto:")) {
        return Err("Unsupported link".into());
    }
    app.opener().open_url(url, None::<&str>).map_err(e)
}

// ---------------- quick setup ----------------

#[tauri::command]
pub async fn discover_account(email: String) -> R<crate::autoconfig::Discovery> {
    crate::autoconfig::discover(&email).await.map_err(ea)
}

fn account_from_discovery(d: &crate::autoconfig::Discovery, email: &str, name: &str, protocol: &str) -> Result<Account> {
    let inc = if protocol == "pop3" { d.pop3.clone() } else { d.imap.clone() }
        .or_else(|| if protocol == "pop3" { d.imap.clone() } else { d.pop3.clone() })
        .ok_or_else(|| anyhow!("No incoming server found"))?;
    let proto = if d.pop3.as_ref().map(|p| p.host == inc.host && p.port == inc.port).unwrap_or(false) && d.imap.as_ref().map(|i| i.host != inc.host || i.port != inc.port).unwrap_or(true) { "pop3" } else { "imap" };
    let smtp = d.smtp.clone().ok_or_else(|| anyhow!("No outgoing (SMTP) server found"))?;
    Ok(Account {
        name: email.to_string(),
        email: email.to_string(),
        display_name: name.to_string(),
        protocol: proto.into(),
        auth: "password".into(),
        provider: d.provider.clone(),
        in_host: inc.host,
        in_port: inc.port,
        in_security: inc.security,
        out_host: smtp.host,
        out_port: smtp.port,
        out_security: smtp.security,
        username: inc.username,
        smtp_username: if smtp.username != email { smtp.username } else { String::new() },
        pop3_leave_on_server: true,
        color: "#0f6cbd".into(),
        calendar_sync: true,
        enabled: true,
        sync_limit: 2000,
        body_limit: 200,
        role_map: "{}".into(),
        ..Default::default()
    })
}

/// One-step add: discover settings, verify the login, then save. Nothing is saved if login fails.
#[tauri::command]
pub async fn quick_add_account(
    state: S<'_>,
    email: String,
    password: String,
    display_name: String,
    protocol: String,
    pop3_dir: Option<String>,
    space_id: Option<i64>,
) -> R<i64> {
    let email = email.trim().to_string();
    let d = crate::autoconfig::discover(&email).await.map_err(ea)?;
    let mut a = account_from_discovery(&d, &email, &display_name, &protocol).map_err(ea)?;
    a.pop3_dir = pop3_dir.unwrap_or_default();
    a.space_id = space_id;
    let creds = || crate::state::Creds::Password { user: a.username.clone(), pass: password.clone() };
    let incoming = if a.protocol == "pop3" { pop3::test_with(&a, creds()).await } else {
        imap::connect_with(&a, creds()).await.map(|_| ())
    };
    incoming.map_err(|e| {
        let hint = if !d.note.is_empty() { format!("\n{}", d.note) } else { String::new() };
        format!("Could not sign in: {e:#}{hint}")
    })?;
    let smtp_user = if a.smtp_username.is_empty() { a.username.clone() } else { a.smtp_username.clone() };
    smtp::test_with(&a, crate::state::Creds::Password { user: smtp_user, pass: password.clone() })
        .await
        .map_err(|e| format!("Receiving works but sending (SMTP) failed: {e:#}"))?;
    let id = state.db.save_account(&a).map_err(ea)?;
    oauth::set_secret(&oauth::password_key(id), &password).map_err(ea)?;
    if a.protocol == "pop3" {
        pop3::ensure_local_folders(&state, id).map_err(ea)?;
    }
    state.changed(id);
    let st = state.inner().clone();
    tauri::async_runtime::spawn(async move { sync::sync_one(st, id).await });
    Ok(id)
}

// ---------------- spaces ----------------

#[tauri::command]
pub fn list_spaces(state: S) -> R<Vec<Space>> {
    state.db.spaces().map_err(ea)
}
#[tauri::command]
pub fn save_space(state: S, space: Space) -> R<i64> {
    state.db.save_space(&space).map_err(ea)
}
#[tauri::command]
pub fn delete_space(state: S, id: i64) -> R<()> {
    state.db.delete_space(id).map_err(ea)
}
#[tauri::command]
pub fn set_account_space(state: S, account_id: i64, space_id: Option<i64>) -> R<()> {
    state.db.set_account_space(account_id, space_id).map_err(ea)?;
    state.changed(account_id);
    Ok(())
}

// ---------------- groups & rules ----------------

#[tauri::command]
pub fn list_groups(state: S) -> R<Vec<Group>> {
    state.db.groups().map_err(ea)
}
#[tauri::command]
pub fn save_group(state: S, group: Group) -> R<i64> {
    let id = state.db.save_group(&group).map_err(ea)?;
    state.changed(0);
    Ok(id)
}
#[tauri::command]
pub fn delete_group(state: S, id: i64) -> R<()> {
    state.db.delete_group(id).map_err(ea)?;
    state.changed(0);
    Ok(())
}
#[tauri::command]
pub fn add_to_group(state: S, ids: Vec<i64>, group_id: i64) -> R<()> {
    state.db.add_to_group(&ids, group_id).map_err(ea)?;
    state.changed(0);
    Ok(())
}
#[tauri::command]
pub fn remove_from_group(state: S, ids: Vec<i64>, group_id: i64) -> R<()> {
    state.db.remove_from_group(&ids, group_id).map_err(ea)?;
    state.changed(0);
    Ok(())
}
#[tauri::command]
pub fn message_groups(state: S, id: i64) -> R<Vec<i64>> {
    state.db.message_group_ids(id).map_err(ea)
}
#[tauri::command]
pub fn list_rules(state: S) -> R<Vec<Rule>> {
    state.db.rules().map_err(ea)
}
#[tauri::command]
pub fn save_rule(state: S, rule: Rule) -> R<i64> {
    state.db.save_rule(&rule).map_err(ea)
}
#[tauri::command]
pub fn delete_rule(state: S, id: i64) -> R<()> {
    state.db.delete_rule(id).map_err(ea)
}
#[tauri::command]
pub async fn run_rules(state: S<'_>, rule_id: Option<i64>) -> R<usize> {
    let st = state.inner().clone();
    let n = tauri::async_runtime::spawn_blocking(move || crate::rules::run_on_existing(&st, rule_id))
        .await
        .map_err(e)?
        .map_err(ea)?;
    state.changed(0);
    Ok(n)
}

// ---------------- AI ----------------

#[tauri::command]
pub fn ai_providers(state: S) -> R<Vec<crate::ai::Provider>> {
    crate::ai::providers(&state).map_err(ea)
}
#[tauri::command]
pub fn ai_save_provider(state: S, provider: crate::ai::Provider, api_key: Option<String>) -> R<crate::ai::Provider> {
    crate::ai::save_provider(&state, provider, api_key).map_err(ea)
}
#[tauri::command]
pub fn ai_delete_provider(state: S, id: String) -> R<()> {
    crate::ai::delete_provider(&state, &id).map_err(ea)
}
#[tauri::command]
pub async fn ai_models(state: S<'_>, id: String) -> R<Vec<String>> {
    crate::ai::list_models(&state, &id).await.map_err(ea)
}
#[tauri::command]
pub async fn ai_test(state: S<'_>, id: String) -> R<String> {
    let msgs = vec![crate::ai::ChatMsg { role: "user".into(), content: "Reply with exactly one word: OK".into(), images: vec![] }];
    crate::ai::complete(&state, &id, None, false, "You are a connection test assistant.", &msgs).await.map_err(ea)
}
#[tauri::command]
pub async fn ai_run(state: S<'_>, request: crate::ai::AiRequest) -> R<crate::ai::AiResponse> {
    crate::ai::run(&state, request).await.map_err(ea)
}

#[tauri::command]
pub fn ai_skills(state: S) -> R<Vec<crate::ai::Skill>> {
    crate::ai::skills(&state).map_err(ea)
}
#[tauri::command]
pub fn ai_save_skill(state: S, skill: crate::ai::Skill) -> R<crate::ai::Skill> {
    crate::ai::save_skill(&state, skill).map_err(ea)
}
#[tauri::command]
pub fn ai_delete_skill(state: S, id: String) -> R<()> {
    crate::ai::delete_skill(&state, &id).map_err(ea)
}
#[tauri::command]
pub fn ai_chats(state: S) -> R<Vec<crate::ai::Chat>> {
    crate::ai::chats(&state).map_err(ea)
}
#[tauri::command]
pub fn ai_chat(state: S, id: i64) -> R<crate::ai::Chat> {
    crate::ai::chat(&state, id).map_err(ea)
}
#[tauri::command]
pub fn ai_save_chat(state: S, chat: crate::ai::Chat) -> R<i64> {
    crate::ai::save_chat(&state, &chat).map_err(ea)
}
#[tauri::command]
pub fn ai_delete_chat(state: S, id: i64) -> R<()> {
    crate::ai::delete_chat(&state, id).map_err(ea)
}
#[tauri::command]
pub async fn ai_read_file(path: String) -> R<crate::ai::AiFile> {
    tauri::async_runtime::spawn_blocking(move || crate::ai::read_file(&path)).await.map_err(e)?.map_err(ea)
}
#[tauri::command]
pub async fn ai_read_bytes(name: String, data: String) -> R<crate::ai::AiFile> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD.decode(data.as_bytes()).map_err(e)?;
    tauri::async_runtime::spawn_blocking(move || crate::ai::read_bytes(&name, bytes)).await.map_err(e)?.map_err(ea)
}
#[tauri::command]
pub fn ai_install_skill_dir(state: S, path: String) -> R<crate::ai::Skill> {
    crate::ai::install_skill_dir(&state, &path).map_err(ea)
}
#[tauri::command]
pub fn ai_cancel(id: String) {
    crate::llm::cancel(&id)
}
#[tauri::command]
pub fn web_config(state: S) -> R<serde_json::Value> {
    let get = |k: &str| state.db.get_setting(k).ok().flatten().unwrap_or_default().trim_matches('"').to_string();
    let has_brave = crate::oauth::get_secret(crate::web::BRAVE_KEY).ok().flatten().is_some();
    Ok(json!({ "engine": if get("web_engine").is_empty() { "duckduckgo".to_string() } else { get("web_engine") }, "searxng": get("web_searxng"), "has_brave_key": has_brave }))
}
#[tauri::command]
pub fn set_web_config(state: S, engine: String, searxng: String, brave_key: Option<String>) -> R<()> {
    state.db.set_setting("web_engine", &serde_json::to_string(&engine).map_err(e)?).map_err(ea)?;
    state.db.set_setting("web_searxng", &serde_json::to_string(&searxng).map_err(e)?).map_err(ea)?;
    if let Some(k) = brave_key.filter(|k| !k.trim().is_empty()) {
        crate::oauth::set_secret(crate::web::BRAVE_KEY, k.trim()).map_err(ea)?;
    }
    Ok(())
}
#[tauri::command]
pub async fn web_test(state: S<'_>, query: String) -> R<Vec<crate::web::Hit>> {
    crate::web::search(&state, &query).await.map_err(ea)
}
#[tauri::command]
pub async fn npu_available(state: S<'_>) -> R<crate::npu::Available> {
    Ok(crate::npu::available(&state).await)
}
#[tauri::command]
pub async fn npu_info(force: Option<bool>) -> R<crate::npu::NpuInfo> {
    Ok(crate::npu::info(force.unwrap_or(false)).await)
}
#[tauri::command]
pub async fn npu_sync_models(state: S<'_>) -> R<Vec<String>> {
    crate::npu::sync_models(&state).await;
    Ok(crate::npu::installed_models(&state))
}

#[tauri::command]
pub fn ai_suggestions(state: S) -> R<Vec<crate::ai::Suggestion>> {
    crate::ai::pending_suggestions(&state).map_err(ea)
}
#[tauri::command]
pub fn ai_resolve_suggestion(state: S, message_id: i64, accept: bool) -> R<()> {
    crate::ai::resolve_suggestion(&state, message_id, accept).map_err(ea)?;
    state.changed(0);
    Ok(())
}
#[tauri::command]
pub async fn ai_suggest_now(state: S<'_>) -> R<usize> {
    crate::ai::suggest_now(&state).await.map_err(ea)
}
#[tauri::command]
pub fn ai_suggest_queue(state: S) -> usize {
    crate::ai::suggest_queue(&state).len()
}
#[tauri::command]
pub async fn npu_dirs(state: S<'_>) -> R<crate::npu::Dirs> {
    Ok(crate::npu::dirs(&state).await)
}
#[tauri::command]
pub async fn npu_set_dirs(state: S<'_>, runtime: Option<String>, models: Option<String>, move_files: bool) -> R<crate::npu::Dirs> {
    crate::npu::set_dirs(&state, runtime, models, move_files).await.map_err(ea)
}
/// Open the NPU runtime or models folder in Explorer (only these two folders).
#[tauri::command]
pub fn npu_open_dir(app: tauri::AppHandle, state: S, which: String) -> R<()> {
    use tauri_plugin_opener::OpenerExt;
    let p = if which == "runtime" { crate::npu::runtime_base(&state) } else { crate::npu::models_dir(&state) };
    std::fs::create_dir_all(&p).map_err(e)?;
    app.opener().open_path(p.display().to_string(), None::<&str>).map_err(e)
}
#[tauri::command]
pub async fn npu_install_driver(state: S<'_>) -> R<crate::driver::Outcome> {
    crate::driver::install(&state).await.map_err(ea)
}

// ---------------- on-device AI (NPU) ----------------

#[tauri::command]
pub fn npu_catalog() -> Vec<crate::npu::CatalogModel> {
    crate::npu::catalog()
}
#[tauri::command]
pub async fn npu_status(state: S<'_>) -> R<crate::npu::Status> {
    Ok(crate::npu::status(&state).await)
}
#[tauri::command]
pub async fn npu_install(state: S<'_>) -> R<String> {
    let st = state.inner().clone();
    match crate::npu::install_runtime(&st).await {
        Ok(v) => Ok(v),
        Err(err) => {
            let msg = format!("{err:#}");
            st.emit("npu-progress", json!({ "kind": "runtime", "tag": "", "finished": true, "error": msg }));
            Err(msg)
        }
    }
}
#[tauri::command]
pub async fn npu_pull(state: S<'_>, tag: String) -> R<()> {
    let st = state.inner().clone();
    crate::npu::pull(&st, &tag).await.map_err(ea)
}
#[tauri::command]
pub async fn npu_redownload(state: S<'_>, tag: String) -> R<()> {
    let st = state.inner().clone();
    crate::npu::redownload(&st, &tag).await.map_err(ea)
}
#[tauri::command]
pub fn config_export(state: S, path: String, with_secrets: bool, with_chats: bool) -> R<()> {
    crate::backup::export(&state, std::path::Path::new(&path), with_secrets, with_chats).map_err(ea)
}
#[tauri::command]
pub async fn data_backup(state: S<'_>, path: String, with_secrets: bool) -> R<u64> {
    let st = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || crate::backup::full_backup(&st, std::path::Path::new(&path), with_secrets))
        .await
        .map_err(e)?
        .map_err(ea)
}
#[tauri::command]
pub async fn data_stage_restore(state: S<'_>, source: String) -> R<String> {
    let st = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || crate::backup::stage_restore(&st, std::path::Path::new(&source)))
        .await
        .map_err(e)?
        .map_err(ea)
}
#[tauri::command]
pub fn data_cancel_restore(state: S) {
    crate::backup::cancel_restore(&state)
}
/// After a restore: where the previous data was kept (once; then forgotten).
#[tauri::command]
pub fn restored_note(state: S) -> Option<String> {
    let v = state.db.get_setting("restored_note").ok().flatten()?;
    let _ = state.db.set_setting("restored_note", "\"\"");
    let p: String = serde_json::from_str(&v).ok()?;
    if p.is_empty() { None } else { Some(p) }
}
#[tauri::command]
pub fn app_restart(app: tauri::AppHandle) {
    app.restart();
}
#[tauri::command]
pub fn config_import(state: S, path: String) -> R<crate::backup::ImportReport> {
    crate::backup::import(&state, std::path::Path::new(&path)).map_err(ea)
}
#[tauri::command]
pub fn sync_log() -> Vec<crate::sync::LogLine> {
    crate::sync::log_lines()
}
#[tauri::command]
pub fn sync_log_clear() {
    crate::sync::log_clear()
}
/// A toast click that happened before the page was listening.
#[tauri::command]
pub fn notification_pending() -> Option<crate::notify::OpenRequest> {
    crate::notify::take_pending()
}
#[tauri::command]
pub fn test_notification(state: S) -> R<()> {
    use tauri_plugin_notification::NotificationExt;
    state
        .app
        .notification()
        .builder()
        .title("New email")
        .body("From: Hermio\nTo: you\nSubject: Notifications are working — new mail will look like this")
        .show()
        .map_err(e)
}
#[tauri::command]
pub fn sync_log_open(app: tauri::AppHandle, state: S) -> R<()> {
    use tauri_plugin_opener::OpenerExt;
    let p = crate::sync::log_file(&state);
    if !p.exists() {
        std::fs::write(&p, "").map_err(e)?;
    }
    app.opener().open_path(p.display().to_string(), None::<&str>).map_err(e)
}
#[tauri::command]
pub fn npu_cancel_pull(tag: String) {
    crate::npu::cancel_pull(&tag)
}
#[tauri::command]
pub async fn npu_remove(state: S<'_>, tag: String) -> R<()> {
    crate::npu::remove_model(&state, &tag).await.map_err(ea)
}
#[tauri::command]
pub async fn npu_start(state: S<'_>) -> R<()> {
    let m = crate::npu::default_model(&state);
    crate::npu::ensure_server(&state, &m).await.map_err(ea)
}
#[tauri::command]
pub async fn npu_stop(state: S<'_>) -> R<()> {
    crate::npu::stop_server(&state).await;
    Ok(())
}

// ---------------- calendars (ICS) ----------------

#[tauri::command]
pub async fn add_ics_calendar(state: S<'_>, name: String, url: String, color: String) -> R<usize> {
    let url = crate::ics::normalize_url(&url);
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("Calendar link must start with https://, http:// or webcal://".into());
    }
    let id = state.db.add_ics_calendar(if name.trim().is_empty() { "ICS calendar" } else { name.trim() }, &url, &color).map_err(ea)?;
    let cal = state.db.calendar(id).map_err(ea)?;
    calendar::sync_ics(&state, &cal).await.map_err(ea)
}
#[tauri::command]
pub fn import_ics_file(state: S, path: String, calendar_id: i64) -> R<usize> {
    let bytes = std::fs::read(&path).map_err(e)?;
    let text = String::from_utf8_lossy(&bytes);
    let (from, to) = crate::ics::window();
    let evs = crate::ics::parse(&text, from - 3650 * 86400, to).map_err(ea)?;
    let n = evs.len();
    for mut ev in evs {
        ev.calendar_id = calendar_id;
        ev.remote_id = String::new();
        state.db.save_event(&ev).map_err(ea)?;
    }
    state.emit("calendar-changed", json!({ "calendar_id": calendar_id }));
    Ok(n)
}
#[tauri::command]
pub fn update_calendar(state: S, id: i64, name: String, color: String) -> R<()> {
    state.db.update_calendar(id, &name, &color).map_err(ea)
}
#[tauri::command]
pub fn delete_calendar(state: S, id: i64) -> R<()> {
    state.db.delete_calendar(id).map_err(ea)
}

// ---------------- app / tray ----------------

#[tauri::command]
pub fn start_hidden() -> bool {
    std::env::args().any(|a| a == "--hidden")
}
#[tauri::command]
pub fn get_autostart(app: tauri::AppHandle) -> bool {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch().is_enabled().unwrap_or(false)
}
#[tauri::command]
pub fn set_autostart(app: tauri::AppHandle, enabled: bool) -> R<()> {
    use tauri_plugin_autostart::ManagerExt;
    let al = app.autolaunch();
    if enabled { al.enable().map_err(e) } else { al.disable().map_err(e) }
}
#[tauri::command]
pub fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}
