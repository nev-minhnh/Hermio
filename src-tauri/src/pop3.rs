//! POP3 client. Messages are stored as .eml files under the account's own data folder.

use crate::models::*;
use crate::net::{self, Conn};
use crate::parse;
use crate::state::{AppState, Creds};
use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// Messages downloaded per sync run (Gmail hands POP3 mail out in batches anyway).
const MAX_PER_RUN: usize = 600;

pub const LOCAL_FOLDERS: &[(&str, &str, &str)] = &[
    ("Inbox", "Inbox", "inbox"),
    ("Drafts", "Drafts", "drafts"),
    ("Sent", "Sent", "sent"),
    ("Archive", "Archive", "archive"),
    ("Junk", "Junk", "junk"),
    ("Trash", "Trash", "trash"),
];

pub fn ensure_local_folders(state: &AppState, account_id: i64) -> Result<()> {
    for (path, name, role) in LOCAL_FOLDERS {
        state.db.upsert_folder(account_id, path, name, "/", role, true)?;
    }
    Ok(())
}

pub fn data_dir(state: &AppState, a: &Account) -> PathBuf {
    if a.pop3_dir.trim().is_empty() {
        state.db.data_dir.join("pop3").join(a.id.to_string())
    } else {
        PathBuf::from(a.pop3_dir.trim())
    }
}

struct Pop {
    rd: BufReader<Conn>,
}

impl Pop {
    async fn line(&mut self) -> Result<String> {
        let mut l = String::new();
        let n = tokio::time::timeout(net::TIMEOUT, self.rd.read_line(&mut l))
            .await
            .map_err(|_| anyhow!("POP3 server is not responding"))??;
        if n == 0 {
            bail!("POP3 server closed the connection");
        }
        Ok(l.trim_end_matches(['\r', '\n']).to_string())
    }

    async fn cmd(&mut self, c: &str) -> Result<String> {
        self.rd.get_mut().write_all(format!("{c}\r\n").as_bytes()).await?;
        self.rd.get_mut().flush().await?;
        let l = self.line().await?;
        if l.starts_with("+OK") || l.starts_with('+') {
            Ok(l)
        } else {
            let shown = if c.starts_with("PASS") { "PASS ****" } else { c.split(' ').next().unwrap_or(c) };
            bail!("{} → {}", shown, l)
        }
    }

    /// Read a dot-terminated multi-line response body.
    async fn multiline(&mut self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        let mut buf = Vec::new();
        loop {
            buf.clear();
            let n = tokio::time::timeout(std::time::Duration::from_secs(120), self.rd.read_until(b'\n', &mut buf))
                .await
                .map_err(|_| anyhow!("Timed out downloading POP3 mail"))??;
            if n == 0 {
                bail!("The POP3 connection dropped");
            }
            let trimmed: &[u8] = if buf.ends_with(b"\r\n") { &buf[..buf.len() - 2] } else if buf.ends_with(b"\n") { &buf[..buf.len() - 1] } else { &buf };
            if trimmed == b"." {
                break;
            }
            let line = if trimmed.starts_with(b"..") { &trimmed[1..] } else { trimmed };
            out.extend_from_slice(line);
            out.extend_from_slice(b"\r\n");
        }
        Ok(out)
    }
}

async fn connect(state: &AppState, a: &Account) -> Result<Pop> {
    let creds = state.creds(a).await?;
    connect_with(a, creds).await
}

async fn connect_with(a: &Account, creds: Creds) -> Result<Pop> {
    let mut pop = match a.in_security.as_str() {
        "starttls" => {
            let tcp = net::tcp(&a.in_host, a.in_port).await?;
            let mut p = Pop { rd: BufReader::new(Conn::Plain(tcp)) };
            p.line().await?;
            p.cmd("STLS").await.context("Server does not support STLS")?;
            let tcp = p.rd.into_inner().into_tcp().ok_or_else(|| anyhow!("STLS"))?;
            Pop { rd: BufReader::new(net::upgrade(&a.in_host, tcp).await?) }
        }
        sec => {
            let c = net::connect(&a.in_host, a.in_port, sec).await?;
            let mut p = Pop { rd: BufReader::new(c) };
            let g = p.line().await?;
            if !g.starts_with("+OK") {
                bail!("Invalid POP3 greeting: {g}");
            }
            p
        }
    };
    let recent = |u: &str| if a.pop3_recent_mode && !u.starts_with("recent:") { format!("recent:{u}") } else { u.to_string() };
    match creds {
        Creds::Password { user, pass } => {
            pop.cmd(&format!("USER {}", recent(&user))).await?;
            pop.cmd(&format!("PASS {pass}")).await.map_err(|e| anyhow!("POP3 sign-in failed: {e}"))?;
        }
        Creds::OAuth { user, token } => {
            let s = STANDARD.encode(crate::oauth::xoauth2_string(&recent(&user), &token));
            pop.rd.get_mut().write_all(b"AUTH XOAUTH2\r\n").await?;
            let l = pop.line().await?;
            if !l.starts_with('+') {
                bail!("POP3 server does not support XOAUTH2: {l}");
            }
            let r = pop.cmd(&s).await;
            if let Err(e) = r {
                // Abort the SASL exchange politely.
                let _ = pop.rd.get_mut().write_all(b"\r\n").await;
                bail!("POP3 sign-in (OAuth) failed: {e}");
            }
        }
    }
    Ok(pop)
}

fn key_file(uidl: &str) -> String {
    let h = Sha256::digest(uidl.as_bytes());
    h.iter().take(10).map(|b| format!("{b:02x}")).collect()
}

pub async fn sync_account(state: &AppState, a: &Account) -> Result<usize> {
    ensure_local_folders(state, a.id)?;
    let inbox = state.db.folder_by_role(a.id, "inbox")?.ok_or_else(|| anyhow!("missing inbox"))?;
    let root = data_dir(state, a);
    std::fs::create_dir_all(&root).with_context(|| format!("Could not create folder {}", root.display()))?;

    crate::sync::log(state, a, "info", format!("Connecting to {}:{} ({})…", a.in_host, a.in_port, a.in_security));
    let mut pop = connect(state, a).await.context("Signing in to the POP3 server")?;
    crate::sync::log(state, a, "ok", "Signed in");
    pop.rd.get_mut().write_all(b"UIDL\r\n").await?;
    let l = pop.line().await?;
    if !l.starts_with("+OK") {
        bail!("POP3 server does not support UIDL: {l}");
    }
    let list = pop.multiline().await?;
    let known = state.db.pop_keys(a.id)?;
    let mut todo: Vec<(u32, String)> = vec![];
    for line in String::from_utf8_lossy(&list).lines() {
        let mut it = line.split_whitespace();
        let (Some(n), Some(uidl)) = (it.next(), it.next()) else { continue };
        let Ok(n) = n.parse::<u32>() else { continue };
        if !known.contains(uidl) {
            todo.push((n, uidl.to_string()));
        }
    }
    let first_sync = known.is_empty();
    crate::sync::log(
        state,
        a,
        "info",
        format!("Inbox: {} message(s) on the server, {} new{}", String::from_utf8_lossy(&list).lines().count(), todo.len(), if todo.len() > MAX_PER_RUN { format!(" (downloading {MAX_PER_RUN} now, the rest next sync)") } else { String::new() }),
    );
    let rules = state.db.rules()?;
    let mut new_count = 0usize;
    let mut new_ids: Vec<i64> = vec![];
    let total = todo.len().min(MAX_PER_RUN);
    for (i, (n, uidl)) in todo.into_iter().take(MAX_PER_RUN).enumerate() {
        pop.rd.get_mut().write_all(format!("RETR {n}\r\n").as_bytes()).await?;
        let l = pop.line().await?;
        if !l.starts_with("+OK") {
            crate::sync::log(state, a, "warn", format!("Message {n} couldn't be downloaded: {}", l.trim()));
            continue;
        }
        let raw = pop.multiline().await?;
        let mut m = parse::summarize(&raw, true);
        let month = chrono::DateTime::from_timestamp(m.date, 0)
            .map(|d| d.format("%Y-%m").to_string())
            .unwrap_or_else(|| "unknown".into());
        let dir = root.join("Inbox").join(month);
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{}.eml", key_file(&uidl)));
        std::fs::write(&path, &raw)?;
        m.account_id = a.id;
        m.folder_id = inbox.id;
        m.remote_key = uidl.clone();
        m.raw_path = path.to_string_lossy().to_string();
        m.seen = false;
        state.db.learn_contact(&m.from_addr, &m.from_name)?;
        let id = state.db.insert_message(&m)?;
        new_ids.push(id);
        let row = state.db.message(id)?;
        let f1 = crate::rules::apply(state, &rules, &row, None)?;
        let f2 = crate::rules::apply(state, &rules, &row, Some(m.body.as_deref().unwrap_or("")))?;
        if !first_sync && !f1.mark_read && !f2.mark_read && !crate::commands::hidden_from_inbox(state, id) {
            state.emit(
                "new-mail",
                serde_json::json!({ "account": a.email, "from": if m.from_name.is_empty() { &m.from_addr } else { &m.from_name }, "subject": m.subject }),
            );
        }
        new_count += 1;
        if !a.pop3_leave_on_server {
            let _ = pop.cmd(&format!("DELE {n}")).await;
        }
        if i % 20 == 0 {
            state.emit("sync-progress", serde_json::json!({ "account_id": a.id, "done": i + 1, "total": total }));
            state.changed(a.id);
        }
    }
    let _ = pop.cmd("QUIT").await;
    // Same matching as "Run all on existing mail" for everything that arrived.
    if let Err(e) = crate::rules::apply_new(&state.db, &new_ids) {
        eprintln!("rules: {e:#}");
    }
    Ok(new_count)
}

pub async fn test(state: &AppState, a: &Account) -> Result<()> {
    let creds = state.creds(a).await?;
    test_with(a, creds).await
}

pub async fn test_with(a: &Account, creds: Creds) -> Result<()> {
    let mut pop = connect_with(a, creds).await?;
    pop.cmd("STAT").await?;
    let _ = pop.cmd("QUIT").await;
    Ok(())
}

/// Save a message (sent copy, draft) into one of the account's local folders.
pub fn store_local(state: &AppState, a: &Account, role: &str, raw: &[u8], seen: bool) -> Result<i64> {
    ensure_local_folders(state, a.id)?;
    let folder = state.db.folder_by_role(a.id, role)?.ok_or_else(|| anyhow!("missing folder {role}"))?;
    let root = data_dir(state, a);
    let mut m = parse::summarize(raw, true);
    let dir = root.join(&folder.path);
    std::fs::create_dir_all(&dir)?;
    let mut rnd = [0u8; 8];
    getrandom::getrandom(&mut rnd).map_err(|e| anyhow!("{e}"))?;
    let name: String = rnd.iter().map(|b| format!("{b:02x}")).collect();
    let path = dir.join(format!("{name}.eml"));
    std::fs::write(&path, raw)?;
    m.account_id = a.id;
    m.folder_id = folder.id;
    m.raw_path = path.to_string_lossy().to_string();
    m.seen = seen;
    if m.date == 0 {
        m.date = chrono::Utc::now().timestamp();
    }
    state.db.insert_message(&m)
}

/// Move the account's stored mail to a new folder and point the index at it.
pub fn relocate(state: &AppState, a: &Account, new_dir: &Path) -> Result<usize> {
    let old = data_dir(state, a);
    std::fs::create_dir_all(new_dir)?;
    let rows: Vec<(i64, String)> = state.db.with(|c| {
        let mut st = c.prepare("SELECT id, raw_path FROM messages WHERE account_id=?1 AND raw_path<>''")?;
        let r = st.query_map([a.id], |r| Ok((r.get(0)?, r.get(1)?)))?;
        r.collect()
    })?;
    let mut moved = 0;
    for (id, p) in rows {
        let src = PathBuf::from(&p);
        let rel = src.strip_prefix(&old).map(|x| x.to_path_buf()).unwrap_or_else(|_| PathBuf::from(src.file_name().unwrap_or_default()));
        let dst = new_dir.join(rel);
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if src.exists() {
            if std::fs::rename(&src, &dst).is_err() {
                std::fs::copy(&src, &dst)?;
                let _ = std::fs::remove_file(&src);
            }
            moved += 1;
        }
        let dsts = dst.to_string_lossy().to_string();
        state.db.with(|c| c.execute("UPDATE messages SET raw_path=?2 WHERE id=?1", rusqlite::params![id, dsts]))?;
    }
    Ok(moved)
}
