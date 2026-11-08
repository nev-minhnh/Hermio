//! Google Calendar sync (two-way for single events) and local calendar.

use crate::models::*;
use crate::state::AppState;
use anyhow::{anyhow, bail, Result};
use chrono::{DateTime, Local, NaiveDate, TimeZone};
use serde_json::{json, Value};

const API: &str = "https://www.googleapis.com/calendar/v3";

async fn token(state: &AppState, a: &Account) -> Result<String> {
    state.tokens.access_token(a.id, &state.google_client()?).await
}

fn parse_time(v: &Value) -> Option<(i64, bool)> {
    if let Some(dt) = v.get("dateTime").and_then(|x| x.as_str()) {
        return DateTime::parse_from_rfc3339(dt).ok().map(|d| (d.timestamp(), false));
    }
    if let Some(d) = v.get("date").and_then(|x| x.as_str()) {
        let nd = NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()?;
        let local = Local.from_local_datetime(&nd.and_hms_opt(0, 0, 0)?).earliest()?;
        return Some((local.timestamp(), true));
    }
    None
}

fn to_google(e: &Event) -> Value {
    let (start, end) = if e.all_day {
        let s = Local.timestamp_opt(e.start_ts, 0).single().unwrap_or_else(Local::now);
        let mut en = Local.timestamp_opt(e.end_ts, 0).single().unwrap_or_else(Local::now);
        if en.date_naive() <= s.date_naive() {
            en = s + chrono::Duration::days(1);
        }
        (json!({ "date": s.format("%Y-%m-%d").to_string() }), json!({ "date": en.format("%Y-%m-%d").to_string() }))
    } else {
        let s = Local.timestamp_opt(e.start_ts, 0).single().unwrap_or_else(Local::now);
        let en = Local.timestamp_opt(e.end_ts, 0).single().unwrap_or_else(Local::now);
        (json!({ "dateTime": s.to_rfc3339() }), json!({ "dateTime": en.to_rfc3339() }))
    };
    json!({
        "summary": e.title,
        "location": e.location,
        "description": e.description,
        "start": start,
        "end": end,
    })
}

fn from_google(item: &Value) -> Option<Event> {
    if item.get("status").and_then(|x| x.as_str()) == Some("cancelled") {
        return None;
    }
    let (start_ts, all_day) = parse_time(item.get("start")?)?;
    let (end_ts, _) = parse_time(item.get("end")?).unwrap_or((start_ts + 3600, all_day));
    let meet = item
        .get("hangoutLink")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            item.pointer("/conferenceData/entryPoints/0/uri").and_then(|x| x.as_str()).map(|s| s.to_string())
        })
        .unwrap_or_default();
    Some(Event {
        id: 0,
        calendar_id: 0,
        remote_id: item.get("id")?.as_str()?.to_string(),
        title: item.get("summary").and_then(|x| x.as_str()).unwrap_or("(No title)").to_string(),
        location: item.get("location").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        description: item.get("description").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        start_ts,
        end_ts,
        all_day,
        recurring: item.get("recurringEventId").is_some(),
        meet_link: meet,
        color: String::new(),
    })
}

pub async fn sync_account(state: &AppState, a: &Account) -> Result<()> {
    if a.auth != "oauth_google" || !a.calendar_sync {
        return Ok(());
    }
    let tok = token(state, a).await?;
    let http = reqwest::Client::new();
    let r = http.get(format!("{API}/users/me/calendarList")).bearer_auth(&tok).send().await?;
    if r.status() == reqwest::StatusCode::FORBIDDEN || r.status() == reqwest::StatusCode::UNAUTHORIZED {
        bail!("No access to Google Calendar. Enable the Google Calendar API in Google Cloud and sign in again.");
    }
    let list: Value = r.error_for_status()?.json().await?;
    let now = chrono::Utc::now().timestamp();
    let from = now - 120 * 86400;
    let to = now + 400 * 86400;
    let tmin = chrono::DateTime::from_timestamp(from, 0).unwrap().to_rfc3339();
    let tmax = chrono::DateTime::from_timestamp(to, 0).unwrap().to_rfc3339();
    for cal in list.get("items").and_then(|x| x.as_array()).cloned().unwrap_or_default() {
        let Some(cid) = cal.get("id").and_then(|x| x.as_str()) else { continue };
        let name = cal.get("summaryOverride").or_else(|| cal.get("summary")).and_then(|x| x.as_str()).unwrap_or(cid);
        let color = cal.get("backgroundColor").and_then(|x| x.as_str()).unwrap_or("#0f6cbd");
        let role = cal.get("accessRole").and_then(|x| x.as_str()).unwrap_or("reader");
        let writable = role == "owner" || role == "writer";
        let local_id = state.db.upsert_calendar(a.id, cid, name, color, writable)?;
        let mut evs = vec![];
        let mut page: Option<String> = None;
        loop {
            let mut req = http
                .get(format!("{API}/calendars/{}/events", urlencoding::encode(cid)))
                .bearer_auth(&tok)
                .query(&[("singleEvents", "true"), ("maxResults", "2500"), ("timeMin", &tmin), ("timeMax", &tmax), ("orderBy", "startTime")]);
            if let Some(p) = &page {
                req = req.query(&[("pageToken", p)]);
            }
            let r = req.send().await?;
            if !r.status().is_success() {
                break;
            }
            let v: Value = r.json().await?;
            for it in v.get("items").and_then(|x| x.as_array()).cloned().unwrap_or_default() {
                if let Some(mut e) = from_google(&it) {
                    e.calendar_id = local_id;
                    evs.push(e);
                }
            }
            page = v.get("nextPageToken").and_then(|x| x.as_str()).map(|s| s.to_string());
            if page.is_none() {
                break;
            }
        }
        state.db.replace_remote_events(local_id, from, to, &evs)?;
    }
    state.emit("calendar-changed", json!({ "account_id": a.id }));
    Ok(())
}

/// Create or update an event, pushing it to Google when the calendar belongs to a Google account.
pub async fn save_event(state: &AppState, mut e: Event) -> Result<Event> {
    let cal = state.db.calendar(e.calendar_id)?;
    if !cal.writable {
        bail!("This calendar is read-only");
    }
    if let Some(aid) = cal.account_id {
        let a = state.db.account(aid)?;
        let tok = token(state, &a).await?;
        let http = reqwest::Client::new();
        let body = to_google(&e);
        let base = format!("{API}/calendars/{}/events", urlencoding::encode(&cal.remote_id));
        let resp = if e.remote_id.is_empty() {
            http.post(&base).bearer_auth(&tok).json(&body).send().await?
        } else {
            http.patch(format!("{base}/{}", urlencoding::encode(&e.remote_id))).bearer_auth(&tok).json(&body).send().await?
        };
        if !resp.status().is_success() {
            let t = resp.text().await.unwrap_or_default();
            bail!("Google Calendar refused: {t}");
        }
        let v: Value = resp.json().await?;
        e.remote_id = v.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string();
    }
    e.id = state.db.save_event(&e)?;
    Ok(e)
}

pub async fn delete_event(state: &AppState, id: i64) -> Result<()> {
    let e = state.db.event(id)?;
    let cal = state.db.calendar(e.calendar_id)?;
    if let Some(aid) = cal.account_id {
        if !e.remote_id.is_empty() {
            let a = state.db.account(aid)?;
            let tok = token(state, &a).await?;
            let r = reqwest::Client::new()
                .delete(format!(
                    "{API}/calendars/{}/events/{}",
                    urlencoding::encode(&cal.remote_id),
                    urlencoding::encode(&e.remote_id)
                ))
                .bearer_auth(&tok)
                .send()
                .await?;
            if !r.status().is_success() && r.status() != reqwest::StatusCode::GONE && r.status() != reqwest::StatusCode::NOT_FOUND {
                return Err(anyhow!("Could not delete the event on Google ({})", r.status()));
            }
        }
    }
    state.db.delete_event(id)
}

/// Refresh one ICS subscription (Outlook "publish calendar" link, webcal://, any .ics URL).
pub async fn sync_ics(state: &AppState, cal: &Calendar) -> Result<usize> {
    let res: Result<usize> = async {
        let text = crate::ics::fetch(&cal.url).await?;
        let (from, to) = crate::ics::window();
        let mut evs = crate::ics::parse(&text, from, to)?;
        for e in evs.iter_mut() {
            e.calendar_id = cal.id;
        }
        state.db.replace_all_events(cal.id, &evs)?;
        Ok(evs.len())
    }
    .await;
    let err = res.as_ref().err().map(|e| format!("{e:#}")).unwrap_or_default();
    state.db.set_calendar_status(cal.id, &err)?;
    state.emit("calendar-changed", json!({ "calendar_id": cal.id }));
    res
}

pub async fn sync_all_ics(state: &AppState) {
    if let Ok(cals) = state.db.calendars() {
        for c in cals.iter().filter(|c| c.kind == "ics" && !c.url.is_empty()) {
            if let Err(e) = sync_ics(state, c).await {
                eprintln!("ics {}: {e:#}", c.name);
            }
        }
    }
}
