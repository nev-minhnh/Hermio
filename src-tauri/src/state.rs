use crate::db::Db;
use crate::models::Account;
use crate::oauth::{self, GoogleClient, TokenCache};
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex as AsyncMutex;

pub struct AppState {
    pub db: Arc<Db>,
    pub tokens: TokenCache,
    pub app: AppHandle,
    pub sync_locks: std::sync::Mutex<HashMap<i64, Arc<AsyncMutex<()>>>>,
}

pub enum Creds {
    Password { user: String, pass: String },
    OAuth { user: String, token: String },
}

impl AppState {
    pub fn lock_for(&self, account_id: i64) -> Arc<AsyncMutex<()>> {
        self.sync_locks.lock().unwrap().entry(account_id).or_insert_with(|| Arc::new(AsyncMutex::new(()))).clone()
    }

    pub fn google_client(&self) -> Result<GoogleClient> {
        let id = self.db.get_setting("google_client_id")?.unwrap_or_default();
        let id = id.trim_matches('"').to_string();
        let secret = oauth::get_secret(oauth::CLIENT_SECRET_KEY)?.unwrap_or_default();
        if id.is_empty() {
            // A client baked in at build time (HERMIO_GOOGLE_CLIENT_ID / _SECRET) makes sign-in one click.
            if let Some((bid, bsec)) = builtin_google() {
                return Ok(GoogleClient { client_id: bid.into(), client_secret: bsec.into() });
            }
        }
        Ok(GoogleClient { client_id: id, client_secret: secret })
    }

    pub async fn creds(&self, a: &Account) -> Result<Creds> {
        if a.auth == "oauth_google" {
            let token = self.tokens.access_token(a.id, &self.google_client()?).await?;
            Ok(Creds::OAuth { user: a.username.clone(), token })
        } else {
            let pass = oauth::get_secret(&oauth::password_key(a.id))?
                .ok_or_else(|| anyhow!("No password saved for account {}", a.email))?;
            Ok(Creds::Password { user: a.username.clone(), pass })
        }
    }

    /// Credentials for SMTP: a separate login when configured (advanced), else the incoming one.
    pub async fn smtp_creds(&self, a: &Account) -> Result<Creds> {
        if a.auth == "password" && !a.smtp_username.trim().is_empty() {
            let pass = oauth::get_secret(&format!("account:{}:smtp_password", a.id))?
                .or(oauth::get_secret(&oauth::password_key(a.id))?)
                .ok_or_else(|| anyhow!("No SMTP password saved for {}", a.email))?;
            return Ok(Creds::Password { user: a.smtp_username.trim().to_string(), pass });
        }
        self.creds(a).await
    }

    pub fn emit<S: serde::Serialize + Clone>(&self, event: &str, payload: S) {
        let _ = self.app.emit(event, payload);
    }

    /// Tell the UI that mail data changed for an account.
    pub fn changed(&self, account_id: i64) {
        self.emit("mail-changed", serde_json::json!({ "account_id": account_id }));
    }
}

/// Google OAuth client embedded at build time (HERMIO_GOOGLE_CLIENT_ID / HERMIO_GOOGLE_CLIENT_SECRET).
pub fn builtin_google() -> Option<(&'static str, &'static str)> {
    match (option_env!("HERMIO_GOOGLE_CLIENT_ID"), option_env!("HERMIO_GOOGLE_CLIENT_SECRET")) {
        (Some(i), Some(s)) if !i.trim().is_empty() && !s.trim().is_empty() => Some((i.trim(), s.trim())),
        _ => None,
    }
}

/// Gemini API key embedded at build time (HERMIO_GEMINI_KEY).
pub fn builtin_gemini_key() -> Option<&'static str> {
    option_env!("HERMIO_GEMINI_KEY").map(|k| k.trim()).filter(|k| !k.is_empty())
}
