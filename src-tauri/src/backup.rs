//! Export / import of the whole configuration, to carry settings into a new version or another
//! copy of Hermio (e.g. a new portable folder).
//!
//! The file is JSON: settings, mailboxes (server settings, not mail), spaces, groups, rules,
//! signatures, calendar subscriptions, contacts and — optionally — saved AI chats. Passwords,
//! sign-in tokens and API keys can be included too; they are then encrypted with Windows DPAPI for
//! the current Windows user, so the file only opens for the same user on the same PC.
//!
//! Import merges: mailboxes are matched by e-mail address and protocol, groups and rules by name,
//! and every id reference (rules → groups, groups → mailboxes, settings → mailboxes/spaces…) is
//! translated to the ids in this copy.

use crate::db::{DB_FILE as DB, LEGACY_DB};
use crate::oauth;
use crate::state::AppState;
use anyhow::{anyhow, bail, Context, Result};
use rusqlite::types::Value as SqlValue;
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::collections::HashMap;

const FORMAT: i64 = 1;

/// Settings that describe this copy's state rather than preferences.
const SKIP_SETTINGS: &[&str] = &["ai_suggest_queue", "npu_models", "npu_version", "ovms_version", "npu_collection", "ovms_collection"];

#[derive(Debug, Default, Serialize)]
pub struct ImportReport {
    pub accounts_added: usize,
    pub accounts_updated: usize,
    pub groups: usize,
    pub rules: usize,
    pub signatures: usize,
    pub settings: usize,
    pub secrets: usize,
    pub notes: Vec<String>,
}

fn to_json(v: SqlValue) -> Value {
    match v {
        SqlValue::Null => Value::Null,
        SqlValue::Integer(i) => json!(i),
        SqlValue::Real(f) => json!(f),
        SqlValue::Text(s) => json!(s),
        SqlValue::Blob(b) => {
            use base64::Engine;
            json!(base64::engine::general_purpose::STANDARD.encode(b))
        }
    }
}

fn to_sql(v: &Value) -> SqlValue {
    match v {
        Value::Null => SqlValue::Null,
        Value::Bool(b) => SqlValue::Integer(*b as i64),
        Value::Number(n) => n.as_i64().map(SqlValue::Integer).unwrap_or_else(|| SqlValue::Real(n.as_f64().unwrap_or(0.0))),
        Value::String(s) => SqlValue::Text(s.clone()),
        other => SqlValue::Text(other.to_string()),
    }
}

fn dump(state: &AppState, table: &str, where_: &str) -> Result<Vec<Map<String, Value>>> {
    state.db.with(|c| {
        let mut st = c.prepare(&format!("SELECT * FROM {table} {where_}"))?;
        let cols: Vec<String> = st.column_names().iter().map(|s| s.to_string()).collect();
        let rows = st.query_map([], |r| {
            let mut m = Map::new();
            for (i, name) in cols.iter().enumerate() {
                m.insert(name.clone(), to_json(r.get::<_, SqlValue>(i)?));
            }
            Ok(m)
        })?;
        rows.collect()
    })
}

fn columns(state: &AppState, table: &str) -> Result<Vec<String>> {
    state.db.with(|c| {
        let mut st = c.prepare(&format!("PRAGMA table_info({table})"))?;
        let r = st.query_map([], |r| r.get::<_, String>(1))?;
        r.collect()
    })
}

/// Insert a row, keeping only the columns this version has (files from newer/older versions work).
fn insert(state: &AppState, table: &str, row: &Map<String, Value>, skip: &[&str]) -> Result<i64> {
    let cols = columns(state, table)?;
    let keys: Vec<&String> = row.keys().filter(|k| cols.contains(k) && k.as_str() != "id" && !skip.contains(&k.as_str())).collect();
    if keys.is_empty() {
        bail!("nothing to import for {table}");
    }
    let sql = format!(
        "INSERT INTO {table}({}) VALUES({})",
        keys.iter().map(|k| k.as_str()).collect::<Vec<_>>().join(","),
        (1..=keys.len()).map(|i| format!("?{i}")).collect::<Vec<_>>().join(",")
    );
    let vals: Vec<SqlValue> = keys.iter().map(|k| to_sql(&row[k.as_str()])).collect();
    state.db.with(|c| {
        c.execute(&sql, rusqlite::params_from_iter(vals.iter()))?;
        Ok(c.last_insert_rowid())
    })
}

fn update(state: &AppState, table: &str, id: i64, row: &Map<String, Value>, skip: &[&str]) -> Result<()> {
    let cols = columns(state, table)?;
    let keys: Vec<&String> = row.keys().filter(|k| cols.contains(k) && k.as_str() != "id" && !skip.contains(&k.as_str())).collect();
    if keys.is_empty() {
        return Ok(());
    }
    let sql = format!(
        "UPDATE {table} SET {} WHERE id=?{}",
        keys.iter().enumerate().map(|(i, k)| format!("{k}=?{}", i + 1)).collect::<Vec<_>>().join(","),
        keys.len() + 1
    );
    let mut vals: Vec<SqlValue> = keys.iter().map(|k| to_sql(&row[k.as_str()])).collect();
    vals.push(SqlValue::Integer(id));
    state.db.with(|c| c.execute(&sql, rusqlite::params_from_iter(vals.iter())).map(|_| ()))
}

fn id_of(row: &Map<String, Value>, k: &str) -> Option<i64> {
    row.get(k).and_then(|v| v.as_i64())
}

// ---------------- secrets (DPAPI) ----------------

#[cfg(windows)]
fn protect(data: &[u8], encrypt: bool) -> Result<Vec<u8>> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB};
    unsafe {
        let input = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 };
        let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
        let ok = if encrypt {
            CryptProtectData(&input, std::ptr::null(), std::ptr::null(), std::ptr::null(), std::ptr::null(), 0, &mut out)
        } else {
            CryptUnprotectData(&input, std::ptr::null_mut(), std::ptr::null(), std::ptr::null(), std::ptr::null(), 0, &mut out)
        };
        if ok == 0 {
            bail!(if encrypt { "Windows couldn't encrypt the passwords" } else { "The passwords in this file were saved by another Windows user or PC and can't be read here" });
        }
        let v = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        LocalFree(out.pbData as _);
        Ok(v)
    }
}

#[cfg(not(windows))]
fn protect(_data: &[u8], _encrypt: bool) -> Result<Vec<u8>> {
    bail!("Encrypted passwords are only supported on Windows")
}

fn secret_names(state: &AppState) -> Result<Vec<String>> {
    let mut names = vec![oauth::CLIENT_SECRET_KEY.to_string(), crate::web::BRAVE_KEY.to_string()];
    for a in state.db.accounts()? {
        names.push(oauth::password_key(a.id));
        names.push(oauth::refresh_key(a.id));
        names.push(format!("account:{}:smtp_password", a.id));
    }
    for p in crate::ai::providers(state)? {
        names.push(format!("ai:{}:key", p.id));
    }
    Ok(names)
}

// ---------------- export ----------------

pub fn export(state: &AppState, path: &std::path::Path, with_secrets: bool, with_chats: bool) -> Result<()> {
    let settings: Map<String, Value> = state
        .db
        .with(|c| {
            let mut st = c.prepare("SELECT key, value FROM settings")?;
            let r = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
            r.collect::<rusqlite::Result<Vec<_>>>()
        })?
        .into_iter()
        .filter(|(k, _)| !SKIP_SETTINGS.contains(&k.as_str()))
        .map(|(k, v)| (k, Value::String(v)))
        .collect();
    let mut doc = json!({
        "hermio_config": FORMAT,
        "app_version": env!("CARGO_PKG_VERSION"),
        "exported_at": chrono::Utc::now().to_rfc3339(),
        "settings": settings,
        "spaces": dump(state, "spaces", "")?,
        "signatures": dump(state, "signatures", "")?,
        "accounts": dump(state, "accounts", "")?,
        "groups": dump(state, "groups", "")?,
        "rules": dump(state, "rules", "")?,
        "calendars": dump(state, "calendars", "WHERE kind='ics'")?,
        "contacts": dump(state, "contacts", "")?,
    });
    if with_chats {
        doc["ai_chats"] = json!(dump(state, "ai_chats", "")?);
    }
    if with_secrets {
        let mut sec = Map::new();
        for n in secret_names(state)? {
            if let Ok(Some(v)) = oauth::get_secret(&n) {
                sec.insert(n, json!(v));
            }
        }
        use base64::Engine;
        let blob = protect(serde_json::to_vec(&sec)?.as_slice(), true)?;
        doc["secrets"] = json!({ "scheme": "dpapi-user", "data": base64::engine::general_purpose::STANDARD.encode(blob) });
    }
    std::fs::write(path, serde_json::to_vec_pretty(&doc)?).with_context(|| format!("Couldn't write {}", path.display()))?;
    Ok(())
}

// ---------------- import ----------------

pub fn import(state: &AppState, path: &std::path::Path) -> Result<ImportReport> {
    let raw = std::fs::read(path).with_context(|| format!("Couldn't read {}", path.display()))?;
    let doc: Value = serde_json::from_slice(&raw).map_err(|_| anyhow!("This isn't a Hermio configuration file"))?;
    let format = doc["hermio_config"].as_i64().or_else(|| doc["nelo_config"].as_i64());
    let Some(format) = format else {
        bail!("This isn't a Hermio configuration file");
    };
    if format > FORMAT {
        bail!("This file comes from a newer Hermio ({}); update Hermio first", doc["app_version"].as_str().unwrap_or("?"));
    }
    let rows = |k: &str| -> Vec<Map<String, Value>> { doc[k].as_array().cloned().unwrap_or_default().into_iter().filter_map(|v| v.as_object().cloned()).collect() };
    let mut rep = ImportReport::default();

    // Spaces (by name)
    let mut space_map: HashMap<i64, i64> = HashMap::new();
    let have_spaces = dump(state, "spaces", "")?;
    for r in rows("spaces") {
        let Some(old) = id_of(&r, "id") else { continue };
        let name = r["name"].as_str().unwrap_or("");
        let id = match have_spaces.iter().find(|x| x["name"].as_str() == Some(name)) {
            Some(x) => id_of(x, "id").unwrap_or(0),
            None => insert(state, "spaces", &r, &[])?,
        };
        space_map.insert(old, id);
    }

    // Signatures (by name)
    let mut sig_map: HashMap<i64, i64> = HashMap::new();
    let have_sigs = dump(state, "signatures", "")?;
    for r in rows("signatures") {
        let Some(old) = id_of(&r, "id") else { continue };
        let name = r["name"].as_str().unwrap_or("");
        let id = match have_sigs.iter().find(|x| x["name"].as_str() == Some(name)) {
            Some(x) => {
                let id = id_of(x, "id").unwrap_or(0);
                update(state, "signatures", id, &r, &[])?;
                id
            }
            None => insert(state, "signatures", &r, &[])?,
        };
        sig_map.insert(old, id);
        rep.signatures += 1;
    }

    // Mailboxes (by address + protocol). Mail itself is downloaded again by the next sync.
    let mut acc_map: HashMap<i64, i64> = HashMap::new();
    let have_acc = dump(state, "accounts", "")?;
    for mut r in rows("accounts") {
        let Some(old) = id_of(&r, "id") else { continue };
        for (k, map) in [("space_id", &space_map), ("sig_new", &sig_map), ("sig_reply", &sig_map)] {
            if let Some(v) = id_of(&r, k) {
                r.insert(k.into(), map.get(&v).map(|x| json!(x)).unwrap_or(Value::Null));
            }
        }
        let skip = ["last_sync", "last_error", "pop3_dir"];
        let email = r["email"].as_str().unwrap_or("").to_lowercase();
        let proto = r["protocol"].as_str().unwrap_or("");
        let id = match have_acc.iter().find(|x| x["email"].as_str().map(|e| e.to_lowercase()) == Some(email.clone()) && x["protocol"].as_str() == Some(proto)) {
            Some(x) => {
                let id = id_of(x, "id").unwrap_or(0);
                // Keep this copy's POP3 storage folder.
                update(state, "accounts", id, &r, &["last_sync", "last_error", "pop3_dir"])?;
                rep.accounts_updated += 1;
                id
            }
            None => {
                let id = insert(state, "accounts", &r, &skip)?;
                rep.accounts_added += 1;
                id
            }
        };
        acc_map.insert(old, id);
    }

    // Groups (by name + mailbox)
    let mut group_map: HashMap<i64, i64> = HashMap::new();
    let have_groups = dump(state, "groups", "")?;
    for mut r in rows("groups") {
        let Some(old) = id_of(&r, "id") else { continue };
        let acc = id_of(&r, "account_id").and_then(|a| acc_map.get(&a).copied());
        r.insert("account_id".into(), acc.map(|x| json!(x)).unwrap_or(Value::Null));
        let name = r["name"].as_str().unwrap_or("");
        let id = match have_groups.iter().find(|x| x["name"].as_str() == Some(name) && id_of(x, "account_id") == acc) {
            Some(x) => {
                let id = id_of(x, "id").unwrap_or(0);
                update(state, "groups", id, &r, &[])?;
                id
            }
            None => insert(state, "groups", &r, &[])?,
        };
        group_map.insert(old, id);
        rep.groups += 1;
    }

    // Rules (by name), with group and mailbox references translated.
    let have_rules = dump(state, "rules", "")?;
    for mut r in rows("rules") {
        if let Some(g) = id_of(&r, "group_id") {
            r.insert("group_id".into(), group_map.get(&g).map(|x| json!(x)).unwrap_or(Value::Null));
        }
        let accs: Vec<i64> = serde_json::from_str(r["account_ids"].as_str().unwrap_or("[]")).unwrap_or_default();
        let accs: Vec<i64> = accs.iter().filter_map(|a| acc_map.get(a).copied()).collect();
        r.insert("account_ids".into(), json!(serde_json::to_string(&accs)?));
        let name = r["name"].as_str().unwrap_or("");
        match have_rules.iter().find(|x| x["name"].as_str() == Some(name)) {
            Some(x) => update(state, "rules", id_of(x, "id").unwrap_or(0), &r, &[])?,
            None => {
                insert(state, "rules", &r, &[])?;
            }
        }
        rep.rules += 1;
    }

    // Calendar subscriptions (by URL)
    let have_cals = dump(state, "calendars", "WHERE kind='ics'")?;
    for r in rows("calendars") {
        if have_cals.iter().any(|x| x["url"] == r["url"]) {
            continue;
        }
        let _ = insert(state, "calendars", &r, &["account_id", "last_sync", "last_error"]);
    }

    // Contacts (address book suggestions)
    for r in rows("contacts") {
        let (Some(addr), name) = (r["addr"].as_str(), r["name"].as_str().unwrap_or("")) else { continue };
        let _ = state.db.with(|c| {
            c.execute(
                "INSERT INTO contacts(addr,name,uses,last_used) VALUES(?1,?2,?3,?4) ON CONFLICT(addr) DO UPDATE SET uses=MAX(uses,excluded.uses)",
                rusqlite::params![addr, name, r["uses"].as_i64().unwrap_or(0), r["last_used"].as_i64().unwrap_or(0)],
            )
        });
    }

    // Saved AI conversations
    for r in rows("ai_chats") {
        let _ = insert(state, "ai_chats", &r, &[]);
    }

    // Settings, with id references translated.
    if let Some(set) = doc["settings"].as_object() {
        for (k, v) in set {
            if SKIP_SETTINGS.contains(&k.as_str()) {
                continue;
            }
            let Some(mut val) = v.as_str().map(|s| s.to_string()) else { continue };
            match k.as_str() {
                "ai_allowed_accounts" => {
                    if let Ok(ids) = serde_json::from_str::<Vec<i64>>(&val) {
                        val = serde_json::to_string(&ids.iter().filter_map(|a| acc_map.get(a)).collect::<Vec<_>>())?;
                    }
                }
                "space" => {
                    if let Ok(id) = val.trim_matches('"').parse::<i64>() {
                        val = space_map.get(&id).map(|x| x.to_string()).unwrap_or_else(|| "0".into());
                    }
                }
                _ => {}
            }
            state.db.set_setting(k, &val)?;
            rep.settings += 1;
        }
    }

    // Passwords, tokens, API keys
    if let Some(data) = doc["secrets"]["data"].as_str() {
        use base64::Engine;
        let blob = base64::engine::general_purpose::STANDARD.decode(data).map_err(|_| anyhow!("The passwords in this file are damaged"))?;
        match protect(&blob, false).and_then(|b| Ok(serde_json::from_slice::<Map<String, Value>>(&b)?)) {
            Ok(sec) => {
                let re = regex_lite::Regex::new(r"^account:(\d+):(.+)$").unwrap();
                for (name, v) in sec {
                    let Some(v) = v.as_str() else { continue };
                    let target = match re.captures(&name) {
                        Some(c) => match c[1].parse::<i64>().ok().and_then(|o| acc_map.get(&o)) {
                            Some(newid) => format!("account:{newid}:{}", &c[2]),
                            None => continue,
                        },
                        None => name.clone(),
                    };
                    if oauth::set_secret(&target, v).is_ok() {
                        rep.secrets += 1;
                    }
                }
            }
            Err(e) => rep.notes.push(format!("{e:#}")),
        }
    } else {
        rep.notes.push("The file has no passwords: enter them again in Settings → Accounts (or sign in with Google again).".into());
    }
    Ok(rep)
}

// ======================================================================================
// Moving ALL data (mail index, POP3 mail, cached messages, settings) between copies of Hermio
// ======================================================================================

/// Folders of the data directory that hold mail; NPU runtimes/models and temp files are left out
/// (they are large and are downloaded again when needed).
const DATA_DIRS: &[&str] = &["pop3", "cache", "outbox"];
const PENDING: &str = "restore-pending";

fn progress(state: &AppState, stage: &str, done: u64, total: u64) {
    state.emit("data-progress", json!({ "stage": stage, "done": done, "total": total }));
}

fn files_under(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            files_under(&p, out);
        } else {
            out.push(p);
        }
    }
}

/// A consistent copy of a Hermio database (also while it is open, WAL included).
fn snapshot_db(src: &std::path::Path, dst: &std::path::Path) -> Result<()> {
    let _ = std::fs::remove_file(dst);
    let c = rusqlite::Connection::open_with_flags(src, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX)
        .with_context(|| format!("Couldn't open {}", src.display()))?;
    c.execute("VACUUM INTO ?1", [dst.to_string_lossy().to_string()]).context("Couldn't copy the database")?;
    Ok(())
}

fn secrets_blob(state: &AppState) -> Result<Vec<u8>> {
    let mut sec = Map::new();
    for n in secret_names(state)? {
        if let Ok(Some(v)) = oauth::get_secret(&n) {
            sec.insert(n, json!(v));
        }
    }
    protect(serde_json::to_vec(&sec)?.as_slice(), true)
}

/// Write everything into one .zip file.
pub fn full_backup(state: &AppState, path: &std::path::Path, with_secrets: bool) -> Result<u64> {
    use std::io::Write;
    let data = state.db.data_dir.clone();
    let tmp = data.join("tmp");
    std::fs::create_dir_all(&tmp)?;
    let snap = tmp.join("backup-hermio.db");
    progress(state, "database", 0, 1);
    state.db.with(|c| c.execute("VACUUM INTO ?1", [snap.to_string_lossy().to_string()]).map(|_| ()))?;
    let mut files = vec![];
    for d in DATA_DIRS {
        files_under(&data.join(d), &mut files);
    }
    let f = std::fs::File::create(path).with_context(|| format!("Couldn't write {}", path.display()))?;
    let mut z = zip::ZipWriter::new(f);
    let opt = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated).large_file(true);
    let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored).large_file(true);
    let manifest = json!({
        "hermio_backup": 1,
        "app_version": env!("CARGO_PKG_VERSION"),
        "created": chrono::Utc::now().to_rfc3339(),
        "data_dir": data.to_string_lossy(),
    });
    z.start_file("manifest.json", opt)?;
    z.write_all(serde_json::to_vec_pretty(&manifest)?.as_slice())?;
    z.start_file(DB, opt)?;
    std::io::copy(&mut std::fs::File::open(&snap)?, &mut z)?;
    let _ = std::fs::remove_file(&snap);
    if with_secrets {
        if let Ok(b) = secrets_blob(state) {
            z.start_file("secrets.dpapi", stored)?;
            z.write_all(&b)?;
        }
    }
    let total = files.len() as u64;
    let mut bytes = 0u64;
    for (i, p) in files.iter().enumerate() {
        let rel = p.strip_prefix(&data).unwrap_or(p).to_string_lossy().replace('\\', "/");
        // Mail files are already compressed or small: store them as they are, quickly.
        z.start_file(format!("data/{rel}"), if rel.ends_with(".eml") { opt } else { stored })?;
        if let Ok(mut src) = std::fs::File::open(p) {
            bytes += std::io::copy(&mut src, &mut z).unwrap_or(0);
        }
        if i % 50 == 0 {
            progress(state, "files", i as u64, total);
        }
    }
    z.finish()?;
    progress(state, "done", total, total);
    Ok(bytes)
}

/// Where an older Hermio keeps its data: the portable folder's `data`, or the folder itself.
fn find_data_dir(p: &std::path::Path) -> Option<(std::path::PathBuf, &'static str)> {
    for c in [p.join("data"), p.to_path_buf(), p.join("Hermio").join("data"), p.join("Hermio").join("data")] {
        for f in [DB, LEGACY_DB] {
            if c.join(f).is_file() {
                return Some((c, f));
            }
        }
    }
    None
}

/// Prepare a restore from a full-backup .zip or from an older Hermio's folder. Nothing is replaced
/// yet: the data is staged and swapped in when Hermio restarts (the database can't be replaced
/// while it is open). Returns a short description of what will be restored.
pub fn stage_restore(state: &AppState, source: &std::path::Path) -> Result<String> {
    let data = state.db.data_dir.clone();
    let pending = data.join(PENDING);
    let _ = std::fs::remove_dir_all(&pending);
    std::fs::create_dir_all(&pending)?;
    let result = (|| -> Result<String> {
        let old_dir: String;
        if source.is_file() {
            // Full backup .zip
            let f = std::fs::File::open(source).with_context(|| format!("Couldn't open {}", source.display()))?;
            let mut z = zip::ZipArchive::new(f).map_err(|_| anyhow!("This isn't a Hermio backup file"))?;
            let manifest: Value = {
                let mut m = z.by_name("manifest.json").map_err(|_| anyhow!("This isn't a Hermio backup file"))?;
                let mut s = String::new();
                std::io::Read::read_to_string(&mut m, &mut s)?;
                serde_json::from_str(&s)?
            };
            old_dir = manifest["data_dir"].as_str().unwrap_or("").to_string();
            let total = z.len() as u64;
            for i in 0..z.len() {
                let mut e = z.by_index(i)?;
                let Some(name) = e.enclosed_name() else { continue };
                let name = name.to_string_lossy().replace('\\', "/");
                let target = if name == DB || name == LEGACY_DB {
                    pending.join(DB)
                } else if let Some(rest) = name.strip_prefix("data/") {
                    pending.join("data").join(rest)
                } else if name == "secrets.dpapi" {
                    pending.join("secrets.dpapi")
                } else {
                    continue;
                };
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                let mut out = std::fs::File::create(&target)?;
                std::io::copy(&mut e, &mut out)?;
                if i % 50 == 0 {
                    progress(state, "files", i as u64, total);
                }
            }
            if !pending.join(DB).is_file() {
                bail!("The backup has no database");
            }
        } else {
            let (src, db_name) = find_data_dir(source).ok_or_else(|| anyhow!("No Hermio data found there. Choose the folder of the old copy (the one with Hermio.exe or Nelo.exe) or its \"data\" folder."))?;
            if src == data {
                bail!("That is this copy's own data folder");
            }
            old_dir = src.to_string_lossy().to_string();
            progress(state, "database", 0, 1);
            snapshot_db(&src.join(db_name), &pending.join(DB))?;
            let mut files = vec![];
            for d in DATA_DIRS {
                files_under(&src.join(d), &mut files);
            }
            let total = files.len() as u64;
            for (i, p) in files.iter().enumerate() {
                let rel = p.strip_prefix(&src).unwrap_or(p);
                let target = pending.join("data").join(rel);
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::copy(p, &target).with_context(|| format!("Couldn't copy {}", p.display()))?;
                if i % 50 == 0 {
                    progress(state, "files", i as u64, total);
                }
            }
        }
        // What's inside, for the confirmation.
        let c = rusqlite::Connection::open_with_flags(pending.join(DB), rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let (acc, msgs): (i64, i64) = c.query_row("SELECT (SELECT COUNT(*) FROM accounts), (SELECT COUNT(*) FROM messages)", [], |r| Ok((r.get(0)?, r.get(1)?)))?;
        drop(c);
        std::fs::write(pending.join("manifest.json"), serde_json::to_vec(&json!({ "old_data_dir": old_dir, "staged": chrono::Utc::now().to_rfc3339() }))?)?;
        progress(state, "done", 1, 1);
        Ok(format!("{acc} mailbox(es), {msgs} message(s)"))
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&pending);
    }
    result
}

pub fn cancel_restore(state: &AppState) {
    let _ = std::fs::remove_dir_all(state.db.data_dir.join(PENDING));
}

/// The app was called Nelo before 0.5. Its installed copy kept its data in `<roaming>/app.nelo.mail`;
/// Hermio uses `<roaming>/app.hermio.mail`. On the first start of Hermio that data is moved over
/// (the folder is renamed, so nothing is copied twice) and paths stored in the database are
/// updated. Returns the old folder when something was moved.
pub fn migrate_from_hermio(new_dir: &std::path::Path) -> Option<std::path::PathBuf> {
    if new_dir.join(DB).exists() || new_dir.join(LEGACY_DB).exists() {
        return None;
    }
    let old = new_dir.parent()?.join("app.nelo.mail");
    if !old.join(LEGACY_DB).is_file() {
        return None;
    }
    let empty = std::fs::read_dir(new_dir).map(|mut d| d.next().is_none()).unwrap_or(true);
    let renamed = empty && {
        let _ = std::fs::remove_dir(new_dir);
        std::fs::rename(&old, new_dir).is_ok()
    };
    if !renamed {
        // Move entry by entry (e.g. the new folder already holds a few files).
        std::fs::create_dir_all(new_dir).ok()?;
        for e in std::fs::read_dir(&old).ok()?.flatten() {
            let to = new_dir.join(e.file_name());
            if !to.exists() {
                let _ = std::fs::rename(e.path(), &to);
            }
        }
        if !new_dir.join(LEGACY_DB).is_file() {
            return None;
        }
    }
    let (o, n) = (old.to_string_lossy().to_string(), new_dir.to_string_lossy().to_string());
    if let Ok(c) = rusqlite::Connection::open(new_dir.join(LEGACY_DB)) {
        let _ = c.execute("UPDATE messages SET raw_path = ?2 || substr(raw_path, length(?1) + 1) WHERE raw_path LIKE ?1 || '%'", [&o, &n]);
        let _ = c.execute("UPDATE accounts SET pop3_dir = ?2 || substr(pop3_dir, length(?1) + 1) WHERE pop3_dir LIKE ?1 || '%'", [&o, &n]);
        // Settings may hold folder paths (runtime / models folders), raw or JSON-escaped.
        let _ = c.execute("UPDATE settings SET value = replace(value, ?1, ?2)", [&o, &n]);
        let _ = c.execute("UPDATE settings SET value = replace(value, ?1, ?2)", [o.replace('\\', "\\\\"), n.replace('\\', "\\\\")]);
    }
    Some(old)
}

/// At start-up, before the database opens: swap in a staged restore. The previous data is kept in
/// `before-restore-<date>` next to it, in case something went wrong.
pub fn apply_pending(data: &std::path::Path) -> Option<String> {
    let pending = data.join(PENDING);
    let manifest: Value = serde_json::from_slice(&std::fs::read(pending.join("manifest.json")).ok()?).ok()?;
    let keep = data.join(format!("before-restore-{}", chrono::Local::now().format("%Y%m%d-%H%M%S")));
    let _ = std::fs::create_dir_all(&keep);
    // The previous Hermio process may still be closing: wait for the database to be free. If it
    // stays locked, nothing is touched and the restore is tried again at the next start.
    let db = data.join(DB);
    if db.exists() {
        let mut moved = false;
        for _ in 0..40 {
            if std::fs::rename(&db, keep.join(DB)).is_ok() {
                moved = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
        if !moved {
            let _ = std::fs::remove_dir(&keep);
            return None;
        }
    }
    for f in [format!("{DB}-wal"), format!("{DB}-shm")] {
        let p = data.join(&f);
        if p.exists() {
            let _ = std::fs::rename(&p, keep.join(&f));
        }
    }
    for d in DATA_DIRS {
        let p = data.join(d);
        if p.exists() {
            let _ = std::fs::rename(&p, keep.join(d));
        }
    }
    if std::fs::rename(pending.join(DB), data.join(DB)).is_err() {
        // Put the old database back.
        let _ = std::fs::rename(keep.join(DB), data.join(DB));
        return None;
    }
    for d in DATA_DIRS {
        let p = pending.join("data").join(d);
        if p.exists() {
            let _ = std::fs::rename(&p, data.join(d));
        }
    }
    // Paths stored in the database still point at the old folder.
    let old = manifest["old_data_dir"].as_str().unwrap_or("").to_string();
    if !old.is_empty() {
        if let Ok(c) = rusqlite::Connection::open(data.join(DB)) {
            let new = data.to_string_lossy().to_string();
            let _ = c.execute("UPDATE messages SET raw_path = ?2 || substr(raw_path, length(?1) + 1) WHERE raw_path LIKE ?1 || '%'", [&old, &new]);
            let _ = c.execute("UPDATE accounts SET pop3_dir = ?2 || substr(pop3_dir, length(?1) + 1) WHERE pop3_dir LIKE ?1 || '%'", [&old, &new]);
        }
    }
    let secrets = std::fs::read(pending.join("secrets.dpapi")).ok();
    let _ = std::fs::remove_dir_all(&pending);
    // Secrets are keyed by mailbox id, and ids come along with the database unchanged.
    if let Some(b) = secrets {
        if let Ok(plain) = protect(&b, false) {
            if let Ok(sec) = serde_json::from_slice::<Map<String, Value>>(&plain) {
                for (k, v) in sec {
                    if let Some(v) = v.as_str() {
                        let _ = oauth::set_secret(&k, v);
                    }
                }
            }
        }
    }
    Some(keep.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::models::*;

    #[test]
    fn restore_from_old_folder() {
        let base = std::env::temp_dir().join(format!("hermio-restore-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let old = base.join("old").join("data");
        let new = base.join("new").join("data");
        {
            let db = Db::open(&old).unwrap();
            let acc = db.save_account(&Account { email: "me@x.com".into(), name: "Me".into(), protocol: "imap".into(), enabled: true, ..Default::default() }).unwrap();
            let f = db.upsert_folder(acc, "INBOX", "Inbox", "/", "inbox", true).unwrap();
            let id = db.insert_message(&NewMessage { account_id: acc, folder_id: f, subject: "Hi".into(), date: 1, ..Default::default() }).unwrap();
            let raw = old.join("cache").join(acc.to_string()).join("1.eml");
            std::fs::create_dir_all(raw.parent().unwrap()).unwrap();
            std::fs::write(&raw, b"Subject: Hi\r\n\r\nbody").unwrap();
            db.with(|c| c.execute("UPDATE messages SET raw_path=?1 WHERE id=?2", rusqlite::params![raw.to_string_lossy(), id])).unwrap();
        }
        // Stage like stage_restore does for a folder source.
        let _ = Db::open(&new).unwrap();
        let pending = new.join(PENDING);
        std::fs::create_dir_all(pending.join("data")).unwrap();
        snapshot_db(&old.join(DB), &pending.join(DB)).unwrap();
        let mut files = vec![];
        files_under(&old.join("cache"), &mut files);
        for p in files {
            let t = pending.join("data").join(p.strip_prefix(&old).unwrap());
            std::fs::create_dir_all(t.parent().unwrap()).unwrap();
            std::fs::copy(&p, &t).unwrap();
        }
        std::fs::write(pending.join("manifest.json"), serde_json::to_vec(&json!({ "old_data_dir": old.to_string_lossy() })).unwrap()).unwrap();
        assert!(find_data_dir(&base.join("old")).is_some());
        apply_pending(&new).unwrap();
        let db = Db::open(&new).unwrap();
        let raw: String = db.with(|c| c.query_row("SELECT raw_path FROM messages", [], |r| r.get(0))).unwrap();
        assert!(raw.starts_with(&new.to_string_lossy().to_string()), "{raw}");
        assert!(std::path::Path::new(&raw).is_file());
        assert!(!new.join(PENDING).exists());
        let _ = std::fs::remove_dir_all(base);
    }
}
