//! iCalendar (.ics) reader for Outlook / Google / any published calendar: VEVENTs, RRULE
//! expansion (DAILY/WEEKLY/MONTHLY/YEARLY, INTERVAL, COUNT, UNTIL, BYDAY, BYMONTHDAY),
//! EXDATE, RECURRENCE-ID overrides, and Windows time zone names used by Outlook.

use crate::models::Event;
use anyhow::{anyhow, Result};
use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveDateTime, TimeZone, Utc, Weekday};
use chrono_tz::Tz;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
struct Prop {
    name: String,
    params: HashMap<String, String>,
    value: String,
}

fn unfold(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = vec![];
    for raw in text.split('\n') {
        let l = raw.trim_end_matches('\r');
        if (l.starts_with(' ') || l.starts_with('\t')) && !lines.is_empty() {
            lines.last_mut().unwrap().push_str(&l[1..]);
        } else {
            lines.push(l.to_string());
        }
    }
    lines
}

fn parse_prop(line: &str) -> Option<Prop> {
    // NAME;P1=V1;P2="V2":VALUE  (':' inside quoted params must be skipped)
    let mut in_q = false;
    let mut colon = None;
    for (i, ch) in line.char_indices() {
        match ch {
            '"' => in_q = !in_q,
            ':' if !in_q => {
                colon = Some(i);
                break;
            }
            _ => {}
        }
    }
    let colon = colon?;
    let (head, value) = (&line[..colon], &line[colon + 1..]);
    let mut parts = head.split(';');
    let name = parts.next()?.to_ascii_uppercase();
    let params = parts
        .filter_map(|p| p.split_once('='))
        .map(|(k, v)| (k.to_ascii_uppercase(), v.trim_matches('"').to_string()))
        .collect();
    Some(Prop { name, params, value: value.to_string() })
}

fn unescape(s: &str) -> String {
    s.replace("\\n", "\n").replace("\\N", "\n").replace("\\,", ",").replace("\\;", ";").replace("\\\\", "\\")
}

/// Map Windows zone names (Outlook) to IANA.
fn windows_tz(name: &str) -> Option<&'static str> {
    Some(match name {
        "SE Asia Standard Time" => "Asia/Bangkok",
        "AUS Eastern Standard Time" => "Australia/Sydney",
        "E. Australia Standard Time" => "Australia/Brisbane",
        "Cen. Australia Standard Time" => "Australia/Adelaide",
        "AUS Central Standard Time" => "Australia/Darwin",
        "W. Australia Standard Time" => "Australia/Perth",
        "Tasmania Standard Time" => "Australia/Hobart",
        "Singapore Standard Time" => "Asia/Singapore",
        "China Standard Time" => "Asia/Shanghai",
        "Taipei Standard Time" => "Asia/Taipei",
        "Tokyo Standard Time" => "Asia/Tokyo",
        "Korea Standard Time" => "Asia/Seoul",
        "India Standard Time" => "Asia/Kolkata",
        "Arabian Standard Time" => "Asia/Dubai",
        "UTC" | "Coordinated Universal Time" => "UTC",
        "GMT Standard Time" => "Europe/London",
        "Greenwich Standard Time" => "Atlantic/Reykjavik",
        "W. Europe Standard Time" => "Europe/Berlin",
        "Romance Standard Time" => "Europe/Paris",
        "Central Europe Standard Time" => "Europe/Budapest",
        "Central European Standard Time" => "Europe/Warsaw",
        "E. Europe Standard Time" => "Europe/Chisinau",
        "FLE Standard Time" => "Europe/Kiev",
        "Russian Standard Time" => "Europe/Moscow",
        "Eastern Standard Time" => "America/New_York",
        "Central Standard Time" => "America/Chicago",
        "Mountain Standard Time" => "America/Denver",
        "US Mountain Standard Time" => "America/Phoenix",
        "Pacific Standard Time" => "America/Los_Angeles",
        "Alaskan Standard Time" => "America/Anchorage",
        "Hawaiian Standard Time" => "Pacific/Honolulu",
        "Atlantic Standard Time" => "America/Halifax",
        "E. South America Standard Time" => "America/Sao_Paulo",
        "New Zealand Standard Time" => "Pacific/Auckland",
        "North Asia East Standard Time" => "Asia/Irkutsk",
        "Myanmar Standard Time" => "Asia/Yangon",
        _ => return None,
    })
}

#[derive(Clone, Copy)]
enum Zone {
    Utc,
    Floating,
    Named(Tz),
    Fixed(i32), // seconds east of UTC
}

fn zone_for(tzid: Option<&String>, fixed: &HashMap<String, i32>) -> Zone {
    let Some(id) = tzid else { return Zone::Floating };
    let id = id.trim().trim_start_matches('/');
    if let Ok(tz) = id.parse::<Tz>() {
        return Zone::Named(tz);
    }
    if let Some(iana) = windows_tz(id) {
        if let Ok(tz) = iana.parse::<Tz>() {
            return Zone::Named(tz);
        }
    }
    if let Some(off) = fixed.get(id) {
        return Zone::Fixed(*off);
    }
    Zone::Floating
}

fn to_ts(ndt: NaiveDateTime, z: Zone) -> i64 {
    match z {
        Zone::Utc => Utc.from_utc_datetime(&ndt).timestamp(),
        Zone::Named(tz) => tz.from_local_datetime(&ndt).earliest().map(|d| d.timestamp()).unwrap_or_else(|| Utc.from_utc_datetime(&ndt).timestamp()),
        Zone::Fixed(off) => Utc.from_utc_datetime(&ndt).timestamp() - off as i64,
        Zone::Floating => chrono::Local.from_local_datetime(&ndt).earliest().map(|d| d.timestamp()).unwrap_or(0),
    }
}

/// Parsed DTSTART-like value: local wall time + zone, or a date.
#[derive(Clone, Copy)]
struct When {
    ndt: NaiveDateTime,
    zone: Zone,
    date_only: bool,
}

impl When {
    fn ts(&self) -> i64 {
        if self.date_only {
            // all-day: local midnight of the viewer
            chrono::Local.from_local_datetime(&self.ndt).earliest().map(|d| d.timestamp()).unwrap_or(0)
        } else {
            to_ts(self.ndt, self.zone)
        }
    }
}

fn parse_when(p: &Prop, fixed: &HashMap<String, i32>) -> Option<When> {
    parse_when_value(&p.value, p.params.get("TZID"), p.params.get("VALUE").map(|s| s.as_str()) == Some("DATE"), fixed)
}

fn parse_when_value(v: &str, tzid: Option<&String>, is_date: bool, fixed: &HashMap<String, i32>) -> Option<When> {
    let v = v.trim();
    if is_date || v.len() == 8 {
        let d = NaiveDate::parse_from_str(&v[..8.min(v.len())], "%Y%m%d").ok()?;
        return Some(When { ndt: d.and_hms_opt(0, 0, 0)?, zone: Zone::Floating, date_only: true });
    }
    let utc = v.ends_with('Z');
    let ndt = NaiveDateTime::parse_from_str(v.trim_end_matches('Z'), "%Y%m%dT%H%M%S").ok()?;
    let zone = if utc { Zone::Utc } else { zone_for(tzid, fixed) };
    Some(When { ndt, zone, date_only: false })
}

fn parse_duration(s: &str) -> Option<i64> {
    // P1DT2H30M, PT45M, P1W
    let s = s.trim();
    let neg = s.starts_with('-');
    let s = s.trim_start_matches(['-', '+']).strip_prefix('P')?;
    let mut total = 0i64;
    let mut num = String::new();
    for ch in s.chars() {
        match ch {
            '0'..='9' => num.push(ch),
            'T' => {}
            'W' => { total += num.parse::<i64>().ok()? * 7 * 86400; num.clear(); }
            'D' => { total += num.parse::<i64>().ok()? * 86400; num.clear(); }
            'H' => { total += num.parse::<i64>().ok()? * 3600; num.clear(); }
            'M' => { total += num.parse::<i64>().ok()? * 60; num.clear(); }
            'S' => { total += num.parse::<i64>().ok()?; num.clear(); }
            _ => return None,
        }
    }
    Some(if neg { -total } else { total })
}

fn weekday(s: &str) -> Option<(i32, Weekday)> {
    // "MO", "1MO", "-1FR"
    let (n, d) = s.split_at(s.len().saturating_sub(2));
    let wd = match d {
        "MO" => Weekday::Mon, "TU" => Weekday::Tue, "WE" => Weekday::Wed, "TH" => Weekday::Thu,
        "FR" => Weekday::Fri, "SA" => Weekday::Sat, "SU" => Weekday::Sun, _ => return None,
    };
    Some((n.parse().unwrap_or(0), wd))
}

fn add_months(d: NaiveDate, n: i32) -> Option<NaiveDate> {
    let m0 = d.month0() as i32 + n;
    let y = d.year() + m0.div_euclid(12);
    let m = m0.rem_euclid(12) as u32 + 1;
    NaiveDate::from_ymd_opt(y, m, 1)
}

fn nth_weekday(first_of_month: NaiveDate, n: i32, wd: Weekday) -> Option<NaiveDate> {
    if n > 0 {
        let off = (wd.num_days_from_monday() as i64 - first_of_month.weekday().num_days_from_monday() as i64).rem_euclid(7);
        let d = first_of_month + Duration::days(off + (n as i64 - 1) * 7);
        (d.month() == first_of_month.month()).then_some(d)
    } else {
        let next = add_months(first_of_month, 1)?;
        let last = next - Duration::days(1);
        let off = (last.weekday().num_days_from_monday() as i64 - wd.num_days_from_monday() as i64).rem_euclid(7);
        let d = last - Duration::days(off + ((-n) as i64 - 1) * 7);
        (d.month() == first_of_month.month()).then_some(d)
    }
}

/// Occurrence start dates (wall-clock) of a recurring event, inside [win_from, win_to].
fn expand(start: When, rule: &str, fixed: &HashMap<String, i32>, win_to: i64) -> Vec<NaiveDateTime> {
    let mut kv: HashMap<String, String> = HashMap::new();
    for part in rule.split(';') {
        if let Some((k, v)) = part.split_once('=') {
            kv.insert(k.to_ascii_uppercase(), v.to_string());
        }
    }
    let freq = kv.get("FREQ").cloned().unwrap_or_default();
    let interval: i64 = kv.get("INTERVAL").and_then(|x| x.parse().ok()).unwrap_or(1).max(1);
    let count: Option<usize> = kv.get("COUNT").and_then(|x| x.parse().ok());
    let until: Option<i64> = kv.get("UNTIL").and_then(|u| parse_when_value(u, None, u.len() == 8, fixed)).map(|w| {
        if w.date_only { w.ts() + 86399 } else { w.ts() }
    });
    let byday: Vec<(i32, Weekday)> = kv.get("BYDAY").map(|s| s.split(',').filter_map(weekday).collect()).unwrap_or_default();
    let bymonthday: Vec<i32> = kv.get("BYMONTHDAY").map(|s| s.split(',').filter_map(|x| x.parse().ok()).collect()).unwrap_or_default();
    let bymonth: Vec<u32> = kv.get("BYMONTH").map(|s| s.split(',').filter_map(|x| x.parse().ok()).collect()).unwrap_or_default();
    let time = start.ndt.time();
    let d0 = start.ndt.date();
    let mut out = vec![];
    let limit_ts = until.unwrap_or(i64::MAX).min(win_to);
    let max_iter = 5000;
    let push = |out: &mut Vec<NaiveDateTime>, d: NaiveDate| -> bool {
        let ndt = d.and_time(time);
        if ndt < start.ndt {
            return true;
        }
        let w = When { ndt, ..start };
        if w.ts() > limit_ts {
            return false;
        }
        out.push(ndt);
        count.map(|c| out.len() < c).unwrap_or(true)
    };
    match freq.as_str() {
        "DAILY" => {
            let mut d = d0;
            for _ in 0..max_iter {
                let ok_wd = byday.is_empty() || byday.iter().any(|(_, w)| *w == d.weekday());
                if ok_wd && !push(&mut out, d) { break; }
                d += Duration::days(interval);
            }
        }
        "WEEKLY" => {
            let days: Vec<Weekday> = if byday.is_empty() { vec![d0.weekday()] } else { byday.iter().map(|x| x.1).collect() };
            let week_start = d0 - Duration::days(d0.weekday().num_days_from_monday() as i64);
            let mut wk = week_start;
            'outer: for _ in 0..max_iter {
                let mut ds: Vec<NaiveDate> = days.iter().map(|w| wk + Duration::days(w.num_days_from_monday() as i64)).collect();
                ds.sort();
                for d in ds {
                    if !push(&mut out, d) { break 'outer; }
                }
                wk += Duration::days(7 * interval);
            }
        }
        "MONTHLY" => {
            let mut m = NaiveDate::from_ymd_opt(d0.year(), d0.month(), 1).unwrap_or(d0);
            'outer: for _ in 0..max_iter {
                let mut ds: Vec<NaiveDate> = vec![];
                if !byday.is_empty() {
                    let setpos: Option<i32> = kv.get("BYSETPOS").and_then(|x| x.parse().ok());
                    for (n, w) in &byday {
                        let n = if *n == 0 { setpos.unwrap_or(1) } else { *n };
                        if let Some(d) = nth_weekday(m, n, *w) { ds.push(d); }
                    }
                } else if !bymonthday.is_empty() {
                    for md in &bymonthday {
                        let d = if *md > 0 { NaiveDate::from_ymd_opt(m.year(), m.month(), *md as u32) } else {
                            add_months(m, 1).map(|n| n + Duration::days(*md as i64))
                        };
                        if let Some(d) = d { ds.push(d); }
                    }
                } else if let Some(d) = NaiveDate::from_ymd_opt(m.year(), m.month(), d0.day()) {
                    ds.push(d);
                }
                ds.sort();
                for d in ds {
                    if !push(&mut out, d) { break 'outer; }
                }
                match add_months(m, interval as i32) { Some(n) => m = n, None => break }
            }
        }
        "YEARLY" => {
            let months: Vec<u32> = if bymonth.is_empty() { vec![d0.month()] } else { bymonth.clone() };
            let mut y = d0.year();
            'outer: for _ in 0..500 {
                let mut ds = vec![];
                for mo in &months {
                    let first = NaiveDate::from_ymd_opt(y, *mo, 1);
                    let Some(first) = first else { continue };
                    if !byday.is_empty() {
                        for (n, w) in &byday {
                            if let Some(d) = nth_weekday(first, if *n == 0 { 1 } else { *n }, *w) { ds.push(d); }
                        }
                    } else if let Some(d) = NaiveDate::from_ymd_opt(y, *mo, if bymonthday.is_empty() { d0.day() } else { bymonthday[0].max(1) as u32 }) {
                        ds.push(d);
                    }
                }
                ds.sort();
                for d in ds {
                    if !push(&mut out, d) { break 'outer; }
                }
                y += interval as i32;
            }
        }
        _ => out.push(start.ndt),
    }
    out
}

/// Parse an .ics document into events within [win_from, win_to] (unix seconds).
pub fn parse(text: &str, win_from: i64, win_to: i64) -> Result<Vec<Event>> {
    if !text.contains("BEGIN:VCALENDAR") {
        return Err(anyhow!("Not an iCalendar (.ics) file"));
    }
    let lines = unfold(text);
    // Fixed offsets for custom VTIMEZONE blocks (STANDARD offset).
    let mut fixed: HashMap<String, i32> = HashMap::new();
    let mut blocks: Vec<Vec<Prop>> = vec![];
    let mut cur: Option<Vec<Prop>> = None;
    let mut tz_id: Option<String> = None;
    let mut in_std = false;
    for l in &lines {
        match l.as_str() {
            "BEGIN:VEVENT" => cur = Some(vec![]),
            "END:VEVENT" => {
                if let Some(b) = cur.take() { blocks.push(b); }
            }
            "BEGIN:VTIMEZONE" => tz_id = None,
            "BEGIN:STANDARD" => in_std = true,
            "END:STANDARD" => in_std = false,
            _ => {
                let Some(p) = parse_prop(l) else { continue };
                if let Some(b) = cur.as_mut() {
                    b.push(p);
                } else if p.name == "TZID" {
                    tz_id = Some(p.value.clone());
                } else if in_std && p.name == "TZOFFSETTO" {
                    if let Some(id) = &tz_id {
                        let v = p.value.trim();
                        let sign = if v.starts_with('-') { -1 } else { 1 };
                        let digits = v.trim_start_matches(['+', '-']);
                        let h: i32 = digits.get(0..2).and_then(|x| x.parse().ok()).unwrap_or(0);
                        let m: i32 = digits.get(2..4).and_then(|x| x.parse().ok()).unwrap_or(0);
                        fixed.insert(id.clone(), sign * (h * 3600 + m * 60));
                    }
                }
            }
        }
    }

    // Overrides: (UID, original start ts) → replacement event
    let mut overrides: HashMap<(String, i64), Vec<Prop>> = HashMap::new();
    let mut masters: Vec<Vec<Prop>> = vec![];
    for b in blocks {
        let uid = b.iter().find(|p| p.name == "UID").map(|p| p.value.clone()).unwrap_or_default();
        if let Some(rid) = b.iter().find(|p| p.name == "RECURRENCE-ID") {
            if let Some(w) = parse_when(rid, &fixed) {
                overrides.insert((uid, w.ts()), b);
                continue;
            }
        }
        masters.push(b);
    }

    let mut out = vec![];
    let make = |b: &[Prop], start_ts: i64, dur: i64, all_day: bool, recurring: bool, idx: usize| -> Option<Event> {
        let get = |n: &str| b.iter().find(|p| p.name == n).map(|p| unescape(&p.value));
        if get("STATUS").as_deref() == Some("CANCELLED") {
            return None;
        }
        let uid = get("UID").unwrap_or_default();
        let desc = get("DESCRIPTION").unwrap_or_default();
        let location = get("LOCATION").unwrap_or_default();
        let meet = [desc.as_str(), location.as_str(), get("X-MICROSOFT-SKYPETEAMSMEETINGURL").as_deref().unwrap_or("")]
            .iter()
            .flat_map(|s| s.split_whitespace())
            .find(|w| w.starts_with("https://teams.microsoft.com/") || w.starts_with("https://meet.google.com/") || w.contains("zoom.us/j/"))
            .map(|s| s.trim_matches(['<', '>', '"']).to_string())
            .unwrap_or_default();
        Some(Event {
            id: 0,
            calendar_id: 0,
            remote_id: format!("{uid}#{idx}"),
            title: get("SUMMARY").unwrap_or_else(|| "(No title)".into()),
            location,
            description: desc.chars().take(4000).collect(),
            start_ts,
            end_ts: start_ts + dur.max(if all_day { 86400 } else { 0 }),
            all_day,
            recurring,
            meet_link: meet,
            color: String::new(),
        })
    };

    for b in &masters {
        let Some(dt) = b.iter().find(|p| p.name == "DTSTART").and_then(|p| parse_when(p, &fixed)) else { continue };
        let start_ts = dt.ts();
        let dur = if let Some(e) = b.iter().find(|p| p.name == "DTEND").and_then(|p| parse_when(p, &fixed)) {
            e.ts() - start_ts
        } else if let Some(d) = b.iter().find(|p| p.name == "DURATION").and_then(|p| parse_duration(&p.value)) {
            d
        } else if dt.date_only {
            86400
        } else {
            0
        };
        let uid = b.iter().find(|p| p.name == "UID").map(|p| p.value.clone()).unwrap_or_default();
        match b.iter().find(|p| p.name == "RRULE") {
            None => {
                if start_ts + dur >= win_from && start_ts <= win_to {
                    if let Some(e) = make(b, start_ts, dur, dt.date_only, false, 0) { out.push(e); }
                }
            }
            Some(rr) => {
                let ex: HashSet<i64> = b
                    .iter()
                    .filter(|p| p.name == "EXDATE")
                    .flat_map(|p| p.value.split(',').filter_map(|v| parse_when_value(v, p.params.get("TZID"), v.len() == 8, &fixed)).map(|w| w.ts()).collect::<Vec<_>>())
                    .collect();
                for (i, occ) in expand(dt, &rr.value, &fixed, win_to).into_iter().enumerate() {
                    let ts = When { ndt: occ, ..dt }.ts();
                    if ex.contains(&ts) || ts + dur < win_from {
                        continue;
                    }
                    if let Some(ov) = overrides.remove(&(uid.clone(), ts)) {
                        let s2 = ov.iter().find(|p| p.name == "DTSTART").and_then(|p| parse_when(p, &fixed)).map(|w| w.ts()).unwrap_or(ts);
                        let e2 = ov.iter().find(|p| p.name == "DTEND").and_then(|p| parse_when(p, &fixed)).map(|w| w.ts()).unwrap_or(s2 + dur);
                        if let Some(e) = make(&ov, s2, e2 - s2, dt.date_only, true, i) { out.push(e); }
                    } else if let Some(e) = make(b, ts, dur, dt.date_only, true, i) {
                        out.push(e);
                    }
                }
            }
        }
    }
    // Overrides whose master had no matching occurrence (moved into the window).
    for ((_, _), ov) in overrides {
        if let Some(s) = ov.iter().find(|p| p.name == "DTSTART").and_then(|p| parse_when(p, &fixed)) {
            let st = s.ts();
            let en = ov.iter().find(|p| p.name == "DTEND").and_then(|p| parse_when(p, &fixed)).map(|w| w.ts()).unwrap_or(st + 3600);
            if en >= win_from && st <= win_to {
                if let Some(e) = make(&ov, st, en - st, s.date_only, true, 9999) { out.push(e); }
            }
        }
    }
    Ok(out)
}

/// What an invitation (.ics in an email) says beyond the event itself.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct InviteMeta {
    /// REQUEST (an invitation), CANCEL, REPLY, PUBLISH… ("" when absent).
    pub method: String,
    pub uid: String,
    pub sequence: String,
    /// Organizer's address (without "mailto:").
    pub organizer: String,
    pub organizer_name: String,
    /// Attendee addresses (lower case).
    pub attendees: Vec<String>,
    /// The raw DTSTART / DTEND / SUMMARY lines of the first VEVENT, for building a reply.
    #[serde(skip)]
    pub raw_lines: Vec<String>,
}

pub fn invite_meta(text: &str) -> InviteMeta {
    let mut m = InviteMeta::default();
    let mut in_event = false;
    let mut events = 0;
    for l in unfold(text) {
        let Some(p) = parse_prop(&l) else {
            match l.trim() {
                "BEGIN:VEVENT" => {
                    in_event = true;
                    events += 1;
                }
                "END:VEVENT" => in_event = false,
                _ => {}
            }
            continue;
        };
        match (p.name.as_str(), p.value.as_str()) {
            ("BEGIN", "VEVENT") => {
                in_event = true;
                events += 1;
                continue;
            }
            ("END", "VEVENT") => {
                in_event = false;
                continue;
            }
            _ => {}
        }
        if p.name == "METHOD" && !in_event {
            m.method = p.value.trim().to_ascii_uppercase();
        }
        if !in_event || events != 1 {
            continue;
        }
        let mailto = |v: &str| v.trim().trim_start_matches("mailto:").trim_start_matches("MAILTO:").to_string();
        match p.name.as_str() {
            "UID" => m.uid = p.value.trim().to_string(),
            "SEQUENCE" => m.sequence = p.value.trim().to_string(),
            "ORGANIZER" => {
                m.organizer = mailto(&p.value);
                m.organizer_name = p.params.get("CN").cloned().unwrap_or_default();
            }
            "ATTENDEE" => m.attendees.push(mailto(&p.value).to_lowercase()),
            _ => {}
        }
        if matches!(p.name.as_str(), "DTSTART" | "DTEND" | "SUMMARY" | "RECURRENCE-ID") {
            m.raw_lines.push(l.clone());
        }
    }
    m
}

pub fn normalize_url(url: &str) -> String {
    let u = url.trim();
    if let Some(rest) = u.strip_prefix("webcal://") {
        format!("https://{rest}")
    } else {
        u.to_string()
    }
}

pub async fn fetch(url: &str) -> Result<String> {
    let r = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()?
        .get(normalize_url(url))
        .header("User-Agent", "Hermio/0.3 (calendar)")
        .send()
        .await?;
    if !r.status().is_success() {
        return Err(anyhow!("Calendar server returned {}", r.status()));
    }
    Ok(r.text().await?)
}

pub fn window() -> (i64, i64) {
    let now = Utc::now().timestamp();
    (now - 180 * 86400, now + 540 * 86400)
}

#[allow(dead_code)]
pub fn _unused(_: DateTime<Utc>) {}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn outlook_weekly() {
        let ics = "BEGIN:VCALENDAR\r\nBEGIN:VTIMEZONE\r\nTZID:SE Asia Standard Time\r\nBEGIN:STANDARD\r\nTZOFFSETTO:+0700\r\nEND:STANDARD\r\nEND:VTIMEZONE\r\n\
BEGIN:VEVENT\r\nUID:abc\r\nSUMMARY:Họp tuần\r\nDTSTART;TZID=SE Asia Standard Time:20261102T090000\r\nDTEND;TZID=SE Asia Standard Time:20261102T100000\r\n\
RRULE:FREQ=WEEKLY;BYDAY=MO,WE;COUNT=6\r\nEXDATE;TZID=SE Asia Standard Time:20261104T090000\r\nLOCATION:https://teams.microsoft.com/l/meetup-join/x\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:abc\r\nRECURRENCE-ID;TZID=SE Asia Standard Time:20261109T090000\r\nSUMMARY:Họp tuần (dời)\r\nDTSTART;TZID=SE Asia Standard Time:20261109T140000\r\nDTEND;TZID=SE Asia Standard Time:20261109T150000\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:d\r\nSUMMARY:Nghỉ lễ\r\nDTSTART;VALUE=DATE:20261120\r\nDTEND;VALUE=DATE:20261121\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let evs = parse(ics, 0, i64::MAX / 4).unwrap();
        for e in &evs {
            println!("{} {} {} {}", e.title, chrono::DateTime::from_timestamp(e.start_ts, 0).unwrap(), e.all_day, e.meet_link);
        }
        // 6 occurrences minus 1 EXDATE = 5 + 1 all-day
        assert_eq!(evs.len(), 6);
        assert!(evs.iter().any(|e| e.title.contains("dời")));
        // 09:00 +07 = 02:00 UTC
        assert!(evs.iter().any(|e| chrono::DateTime::from_timestamp(e.start_ts, 0).unwrap().format("%H:%M").to_string() == "02:00"));
    }
    #[test]
    fn monthly_last_friday() {
        let ics = "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:m\nSUMMARY:Báo cáo\nDTSTART:20260130T080000Z\nDURATION:PT1H\nRRULE:FREQ=MONTHLY;BYDAY=-1FR;UNTIL=20260601T000000Z\nEND:VEVENT\nEND:VCALENDAR";
        let evs = parse(ics, 0, i64::MAX / 4).unwrap();
        let days: Vec<String> = evs.iter().map(|e| chrono::DateTime::from_timestamp(e.start_ts, 0).unwrap().format("%m-%d").to_string()).collect();
        assert_eq!(days, vec!["01-30", "02-27", "03-27", "04-24", "05-29"]);
    }
}
