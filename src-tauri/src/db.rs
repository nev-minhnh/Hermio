//! SQLite storage: accounts, folders, message index (with FTS5), signatures, calendar, settings.

use crate::models::*;
use anyhow::{anyhow, Result};
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub struct Db {
    pub conn: Mutex<Connection>,
    pub data_dir: PathBuf,
}

const SCHEMA: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS accounts (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  email TEXT NOT NULL,
  display_name TEXT NOT NULL DEFAULT '',
  protocol TEXT NOT NULL,            -- imap | pop3
  auth TEXT NOT NULL,                -- password | oauth_google
  provider TEXT NOT NULL DEFAULT 'other',
  in_host TEXT NOT NULL, in_port INTEGER NOT NULL, in_security TEXT NOT NULL,
  out_host TEXT NOT NULL, out_port INTEGER NOT NULL, out_security TEXT NOT NULL,
  username TEXT NOT NULL,
  pop3_dir TEXT NOT NULL DEFAULT '',
  pop3_leave_on_server INTEGER NOT NULL DEFAULT 1,
  pop3_recent_mode INTEGER NOT NULL DEFAULT 0,
  color TEXT NOT NULL DEFAULT '#0f6cbd',
  sort_order INTEGER NOT NULL DEFAULT 0,
  sig_new INTEGER, sig_reply INTEGER,
  calendar_sync INTEGER NOT NULL DEFAULT 1,
  enabled INTEGER NOT NULL DEFAULT 1,
  last_sync INTEGER NOT NULL DEFAULT 0,
  last_error TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS folders (
  id INTEGER PRIMARY KEY,
  account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  path TEXT NOT NULL,
  name TEXT NOT NULL,
  delim TEXT NOT NULL DEFAULT '/',
  role TEXT NOT NULL DEFAULT '',     -- inbox sent drafts trash junk archive all ''
  selectable INTEGER NOT NULL DEFAULT 1,
  uidvalidity INTEGER NOT NULL DEFAULT 0,
  last_uid INTEGER NOT NULL DEFAULT 0,
  sort_order INTEGER NOT NULL DEFAULT 100,
  UNIQUE(account_id, path)
);

CREATE TABLE IF NOT EXISTS messages (
  id INTEGER PRIMARY KEY,
  account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  folder_id INTEGER NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
  uid INTEGER NOT NULL DEFAULT 0,    -- IMAP UID
  remote_key TEXT NOT NULL DEFAULT '', -- POP3 UIDL
  message_id TEXT NOT NULL DEFAULT '',
  in_reply_to TEXT NOT NULL DEFAULT '',
  subject TEXT NOT NULL DEFAULT '',
  from_name TEXT NOT NULL DEFAULT '',
  from_addr TEXT NOT NULL DEFAULT '',
  to_list TEXT NOT NULL DEFAULT '',
  cc_list TEXT NOT NULL DEFAULT '',
  date INTEGER NOT NULL DEFAULT 0,
  snippet TEXT NOT NULL DEFAULT '',
  seen INTEGER NOT NULL DEFAULT 0,
  flagged INTEGER NOT NULL DEFAULT 0,
  answered INTEGER NOT NULL DEFAULT 0,
  has_attachments INTEGER NOT NULL DEFAULT 0,
  size INTEGER NOT NULL DEFAULT 0,
  raw_path TEXT NOT NULL DEFAULT '', -- cached/stored .eml
  body_indexed INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS ix_msg_folder_date ON messages(folder_id, date DESC);
CREATE INDEX IF NOT EXISTS ix_msg_uid ON messages(folder_id, uid);
CREATE INDEX IF NOT EXISTS ix_msg_key ON messages(account_id, remote_key);
CREATE INDEX IF NOT EXISTS ix_msg_date ON messages(date DESC);

CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
  subject, sender, body,
  tokenize = 'unicode61 remove_diacritics 2'
);

-- Sender / To / Cc, for "People" search and the from: / to: / cc: operators. Kept in step with
-- `messages` by triggers.
CREATE VIRTUAL TABLE IF NOT EXISTS people_fts USING fts5(
  sender, recipients, cc,
  tokenize = 'unicode61 remove_diacritics 2'
);
CREATE TRIGGER IF NOT EXISTS people_ai AFTER INSERT ON messages BEGIN
  INSERT INTO people_fts(rowid, sender, recipients, cc) VALUES (new.id, new.from_name || ' <' || new.from_addr || '>', new.to_list, new.cc_list);
END;
CREATE TRIGGER IF NOT EXISTS people_ad AFTER DELETE ON messages BEGIN
  DELETE FROM people_fts WHERE rowid = old.id;
END;
CREATE TRIGGER IF NOT EXISTS people_au AFTER UPDATE OF from_name, from_addr, to_list, cc_list ON messages BEGIN
  DELETE FROM people_fts WHERE rowid = old.id;
  INSERT INTO people_fts(rowid, sender, recipients, cc) VALUES (new.id, new.from_name || ' <' || new.from_addr || '>', new.to_list, new.cc_list);
END;

CREATE TABLE IF NOT EXISTS signatures (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  html TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS contacts (
  addr TEXT PRIMARY KEY,
  name TEXT NOT NULL DEFAULT '',
  uses INTEGER NOT NULL DEFAULT 0,
  last_used INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS calendars (
  id INTEGER PRIMARY KEY,
  account_id INTEGER REFERENCES accounts(id) ON DELETE CASCADE,
  remote_id TEXT NOT NULL DEFAULT '',
  name TEXT NOT NULL,
  color TEXT NOT NULL DEFAULT '#0f6cbd',
  visible INTEGER NOT NULL DEFAULT 1,
  writable INTEGER NOT NULL DEFAULT 1,
  UNIQUE(account_id, remote_id)
);

CREATE TABLE IF NOT EXISTS events (
  id INTEGER PRIMARY KEY,
  calendar_id INTEGER NOT NULL REFERENCES calendars(id) ON DELETE CASCADE,
  remote_id TEXT NOT NULL DEFAULT '',
  title TEXT NOT NULL DEFAULT '',
  location TEXT NOT NULL DEFAULT '',
  description TEXT NOT NULL DEFAULT '',
  start_ts INTEGER NOT NULL,
  end_ts INTEGER NOT NULL,
  all_day INTEGER NOT NULL DEFAULT 0,
  recurring INTEGER NOT NULL DEFAULT 0,
  meet_link TEXT NOT NULL DEFAULT '',
  updated INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS ix_events_range ON events(start_ts, end_ts);

-- Calendar reminders already shown: `key` is "<event id>@<start>", `fired` the smallest lead
-- time (seconds) announced so far, `skipped` when the user chose to skip the rest.
CREATE TABLE IF NOT EXISTS reminders (
  key TEXT PRIMARY KEY,
  fired INTEGER NOT NULL DEFAULT 0,
  skipped INTEGER NOT NULL DEFAULT 0,
  start_ts INTEGER NOT NULL DEFAULT 0
);

-- What the user did with the calendar file in an email (so the prompt isn't repeated).
CREATE TABLE IF NOT EXISTS invites (
  message_id INTEGER PRIMARY KEY,
  state TEXT NOT NULL,
  event_id INTEGER NOT NULL DEFAULT 0,
  updated INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS spaces (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  color TEXT NOT NULL DEFAULT '#0f6cbd',
  sort_order INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS groups (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  color TEXT NOT NULL DEFAULT '#8764b8',
  hide_from_inbox INTEGER NOT NULL DEFAULT 1,
  sort_order INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS message_groups (
  message_id INTEGER NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
  group_id INTEGER NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
  PRIMARY KEY (message_id, group_id)
);
CREATE INDEX IF NOT EXISTS ix_mg_group ON message_groups(group_id);

CREATE TABLE IF NOT EXISTS rules (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 1,
  match_all INTEGER NOT NULL DEFAULT 1,
  conditions TEXT NOT NULL DEFAULT '[]',
  account_ids TEXT NOT NULL DEFAULT '[]',
  group_id INTEGER REFERENCES groups(id) ON DELETE SET NULL,
  mark_read INTEGER NOT NULL DEFAULT 0,
  flag INTEGER NOT NULL DEFAULT 0,
  stop INTEGER NOT NULL DEFAULT 0,
  sort_order INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS outbox (
  id INTEGER PRIMARY KEY,
  account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  payload TEXT NOT NULL,
  subject TEXT NOT NULL DEFAULT '',
  recipients TEXT NOT NULL DEFAULT '',
  send_at INTEGER NOT NULL,
  status TEXT NOT NULL DEFAULT 'scheduled',
  error TEXT NOT NULL DEFAULT '',
  created INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS ai_chats (
  id INTEGER PRIMARY KEY,
  title TEXT NOT NULL DEFAULT '',
  entries TEXT NOT NULL DEFAULT '[]',
  created INTEGER NOT NULL DEFAULT 0,
  updated INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS ai_suggestions (
  message_id INTEGER PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,
  group_id INTEGER REFERENCES groups(id) ON DELETE CASCADE,
  new_group TEXT NOT NULL DEFAULT '',
  reason TEXT NOT NULL DEFAULT '',
  status TEXT NOT NULL DEFAULT 'pending',
  created INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
"#;

const MIGRATIONS: &[&str] = &[
    "ALTER TABLE groups ADD COLUMN account_id INTEGER",
    "UPDATE groups SET hide_from_inbox=1",
    "ALTER TABLE accounts ADD COLUMN smtp_username TEXT NOT NULL DEFAULT ''",
    "ALTER TABLE accounts ADD COLUMN space_id INTEGER",
    "ALTER TABLE accounts ADD COLUMN sync_limit INTEGER NOT NULL DEFAULT 2000",
    "ALTER TABLE accounts ADD COLUMN body_limit INTEGER NOT NULL DEFAULT 200",
    "ALTER TABLE accounts ADD COLUMN role_map TEXT NOT NULL DEFAULT '{}'",
    "ALTER TABLE calendars ADD COLUMN kind TEXT NOT NULL DEFAULT 'local'",
    "ALTER TABLE calendars ADD COLUMN url TEXT NOT NULL DEFAULT ''",
    "ALTER TABLE calendars ADD COLUMN last_sync INTEGER NOT NULL DEFAULT 0",
    "ALTER TABLE calendars ADD COLUMN last_error TEXT NOT NULL DEFAULT ''",
    "ALTER TABLE events ADD COLUMN uid TEXT NOT NULL DEFAULT ''",
];

/// The database file in the data folder.
pub const DB_FILE: &str = "hermio.db";
/// Its name before the app was renamed from Nelo to Hermio.
pub const LEGACY_DB: &str = "nelo.db";

impl Db {
    pub fn open(data_dir: &Path) -> Result<Db> {
        std::fs::create_dir_all(data_dir)?;
        // Databases from before the rename (Nelo) are taken over under the new name.
        if !data_dir.join(DB_FILE).exists() && data_dir.join(LEGACY_DB).exists() {
            for ext in ["", "-wal", "-shm"] {
                let _ = std::fs::rename(data_dir.join(format!("{LEGACY_DB}{ext}")), data_dir.join(format!("{DB_FILE}{ext}")));
            }
        }
        let conn = Connection::open(data_dir.join(DB_FILE))?;
        conn.execute_batch(SCHEMA)?;
        for m in MIGRATIONS {
            let _ = conn.execute(m, []); // "duplicate column" on later runs is fine
        }
        conn.execute("UPDATE calendars SET kind='google' WHERE account_id IS NOT NULL AND kind='local'", [])?;
        // Fill the people index for mail stored before it existed.
        let (n_msg, n_people): (i64, i64) =
            conn.query_row("SELECT (SELECT COUNT(*) FROM messages), (SELECT COUNT(*) FROM people_fts)", [], |r| Ok((r.get(0)?, r.get(1)?)))?;
        if n_msg != n_people {
            conn.execute_batch(
                "DELETE FROM people_fts;
                 INSERT INTO people_fts(rowid, sender, recipients, cc)
                   SELECT id, from_name || ' <' || from_addr || '>', to_list, cc_list FROM messages;",
            )?;
        }
        // Ensure the local calendar exists.
        conn.execute(
            "INSERT OR IGNORE INTO calendars(id, account_id, remote_id, name, color) VALUES (1, NULL, 'local', 'My calendar', '#0f6cbd')",
            [],
        )?;
        Ok(Db { conn: Mutex::new(conn), data_dir: data_dir.to_path_buf() })
    }

    pub fn cache_dir(&self, account_id: i64) -> PathBuf {
        self.data_dir.join("cache").join(account_id.to_string())
    }

    pub fn with<T>(&self, f: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> Result<T> {
        let c = self.conn.lock().map_err(|_| anyhow!("database lock poisoned"))?;
        Ok(f(&c)?)
    }

    // ---------- settings ----------
    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        self.with(|c| {
            c.query_row("SELECT value FROM settings WHERE key=?1", [key], |r| r.get(0)).optional()
        })
    }
    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                params![key, value],
            )
        })?;
        Ok(())
    }
    pub fn all_settings(&self) -> Result<serde_json::Map<String, serde_json::Value>> {
        self.with(|c| {
            let mut st = c.prepare("SELECT key, value FROM settings")?;
            let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
            let mut m = serde_json::Map::new();
            for r in rows {
                let (k, v) = r?;
                let val = serde_json::from_str(&v).unwrap_or(serde_json::Value::String(v));
                m.insert(k, val);
            }
            Ok(m)
        })
    }

    // ---------- accounts ----------
    pub fn accounts(&self) -> Result<Vec<Account>> {
        self.with(|c| {
            let mut st = c.prepare(&format!("SELECT {ACCOUNT_COLS} FROM accounts ORDER BY sort_order, id"))?;
            let rows = st.query_map([], account_from_row)?;
            rows.collect()
        })
    }
    pub fn account(&self, id: i64) -> Result<Account> {
        self.with(|c| {
            c.query_row(&format!("SELECT {ACCOUNT_COLS} FROM accounts WHERE id=?1"), [id], account_from_row)
        })
    }
    pub fn save_account(&self, a: &Account) -> Result<i64> {
        self.with(|c| {
            if a.id > 0 {
                c.execute(
                    "UPDATE accounts SET name=?2,email=?3,display_name=?4,protocol=?5,auth=?6,provider=?7,
                     in_host=?8,in_port=?9,in_security=?10,out_host=?11,out_port=?12,out_security=?13,username=?14,
                     pop3_dir=?15,pop3_leave_on_server=?16,pop3_recent_mode=?17,color=?18,sig_new=?19,sig_reply=?20,
                     calendar_sync=?21,enabled=?22,smtp_username=?23,space_id=?24,sync_limit=?25,body_limit=?26,
                     role_map=?27 WHERE id=?1",
                    params![a.id, a.name, a.email, a.display_name, a.protocol, a.auth, a.provider, a.in_host, a.in_port,
                        a.in_security, a.out_host, a.out_port, a.out_security, a.username, a.pop3_dir,
                        a.pop3_leave_on_server, a.pop3_recent_mode, a.color, a.sig_new, a.sig_reply,
                        a.calendar_sync, a.enabled, a.smtp_username, a.space_id, a.sync_limit, a.body_limit, a.role_map],
                )?;
                Ok(a.id)
            } else {
                let order: i64 = c.query_row("SELECT COALESCE(MAX(sort_order),0)+1 FROM accounts", [], |r| r.get(0))?;
                c.execute(
                    "INSERT INTO accounts(name,email,display_name,protocol,auth,provider,in_host,in_port,in_security,
                     out_host,out_port,out_security,username,pop3_dir,pop3_leave_on_server,pop3_recent_mode,color,
                     sort_order,sig_new,sig_reply,calendar_sync,enabled,smtp_username,space_id,sync_limit,body_limit,role_map)
                     VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25,?26,?27)",
                    params![a.name, a.email, a.display_name, a.protocol, a.auth, a.provider, a.in_host, a.in_port,
                        a.in_security, a.out_host, a.out_port, a.out_security, a.username, a.pop3_dir,
                        a.pop3_leave_on_server, a.pop3_recent_mode, a.color, order, a.sig_new, a.sig_reply,
                        a.calendar_sync, a.enabled, a.smtp_username, a.space_id, a.sync_limit, a.body_limit, a.role_map],
                )?;
                Ok(c.last_insert_rowid())
            }
        })
    }
    pub fn delete_account(&self, id: i64) -> Result<()> {
        self.with(|c| {
            c.execute(
                "DELETE FROM messages_fts WHERE rowid IN (SELECT id FROM messages WHERE account_id=?1)",
                [id],
            )?;
            c.execute("DELETE FROM accounts WHERE id=?1", [id])
        })?;
        Ok(())
    }
    pub fn reorder_accounts(&self, ids: &[i64]) -> Result<()> {
        self.with(|c| {
            for (i, id) in ids.iter().enumerate() {
                c.execute("UPDATE accounts SET sort_order=?2 WHERE id=?1", params![id, i as i64])?;
            }
            Ok(())
        })
    }
    pub fn set_account_status(&self, id: i64, error: &str) -> Result<()> {
        let now = chrono::Utc::now().timestamp();
        self.with(|c| {
            if error.is_empty() {
                c.execute("UPDATE accounts SET last_sync=?2,last_error='' WHERE id=?1", params![id, now])
            } else {
                c.execute("UPDATE accounts SET last_error=?2 WHERE id=?1", params![id, error])
            }
        })?;
        Ok(())
    }

    // ---------- folders ----------
    pub fn folders(&self) -> Result<Vec<Folder>> {
        self.with(|c| {
            let mut st = c.prepare(
                "SELECT f.id,f.account_id,f.path,f.name,f.delim,f.role,f.selectable,f.uidvalidity,f.last_uid,
                   (SELECT COUNT(*) FROM messages m WHERE m.folder_id=f.id AND m.seen=0 AND (f.role<>'inbox' OR m.id NOT IN
                      (SELECT mg.message_id FROM message_groups mg JOIN groups g ON g.id=mg.group_id))),
                   (SELECT COUNT(*) FROM messages m WHERE m.folder_id=f.id)
                 FROM folders f ORDER BY f.account_id, f.sort_order, f.path COLLATE NOCASE",
            )?;
            let rows = st.query_map([], folder_from_row)?;
            rows.collect()
        })
    }
    pub fn folder(&self, id: i64) -> Result<Folder> {
        self.with(|c| {
            c.query_row(
                "SELECT f.id,f.account_id,f.path,f.name,f.delim,f.role,f.selectable,f.uidvalidity,f.last_uid,0,0
                 FROM folders f WHERE f.id=?1",
                [id],
                folder_from_row,
            )
        })
    }
    pub fn folder_by_role(&self, account_id: i64, role: &str) -> Result<Option<Folder>> {
        self.with(|c| {
            c.query_row(
                "SELECT f.id,f.account_id,f.path,f.name,f.delim,f.role,f.selectable,f.uidvalidity,f.last_uid,0,0
                 FROM folders f WHERE f.account_id=?1 AND f.role=?2 LIMIT 1",
                params![account_id, role],
                folder_from_row,
            )
            .optional()
        })
    }
    pub fn upsert_folder(&self, account_id: i64, path: &str, name: &str, delim: &str, role: &str, selectable: bool) -> Result<i64> {
        let order = match role {
            "inbox" => 0, "drafts" => 1, "sent" => 2, "archive" => 3, "junk" => 4, "trash" => 5, "all" => 6, _ => 100,
        };
        self.with(|c| {
            c.execute(
                "INSERT INTO folders(account_id,path,name,delim,role,selectable,sort_order) VALUES(?1,?2,?3,?4,?5,?6,?7)
                 ON CONFLICT(account_id,path) DO UPDATE SET name=excluded.name, delim=excluded.delim,
                   role=excluded.role, selectable=excluded.selectable, sort_order=excluded.sort_order",
                params![account_id, path, name, delim, role, selectable, order],
            )?;
            c.query_row("SELECT id FROM folders WHERE account_id=?1 AND path=?2", params![account_id, path], |r| r.get(0))
        })
    }
    pub fn remove_folders_not_in(&self, account_id: i64, keep: &[String]) -> Result<()> {
        let existing: Vec<(i64, String)> = self.with(|c| {
            let mut st = c.prepare("SELECT id, path FROM folders WHERE account_id=?1")?;
            let r = st.query_map([account_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
            r.collect()
        })?;
        for (id, path) in existing {
            if !keep.contains(&path) {
                self.with(|c| {
                    c.execute("DELETE FROM messages_fts WHERE rowid IN (SELECT id FROM messages WHERE folder_id=?1)", [id])?;
                    c.execute("DELETE FROM folders WHERE id=?1", [id])
                })?;
            }
        }
        Ok(())
    }
    pub fn set_folder_sync(&self, id: i64, uidvalidity: u32, last_uid: u32) -> Result<()> {
        self.with(|c| c.execute("UPDATE folders SET uidvalidity=?2,last_uid=?3 WHERE id=?1", params![id, uidvalidity, last_uid]))?;
        Ok(())
    }
    pub fn clear_folder(&self, id: i64) -> Result<()> {
        self.with(|c| {
            c.execute("DELETE FROM messages_fts WHERE rowid IN (SELECT id FROM messages WHERE folder_id=?1)", [id])?;
            c.execute("DELETE FROM messages WHERE folder_id=?1", [id])
        })?;
        Ok(())
    }

    // ---------- messages ----------
    pub fn insert_message(&self, m: &NewMessage) -> Result<i64> {
        self.with(|c| {
            // A folder can be synced by two tasks at once (a quick refresh after saving a draft
            // and the regular sync): the same IMAP message is stored once.
            if m.uid > 0 {
                use rusqlite::OptionalExtension;
                let had: Option<i64> = c
                    .query_row("SELECT id FROM messages WHERE folder_id=?1 AND uid=?2", params![m.folder_id, m.uid], |r| r.get(0))
                    .optional()?;
                if let Some(id) = had {
                    return Ok(id);
                }
            }
            c.execute(
                "INSERT INTO messages(account_id,folder_id,uid,remote_key,message_id,in_reply_to,subject,from_name,from_addr,
                   to_list,cc_list,date,snippet,seen,flagged,answered,has_attachments,size,raw_path,body_indexed)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20)",
                params![m.account_id, m.folder_id, m.uid, m.remote_key, m.message_id, m.in_reply_to, m.subject,
                    m.from_name, m.from_addr, m.to_list, m.cc_list, m.date, m.snippet, m.seen, m.flagged, m.answered,
                    m.has_attachments, m.size, m.raw_path, m.body.is_some()],
            )?;
            let id = c.last_insert_rowid();
            let sender = format!("{} {}", m.from_name, m.from_addr);
            c.execute(
                "INSERT INTO messages_fts(rowid, subject, sender, body) VALUES(?1,?2,?3,?4)",
                params![id, m.subject, sender, m.body.as_deref().unwrap_or("")],
            )?;
            Ok(id)
        })
    }
    pub fn set_body_index(&self, id: i64, raw_path: &str, body: &str, snippet: &str, has_att: bool) -> Result<()> {
        self.with(|c| {
            c.execute(
                "UPDATE messages SET raw_path=?2, body_indexed=1, snippet=CASE WHEN ?3<>'' THEN ?3 ELSE snippet END,
                   has_attachments=?4 WHERE id=?1",
                params![id, raw_path, snippet, has_att],
            )?;
            c.execute("UPDATE messages_fts SET body=?2 WHERE rowid=?1", params![id, body])
        })?;
        Ok(())
    }
    /// Index rows of messages removed from a folder on the server.
    pub fn delete_by_uids(&self, folder_id: i64, uids: &[u32]) -> Result<()> {
        let ids: Vec<i64> = self.with(|c| {
            let mut st = c.prepare("SELECT id FROM messages WHERE folder_id=?1 AND uid=?2")?;
            let mut out = vec![];
            for u in uids {
                if let Ok(id) = st.query_row(params![folder_id, *u as i64], |r| r.get(0)) {
                    out.push(id);
                }
            }
            Ok(out)
        })?;
        if ids.is_empty() {
            return Ok(());
        }
        self.delete_messages(&ids)
    }
    pub fn delete_messages(&self, ids: &[i64]) -> Result<()> {
        self.with(|c| {
            for id in ids {
                c.execute("DELETE FROM messages_fts WHERE rowid=?1", [id])?;
                c.execute("DELETE FROM messages WHERE id=?1", [id])?;
            }
            Ok(())
        })
    }
    pub fn message_id_by_uid(&self, folder_id: i64, uid: i64) -> Result<Option<i64>> {
        use rusqlite::OptionalExtension;
        Ok(self.with(|c| c.query_row("SELECT id FROM messages WHERE folder_id=?1 AND uid=?2", params![folder_id, uid], |r| r.get(0)).optional())?)
    }
    pub fn message(&self, id: i64) -> Result<MessageRow> {
        self.with(|c| c.query_row(&format!("SELECT {MSG_COLS} FROM messages m WHERE m.id=?1"), [id], msg_from_row))
    }
    pub fn messages_by_ids(&self, ids: &[i64]) -> Result<Vec<MessageRow>> {
        ids.iter().map(|id| self.message(*id)).collect()
    }
    pub fn folder_uids(&self, folder_id: i64) -> Result<Vec<(i64, u32, bool, bool, bool)>> {
        self.with(|c| {
            let mut st = c.prepare("SELECT id, uid, seen, flagged, answered FROM messages WHERE folder_id=?1")?;
            let r = st.query_map([folder_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?;
            r.collect()
        })
    }
    pub fn pop_keys(&self, account_id: i64) -> Result<std::collections::HashSet<String>> {
        self.with(|c| {
            let mut st = c.prepare("SELECT remote_key FROM messages WHERE account_id=?1 AND remote_key<>''")?;
            let r = st.query_map([account_id], |r| r.get::<_, String>(0))?;
            r.collect()
        })
    }
    pub fn set_flags(&self, id: i64, seen: Option<bool>, flagged: Option<bool>, answered: Option<bool>) -> Result<()> {
        self.with(|c| {
            if let Some(v) = seen { c.execute("UPDATE messages SET seen=?2 WHERE id=?1", params![id, v])?; }
            if let Some(v) = flagged { c.execute("UPDATE messages SET flagged=?2 WHERE id=?1", params![id, v])?; }
            if let Some(v) = answered { c.execute("UPDATE messages SET answered=?2 WHERE id=?1", params![id, v])?; }
            Ok(())
        })
    }
    pub fn move_local(&self, id: i64, folder_id: i64) -> Result<()> {
        self.with(|c| c.execute("UPDATE messages SET folder_id=?2 WHERE id=?1", params![id, folder_id]))?;
        Ok(())
    }
    pub fn unindexed_messages(&self, folder_id: i64, limit: i64) -> Result<Vec<(i64, u32)>> {
        self.with(|c| {
            let mut st = c.prepare(
                "SELECT id, uid FROM messages WHERE folder_id=?1 AND body_indexed=0 AND size < 4000000
                 ORDER BY date DESC LIMIT ?2",
            )?;
            let r = st.query_map(params![folder_id, limit], |r| Ok((r.get(0)?, r.get(1)?)))?;
            r.collect()
        })
    }

    /// List messages for a view. `folder` may be a folder id, or a virtual view:
    /// "unified:inbox", "unified:flagged", "unified:unread".
    pub fn list_messages(&self, q: &ListQuery) -> Result<Vec<MessageRow>> {
        let mut sql = format!("SELECT {MSG_COLS} FROM messages m ");
        let mut where_: Vec<String> = vec![];
        let mut args: Vec<rusqlite::types::Value> = vec![];
        let search = q.search.trim();
        let parsed = parse_search(search);
        let mut body_hit = false;
        let mut people_hit = false;
        if !parsed.free.is_empty() && q.search_mode != "people" {
            sql.push_str("JOIN messages_fts f ON f.rowid = m.id ");
            let col = if q.search_mode == "subject" { "{subject}" } else { "{subject sender body}" };
            where_.push("messages_fts MATCH ?".into());
            args.push(format!("{col} : ({})", fts_terms(&parsed.free)).into());
            body_hit = q.search_mode != "subject";
        }
        let mut people: Vec<String> = vec![];
        if !parsed.free.is_empty() && q.search_mode == "people" {
            people.push(format!("{{sender recipients cc}} : ({})", fts_terms(&parsed.free)));
        }
        for (col, term, phrase) in &parsed.ops {
            let t = fts_terms(&[(term.clone(), *phrase)]);
            if t != "\"\"" {
                people.push(format!("{{{col}}} : ({t})"));
            }
        }
        if !people.is_empty() {
            sql.push_str("JOIN people_fts p ON p.rowid = m.id ");
            where_.push("people_fts MATCH ?".into());
            args.push(people.join(" AND ").into());
            people_hit = true;
        }
        if body_hit || people_hit {
            // Matching text for the result list, matches wrapped in U+E000 … U+E001.
            let b = if body_hit { "snippet(messages_fts, 2, char(57344), char(57345), '…', 14)" } else { "NULL" };
            let p = if people_hit {
                "highlight(people_fts, 0, char(57344), char(57345)), highlight(people_fts, 1, char(57344), char(57345)), highlight(people_fts, 2, char(57344), char(57345))"
            } else {
                "NULL, NULL, NULL"
            };
            sql = sql.replacen(&format!("SELECT {MSG_COLS} FROM"), &format!("SELECT {MSG_COLS}, {b}, {p} FROM"), 1);
        }
        const HIDDEN: &str = "m.id NOT IN (SELECT mg.message_id FROM message_groups mg JOIN groups g ON g.id=mg.group_id)";
        if q.space > 0 {
            where_.push("m.account_id IN (SELECT id FROM accounts WHERE space_id = ?)".into());
            args.push(q.space.into());
        }
        let is_inbox_folder = q.view.strip_prefix("folder:").and_then(|s| s.parse::<i64>().ok())
            .map(|id| self.folder(id).map(|f| f.role == "inbox").unwrap_or(false)).unwrap_or(false);
        if search.is_empty() && (q.view == "unified:inbox" || q.view == "unified:unread" || is_inbox_folder) {
            where_.push(HIDDEN.into());
        }
        match q.view.as_str() {
            "unified:inbox" => where_.push("m.folder_id IN (SELECT id FROM folders WHERE role='inbox')".into()),
            "unified:flagged" => where_.push("m.flagged=1 AND m.folder_id NOT IN (SELECT id FROM folders WHERE role IN ('trash','junk','all'))".into()),
            "unified:unread" => where_.push("m.seen=0 AND m.folder_id IN (SELECT id FROM folders WHERE role='inbox')".into()),
            "unified:sent" => where_.push("m.folder_id IN (SELECT id FROM folders WHERE role='sent')".into()),
            "unified:drafts" => where_.push("m.folder_id IN (SELECT id FROM folders WHERE role='drafts')".into()),
            "all" => where_.push("m.folder_id NOT IN (SELECT id FROM folders WHERE role IN ('all'))".into()),
            v => {
                if let Some(list) = v.strip_prefix("ids:") {
                    let ids: Vec<String> = list.split(',').filter_map(|x| x.trim().parse::<i64>().ok()).map(|x| x.to_string()).collect();
                    where_.push(format!("m.id IN ({})", if ids.is_empty() { "-1".into() } else { ids.join(",") }));
                } else if let Some(id) = v.strip_prefix("group:").and_then(|s| s.parse::<i64>().ok()) {
                    where_.push("m.id IN (SELECT message_id FROM message_groups WHERE group_id = ?)".into());
                    args.push(id.into());
                } else if let Some(id) = v.strip_prefix("folder:").and_then(|s| s.parse::<i64>().ok()) {
                    where_.push("m.folder_id = ?".into());
                    args.push(id.into());
                } else if let Some(id) = v.strip_prefix("account:").and_then(|s| s.parse::<i64>().ok()) {
                    where_.push("m.account_id = ? AND m.folder_id NOT IN (SELECT id FROM folders WHERE role='all')".into());
                    args.push(id.into());
                }
            }
        }
        match q.filter.as_str() {
            "unread" => where_.push("m.seen=0".into()),
            "flagged" => where_.push("m.flagged=1".into()),
            "attachments" => where_.push("m.has_attachments=1".into()),
            _ => {}
        }
        if !where_.is_empty() {
            sql.push_str("WHERE ");
            sql.push_str(&where_.join(" AND "));
        }
        sql.push_str(if q.sort_asc { " ORDER BY m.date ASC" } else { " ORDER BY m.date DESC" });
        sql.push_str(" LIMIT ? OFFSET ?");
        args.push(q.limit.into());
        args.push(q.offset.into());
        self.with(|c| {
            let mut st = c.prepare(&sql)?;
            let hits = body_hit || people_hit;
            let r = st.query_map(rusqlite::params_from_iter(args.iter()), |r| {
                let mut m = msg_from_row(r)?;
                if hits {
                    let body: Option<String> = r.get(20)?;
                    let cols: [(&str, Option<String>); 3] = [("from", r.get(21)?), ("to", r.get(22)?), ("cc", r.get(23)?)];
                    // People matches first (they explain why a message is here), then the body.
                    if let Some((f, t)) = cols.iter().find(|(_, t)| t.as_deref().map(|x| x.contains('\u{E000}')).unwrap_or(false)) {
                        m.hit = hit_window(t.as_deref().unwrap_or(""), 160);
                        m.hit_field = f.to_string();
                    } else if let Some(b) = body.filter(|b| b.contains('\u{E000}')) {
                        m.hit = crate::parse::collapse_ws(&b);
                        m.hit_field = "body".into();
                    }
                }
                Ok(m)
            })?;
            r.collect()
        })
    }

    pub fn unread_counts(&self, space: i64) -> Result<serde_json::Value> {
        self.with(|c| {
            let sp = if space > 0 { format!(" AND account_id IN (SELECT id FROM accounts WHERE space_id={space})") } else { String::new() };
            let unified: i64 = c.query_row(
                &format!("SELECT COUNT(*) FROM messages WHERE seen=0 AND folder_id IN (SELECT id FROM folders WHERE role='inbox')
                  AND id NOT IN (SELECT mg.message_id FROM message_groups mg JOIN groups g ON g.id=mg.group_id){sp}"),
                [], |r| r.get(0))?;
            let mut groups = serde_json::Map::new();
            let mut st = c.prepare(&format!(
                "SELECT mg.group_id, COUNT(*), SUM(CASE WHEN m.seen=0 THEN 1 ELSE 0 END) FROM message_groups mg
                 JOIN messages m ON m.id=mg.message_id WHERE 1=1{} GROUP BY mg.group_id", sp.replace("account_id", "m.account_id")))?;
            let rows = st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?)))?;
            for r in rows {
                let (g, total, unread) = r?;
                groups.insert(g.to_string(), serde_json::json!({ "total": total, "unread": unread }));
            }
            let mut accs = serde_json::Map::new();
            let mut st = c.prepare(
                "SELECT m.account_id, COUNT(*) FROM messages m WHERE m.seen=0 AND m.folder_id IN (SELECT id FROM folders WHERE role='inbox')
                 AND m.id NOT IN (SELECT mg.message_id FROM message_groups mg JOIN groups g ON g.id=mg.group_id)
                 GROUP BY m.account_id")?;
            let rows = st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?;
            for r in rows {
                let (a, n) = r?;
                accs.insert(a.to_string(), n.into());
            }
            let flagged: i64 = c.query_row(
                "SELECT COUNT(*) FROM messages WHERE flagged=1 AND folder_id NOT IN (SELECT id FROM folders WHERE role IN ('trash','junk','all'))",
                [], |r| r.get(0))?;
            let drafts: i64 = c.query_row(
                &format!("SELECT COUNT(*) FROM messages WHERE folder_id IN (SELECT id FROM folders WHERE role='drafts'){sp}"),
                [], |r| r.get(0))?;
            Ok(serde_json::json!({ "inbox": unified, "flagged": flagged, "drafts": drafts, "groups": groups, "accounts": accs }))
        })
    }

    // ---------- contacts ----------
    pub fn touch_contact(&self, addr: &str, name: &str) -> Result<()> {
        let now = chrono::Utc::now().timestamp();
        let addr = addr.trim().to_lowercase();
        if addr.is_empty() { return Ok(()); }
        self.with(|c| {
            c.execute(
                "INSERT INTO contacts(addr,name,uses,last_used) VALUES(?1,?2,1,?3)
                 ON CONFLICT(addr) DO UPDATE SET uses=uses+1, last_used=?3,
                   name=CASE WHEN excluded.name<>'' THEN excluded.name ELSE name END",
                params![addr, name, now],
            )
        })?;
        Ok(())
    }
    pub fn learn_contact(&self, addr: &str, name: &str) -> Result<()> {
        let addr = addr.trim().to_lowercase();
        if addr.is_empty() || addr.contains("noreply") || addr.contains("no-reply") { return Ok(()); }
        self.with(|c| {
            c.execute(
                "INSERT INTO contacts(addr,name,uses,last_used) VALUES(?1,?2,0,0)
                 ON CONFLICT(addr) DO UPDATE SET name=CASE WHEN name='' THEN excluded.name ELSE name END",
                params![addr, name],
            )
        })?;
        Ok(())
    }
    pub fn suggest_contacts(&self, prefix: &str) -> Result<Vec<Contact>> {
        let like = format!("%{}%", prefix.trim().to_lowercase().replace('%', ""));
        self.with(|c| {
            let mut st = c.prepare(
                "SELECT addr, name FROM contacts WHERE addr LIKE ?1 OR lower(name) LIKE ?1
                 ORDER BY uses DESC, last_used DESC LIMIT 8",
            )?;
            let r = st.query_map([like], |r| Ok(Contact { addr: r.get(0)?, name: r.get(1)? }))?;
            r.collect()
        })
    }

    // ---------- signatures ----------
    pub fn signatures(&self) -> Result<Vec<Signature>> {
        self.with(|c| {
            let mut st = c.prepare("SELECT id,name,html FROM signatures ORDER BY id")?;
            let r = st.query_map([], |r| Ok(Signature { id: r.get(0)?, name: r.get(1)?, html: r.get(2)? }))?;
            r.collect()
        })
    }
    pub fn save_signature(&self, s: &Signature) -> Result<i64> {
        self.with(|c| {
            if s.id > 0 {
                c.execute("UPDATE signatures SET name=?2, html=?3 WHERE id=?1", params![s.id, s.name, s.html])?;
                Ok(s.id)
            } else {
                c.execute("INSERT INTO signatures(name,html) VALUES(?1,?2)", params![s.name, s.html])?;
                Ok(c.last_insert_rowid())
            }
        })
    }
    pub fn delete_signature(&self, id: i64) -> Result<()> {
        self.with(|c| {
            c.execute("UPDATE accounts SET sig_new=NULL WHERE sig_new=?1", [id])?;
            c.execute("UPDATE accounts SET sig_reply=NULL WHERE sig_reply=?1", [id])?;
            c.execute("DELETE FROM signatures WHERE id=?1", [id])
        })?;
        Ok(())
    }

    // ---------- calendar ----------
    pub fn calendars(&self) -> Result<Vec<Calendar>> {
        self.with(|c| {
            let mut st = c.prepare("SELECT id,account_id,remote_id,name,color,visible,writable,kind,url,last_sync,last_error
                FROM calendars ORDER BY account_id IS NOT NULL, account_id, kind, id")?;
            let r = st.query_map([], |r| {
                Ok(Calendar {
                    id: r.get(0)?, account_id: r.get(1)?, remote_id: r.get(2)?, name: r.get(3)?,
                    color: r.get(4)?, visible: r.get(5)?, writable: r.get(6)?, kind: r.get(7)?, url: r.get(8)?,
                    last_sync: r.get(9)?, last_error: r.get(10)?,
                })
            })?;
            r.collect()
        })
    }
    pub fn calendar(&self, id: i64) -> Result<Calendar> {
        Ok(self.calendars()?.into_iter().find(|c| c.id == id).ok_or_else(|| anyhow!("calendar not found"))?)
    }
    pub fn upsert_calendar(&self, account_id: i64, remote_id: &str, name: &str, color: &str, writable: bool) -> Result<i64> {
        self.with(|c| {
            c.execute(
                "INSERT INTO calendars(account_id,remote_id,name,color,writable,kind) VALUES(?1,?2,?3,?4,?5,'google')
                 ON CONFLICT(account_id,remote_id) DO UPDATE SET name=excluded.name, color=excluded.color, writable=excluded.writable",
                params![account_id, remote_id, name, color, writable],
            )?;
            c.query_row("SELECT id FROM calendars WHERE account_id=?1 AND remote_id=?2", params![account_id, remote_id], |r| r.get(0))
        })
    }
    pub fn set_calendar_visible(&self, id: i64, visible: bool) -> Result<()> {
        self.with(|c| c.execute("UPDATE calendars SET visible=?2 WHERE id=?1", params![id, visible]))?;
        Ok(())
    }
    pub fn events(&self, from: i64, to: i64) -> Result<Vec<Event>> {
        self.with(|c| {
            let mut st = c.prepare(
                "SELECT e.id,e.calendar_id,e.remote_id,e.title,e.location,e.description,e.start_ts,e.end_ts,e.all_day,
                        e.recurring,e.meet_link,c.color
                 FROM events e JOIN calendars c ON c.id=e.calendar_id
                 WHERE c.visible=1 AND e.end_ts > ?1 AND e.start_ts < ?2 ORDER BY e.start_ts",
            )?;
            let r = st.query_map(params![from, to], event_from_row)?;
            r.collect()
        })
    }
    /// Timed events starting in (now, now + ahead], visible calendars only.
    pub fn upcoming_events(&self, now: i64, ahead: i64) -> Result<Vec<Event>> {
        self.with(|c| {
            let mut st = c.prepare(
                "SELECT e.id,e.calendar_id,e.remote_id,e.title,e.location,e.description,e.start_ts,e.end_ts,e.all_day,
                        e.recurring,e.meet_link,c.color
                 FROM events e JOIN calendars c ON c.id=e.calendar_id
                 WHERE c.visible=1 AND e.all_day=0 AND e.start_ts > ?1 AND e.start_ts <= ?2 ORDER BY e.start_ts",
            )?;
            let r = st.query_map(params![now, now + ahead], event_from_row)?;
            r.collect()
        })
    }
    /// (fired lead time, skipped) for a reminder key.
    pub fn reminder(&self, key: &str) -> Result<Option<(i64, bool)>> {
        self.with(|c| {
            c.query_row("SELECT fired, skipped FROM reminders WHERE key=?1", [key], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? != 0)))
                .optional()
        })
    }
    pub fn set_reminder(&self, key: &str, start_ts: i64, fired: Option<i64>, skipped: Option<bool>) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO reminders(key, fired, skipped, start_ts) VALUES(?1, COALESCE(?2, 0), COALESCE(?3, 0), ?4)
                 ON CONFLICT(key) DO UPDATE SET fired=COALESCE(?2, fired), skipped=COALESCE(?3, skipped)",
                params![key, fired, skipped.map(|b| b as i64), start_ts],
            )?;
            // Forget reminders of events long gone.
            c.execute("DELETE FROM reminders WHERE start_ts < ?1", [chrono::Utc::now().timestamp() - 3 * 86_400])?;
            Ok(())
        })
    }
    pub fn invite_state(&self, message_id: i64) -> Result<Option<(String, i64)>> {
        self.with(|c| {
            c.query_row("SELECT state, event_id FROM invites WHERE message_id=?1", [message_id], |r| Ok((r.get(0)?, r.get(1)?)))
                .optional()
        })
    }
    pub fn set_invite_state(&self, message_id: i64, state: &str, event_id: i64) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO invites(message_id, state, event_id, updated) VALUES(?1, ?2, ?3, ?4)
                 ON CONFLICT(message_id) DO UPDATE SET state=?2, event_id=CASE WHEN ?3 > 0 THEN ?3 ELSE event_id END, updated=?4",
                params![message_id, state, event_id, chrono::Utc::now().timestamp()],
            )?;
            Ok(())
        })
    }
    /// An event already in the calendars with this title starting within a minute of `start`.
    pub fn find_event(&self, title: &str, start: i64) -> Result<Option<i64>> {
        self.with(|c| {
            c.query_row(
                "SELECT id FROM events WHERE lower(trim(title))=lower(trim(?1)) AND abs(start_ts - ?2) <= 60 LIMIT 1",
                params![title, start],
                |r| r.get(0),
            )
            .optional()
        })
    }
    pub fn event(&self, id: i64) -> Result<Event> {
        self.with(|c| {
            c.query_row(
                "SELECT e.id,e.calendar_id,e.remote_id,e.title,e.location,e.description,e.start_ts,e.end_ts,e.all_day,
                        e.recurring,e.meet_link,c.color
                 FROM events e JOIN calendars c ON c.id=e.calendar_id WHERE e.id=?1",
                [id], event_from_row,
            )
        })
    }
    pub fn save_event(&self, e: &Event) -> Result<i64> {
        let now = chrono::Utc::now().timestamp();
        self.with(|c| {
            if e.id > 0 {
                c.execute(
                    "UPDATE events SET calendar_id=?2,remote_id=?3,title=?4,location=?5,description=?6,start_ts=?7,end_ts=?8,
                       all_day=?9,recurring=?10,meet_link=?11,updated=?12 WHERE id=?1",
                    params![e.id, e.calendar_id, e.remote_id, e.title, e.location, e.description, e.start_ts, e.end_ts,
                        e.all_day, e.recurring, e.meet_link, now],
                )?;
                Ok(e.id)
            } else {
                c.execute(
                    "INSERT INTO events(calendar_id,remote_id,title,location,description,start_ts,end_ts,all_day,recurring,meet_link,updated)
                     VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
                    params![e.calendar_id, e.remote_id, e.title, e.location, e.description, e.start_ts, e.end_ts,
                        e.all_day, e.recurring, e.meet_link, now],
                )?;
                Ok(c.last_insert_rowid())
            }
        })
    }
    pub fn delete_event(&self, id: i64) -> Result<()> {
        self.with(|c| c.execute("DELETE FROM events WHERE id=?1", [id]))?;
        Ok(())
    }
    /// Replace the cached events of a remote calendar inside a time window.
    pub fn replace_remote_events(&self, calendar_id: i64, from: i64, to: i64, evs: &[Event]) -> Result<()> {
        let mut c = self.conn.lock().map_err(|_| anyhow!("db lock"))?;
        let tx = c.transaction()?;
        tx.execute(
            "DELETE FROM events WHERE calendar_id=?1 AND end_ts > ?2 AND start_ts < ?3",
            params![calendar_id, from, to],
        )?;
        for e in evs {
            tx.execute(
                "INSERT INTO events(calendar_id,remote_id,title,location,description,start_ts,end_ts,all_day,recurring,meet_link,updated)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,0)",
                params![calendar_id, e.remote_id, e.title, e.location, e.description, e.start_ts, e.end_ts, e.all_day,
                    e.recurring, e.meet_link],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
}

/// Split a search string into terms: bare words, or "quoted phrases" kept whole.
pub fn search_terms(s: &str) -> Vec<(String, bool)> {
    let mut out = vec![];
    let mut cur = String::new();
    let mut in_q = false;
    for ch in s.chars() {
        match ch {
            '"' => {
                let t = cur.trim().to_string();
                if !t.is_empty() {
                    out.push((t, in_q));
                }
                cur.clear();
                in_q = !in_q;
            }
            c if c.is_whitespace() && !in_q => {
                let t = cur.trim().to_string();
                if !t.is_empty() {
                    out.push((t, false));
                }
                cur.clear();
            }
            c => cur.push(c),
        }
    }
    let t = cur.trim().to_string();
    if !t.is_empty() {
        out.push((t, in_q));
    }
    out
}

pub struct ParsedSearch {
    /// Plain words / "phrases" (searched according to the search mode).
    pub free: Vec<(String, bool)>,
    /// `from:` / `to:` / `cc:` filters: (people_fts column, term, is phrase). All must match.
    pub ops: Vec<(&'static str, String, bool)>,
}

/// Split the query into free terms and `from:` / `to:` / `cc:` filters (`to:"Lan Pham"` works too).
pub fn parse_search(s: &str) -> ParsedSearch {
    let mut out = ParsedSearch { free: vec![], ops: vec![] };
    let terms = search_terms(s);
    let col_of = |k: &str| -> Option<&'static str> {
        match k.to_lowercase().as_str() {
            "from" | "tu" | "từ" => Some("sender"),
            "to" | "den" | "đến" => Some("recipients"),
            "cc" => Some("cc"),
            _ => None,
        }
    };
    let mut i = 0;
    while i < terms.len() {
        let (t, phrase) = &terms[i];
        if !*phrase {
            if let Some((k, v)) = t.split_once(':') {
                if let Some(col) = col_of(k) {
                    if v.is_empty() {
                        // `to: "Lan Pham"` / `to:"Lan Pham"` — the value is the next term.
                        if let Some((nv, np)) = terms.get(i + 1) {
                            out.ops.push((col, nv.clone(), *np));
                            i += 2;
                            continue;
                        }
                    } else {
                        out.ops.push((col, v.to_string(), false));
                        i += 1;
                        continue;
                    }
                }
            }
        }
        out.free.push((t.clone(), *phrase));
        i += 1;
    }
    out
}

/// Keep ~`max` characters of a highlighted field around its first match.
fn hit_window(text: &str, max: usize) -> String {
    let text = crate::parse::collapse_ws(text);
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max {
        return text;
    }
    let first = chars.iter().position(|&c| c == '\u{E000}').unwrap_or(0);
    let start = first.saturating_sub(max / 3);
    let end = (start + max).min(chars.len());
    let mut s: String = chars[start..end].iter().collect();
    // Never cut a marker pair in half.
    if s.matches('\u{E000}').count() > s.matches('\u{E001}').count() {
        s.push('\u{E001}');
    }
    let s = s.trim_start_matches('\u{E001}').to_string();
    format!("{}{s}{}", if start > 0 { "…" } else { "" }, if end < chars.len() { "…" } else { "" })
}

fn fts_terms(terms: &[(String, bool)]) -> String {
    let parts: Vec<String> = terms
        .iter()
        .map(|(t, phrase)| {
            let clean: String = t.chars().map(|c| if c == '"' || c == '*' { ' ' } else { c }).collect();
            let clean = clean.split_whitespace().collect::<Vec<_>>().join(" ");
            if *phrase { format!("\"{clean}\"") } else { format!("\"{clean}\"*") }
        })
        .filter(|p| p.len() > 3)
        .collect();
    if parts.is_empty() { "\"\"".into() } else { parts.join(" OR ") }
}

/// FTS5 query: words are OR-ed (prefix match), a "quoted phrase" must appear as written, in order.
#[cfg_attr(not(test), allow(dead_code))]
pub fn fts_query(s: &str) -> String {
    fts_terms(&search_terms(s))
}

const ACCOUNT_COLS: &str = "id,name,email,display_name,protocol,auth,provider,in_host,in_port,in_security,out_host,out_port,
  out_security,username,pop3_dir,pop3_leave_on_server,pop3_recent_mode,color,sort_order,sig_new,sig_reply,calendar_sync,
  enabled,last_sync,last_error,smtp_username,space_id,sync_limit,body_limit,role_map";

fn account_from_row(r: &Row) -> rusqlite::Result<Account> {
    Ok(Account {
        id: r.get(0)?, name: r.get(1)?, email: r.get(2)?, display_name: r.get(3)?, protocol: r.get(4)?,
        auth: r.get(5)?, provider: r.get(6)?, in_host: r.get(7)?, in_port: r.get(8)?, in_security: r.get(9)?,
        out_host: r.get(10)?, out_port: r.get(11)?, out_security: r.get(12)?, username: r.get(13)?,
        pop3_dir: r.get(14)?, pop3_leave_on_server: r.get(15)?, pop3_recent_mode: r.get(16)?, color: r.get(17)?,
        sort_order: r.get(18)?, sig_new: r.get(19)?, sig_reply: r.get(20)?, calendar_sync: r.get(21)?,
        enabled: r.get(22)?, last_sync: r.get(23)?, last_error: r.get(24)?, smtp_username: r.get(25)?,
        space_id: r.get(26)?, sync_limit: r.get(27)?, body_limit: r.get(28)?, role_map: r.get(29)?,
    })
}

fn folder_from_row(r: &Row) -> rusqlite::Result<Folder> {
    Ok(Folder {
        id: r.get(0)?, account_id: r.get(1)?, path: r.get(2)?, name: r.get(3)?, delim: r.get(4)?, role: r.get(5)?,
        selectable: r.get(6)?, uidvalidity: r.get(7)?, last_uid: r.get(8)?, unread: r.get(9)?, total: r.get(10)?,
    })
}

const MSG_COLS: &str = "m.id,m.account_id,m.folder_id,m.uid,m.remote_key,m.message_id,m.in_reply_to,m.subject,m.from_name,
  m.from_addr,m.to_list,m.cc_list,m.date,m.snippet,m.seen,m.flagged,m.answered,m.has_attachments,m.size,m.raw_path";

fn msg_from_row(r: &Row) -> rusqlite::Result<MessageRow> {
    Ok(MessageRow {
        id: r.get(0)?, account_id: r.get(1)?, folder_id: r.get(2)?, uid: r.get(3)?, remote_key: r.get(4)?,
        message_id: r.get(5)?, in_reply_to: r.get(6)?, subject: r.get(7)?, from_name: r.get(8)?, from_addr: r.get(9)?,
        to_list: r.get(10)?, cc_list: r.get(11)?, date: r.get(12)?, snippet: r.get(13)?, seen: r.get(14)?,
        flagged: r.get(15)?, answered: r.get(16)?, has_attachments: r.get(17)?, size: r.get(18)?, raw_path: r.get(19)?,
        hit: String::new(), hit_field: String::new(),
    })
}

fn event_from_row(r: &Row) -> rusqlite::Result<Event> {
    Ok(Event {
        id: r.get(0)?, calendar_id: r.get(1)?, remote_id: r.get(2)?, title: r.get(3)?, location: r.get(4)?,
        description: r.get(5)?, start_ts: r.get(6)?, end_ts: r.get(7)?, all_day: r.get(8)?, recurring: r.get(9)?,
        meet_link: r.get(10)?, color: r.get(11)?,
    })
}

// ---------- spaces, groups, rules, ICS calendars ----------
impl Db {
    pub fn spaces(&self) -> Result<Vec<Space>> {
        self.with(|c| {
            let mut st = c.prepare("SELECT id,name,color,sort_order FROM spaces ORDER BY sort_order, id")?;
            let r = st.query_map([], |r| Ok(Space { id: r.get(0)?, name: r.get(1)?, color: r.get(2)?, sort_order: r.get(3)? }))?;
            r.collect()
        })
    }
    pub fn save_space(&self, s: &Space) -> Result<i64> {
        self.with(|c| {
            if s.id > 0 {
                c.execute("UPDATE spaces SET name=?2,color=?3,sort_order=?4 WHERE id=?1", params![s.id, s.name, s.color, s.sort_order])?;
                Ok(s.id)
            } else {
                let order: i64 = c.query_row("SELECT COALESCE(MAX(sort_order),0)+1 FROM spaces", [], |r| r.get(0))?;
                c.execute("INSERT INTO spaces(name,color,sort_order) VALUES(?1,?2,?3)", params![s.name, s.color, order])?;
                Ok(c.last_insert_rowid())
            }
        })
    }
    pub fn delete_space(&self, id: i64) -> Result<()> {
        self.with(|c| {
            c.execute("UPDATE accounts SET space_id=NULL WHERE space_id=?1", [id])?;
            c.execute("DELETE FROM spaces WHERE id=?1", [id])
        })?;
        Ok(())
    }
    pub fn set_account_space(&self, account_id: i64, space_id: Option<i64>) -> Result<()> {
        self.with(|c| c.execute("UPDATE accounts SET space_id=?2 WHERE id=?1", params![account_id, space_id]))?;
        Ok(())
    }

    pub fn groups(&self) -> Result<Vec<Group>> {
        self.with(|c| {
            let mut st = c.prepare("SELECT id,name,color,hide_from_inbox,sort_order,account_id FROM groups ORDER BY account_id IS NOT NULL, sort_order, id")?;
            let r = st.query_map([], |r| {
                Ok(Group { id: r.get(0)?, name: r.get(1)?, color: r.get(2)?, hide_from_inbox: r.get(3)?, sort_order: r.get(4)?, account_id: r.get(5)? })
            })?;
            r.collect()
        })
    }
    pub fn save_group(&self, g: &Group) -> Result<i64> {
        self.with(|c| {
            if g.id > 0 {
                c.execute(
                    "UPDATE groups SET name=?2,color=?3,hide_from_inbox=1,sort_order=?4,account_id=?5 WHERE id=?1",
                    params![g.id, g.name, g.color, g.sort_order, g.account_id],
                )?;
                Ok(g.id)
            } else {
                let order: i64 = c.query_row("SELECT COALESCE(MAX(sort_order),0)+1 FROM groups", [], |r| r.get(0))?;
                c.execute(
                    "INSERT INTO groups(name,color,hide_from_inbox,sort_order,account_id) VALUES(?1,?2,1,?3,?4)",
                    params![g.name, g.color, order, g.account_id],
                )?;
                Ok(c.last_insert_rowid())
            }
        })
    }
    pub fn delete_group(&self, id: i64) -> Result<()> {
        self.with(|c| c.execute("DELETE FROM groups WHERE id=?1", [id]))?;
        Ok(())
    }
    pub fn add_to_group(&self, ids: &[i64], group_id: i64) -> Result<()> {
        self.with(|c| {
            for id in ids {
                c.execute("INSERT OR IGNORE INTO message_groups(message_id,group_id) VALUES(?1,?2)", params![id, group_id])?;
            }
            Ok(())
        })
    }
    pub fn remove_from_group(&self, ids: &[i64], group_id: i64) -> Result<()> {
        self.with(|c| {
            for id in ids {
                c.execute("DELETE FROM message_groups WHERE message_id=?1 AND group_id=?2", params![id, group_id])?;
            }
            Ok(())
        })
    }
    pub fn message_group_ids(&self, id: i64) -> Result<Vec<i64>> {
        self.with(|c| {
            let mut st = c.prepare("SELECT group_id FROM message_groups WHERE message_id=?1")?;
            let r = st.query_map([id], |r| r.get(0))?;
            r.collect()
        })
    }

    pub fn rules(&self) -> Result<Vec<Rule>> {
        self.with(|c| {
            let mut st = c.prepare(
                "SELECT id,name,enabled,match_all,conditions,account_ids,group_id,mark_read,flag,stop,sort_order FROM rules ORDER BY sort_order, id",
            )?;
            let r = st.query_map([], |r| {
                let conds: String = r.get(4)?;
                let accs: String = r.get(5)?;
                Ok(Rule {
                    id: r.get(0)?, name: r.get(1)?, enabled: r.get(2)?, match_all: r.get(3)?,
                    conditions: serde_json::from_str(&conds).unwrap_or_default(),
                    account_ids: serde_json::from_str(&accs).unwrap_or_default(),
                    group_id: r.get(6)?, mark_read: r.get(7)?, flag: r.get(8)?, stop: r.get(9)?, sort_order: r.get(10)?,
                })
            })?;
            r.collect()
        })
    }
    pub fn save_rule(&self, ru: &Rule) -> Result<i64> {
        let conds = serde_json::to_string(&ru.conditions)?;
        let accs = serde_json::to_string(&ru.account_ids)?;
        self.with(|c| {
            if ru.id > 0 {
                c.execute(
                    "UPDATE rules SET name=?2,enabled=?3,match_all=?4,conditions=?5,account_ids=?6,group_id=?7,mark_read=?8,flag=?9,stop=?10,sort_order=?11 WHERE id=?1",
                    params![ru.id, ru.name, ru.enabled, ru.match_all, conds, accs, ru.group_id, ru.mark_read, ru.flag, ru.stop, ru.sort_order],
                )?;
                Ok(ru.id)
            } else {
                let order: i64 = c.query_row("SELECT COALESCE(MAX(sort_order),0)+1 FROM rules", [], |r| r.get(0))?;
                c.execute(
                    "INSERT INTO rules(name,enabled,match_all,conditions,account_ids,group_id,mark_read,flag,stop,sort_order) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                    params![ru.name, ru.enabled, ru.match_all, conds, accs, ru.group_id, ru.mark_read, ru.flag, ru.stop, order],
                )?;
                Ok(c.last_insert_rowid())
            }
        })
    }
    pub fn delete_rule(&self, id: i64) -> Result<()> {
        self.with(|c| c.execute("DELETE FROM rules WHERE id=?1", [id]))?;
        Ok(())
    }
    /// Plain-text body from the search index (empty until the body is downloaded).
    pub fn indexed_body(&self, id: i64) -> Result<String> {
        self.with(|c| {
            c.query_row("SELECT body FROM messages_fts WHERE rowid=?1", [id], |r| r.get::<_, String>(0))
                .optional()
                .map(|x| x.unwrap_or_default())
        })
    }
    pub fn message_ids_where(&self, sql_where: &str, limit: i64) -> Result<Vec<i64>> {
        self.with(|c| {
            let mut st = c.prepare(&format!("SELECT id FROM messages WHERE {sql_where} ORDER BY date DESC LIMIT {limit}"))?;
            let r = st.query_map([], |r| r.get(0))?;
            r.collect()
        })
    }

    // ICS subscriptions live in `calendars` with kind='ics'.
    pub fn add_ics_calendar(&self, name: &str, url: &str, color: &str) -> Result<i64> {
        self.with(|c| {
            c.execute(
                "INSERT INTO calendars(account_id,remote_id,name,color,writable,kind,url) VALUES(NULL,?1,?2,?3,0,'ics',?4)",
                params![format!("ics:{url}"), name, color, url],
            )?;
            Ok(c.last_insert_rowid())
        })
    }
    pub fn update_calendar(&self, id: i64, name: &str, color: &str) -> Result<()> {
        self.with(|c| c.execute("UPDATE calendars SET name=?2,color=?3 WHERE id=?1", params![id, name, color]))?;
        Ok(())
    }
    pub fn delete_calendar(&self, id: i64) -> Result<()> {
        self.with(|c| c.execute("DELETE FROM calendars WHERE id=?1 AND id<>1", [id]))?;
        Ok(())
    }
    pub fn set_calendar_status(&self, id: i64, err: &str) -> Result<()> {
        let now = chrono::Utc::now().timestamp();
        self.with(|c| c.execute("UPDATE calendars SET last_sync=?2,last_error=?3 WHERE id=?1", params![id, now, err]))?;
        Ok(())
    }
    pub fn replace_all_events(&self, calendar_id: i64, evs: &[Event]) -> Result<()> {
        let mut c = self.conn.lock().map_err(|_| anyhow!("db lock"))?;
        let tx = c.transaction()?;
        tx.execute("DELETE FROM events WHERE calendar_id=?1", [calendar_id])?;
        for e in evs {
            tx.execute(
                "INSERT INTO events(calendar_id,remote_id,title,location,description,start_ts,end_ts,all_day,recurring,meet_link,updated)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,0)",
                params![calendar_id, e.remote_id, e.title, e.location, e.description, e.start_ts, e.end_ts, e.all_day, e.recurring, e.meet_link],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod search_tests {
    use super::*;
    #[test]
    fn or_and_phrase() {
        assert_eq!(fts_query("hop nhom"), "\"hop\"* OR \"nhom\"*");
        assert_eq!(fts_query("\"bao cao\" thang"), "\"bao cao\" OR \"thang\"*");
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE VIRTUAL TABLE t USING fts5(subject, tokenize='unicode61 remove_diacritics 2');
            INSERT INTO t(rowid,subject) VALUES (1,'Báo cáo tháng 10'),(2,'Cáo báo ngược'),(3,'Họp nhóm thứ Năm');").unwrap();
        let q = |s: &str| -> Vec<i64> {
            let mut st = c.prepare("SELECT rowid FROM t WHERE t MATCH ?1 ORDER BY rowid").unwrap();
            st.query_map([fts_query(s)], |r| r.get(0)).unwrap().map(|x| x.unwrap()).collect()
        };
        assert_eq!(q("\"bao cao\""), vec![1]); // phrase: in order, adjacent
        assert_eq!(q("bao cao"), vec![1, 2]); // words: OR
        assert_eq!(q("hop thang"), vec![1, 3]);
    }

    #[test]
    fn people_and_hits() {
        let dir = std::env::temp_dir().join(format!("hermio-search-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let db = Db::open(&dir).unwrap();
        let acc = db.save_account(&Account { email: "me@x.com".into(), name: "Me".into(), protocol: "imap".into(), enabled: true, ..Default::default() }).unwrap();
        let inbox = db.upsert_folder(acc, "INBOX", "Inbox", "/", "inbox", true).unwrap();
        let mk = |from_name: &str, from: &str, to: &str, cc: &str, subject: &str, body: &str| {
            let m = NewMessage {
                account_id: acc, folder_id: inbox, subject: subject.into(), from_name: from_name.into(), from_addr: from.into(),
                to_list: to.into(), cc_list: cc.into(), date: 1, body: Some(body.into()), ..Default::default()
            };
            db.insert_message(&m).unwrap()
        };
        let a = mk("Phạm Lan", "lan@monash.edu", "me@x.com", "", "Họp nhóm", "Hẹn gặp ở Clayton campus của Monash University vào thứ Năm");
        let b = mk("GitHub", "noreply@github.com", "me@x.com", "theo.tang@monash.edu", "PR merged", "nothing here");
        let c = mk("Shop", "a@shop.vn", "minh@student.monash.edu, other@x.com", "", "Order", "Your invoice");
        let q = |search: &str, mode: &str| -> Vec<MessageRow> {
            db.list_messages(&ListQuery { view: "all".into(), search: search.into(), search_mode: mode.into(), limit: 50, ..Default::default() }).unwrap()
        };
        let ids = |v: &[MessageRow]| { let mut x: Vec<i64> = v.iter().map(|m| m.id).collect(); x.sort(); x };
        // People mode: sender name (no diacritics), To and Cc addresses.
        assert_eq!(ids(&q("lan", "people")), vec![a]);
        assert_eq!(ids(&q("monash", "people")), vec![a, b, c]);
        let r = q("theo.tang@monash.edu", "people");
        assert_eq!(ids(&r), vec![b]);
        assert_eq!(r[0].hit_field, "cc");
        assert!(r[0].hit.contains('\u{E000}'));
        // Operators work in any mode.
        assert_eq!(ids(&q("to:minh", "content")), vec![c]);
        assert_eq!(ids(&q("cc:monash", "subject")), vec![b]);
        assert_eq!(ids(&q("from:\"Pham Lan\"", "content")), vec![a]);
        // Content mode: body hit with markers around the match.
        let r = q("clayton", "content");
        assert_eq!(ids(&r), vec![a]);
        assert_eq!(r[0].hit_field, "body");
        assert!(r[0].hit.contains("\u{E000}Clayton\u{E001}"), "{}", r[0].hit);
        let _ = std::fs::remove_dir_all(dir);
    }
}
