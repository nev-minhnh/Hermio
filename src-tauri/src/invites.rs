//! Calendar files (.ics) in emails: add the event to a calendar and answer invitations
//! (Yes / Maybe / No) with an iCalendar REPLY to the organizer.

use crate::models::{Calendar, Event};
use crate::state::AppState;
use anyhow::{anyhow, bail, Result};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct InviteInfo {
    /// REQUEST for an invitation (Yes / Maybe / No), CANCEL, PUBLISH or "".
    pub method: String,
    pub title: String,
    pub location: String,
    pub start_ts: i64,
    pub end_ts: i64,
    pub all_day: bool,
    /// More than one occurrence (only the next one is added).
    pub recurring: bool,
    pub organizer: String,
    pub organizer_name: String,
    /// What the user already did: "", added, accepted, tentative, declined, dismissed.
    pub state: String,
    /// The event is already in one of the calendars.
    pub in_calendar: bool,
    /// An invitation the user can answer (they aren't the organizer).
    pub can_reply: bool,
    /// The calendar it would go to.
    pub calendar: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InviteResult {
    pub event_id: i64,
    pub added: bool,
    pub already: bool,
    pub replied: bool,
    pub title: String,
    pub start_ts: i64,
    pub all_day: bool,
    pub calendar: String,
}

struct Loaded {
    account: crate::models::Account,
    meta: crate::ics::InviteMeta,
    event: Event,
    recurring: bool,
}

async fn load(state: &AppState, message_id: i64) -> Result<Option<Loaded>> {
    let row = state.db.message(message_id)?;
    let account = state.db.account(row.account_id)?;
    let raw = crate::commands::raw_bytes(state, message_id).await?;
    let body = crate::parse::body(message_id, &raw);
    let Some(ics) = body.ics else { return Ok(None) };
    let meta = crate::ics::invite_meta(&ics);
    let now = chrono::Utc::now().timestamp();
    let events = crate::ics::parse(&ics, now - 2 * 365 * 86_400, now + 3 * 365 * 86_400)?;
    if events.is_empty() {
        return Ok(None);
    }
    let recurring = events.len() > 1 || events.iter().any(|e| e.recurring);
    // The next occurrence (or the last one when all are past).
    let event = events.iter().find(|e| e.end_ts > now).or(events.last()).cloned().unwrap();
    Ok(Some(Loaded { account, meta, event, recurring }))
}

/// The calendar an event from this mailbox goes to: the mailbox's own (Google) calendar —
/// its primary one when known — else the local calendar, else any writable one.
fn target_calendar(state: &AppState, account: &crate::models::Account) -> Result<Calendar> {
    let cals: Vec<Calendar> = state.db.calendars()?.into_iter().filter(|c| c.writable).collect();
    let mine: Vec<&Calendar> = cals.iter().filter(|c| c.account_id == Some(account.id)).collect();
    if let Some(c) = mine.iter().find(|c| c.remote_id.eq_ignore_ascii_case(&account.email) || c.remote_id == "primary") {
        return Ok((*c).clone());
    }
    if let Some(c) = mine.first() {
        return Ok((*c).clone());
    }
    if let Some(c) = cals.iter().find(|c| c.account_id.is_none() && c.kind == "local") {
        return Ok(c.clone());
    }
    cals.into_iter().next().ok_or_else(|| anyhow!("There is no calendar to add the event to"))
}

pub async fn info(state: &AppState, message_id: i64) -> Result<Option<InviteInfo>> {
    let Some(l) = load(state, message_id).await? else { return Ok(None) };
    let st = state.db.invite_state(message_id)?.map(|(s, _)| s).unwrap_or_default();
    let in_calendar = state.db.find_event(&l.event.title, l.event.start_ts)?.is_some();
    let can_reply = l.meta.method == "REQUEST" && !l.meta.organizer.is_empty() && !l.meta.organizer.eq_ignore_ascii_case(&l.account.email);
    let calendar = target_calendar(state, &l.account).map(|c| c.name).unwrap_or_default();
    Ok(Some(InviteInfo {
        method: l.meta.method.clone(),
        title: l.event.title.clone(),
        location: l.event.location.clone(),
        start_ts: l.event.start_ts,
        end_ts: l.event.end_ts,
        all_day: l.event.all_day,
        recurring: l.recurring,
        organizer: l.meta.organizer.clone(),
        organizer_name: l.meta.organizer_name.clone(),
        state: st,
        in_calendar,
        can_reply,
        calendar,
    }))
}

pub async fn respond(state: &AppState, message_id: i64, action: &str) -> Result<InviteResult> {
    let l = load(state, message_id).await?.ok_or_else(|| anyhow!("This email has no calendar event"))?;
    let mut res = InviteResult {
        event_id: 0,
        added: false,
        already: false,
        replied: false,
        title: l.event.title.clone(),
        start_ts: l.event.start_ts,
        all_day: l.event.all_day,
        calendar: String::new(),
    };
    match action {
        "dismiss" => {
            state.db.set_invite_state(message_id, "dismissed", 0)?;
            return Ok(res);
        }
        "add" | "accepted" | "tentative" | "declined" => {}
        _ => bail!("Unknown answer: {action}"),
    }
    // Answer the organizer first: if that fails nothing has changed yet.
    let can_reply = l.meta.method == "REQUEST" && !l.meta.organizer.is_empty() && !l.meta.organizer.eq_ignore_ascii_case(&l.account.email);
    if action != "add" && can_reply {
        send_reply(state, &l, action).await?;
        res.replied = true;
    }
    if action != "declined" {
        if let Some(id) = state.db.find_event(&l.event.title, l.event.start_ts)? {
            res.already = true;
            res.event_id = id;
        } else {
            let cal = target_calendar(state, &l.account)?;
            let mut e = l.event.clone();
            e.id = 0;
            e.calendar_id = cal.id;
            e.remote_id = String::new();
            e.recurring = false;
            if !l.meta.organizer.is_empty() {
                let who = if l.meta.organizer_name.is_empty() { l.meta.organizer.clone() } else { format!("{} <{}>", l.meta.organizer_name, l.meta.organizer) };
                e.description = format!("Organizer: {who}\n\n{}", e.description).trim().to_string();
            }
            let saved = crate::calendar::save_event(state, e).await?;
            res.event_id = saved.id;
            res.added = true;
            res.calendar = cal.name;
        }
    }
    state.db.set_invite_state(message_id, if action == "add" { "added" } else { action }, res.event_id)?;
    state.emit("calendar-changed", serde_json::json!({}));
    Ok(res)
}

/// iTIP REPLY (RFC 5546) to the organizer with the user's PARTSTAT.
async fn send_reply(state: &AppState, l: &Loaded, action: &str) -> Result<()> {
    use lettre::message::header::ContentType;
    use lettre::message::{MultiPart, SinglePart};
    let partstat = match action {
        "accepted" => "ACCEPTED",
        "tentative" => "TENTATIVE",
        _ => "DECLINED",
    };
    let a = &l.account;
    let me = if a.display_name.is_empty() { a.name.clone() } else { a.display_name.clone() };
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
    let mut ics = String::from("BEGIN:VCALENDAR\r\nPRODID:-//Hermio//Hermio Mail//EN\r\nVERSION:2.0\r\nMETHOD:REPLY\r\nBEGIN:VEVENT\r\n");
    ics += &format!("UID:{}\r\n", l.meta.uid);
    if !l.meta.sequence.is_empty() {
        ics += &format!("SEQUENCE:{}\r\n", l.meta.sequence);
    }
    ics += &format!("DTSTAMP:{stamp}\r\n");
    for line in &l.meta.raw_lines {
        ics += line;
        ics += "\r\n";
    }
    ics += &format!("ORGANIZER:mailto:{}\r\n", l.meta.organizer);
    let cn = me.replace(['"', ';', ':', ','], " ");
    ics += &format!("ATTENDEE;PARTSTAT={partstat};CN=\"{}\":mailto:{}\r\n", cn.trim(), a.email);
    ics += "END:VEVENT\r\nEND:VCALENDAR\r\n";

    let (verb, verb_vi) = match partstat {
        "ACCEPTED" => ("Accepted", "đã chấp nhận"),
        "TENTATIVE" => ("Tentative", "có thể tham dự"),
        _ => ("Declined", "đã từ chối"),
    };
    let who = if me.trim().is_empty() { a.email.clone() } else { me.clone() };
    let text = format!("{who} {} this invitation: {}\n({who} {verb_vi})", verb.to_lowercase(), l.event.title);
    let from = format!("{} <{}>", who.replace(['<', '>', '"'], ""), a.email).parse().or_else(|_| a.email.parse())?;
    let to = l.meta.organizer.parse()?;
    let msg = lettre::Message::builder()
        .from(from)
        .to(to)
        .subject(format!("{verb}: {}", l.event.title))
        .date_now()
        .multipart(
            MultiPart::alternative().singlepart(SinglePart::plain(text)).singlepart(
                SinglePart::builder()
                    .header(ContentType::parse("text/calendar; charset=utf-8; method=REPLY").map_err(|e| anyhow!("{e}"))?)
                    .body(ics),
            ),
        )?;
    crate::smtp::send(state, a, msg).await
}
