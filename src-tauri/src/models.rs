use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Account {
    pub id: i64,
    pub name: String,
    pub email: String,
    pub display_name: String,
    /// "imap" | "pop3"
    pub protocol: String,
    /// "password" | "oauth_google"
    pub auth: String,
    /// "google" | "microsoft" | "other"
    pub provider: String,
    pub in_host: String,
    pub in_port: u16,
    /// "ssl" | "starttls" | "none"
    pub in_security: String,
    pub out_host: String,
    pub out_port: u16,
    pub out_security: String,
    pub username: String,
    pub pop3_dir: String,
    pub pop3_leave_on_server: bool,
    pub pop3_recent_mode: bool,
    pub color: String,
    pub sort_order: i64,
    pub sig_new: Option<i64>,
    pub sig_reply: Option<i64>,
    pub calendar_sync: bool,
    pub enabled: bool,
    pub last_sync: i64,
    pub last_error: String,
    /// Separate SMTP login (advanced); empty = same as incoming.
    pub smtp_username: String,
    pub space_id: Option<i64>,
    /// Newest messages listed per folder on first sync.
    pub sync_limit: i64,
    /// Bodies downloaded per folder per sync (for full-text search).
    pub body_limit: i64,
    /// JSON {"sent": "Folder path", ...} overriding detected special folders.
    pub role_map: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Space {
    pub id: i64,
    pub name: String,
    pub color: String,
    pub sort_order: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Group {
    pub id: i64,
    pub name: String,
    pub color: String,
    /// Always true: grouped mail never shows in the inbox.
    pub hide_from_inbox: bool,
    pub sort_order: i64,
    /// None = global group (all mailboxes); Some(id) = local to one mailbox.
    pub account_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Condition {
    /// from | to | cc | subject | body | any | account | attachment
    pub field: String,
    /// contains | not_contains | equals | starts_with | ends_with | regex | domain
    pub op: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Rule {
    pub id: i64,
    pub name: String,
    pub enabled: bool,
    pub match_all: bool,
    pub conditions: Vec<Condition>,
    pub account_ids: Vec<i64>,
    pub group_id: Option<i64>,
    pub mark_read: bool,
    pub flag: bool,
    pub stop: bool,
    pub sort_order: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Folder {
    pub id: i64,
    pub account_id: i64,
    pub path: String,
    pub name: String,
    pub delim: String,
    pub role: String,
    pub selectable: bool,
    pub uidvalidity: u32,
    pub last_uid: u32,
    pub unread: i64,
    pub total: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MessageRow {
    pub id: i64,
    pub account_id: i64,
    pub folder_id: i64,
    pub uid: u32,
    pub remote_key: String,
    pub message_id: String,
    pub in_reply_to: String,
    pub subject: String,
    pub from_name: String,
    pub from_addr: String,
    pub to_list: String,
    pub cc_list: String,
    pub date: i64,
    pub snippet: String,
    pub seen: bool,
    pub flagged: bool,
    pub answered: bool,
    pub has_attachments: bool,
    pub size: i64,
    pub raw_path: String,
    /// Search results only: the matching text, with matches wrapped in U+E000 … U+E001.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub hit: String,
    /// Where `hit` comes from: "body", "from", "to" or "cc".
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub hit_field: String,
}

#[derive(Debug, Clone, Default)]
pub struct NewMessage {
    pub account_id: i64,
    pub folder_id: i64,
    pub uid: u32,
    pub remote_key: String,
    pub message_id: String,
    pub in_reply_to: String,
    pub subject: String,
    pub from_name: String,
    pub from_addr: String,
    pub to_list: String,
    pub cc_list: String,
    pub date: i64,
    pub snippet: String,
    pub seen: bool,
    pub flagged: bool,
    pub answered: bool,
    pub has_attachments: bool,
    pub size: i64,
    pub raw_path: String,
    /// Plain-text body for the full-text index, when available.
    pub body: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ListQuery {
    pub view: String,
    #[serde(default)]
    pub search: String,
    #[serde(default)]
    pub search_mode: String,
    #[serde(default)]
    pub filter: String,
    #[serde(default)]
    pub sort_asc: bool,
    #[serde(default)]
    pub space: i64,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Contact {
    pub addr: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signature {
    pub id: i64,
    pub name: String,
    pub html: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Calendar {
    pub id: i64,
    pub account_id: Option<i64>,
    pub remote_id: String,
    pub name: String,
    pub color: String,
    pub visible: bool,
    pub writable: bool,
    /// local | google | ics
    pub kind: String,
    pub url: String,
    pub last_sync: i64,
    pub last_error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Event {
    pub id: i64,
    pub calendar_id: i64,
    pub remote_id: String,
    pub title: String,
    pub location: String,
    pub description: String,
    pub start_ts: i64,
    pub end_ts: i64,
    pub all_day: bool,
    pub recurring: bool,
    pub meet_link: String,
    pub color: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Attachment {
    pub index: usize,
    pub name: String,
    pub mime: String,
    pub size: usize,
    pub inline: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct MessageBody {
    pub id: i64,
    pub html: String,
    pub text: String,
    pub has_remote_images: bool,
    pub attachments: Vec<Attachment>,
    pub to: String,
    pub cc: String,
    pub reply_to: String,
    pub references: String,
    pub ics: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutgoingAttachment {
    pub path: Option<String>,
    /// For forwarding: (message id, attachment index)
    pub from_message: Option<(i64, usize)>,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Outgoing {
    pub account_id: i64,
    pub to: Vec<String>,
    #[serde(default)]
    pub cc: Vec<String>,
    #[serde(default)]
    pub bcc: Vec<String>,
    pub subject: String,
    pub html: String,
    #[serde(default)]
    pub in_reply_to: String,
    #[serde(default)]
    pub references: String,
    #[serde(default)]
    pub attachments: Vec<OutgoingAttachment>,
    /// Message id this replies to (to mark answered)
    #[serde(default)]
    pub reply_of: Option<i64>,
    /// Existing draft to delete after sending/saving
    #[serde(default)]
    pub draft_id: Option<i64>,
}
