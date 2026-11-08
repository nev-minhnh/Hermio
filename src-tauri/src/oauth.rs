//! Google OAuth2 (installed-app flow with PKCE and a loopback redirect) and secret storage.

use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const SERVICE: &str = "Hermio Mail";
/// Where passwords and tokens were stored before the rename from Nelo.
const LEGACY_SERVICE: &str = "Nelo Mail";
pub const GOOGLE_SCOPES: &str = "https://mail.google.com/ https://www.googleapis.com/auth/calendar openid email profile";

// ---------- secrets (Windows Credential Manager) ----------

pub fn set_secret(key: &str, value: &str) -> Result<()> {
    let e = keyring::Entry::new(SERVICE, key)?;
    if value.is_empty() {
        let _ = e.delete_credential();
        return Ok(());
    }
    e.set_password(value)?;
    Ok(())
}

pub fn get_secret(key: &str) -> Result<Option<String>> {
    let e = keyring::Entry::new(SERVICE, key)?;
    match e.get_password() {
        Ok(v) => Ok(Some(v)),
        Err(keyring::Error::NoEntry) => {
            // Saved by Hermio: move it over to Hermio's name on first use.
            let old = keyring::Entry::new(LEGACY_SERVICE, key)?;
            match old.get_password() {
                Ok(v) => {
                    if e.set_password(&v).is_ok() {
                        let _ = old.delete_credential();
                    }
                    Ok(Some(v))
                }
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(err) => Err(err.into()),
            }
        }
        Err(err) => Err(err.into()),
    }
}

pub fn delete_secret(key: &str) {
    for s in [SERVICE, LEGACY_SERVICE] {
        if let Ok(e) = keyring::Entry::new(s, key) {
            let _ = e.delete_credential();
        }
    }
}

pub fn password_key(account_id: i64) -> String {
    format!("account:{account_id}:password")
}
pub fn refresh_key(account_id: i64) -> String {
    format!("account:{account_id}:refresh_token")
}
pub const CLIENT_SECRET_KEY: &str = "google:client_secret";

// ---------- token cache ----------

#[derive(Default)]
pub struct TokenCache {
    tokens: Mutex<HashMap<i64, (String, Instant)>>,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: Option<u64>,
    refresh_token: Option<String>,
}

#[derive(Clone)]
pub struct GoogleClient {
    pub client_id: String,
    pub client_secret: String,
}

impl TokenCache {
    pub fn put(&self, account_id: i64, token: &str, expires_in: u64) {
        let exp = Instant::now() + Duration::from_secs(expires_in.saturating_sub(120));
        self.tokens.lock().unwrap().insert(account_id, (token.to_string(), exp));
    }

    pub fn forget(&self, account_id: i64) {
        self.tokens.lock().unwrap().remove(&account_id);
    }

    /// A valid access token for the account, refreshing it when needed.
    pub async fn access_token(&self, account_id: i64, client: &GoogleClient) -> Result<String> {
        if let Some((t, exp)) = self.tokens.lock().unwrap().get(&account_id) {
            if Instant::now() < *exp {
                return Ok(t.clone());
            }
        }
        let refresh = get_secret(&refresh_key(account_id))?
            .ok_or_else(|| anyhow!("This Google account is not signed in. Sign in again in Settings → Accounts."))?;
        let http = reqwest::Client::new();
        let resp = http
            .post("https://oauth2.googleapis.com/token")
            .form(&[
                ("client_id", client.client_id.as_str()),
                ("client_secret", client.client_secret.as_str()),
                ("refresh_token", refresh.as_str()),
                ("grant_type", "refresh_token"),
            ])
            .send()
            .await?;
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            bail!("Could not refresh the Google token ({body}). Please sign in to the account again.");
        }
        let tr: TokenResponse = resp.json().await?;
        if let Some(rt) = &tr.refresh_token {
            set_secret(&refresh_key(account_id), rt)?;
        }
        self.put(account_id, &tr.access_token, tr.expires_in.unwrap_or(3600));
        Ok(tr.access_token)
    }
}

pub fn xoauth2_string(user: &str, token: &str) -> String {
    format!("user={user}\x01auth=Bearer {token}\x01\x01")
}

// ---------- interactive sign-in ----------

pub struct SignInResult {
    pub email: String,
    pub name: String,
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u64,
}

fn random_b64(n: usize) -> String {
    let mut buf = vec![0u8; n];
    getrandom::getrandom(&mut buf).expect("random");
    URL_SAFE_NO_PAD.encode(buf)
}

pub async fn google_sign_in(
    client: &GoogleClient,
    login_hint: &str,
    open_url: impl Fn(&str) -> Result<()>,
    cancel: Option<tokio::sync::oneshot::Receiver<()>>,
) -> Result<SignInResult> {
    if client.client_id.trim().is_empty() {
        bail!("Google OAuth Client ID is not configured (Settings → Google sign-in).");
    }
    let verifier = random_b64(48);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let state = random_b64(16);
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let redirect = format!("http://127.0.0.1:{port}");
    let mut url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope={}\
         &code_challenge={}&code_challenge_method=S256&state={}&access_type=offline&prompt=consent",
        urlencoding::encode(&client.client_id),
        urlencoding::encode(&redirect),
        urlencoding::encode(GOOGLE_SCOPES),
        challenge,
        state
    );
    if !login_hint.is_empty() {
        url.push_str(&format!("&login_hint={}", urlencoding::encode(login_hint)));
    }
    open_url(&url)?;

    // Wait for the browser to come back (5 minutes), or for the popup to be closed.
    let wait = tokio::time::timeout(Duration::from_secs(300), async {
        loop {
            let (mut sock, _) = listener.accept().await?;
            let mut buf = vec![0u8; 8192];
            let n = sock.read(&mut buf).await?;
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            let line = req.lines().next().unwrap_or("");
            let path = line.split_whitespace().nth(1).unwrap_or("/");
            let query = path.split_once('?').map(|x| x.1).unwrap_or("");
            let params: HashMap<String, String> = query
                .split('&')
                .filter_map(|kv| kv.split_once('='))
                .map(|(k, v)| (k.to_string(), urlencoding::decode(v).map(|s| s.into_owned()).unwrap_or_default()))
                .collect();
            if params.is_empty() {
                let _ = sock.write_all(b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n").await;
                continue;
            }
            let ok = params.get("state") == Some(&state) && params.contains_key("code");
            let page = if ok {
                "<div style='font-family:Segoe UI;text-align:center;margin-top:30vh'><h2>Signed in ✔</h2><p>Hermio is finishing up; this window will close itself.</p></div>"
            } else {
                "<h2 style='font-family:Segoe UI'>Sign-in failed</h2><p style='font-family:Segoe UI'>Go back to Hermio and try again.</p>"
            };
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                page.len(), page
            );
            let _ = sock.write_all(resp.as_bytes()).await;
            if let Some(err) = params.get("error") {
                return Err(anyhow!("Google refused the sign-in: {err}"));
            }
            if !ok {
                return Err(anyhow!("Invalid sign-in response"));
            }
            return Ok::<String, anyhow::Error>(params["code"].clone());
        }
    });
    let code = match cancel {
        Some(rx) => tokio::select! {
            r = wait => r.context("Timed out waiting for Google sign-in")??,
            _ = rx => bail!("The Google sign-in window was closed"),
        },
        None => wait.await.context("Timed out waiting for Google sign-in")??,
    };

    let http = reqwest::Client::new();
    let resp = http
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("client_id", client.client_id.as_str()),
            ("client_secret", client.client_secret.as_str()),
            ("code", code.as_str()),
            ("code_verifier", verifier.as_str()),
            ("grant_type", "authorization_code"),
            ("redirect_uri", redirect.as_str()),
        ])
        .send()
        .await?;
    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        bail!("Exchanging the sign-in code failed: {body}");
    }
    let tr: TokenResponse = resp.json().await?;
    let refresh = tr.refresh_token.clone().ok_or_else(|| anyhow!("Google did not return a refresh token"))?;

    #[derive(Deserialize)]
    struct UserInfo {
        email: Option<String>,
        name: Option<String>,
    }
    let ui: UserInfo = http
        .get("https://openidconnect.googleapis.com/v1/userinfo")
        .bearer_auth(&tr.access_token)
        .send()
        .await?
        .json()
        .await?;
    Ok(SignInResult {
        email: ui.email.unwrap_or_default(),
        name: ui.name.unwrap_or_default(),
        access_token: tr.access_token,
        refresh_token: refresh,
        expires_in: tr.expires_in.unwrap_or(3600),
    })
}
