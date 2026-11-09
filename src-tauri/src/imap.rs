//! IMAP: folder discovery, incremental sync, body caching and server-side operations.

use crate::models::*;
use crate::net::{self, Conn};
use crate::parse;
use crate::rules;
use crate::state::{AppState, Creds};
use anyhow::{anyhow, bail, Context, Result};
use async_imap::types::{Fetch, Flag, NameAttribute};
use async_imap::Session;
use futures::TryStreamExt;
use std::collections::HashMap;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

pub type ImapSession = Session<Conn>;

/// How many of the newest messages per folder are listed on first sync.
const INITIAL_WINDOW: u32 = 2000;
/// How many bodies per folder are downloaded (for full-text search) on each sync.
const BODY_BATCH: i64 = 200;

struct XOAuth2(String);
impl async_imap::Authenticator for XOAuth2 {
    type Response = String;
    fn process(&mut self, _challenge: &[u8]) -> String {
        std::mem::take(&mut self.0)
    }
}

pub async fn connect(state: &AppState, a: &Account) -> Result<ImapSession> {
    let creds = state.creds(a).await?;
    connect_with(a, creds).await
}

pub async fn connect_with(a: &Account, creds: Creds) -> Result<ImapSession> {
    let conn = match a.in_security.as_str() {
        "starttls" => {
            let tcp = net::tcp(&a.in_host, a.in_port).await?;
            let mut rd = BufReader::new(tcp);
            let mut line = String::new();
            rd.read_line(&mut line).await?; // greeting
            rd.get_mut().write_all(b"a0 STARTTLS\r\n").await?;
            loop {
                line.clear();
                if rd.read_line(&mut line).await? == 0 {
                    bail!("Server closed the connection during STARTTLS");
                }
                if line.starts_with("a0 ") {
                    if !line.to_ascii_uppercase().starts_with("A0 OK") {
                        bail!("Server does not support STARTTLS: {}", line.trim());
                    }
                    break;
                }
            }
            net::upgrade(&a.in_host, rd.into_inner()).await?
        }
        sec => net::connect(&a.in_host, a.in_port, sec).await?,
    };
    let greet_needed = a.in_security != "starttls";
    let mut client = async_imap::Client::new(conn);
    if greet_needed {
        tokio::time::timeout(net::TIMEOUT, client.read_response())
            .await
            .map_err(|_| anyhow!("IMAP server is not responding"))??
            .ok_or_else(|| anyhow!("IMAP server closed the connection"))?;
    }
    // A server that never answers the login must not hang the sync forever.
    let wait = net::TIMEOUT * 2;
    let session = match creds {
        Creds::Password { user, pass } => tokio::time::timeout(wait, client.login(&user, &pass))
            .await
            .map_err(|_| anyhow!("The server didn't answer the sign-in within {}s", wait.as_secs()))?
            .map_err(|(e, _)| auth_err(e))?,
        Creds::OAuth { user, token } => tokio::time::timeout(wait, client.authenticate("XOAUTH2", XOAuth2(crate::oauth::xoauth2_string(&user, &token))))
            .await
            .map_err(|_| anyhow!("The server didn't answer the Google sign-in within {}s", wait.as_secs()))?
            .map_err(|(e, _)| auth_err(e))?,
    };
    Ok(session)
}

fn auth_err(e: async_imap::error::Error) -> anyhow::Error {
    anyhow!("IMAP sign-in failed: {e}. Check the username/password (Gmail needs OAuth or an App Password).")
}

fn role_of(path: &str, attrs: &[NameAttribute]) -> &'static str {
    for a in attrs {
        match a {
            NameAttribute::Sent => return "sent",
            NameAttribute::Drafts => return "drafts",
            NameAttribute::Trash => return "trash",
            NameAttribute::Junk => return "junk",
            NameAttribute::Archive => return "archive",
            NameAttribute::All => return "all",
            NameAttribute::Flagged => return "flagged",
            NameAttribute::Extension(x) if x.eq_ignore_ascii_case("\\Important") => return "important",
            _ => {}
        }
    }
    let last = path.rsplit(['/', '.']).next().unwrap_or(path).to_lowercase();
    if path.eq_ignore_ascii_case("INBOX") {
        return "inbox";
    }
    match last.as_str() {
        "sent" | "sent items" | "sent mail" | "sent messages" | "đã gửi" => "sent",
        "drafts" | "draft" | "thư nháp" => "drafts",
        "trash" | "deleted items" | "deleted messages" | "bin" | "thùng rác" => "trash",
        "junk" | "spam" | "junk e-mail" | "junk email" | "thư rác" => "junk",
        "archive" | "archives" | "lưu trữ" => "archive",
        _ => "",
    }
}

fn display_name(path: &str, delim: &str) -> String {
    if path.eq_ignore_ascii_case("INBOX") {
        return "Inbox".into();
    }
    let decoded = utf7_imap::decode_utf7_imap(path.to_string());
    match delim {
        "" => decoded,
        d => decoded.rsplit(d).next().unwrap_or(&decoded).to_string(),
    }
}

pub async fn sync_folders(state: &AppState, s: &mut ImapSession, a: &Account) -> Result<()> {
    let names: Vec<_> = s.list(Some(""), Some("*")).await?.try_collect().await?;
    let mut keep = vec![];
    // Advanced: user-chosen special folders override detection.
    let overrides: HashMap<String, String> = serde_json::from_str(&a.role_map).unwrap_or_default();
    for n in &names {
        let path = n.name().to_string();
        let delim = n.delimiter().unwrap_or("/").to_string();
        let selectable = !n.attributes().iter().any(|x| matches!(x, NameAttribute::NoSelect));
        let mut role = role_of(&path, n.attributes());
        // Gmail's root container "[Gmail]" is not selectable; keep it out of the tree.
        if !selectable && (path == "[Gmail]" || path == "[Google Mail]") {
            continue;
        }
        if role == "inbox" && path != "INBOX" && !path.eq_ignore_ascii_case("INBOX") {
            role = "";
        }
        if !overrides.is_empty() {
            if let Some((r, _)) = overrides.iter().find(|(_, p)| **p == path) {
                role = match r.as_str() { "sent" => "sent", "drafts" => "drafts", "trash" => "trash", "junk" => "junk", "archive" => "archive", _ => role };
            } else if overrides.contains_key(role) {
                role = "";
            }
        }
        state.db.upsert_folder(a.id, &path, &display_name(&path, &delim), &delim, role, selectable)?;
        keep.push(path);
    }
    state.db.remove_folders_not_in(a.id, &keep)?;
    Ok(())
}

fn flags_of(f: &Fetch) -> (bool, bool, bool) {
    let mut seen = false;
    let mut flagged = false;
    let mut answered = false;
    for fl in f.flags() {
        match fl {
            Flag::Seen => seen = true,
            Flag::Flagged => flagged = true,
            Flag::Answered => answered = true,
            _ => {}
        }
    }
    (seen, flagged, answered)
}

/// Whether a folder's messages are synced (virtual Gmail folders are not).
pub fn syncable(f: &Folder) -> bool {
    f.selectable && !matches!(f.role.as_str(), "all" | "flagged" | "important")
}

/// Sync one folder: new headers, flag changes, deletions and a batch of bodies.
/// Returns the number of new unread messages in this folder.
pub async fn sync_folder(state: &AppState, s: &mut ImapSession, a: &Account, folder: &Folder) -> Result<usize> {
    let mb = tokio::time::timeout(net::TIMEOUT * 2, s.select(&folder.path))
        .await
        .map_err(|_| anyhow!("SELECT {} timed out", folder.path))?
        .with_context(|| format!("SELECT {}", folder.path))?;
    let validity = mb.uid_validity.unwrap_or(0);
    let mut last_uid = folder.last_uid;
    if folder.uidvalidity != 0 && folder.uidvalidity != validity {
        state.db.clear_folder(folder.id)?;
        last_uid = 0;
    }
    let exists = mb.exists;
    let mut new_unread = 0usize;
    let mut max_uid = last_uid;
    let rules = if folder.role == "inbox" { state.db.rules()? } else { vec![] };
    let mut to_seen: Vec<u32> = vec![];
    let mut to_flag: Vec<u32> = vec![];
    let window = if a.sync_limit > 0 { a.sync_limit as u32 } else { INITIAL_WINDOW };
    let body_batch = if a.body_limit > 0 { a.body_limit } else { BODY_BATCH };
    let mut new_ids: Vec<i64> = vec![];

    let inbox = folder.role == "inbox";
    if inbox {
        crate::sync::log(state, a, "info", format!("{}: {} messages on the server (UIDVALIDITY {validity}, last seen UID {last_uid})", folder.name, exists));
    }
    if exists > 0 {
        // 1) New messages. First just their UIDs, then the headers in small batches, so one
        //    message the server (or the parser) chokes on can't block the whole folder.
        let query = "(UID FLAGS RFC822.SIZE INTERNALDATE BODY.PEEK[HEADER])";
        let mut uids: Vec<u32> = if last_uid == 0 {
            let start = exists.saturating_sub(window.max(1) - 1).max(1);
            let v: Vec<Fetch> = s.fetch(format!("{start}:*"), "(UID)").await.context("listing new messages")?.try_collect().await.context("listing new messages")?;
            v.iter().filter_map(|f| f.uid).collect()
        } else {
            let v: Vec<Fetch> = s.uid_fetch(format!("{}:*", last_uid + 1), "(UID)").await.context("listing new messages")?.try_collect().await.context("listing new messages")?;
            v.iter().filter_map(|f| f.uid).filter(|u| *u > last_uid).collect()
        };
        uids.sort_unstable();
        uids.dedup();
        let mut fetches: Vec<Fetch> = vec![];
        let mut skipped = 0usize;
        for chunk in uids.chunks(100) {
            let set = chunk.iter().map(|u| u.to_string()).collect::<Vec<_>>().join(",");
            let batch: Result<Vec<Fetch>> = match s.uid_fetch(&set, query).await {
                Ok(st) => st.try_collect().await.map_err(Into::into),
                Err(e) => Err(e.into()),
            };
            match batch {
                Ok(v) => fetches.extend(v),
                Err(e) => {
                    crate::sync::log(state, a, "warn", format!("{}: a batch of {} headers failed ({e:#}); retrying one by one", folder.name, chunk.len()));
                    *s = connect(state, a).await?;
                    s.select(&folder.path).await?;
                    for u in chunk {
                        let one: Result<Vec<Fetch>> = match s.uid_fetch(u.to_string(), query).await {
                            Ok(st) => st.try_collect().await.map_err(Into::into),
                            Err(e) => Err(e.into()),
                        };
                        match one {
                            Ok(v) => fetches.extend(v),
                            Err(e) => {
                                skipped += 1;
                                crate::sync::log(state, a, "warn", format!("{}: skipped message UID {u}: {e:#}", folder.name));
                                *s = connect(state, a).await?;
                                s.select(&folder.path).await?;
                            }
                        }
                    }
                }
            }
        }
        // Never get stuck on a message that can't be read: move past it.
        if let Some(&top) = uids.last() {
            max_uid = max_uid.max(top);
        }
        let n_new = fetches.len();
        let notify_grouped = crate::notify::include_grouped(state);
        let mut to_notify: Vec<crate::notify::NewMail> = vec![];
        let (mut quiet_rules, mut quiet_groups) = (0usize, 0usize);
        for f in fetches {
            let Some(uid) = f.uid else { continue };
            if uid <= last_uid {
                continue;
            }
            let header = f.header().unwrap_or(b"");
            let mut m = parse::summarize(header, false);
            let (seen, flagged, answered) = flags_of(&f);
            m.account_id = a.id;
            m.folder_id = folder.id;
            m.uid = uid;
            m.seen = seen;
            m.flagged = flagged;
            m.answered = answered;
            m.size = f.size.unwrap_or(0) as i64;
            if m.date == 0 {
                m.date = f.internal_date().map(|d| d.timestamp()).unwrap_or(0);
            }
            m.has_attachments = header_suggests_attachments(header);
            if folder.role == "inbox" {
                state.db.learn_contact(&m.from_addr, &m.from_name)?;
            }
            let id = state.db.insert_message(&m)?;
            new_ids.push(id);
            max_uid = max_uid.max(uid);
            if folder.role == "inbox" {
                let fx = match state.db.message(id).and_then(|row| rules::apply(state, &rules, &row, None)) {
                    Ok(fx) => fx,
                    Err(e) => {
                        eprintln!("rules: {e:#}");
                        rules::Effects::default()
                    }
                };
                if fx.mark_read && !seen { to_seen.push(uid); }
                if fx.flag && !flagged { to_flag.push(uid); }
                // The first sync of a folder brings in old mail: nothing to announce.
                if !seen && last_uid != 0 {
                    if fx.mark_read {
                        quiet_rules += 1;
                    } else if crate::commands::hidden_from_inbox(state, id) && !notify_grouped {
                        quiet_groups += 1;
                    } else {
                        new_unread += 1;
                        let from = if m.from_name.is_empty() { m.from_addr.clone() } else { m.from_name.clone() };
                        state.emit("new-mail", serde_json::json!({ "account": a.email, "from": from, "subject": m.subject }));
                        to_notify.push(crate::notify::NewMail { id, from_name: m.from_name.clone(), from_addr: m.from_addr.clone(), date: m.date, subject: m.subject.clone() });
                    }
                }
            }
        }

        if !to_notify.is_empty() || quiet_rules + quiet_groups > 0 {
            let mut why = vec![];
            if quiet_groups > 0 {
                why.push(format!("{quiet_groups} sorted into hidden groups"));
            }
            if quiet_rules > 0 {
                why.push(format!("{quiet_rules} marked read by rules"));
            }
            crate::sync::log(
                state,
                a,
                "info",
                format!("{}: notifying about {} new message(s){}", folder.name, to_notify.len(), if why.is_empty() { String::new() } else { format!(" (no notification for {})", why.join(", ")) }),
            );
        }
        crate::notify::new_mail(state, a.id, &a.email, &to_notify);
        if inbox || n_new > 0 {
            crate::sync::log(
                state,
                a,
                if skipped > 0 { "warn" } else { "info" },
                format!("{}: {n_new} new message(s){}", folder.name, if skipped > 0 { format!(", {skipped} skipped") } else { String::new() }),
            );
        }
        // Remember the new high-water mark right away: a later error must not make the next sync
        // download (and duplicate) these messages again.
        state.db.set_folder_sync(folder.id, validity, max_uid)?;

        // 2) Flags + deletions for the window we keep locally
        if let Err(e) = sync_flags(state, s, folder).await {
            crate::sync::log(state, a, "warn", format!("{}: updating read/flag state failed: {e:#}", folder.name));
        }
    }
    state.db.set_folder_sync(folder.id, validity, max_uid)?;

    // 3) Bodies for full-text search (newest first). New mail is always included, whatever its size.
    match sync_bodies(state, s, a, folder, body_batch, &new_ids, &mut to_seen, &mut to_flag).await {
        Ok(n) if n > 0 => crate::sync::log(state, a, "info", format!("{}: downloaded {n} message bodies for search", folder.name)),
        Ok(_) => {}
        Err(e) => crate::sync::log(state, a, "warn", format!("{}: downloading message bodies failed: {e:#}", folder.name)),
    }
    // 4) Rules once more over the new mail, exactly like "Run all on existing mail".
    if folder.role == "inbox" && !new_ids.is_empty() {
        match rules::apply_new(&state.db, &new_ids) {
            Ok(n) if n > 0 => crate::sync::log(state, a, "info", format!("Rules sorted {n} new message(s)")),
            Ok(_) => {}
            Err(e) => crate::sync::log(state, a, "warn", format!("Rules: {e:#}")),
        }
    }
    // Push rule effects back to the server.
    let join = |v: &[u32]| v.iter().map(|u| u.to_string()).collect::<Vec<_>>().join(",");
    if !to_seen.is_empty() {
        let _: Vec<_> = s.uid_store(join(&to_seen), "+FLAGS.SILENT (\\Seen)").await?.try_collect().await?;
    }
    if !to_flag.is_empty() {
        let _: Vec<_> = s.uid_store(join(&to_flag), "+FLAGS.SILENT (\\Flagged)").await?.try_collect().await?;
    }
    Ok(new_unread)
}

async fn sync_flags(state: &AppState, s: &mut ImapSession, folder: &Folder) -> Result<()> {
    {
        let local = state.db.folder_uids(folder.id)?;
        if !local.is_empty() {
            let min_uid = local.iter().map(|x| x.1).min().unwrap_or(1).max(1);
            let server: Vec<Fetch> = s.uid_fetch(format!("{min_uid}:*"), "(UID FLAGS)").await?.try_collect().await?;
            let mut server_map: HashMap<u32, (bool, bool, bool)> = HashMap::new();
            for f in &server {
                if let Some(uid) = f.uid {
                    server_map.insert(uid, flags_of(f));
                }
            }
            let mut gone = vec![];
            for (id, uid, seen, flagged, answered) in local {
                match server_map.get(&uid) {
                    None => gone.push(id),
                    Some(&(s2, f2, a2)) => {
                        if s2 != seen || f2 != flagged || a2 != answered {
                            state.db.set_flags(id, Some(s2), Some(f2), Some(a2))?;
                        }
                    }
                }
            }
            if !gone.is_empty() {
                state.db.delete_messages(&gone)?;
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn sync_bodies(
    state: &AppState,
    s: &mut ImapSession,
    a: &Account,
    folder: &Folder,
    body_batch: i64,
    new_ids: &[i64],
    to_seen: &mut Vec<u32>,
    to_flag: &mut Vec<u32>,
) -> Result<usize> {
    let mut indexed = 0usize;
    let mut todo = state.db.unindexed_messages(folder.id, body_batch)?;
    for &id in new_ids {
        if !todo.iter().any(|(x, _)| *x == id) {
            if let Ok(m) = state.db.message(id) {
                // Very large new messages too (up to 40 MB), so body rules can see them.
                if m.raw_path.is_empty() && m.size < 40_000_000 {
                    todo.push((id, m.uid));
                }
            }
        }
    }
    for chunk in todo.chunks(25) {
        let set = chunk.iter().map(|(_, u)| u.to_string()).collect::<Vec<_>>().join(",");
        let ids: HashMap<u32, i64> = chunk.iter().map(|(id, u)| (*u, *id)).collect();
        // One bad message must not stop the others (or every later sync) from being indexed and sorted.
        let bodies: Vec<Fetch> = match s.uid_fetch(&set, "(UID BODY.PEEK[])").await {
            Ok(stream) => match stream.try_collect().await {
                Ok(v) => v,
                Err(e) => {
                    crate::sync::log(state, a, "warn", format!("{}: a batch of bodies failed: {e:#}", folder.name));
                    continue;
                }
            },
            Err(e) => return Err(e.into()),
        };
        for f in bodies {
            let (Some(uid), Some(raw)) = (f.uid, f.body()) else { continue };
            let Some(&id) = ids.get(&uid) else { continue };
            match store_body(state, a.id, folder.id, id, uid, raw) {
                Ok((_, fx)) => {
                    indexed += 1;
                    if fx.mark_read { to_seen.push(uid); }
                    if fx.flag { to_flag.push(uid); }
                }
                Err(e) => crate::sync::log(state, a, "warn", format!("{}: couldn't store message UID {uid}: {e:#}", folder.name)),
            }
        }
    }
    Ok(indexed)
}

fn header_suggests_attachments(h: &[u8]) -> bool {
    let s = String::from_utf8_lossy(h).to_ascii_lowercase();
    s.contains("multipart/mixed")
}

pub fn store_body(state: &AppState, account_id: i64, folder_id: i64, id: i64, uid: u32, raw: &[u8]) -> Result<(String, rules::Effects)> {
    let dir = state.db.cache_dir(account_id).join(folder_id.to_string());
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{uid}.eml"));
    std::fs::write(&path, raw)?;
    let sum = parse::summarize(raw, true);
    let p = path.to_string_lossy().to_string();
    let body = sum.body.as_deref().unwrap_or("");
    state.db.set_body_index(id, &p, body, &sum.snippet, sum.has_attachments)?;
    let rules = state.db.rules()?;
    let fx = if rules.iter().any(|r| r.enabled) {
        let row = state.db.message(id)?;
        rules::apply(state, &rules, &row, Some(body))?
    } else {
        rules::Effects::default()
    };
    Ok((p, fx))
}

/// Download one message's full source (used when opening a message that isn't cached yet).
pub async fn fetch_raw(state: &AppState, a: &Account, m: &MessageRow) -> Result<Vec<u8>> {
    let folder = state.db.folder(m.folder_id)?;
    let mut s = connect(state, a).await?;
    s.examine(&folder.path).await?;
    let f: Vec<Fetch> = s.uid_fetch(m.uid.to_string(), "(UID BODY.PEEK[])").await?.try_collect().await?;
    let _ = s.logout().await;
    let raw = f
        .iter()
        .find_map(|x| x.body().map(|b| b.to_vec()))
        .ok_or_else(|| anyhow!("The message is no longer on the server"))?;
    store_body(state, a.id, folder.id, m.id, m.uid, &raw)?;
    Ok(raw)
}

pub async fn sync_account(state: &AppState, a: &Account) -> Result<usize> {
    crate::sync::log(state, a, "info", format!("Connecting to {}:{} ({})…", a.in_host, a.in_port, a.in_security));
    let mut s = connect(state, a).await.context("Signing in to the mail server")?;
    crate::sync::log(state, a, "ok", "Signed in");
    sync_folders(state, &mut s, a).await.context("Listing folders")?;
    state.changed(a.id);
    let folders: Vec<Folder> = state.db.folders()?.into_iter().filter(|f| f.account_id == a.id && syncable(f)).collect();
    if !folders.iter().any(|f| f.role == "inbox") {
        crate::sync::log(state, a, "warn", "The server listed no INBOX folder");
    }
    crate::sync::log(state, a, "info", format!("{} folders to sync", folders.len()));
    // Inbox first, then the special folders, then the rest.
    let mut folders = folders;
    folders.sort_by_key(|f| match f.role.as_str() { "inbox" => 0, "sent" => 1, "drafts" => 2, "" => 4, _ => 3 });
    let mut new_total = 0;
    let mut inbox_err: Option<anyhow::Error> = None;
    for f in folders {
        match sync_folder(state, &mut s, a, &f).await {
            Ok(n) => new_total += n,
            Err(e) => {
                crate::sync::log(state, a, "error", format!("{} failed: {e:#}", f.name));
                if f.role == "inbox" {
                    inbox_err = Some(e);
                }
                // The connection may be unusable after an error: start a fresh one.
                match connect(state, a).await {
                    Ok(ns) => s = ns,
                    Err(e) => return Err(e.context("Reconnecting after an error")),
                }
            }
        }
        state.changed(a.id);
    }
    let _ = s.logout().await;
    // The account isn't "fine" when its inbox couldn't be synced.
    if let Some(e) = inbox_err {
        return Err(e.context("Inbox"));
    }
    Ok(new_total)
}

fn uid_set(msgs: &[MessageRow]) -> String {
    msgs.iter().map(|m| m.uid.to_string()).collect::<Vec<_>>().join(",")
}

/// Group messages by folder so each server operation happens in one SELECT.
fn by_folder(msgs: &[MessageRow]) -> HashMap<i64, Vec<MessageRow>> {
    let mut map: HashMap<i64, Vec<MessageRow>> = HashMap::new();
    for m in msgs {
        map.entry(m.folder_id).or_default().push(m.clone());
    }
    map
}

pub async fn store_flags(state: &AppState, a: &Account, msgs: &[MessageRow], flag: &str, on: bool) -> Result<()> {
    let mut s = connect(state, a).await?;
    for (fid, group) in by_folder(msgs) {
        let folder = state.db.folder(fid)?;
        s.select(&folder.path).await?;
        let op = if on { "+FLAGS.SILENT" } else { "-FLAGS.SILENT" };
        let _: Vec<_> = s.uid_store(uid_set(&group), format!("{op} ({flag})")).await?.try_collect().await?;
    }
    let _ = s.logout().await;
    Ok(())
}

/// Move messages to another folder (MOVE, or COPY + delete + EXPUNGE).
pub async fn move_to(state: &AppState, a: &Account, msgs: &[MessageRow], dest: &Folder) -> Result<()> {
    let mut s = connect(state, a).await?;
    let caps = s.capabilities().await?;
    let has_move = caps.has_str("MOVE");
    for (fid, group) in by_folder(msgs) {
        if fid == dest.id {
            continue;
        }
        let folder = state.db.folder(fid)?;
        s.select(&folder.path).await?;
        let set = uid_set(&group);
        if has_move {
            s.uid_mv(&set, &dest.path).await?;
        } else {
            s.uid_copy(&set, &dest.path).await?;
            let _: Vec<_> = s.uid_store(&set, "+FLAGS.SILENT (\\Deleted)").await?.try_collect().await?;
            let _: Vec<_> = s.uid_expunge(&set).await?.try_collect().await?;
        }
    }
    let _ = s.logout().await;
    Ok(())
}

/// Permanently delete messages.
pub async fn expunge(state: &AppState, a: &Account, msgs: &[MessageRow]) -> Result<()> {
    let mut s = connect(state, a).await?;
    for (fid, group) in by_folder(msgs) {
        let folder = state.db.folder(fid)?;
        s.select(&folder.path).await?;
        let set = uid_set(&group);
        let _: Vec<_> = s.uid_store(&set, "+FLAGS.SILENT (\\Deleted)").await?.try_collect().await?;
        let _: Vec<_> = s.uid_expunge(&set).await?.try_collect().await?;
    }
    let _ = s.logout().await;
    Ok(())
}

pub async fn append(state: &AppState, a: &Account, folder: &Folder, raw: &[u8], flags: &str) -> Result<()> {
    let mut s = connect(state, a).await?;
    s.append(&folder.path, Some(flags), None, raw).await?;
    let _ = s.logout().await;
    Ok(())
}

pub async fn create_folder(state: &AppState, a: &Account, path: &str) -> Result<()> {
    let mut s = connect(state, a).await?;
    s.create(path).await?;
    s.subscribe(path).await.ok();
    sync_folders(state, &mut s, a).await?;
    let _ = s.logout().await;
    Ok(())
}

pub async fn test(state: &AppState, a: &Account) -> Result<()> {
    let mut s = connect(state, a).await?;
    s.examine("INBOX").await?;
    let _ = s.logout().await;
    Ok(())
}

/// Download (and index) the bodies of several messages over one connection.
pub async fn fetch_bodies(state: &AppState, a: &Account, msgs: &[MessageRow]) -> Result<()> {
    if msgs.is_empty() {
        return Ok(());
    }
    let mut s = connect(state, a).await?;
    for (fid, group) in by_folder(msgs) {
        let folder = state.db.folder(fid)?;
        if s.examine(&folder.path).await.is_err() {
            continue;
        }
        for chunk in group.chunks(25) {
            let set = chunk.iter().map(|m| m.uid.to_string()).collect::<Vec<_>>().join(",");
            let ids: HashMap<u32, i64> = chunk.iter().map(|m| (m.uid, m.id)).collect();
            let bodies: Vec<Fetch> = s.uid_fetch(&set, "(UID BODY.PEEK[])").await?.try_collect().await?;
            for f in bodies {
                let (Some(uid), Some(raw)) = (f.uid, f.body()) else { continue };
                if let Some(&id) = ids.get(&uid) {
                    let _ = store_body(state, a.id, folder.id, id, uid, raw);
                }
            }
        }
    }
    let _ = s.logout().await;
    Ok(())
}
