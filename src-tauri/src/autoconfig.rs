//! Thunderbird-style account discovery: known providers, Mozilla ISPDB, domain autoconfig,
//! MX lookup (detects Google Workspace / Microsoft 365 behind custom domains), then guessing.

use anyhow::Result;
use serde::Serialize;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Default)]
pub struct Server {
    pub host: String,
    pub port: u16,
    /// ssl | starttls | none
    pub security: String,
    /// "%EMAILADDRESS%" style template already resolved
    pub username: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Discovery {
    /// google | microsoft | other
    pub provider: String,
    pub imap: Option<Server>,
    pub pop3: Option<Server>,
    pub smtp: Option<Server>,
    /// where the settings came from, for display
    pub source: String,
    /// human hint (app password needed etc.)
    pub note: String,
}

fn srv(host: &str, port: u16, sec: &str, user: &str) -> Server {
    Server { host: host.into(), port, security: sec.into(), username: user.into() }
}

fn known(domain: &str, email: &str) -> Option<Discovery> {
    let d = |p: &str, i: Option<Server>, po: Option<Server>, s: Server, note: &str| Discovery {
        provider: p.into(), imap: i, pop3: po, smtp: Some(s), source: "Known provider".into(), note: note.into(),
    };
    Some(match domain {
        "gmail.com" | "googlemail.com" => d(
            "google",
            Some(srv("imap.gmail.com", 993, "ssl", email)),
            Some(srv("pop.gmail.com", 995, "ssl", email)),
            srv("smtp.gmail.com", 465, "ssl", email),
            "Gmail: use Sign in with Google, or an App Password if OAuth is not set up.",
        ),
        "yahoo.com" | "ymail.com" | "rocketmail.com" | "yahoo.com.vn" => d(
            "other",
            Some(srv("imap.mail.yahoo.com", 993, "ssl", email)),
            Some(srv("pop.mail.yahoo.com", 995, "ssl", email)),
            srv("smtp.mail.yahoo.com", 465, "ssl", email),
            "Yahoo requires an App Password (Account security → Generate app password).",
        ),
        "icloud.com" | "me.com" | "mac.com" => d(
            "other",
            Some(srv("imap.mail.me.com", 993, "ssl", email)),
            None,
            srv("smtp.mail.me.com", 587, "starttls", email),
            "iCloud requires an app-specific password (appleid.apple.com).",
        ),
        "outlook.com" | "hotmail.com" | "live.com" | "msn.com" => d(
            "microsoft",
            Some(srv("outlook.office365.com", 993, "ssl", email)),
            Some(srv("outlook.office365.com", 995, "ssl", email)),
            srv("smtp-mail.outlook.com", 587, "starttls", email),
            "Microsoft has turned off password sign-in for most accounts; signing in may fail.",
        ),
        "aol.com" => d(
            "other",
            Some(srv("imap.aol.com", 993, "ssl", email)),
            Some(srv("pop.aol.com", 995, "ssl", email)),
            srv("smtp.aol.com", 465, "ssl", email),
            "AOL requires an App Password.",
        ),
        "zoho.com" | "zohomail.com" => d(
            "other",
            Some(srv("imap.zoho.com", 993, "ssl", email)),
            Some(srv("pop.zoho.com", 995, "ssl", email)),
            srv("smtp.zoho.com", 465, "ssl", email),
            "",
        ),
        "gmx.com" | "gmx.net" | "gmx.de" => d(
            "other",
            Some(srv("imap.gmx.com", 993, "ssl", email)),
            Some(srv("pop.gmx.com", 995, "ssl", email)),
            srv("mail.gmx.com", 587, "starttls", email),
            "",
        ),
        "fastmail.com" | "fastmail.fm" => d(
            "other",
            Some(srv("imap.fastmail.com", 993, "ssl", email)),
            Some(srv("pop.fastmail.com", 995, "ssl", email)),
            srv("smtp.fastmail.com", 465, "ssl", email),
            "Fastmail requires an App Password.",
        ),
        "yandex.com" | "yandex.ru" => d(
            "other",
            Some(srv("imap.yandex.com", 993, "ssl", email)),
            Some(srv("pop.yandex.com", 995, "ssl", email)),
            srv("smtp.yandex.com", 465, "ssl", email),
            "",
        ),
        _ => return None,
    })
}

fn google(email: &str, source: &str) -> Discovery {
    let mut d = known("gmail.com", email).unwrap();
    d.source = source.into();
    d.note = "This domain uses Google Workspace. Sign in with Google, or use an App Password if your admin allows it.".into();
    d
}

fn microsoft365(email: &str) -> Discovery {
    let mut d = known("outlook.com", email).unwrap();
    d.smtp = Some(srv("smtp.office365.com", 587, "starttls", email));
    d.source = "MX record (Microsoft 365)".into();
    d
}

fn resolve_user(tpl: &str, email: &str) -> String {
    let local = email.split('@').next().unwrap_or(email);
    let domain = email.rsplit('@').next().unwrap_or("");
    tpl.replace("%EMAILADDRESS%", email).replace("%EMAILLOCALPART%", local).replace("%EMAILDOMAIN%", domain)
}

/// Parse a Mozilla autoconfig (config-v1.1) document.
fn parse_autoconfig(xml: &str, email: &str, source: &str) -> Option<Discovery> {
    let doc = roxmltree::Document::parse(xml).ok()?;
    let mut d = Discovery { provider: "other".into(), source: source.into(), ..Default::default() };
    let read = |n: roxmltree::Node| -> Option<Server> {
        let get = |tag: &str| n.children().find(|c| c.has_tag_name(tag)).and_then(|c| c.text()).map(|s| s.trim().to_string());
        let host = get("hostname")?;
        let port: u16 = get("port")?.parse().ok()?;
        let security = match get("socketType").unwrap_or_default().as_str() {
            "SSL" => "ssl",
            "STARTTLS" => "starttls",
            _ => "none",
        };
        let user = resolve_user(&get("username").unwrap_or_else(|| "%EMAILADDRESS%".into()), email);
        // Skip OAuth-only servers we can't use with a password.
        let auths: Vec<String> = n.children().filter(|c| c.has_tag_name("authentication")).filter_map(|c| c.text().map(|t| t.to_string())).collect();
        if !auths.is_empty() && auths.iter().all(|a| a == "OAuth2") {
            return None;
        }
        Some(Server { host, port, security: security.into(), username: user })
    };
    for n in doc.descendants().filter(|n| n.has_tag_name("incomingServer")) {
        match n.attribute("type") {
            Some("imap") if d.imap.as_ref().map(|s| s.security != "ssl").unwrap_or(true) => {
                if let Some(s) = read(n) { if d.imap.is_none() || s.security == "ssl" { d.imap = Some(s); } }
            }
            Some("pop3") if d.pop3.is_none() => d.pop3 = read(n),
            _ => {}
        }
    }
    for n in doc.descendants().filter(|n| n.has_tag_name("outgoingServer")) {
        if n.attribute("type") == Some("smtp") {
            if let Some(s) = read(n) {
                if d.smtp.is_none() || s.security == "ssl" { d.smtp = Some(s); }
            }
        }
    }
    if let Some(i) = &d.imap {
        if i.host.ends_with("gmail.com") || i.host.ends_with("googlemail.com") {
            d.provider = "google".into();
        }
    }
    (d.imap.is_some() || d.pop3.is_some()).then_some(d)
}

async fn fetch(http: &reqwest::Client, url: &str) -> Option<String> {
    let r = http.get(url).send().await.ok()?;
    if !r.status().is_success() {
        return None;
    }
    r.text().await.ok()
}

/// MX hosts for a domain via DNS-over-HTTPS.
async fn mx(http: &reqwest::Client, domain: &str) -> Vec<String> {
    #[derive(serde::Deserialize)]
    struct Ans {
        data: String,
    }
    #[derive(serde::Deserialize)]
    struct Resp {
        #[serde(rename = "Answer", default)]
        answer: Vec<Ans>,
    }
    let url = format!("https://dns.google/resolve?name={}&type=MX", urlencoding::encode(domain));
    let Ok(r) = http.get(&url).send().await else { return vec![] };
    let Ok(v) = r.json::<Resp>().await else { return vec![] };
    let mut out: Vec<(u32, String)> = v
        .answer
        .iter()
        .filter_map(|a| {
            let mut it = a.data.split_whitespace();
            let pri = it.next()?.parse().ok()?;
            Some((pri, it.next()?.trim_end_matches('.').to_lowercase()))
        })
        .collect();
    out.sort();
    out.into_iter().map(|x| x.1).collect()
}

async fn port_open(host: &str, port: u16) -> bool {
    matches!(
        tokio::time::timeout(Duration::from_secs(4), tokio::net::TcpStream::connect((host, port))).await,
        Ok(Ok(_))
    )
}

/// The registrable part of a host name ("mx1.mail.example.co.uk" → "example.co.uk", roughly).
fn base_domain(host: &str) -> String {
    let parts: Vec<&str> = host.split('.').collect();
    if parts.len() <= 2 {
        return host.to_string();
    }
    let sld = parts[parts.len() - 2];
    let n = if ["co", "com", "net", "org", "edu", "gov", "ac"].contains(&sld) && parts[parts.len() - 1].len() == 2 { 3 } else { 2 };
    parts[parts.len().saturating_sub(n)..].join(".")
}

pub async fn discover(email: &str) -> Result<Discovery> {
    let email = email.trim();
    let domain = email.rsplit('@').next().unwrap_or("").to_lowercase();
    if domain.is_empty() || !email.contains('@') {
        anyhow::bail!("Invalid email address");
    }
    if let Some(d) = known(&domain, email) {
        return Ok(d);
    }
    let http = reqwest::Client::builder().timeout(Duration::from_secs(8)).build()?;

    // MX first: custom domains on Google Workspace / Microsoft 365 are very common (schools, companies).
    let mxs = mx(&http, &domain).await;
    if let Some(first) = mxs.first() {
        if first.ends_with("google.com") || first.ends_with("googlemail.com") {
            return Ok(google(email, "MX record (Google Workspace)"));
        }
        if first.ends_with("outlook.com") || first.ends_with("office365.com") {
            return Ok(microsoft365(email));
        }
    }

    // Domain's own autoconfig, then Mozilla ISPDB.
    let candidates = [
        (format!("https://autoconfig.{domain}/mail/config-v1.1.xml?emailaddress={}", urlencoding::encode(email)), "Domain autoconfig"),
        (format!("https://{domain}/.well-known/autoconfig/mail/config-v1.1.xml"), "Domain autoconfig"),
        (format!("https://autoconfig.thunderbird.net/v1.1/{domain}"), "Thunderbird database"),
    ];
    for (url, src) in candidates.iter() {
        if let Some(xml) = fetch(&http, url).await {
            if let Some(d) = parse_autoconfig(&xml, email, src) {
                return Ok(d);
            }
        }
    }
    // ISPDB entry for the mail host's domain (hosting providers).
    if let Some(first) = mxs.first() {
        let bd = base_domain(first);
        if bd != domain {
            if let Some(xml) = fetch(&http, &format!("https://autoconfig.thunderbird.net/v1.1/{bd}")).await {
                if let Some(d) = parse_autoconfig(&xml, email, "Thunderbird database (via MX)") {
                    return Ok(d);
                }
            }
        }
    }

    // Guess common host names and probe ports.
    let mut d = Discovery { provider: "other".into(), source: "Server probe".into(), ..Default::default() };
    for h in [format!("imap.{domain}"), format!("mail.{domain}"), domain.clone()] {
        if port_open(&h, 993).await {
            d.imap = Some(srv(&h, 993, "ssl", email));
            break;
        }
        if port_open(&h, 143).await {
            d.imap = Some(srv(&h, 143, "starttls", email));
            break;
        }
    }
    for h in [format!("pop.{domain}"), format!("pop3.{domain}"), format!("mail.{domain}")] {
        if port_open(&h, 995).await {
            d.pop3 = Some(srv(&h, 995, "ssl", email));
            break;
        }
    }
    for h in [format!("smtp.{domain}"), format!("mail.{domain}"), domain.clone()] {
        if port_open(&h, 465).await {
            d.smtp = Some(srv(&h, 465, "ssl", email));
            break;
        }
        if port_open(&h, 587).await {
            d.smtp = Some(srv(&h, 587, "starttls", email));
            break;
        }
    }
    if d.imap.is_none() && d.pop3.is_none() {
        anyhow::bail!("Could not find settings for {domain}. Use Advanced setup.");
    }
    d.note = "Servers found by probing; Hermio will sign in to confirm.".into();
    Ok(d)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ispdb_xml() {
        let xml = r#"<?xml version="1.0"?><clientConfig version="1.1"><emailProvider id="example.com">
<incomingServer type="imap"><hostname>imap.example.com</hostname><port>143</port><socketType>STARTTLS</socketType><username>%EMAILADDRESS%</username><authentication>password-cleartext</authentication></incomingServer>
<incomingServer type="imap"><hostname>imap.example.com</hostname><port>993</port><socketType>SSL</socketType><username>%EMAILADDRESS%</username><authentication>password-cleartext</authentication></incomingServer>
<incomingServer type="pop3"><hostname>pop.example.com</hostname><port>995</port><socketType>SSL</socketType><username>%EMAILLOCALPART%</username></incomingServer>
<outgoingServer type="smtp"><hostname>smtp.example.com</hostname><port>587</port><socketType>STARTTLS</socketType><username>%EMAILADDRESS%</username></outgoingServer>
</emailProvider></clientConfig>"#;
        let d = parse_autoconfig(xml, "a@example.com", "t").unwrap();
        let i = d.imap.unwrap();
        assert_eq!((i.port, i.security.as_str()), (993, "ssl"));
        assert_eq!(d.pop3.unwrap().username, "a");
        assert_eq!(d.smtp.unwrap().port, 587);
        assert_eq!(base_domain("mx1.mail.example.co.uk"), "example.co.uk");
    }
}
