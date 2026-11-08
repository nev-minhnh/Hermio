//! Sorting rules: put matching messages into groups (and optionally mark read / flag).

use crate::models::*;
use crate::state::AppState;
use anyhow::Result;

#[derive(Default, Debug, Clone, Copy)]
pub struct Effects {
    pub mark_read: bool,
    pub flag: bool,
}

fn needs_body(r: &Rule) -> bool {
    r.conditions.iter().any(|c| c.field == "body" || c.field == "any")
}

fn text_match(hay: &str, op: &str, value: &str) -> bool {
    let hay_l = hay.to_lowercase();
    // "a, b; c" means any of the listed values.
    let values: Vec<String> = value
        .split([',', ';', '\n'])
        .map(|v| v.trim().to_lowercase())
        .filter(|v| !v.is_empty())
        .collect();
    if values.is_empty() {
        return false;
    }
    match op {
        "not_contains" => values.iter().all(|v| !hay_l.contains(v.as_str())),
        "equals" => values.iter().any(|v| hay_l.trim() == v),
        "starts_with" => values.iter().any(|v| hay_l.trim_start().starts_with(v.as_str())),
        "ends_with" => values.iter().any(|v| hay_l.trim_end().ends_with(v.as_str())),
        "domain" => values.iter().any(|v| {
            let d = v.trim_start_matches('@');
            hay_l.split([',', '<', '>', ' ']).any(|p| p.ends_with(&format!("@{d}")) || p.ends_with(&format!(".{d}")))
        }),
        "regex" => regex_lite::Regex::new(&format!("(?i){value}")).map(|re| re.is_match(hay)).unwrap_or(false),
        _ => values.iter().any(|v| hay_l.contains(v.as_str())),
    }
}

fn cond_match(c: &Condition, m: &MessageRow, account_email: &str, body: &str) -> bool {
    let from = format!("{} <{}>", m.from_name, m.from_addr);
    match c.field.as_str() {
        "from" => text_match(&from, &c.op, &c.value),
        "to" => text_match(&format!("{}, {}", m.to_list, m.cc_list), &c.op, &c.value),
        "cc" => text_match(&m.cc_list, &c.op, &c.value),
        "subject" => text_match(&m.subject, &c.op, &c.value),
        "body" => text_match(body, &c.op, &c.value),
        "any" => text_match(&format!("{from}\n{}\n{}\n{}", m.to_list, m.subject, body), &c.op, &c.value),
        "account" => text_match(account_email, &c.op, &c.value),
        "attachment" => m.has_attachments == (c.value != "no"),
        _ => false,
    }
}

/// A rule that files into a local group only applies to that group's mailbox.
fn group_allows(groups: &[Group], r: &Rule, m: &MessageRow) -> bool {
    match r.group_id.and_then(|g| groups.iter().find(|x| x.id == g)) {
        Some(g) => g.account_id.map(|a| a == m.account_id).unwrap_or(true),
        None => true,
    }
}

pub fn matches(r: &Rule, m: &MessageRow, account_email: &str, body: &str) -> bool {
    if !r.account_ids.is_empty() && !r.account_ids.contains(&m.account_id) {
        return false;
    }
    let conds: Vec<&Condition> = r.conditions.iter().filter(|c| !c.value.trim().is_empty() || c.field == "attachment").collect();
    if conds.is_empty() {
        return false;
    }
    if r.match_all {
        conds.iter().all(|c| cond_match(c, m, account_email, body))
    } else {
        conds.iter().any(|c| cond_match(c, m, account_email, body))
    }
}

/// Apply rules to one message. `body = None` is the header phase (rules that need the body
/// wait until the body is downloaded and indexed, when this is called again with `Some`).
pub fn apply(state: &AppState, rules: &[Rule], m: &MessageRow, body: Option<&str>) -> Result<Effects> {
    apply_db(&state.db, rules, m, body)
}

pub fn apply_db(db: &crate::db::Db, rules: &[Rule], m: &MessageRow, body: Option<&str>) -> Result<Effects> {
    let mut fx = Effects::default();
    // Only incoming mail is sorted.
    let role = db.folder(m.folder_id).map(|f| f.role).unwrap_or_default();
    if role != "inbox" && !role.is_empty() {
        return Ok(fx);
    }
    let account_email = db.account(m.account_id).map(|a| a.email).unwrap_or_default();
    let groups = db.groups()?;
    for r in rules.iter().filter(|r| r.enabled && group_allows(&groups, r, m)) {
        let body_rule = needs_body(r);
        let text = match (body, body_rule) {
            (None, true) => continue,   // decide later, once the body is here
            (Some(_), false) => continue, // already handled in the header phase
            (Some(b), true) => b,
            (None, false) => "",
        };
        if matches(r, m, &account_email, text) {
            if let Some(g) = r.group_id {
                db.add_to_group(&[m.id], g)?;
            }
            fx.mark_read |= r.mark_read;
            fx.flag |= r.flag;
            if r.stop {
                break;
            }
        }
    }
    if fx.mark_read || fx.flag {
        db.set_flags(m.id, fx.mark_read.then_some(true), fx.flag.then_some(true), None)?;
    }
    Ok(fx)
}

/// Final pass for mail that arrived in this sync: exactly the same matching as "Run all on
/// existing mail" (headers + indexed body + real attachment flag), so new mail always ends up
/// where the button would put it. Only adds group memberships (idempotent).
pub fn apply_new(db: &crate::db::Db, ids: &[i64]) -> Result<usize> {
    if ids.is_empty() {
        return Ok(0);
    }
    let rules: Vec<Rule> = db.rules()?.into_iter().filter(|r| r.enabled).collect();
    if rules.is_empty() {
        return Ok(0);
    }
    let groups = db.groups()?;
    let mut n = 0;
    for &id in ids {
        let Ok(m) = db.message(id) else { continue };
        let role = db.folder(m.folder_id).map(|f| f.role).unwrap_or_default();
        if role != "inbox" && !role.is_empty() {
            continue;
        }
        let account_email = db.account(m.account_id).map(|a| a.email).unwrap_or_default();
        let body = db.indexed_body(id).unwrap_or_default();
        let mut hit = false;
        for r in rules.iter().filter(|r| group_allows(&groups, r, &m)) {
            if matches(r, &m, &account_email, &body) {
                hit = true;
                if let Some(g) = r.group_id {
                    db.add_to_group(&[id], g)?;
                }
                if r.stop {
                    break;
                }
            }
        }
        if hit {
            n += 1;
        }
    }
    Ok(n)
}

/// Run all rules over existing inbox messages. Returns how many messages matched something.
pub fn run_on_existing(state: &AppState, rule_id: Option<i64>) -> Result<usize> {
    let rules: Vec<Rule> = state.db.rules()?.into_iter().filter(|r| rule_id.map(|id| id == r.id).unwrap_or(true)).collect();
    let ids = state.db.message_ids_where("folder_id IN (SELECT id FROM folders WHERE role='inbox')", 50_000)?;
    let groups = state.db.groups()?;
    let mut n = 0;
    for id in ids {
        let m = state.db.message(id)?;
        let account_email = state.db.account(m.account_id).map(|a| a.email).unwrap_or_default();
        let body = state.db.indexed_body(id).unwrap_or_default();
        let mut hit = false;
        for r in rules.iter().filter(|r| r.enabled && group_allows(&groups, r, &m)) {
            if matches(r, &m, &account_email, &body) {
                hit = true;
                if let Some(g) = r.group_id {
                    state.db.add_to_group(&[id], g)?;
                }
                if r.stop {
                    break;
                }
            }
        }
        if hit {
            n += 1;
        }
    }
    Ok(n)
}

#[cfg(test)]
mod sync_tests {
    use super::*;
    use crate::db::Db;

    fn setup() -> (Db, i64, i64, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("hermio-rules-{}", std::process::id() as u64 * 1000 + rand_n()));
        let _ = std::fs::remove_dir_all(&dir);
        let db = Db::open(&dir).unwrap();
        let acc = db.save_account(&Account { email: "me@example.com".into(), name: "Me".into(), protocol: "imap".into(), enabled: true, ..Default::default() }).unwrap();
        let inbox = db.upsert_folder(acc, "INBOX", "Inbox", "/", "inbox", true).unwrap();
        (db, acc, inbox, dir)
    }
    fn rand_n() -> u64 {
        let mut b = [0u8; 2];
        let _ = getrandom::getrandom(&mut b);
        u16::from_le_bytes(b) as u64
    }

    fn raw(from: &str, subject: &str, body: &str) -> Vec<u8> {
        format!("From: {from}\r\nTo: me@example.com\r\nSubject: {subject}\r\nDate: Sat, 7 Nov 2026 10:00:00 +0700\r\nMessage-ID: <{subject}@x>\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n{body}\r\n").into_bytes()
    }

    /// Mimics imap::sync_folder: header phase on arrival, then the body phase.
    fn arrive(db: &Db, acc: i64, inbox: i64, uid: u32, r: &[u8]) -> i64 {
        let hdr_end = r.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
        let mut m = crate::parse::summarize(&r[..hdr_end], false);
        m.account_id = acc;
        m.folder_id = inbox;
        m.uid = uid;
        let id = db.insert_message(&m).unwrap();
        let rules = db.rules().unwrap();
        let row = db.message(id).unwrap();
        apply_db(db, &rules, &row, None).unwrap();
        let sum = crate::parse::summarize(r, true);
        let body = sum.body.clone().unwrap_or_default();
        db.set_body_index(id, "", &body, &sum.snippet, sum.has_attachments).unwrap();
        let row = db.message(id).unwrap();
        apply_db(db, &rules, &row, Some(&body)).unwrap();
        apply_new(db, &[id]).unwrap();
        id
    }

    #[test]
    fn new_mail_is_sorted() {
        let (db, acc, inbox, dir) = setup();
        let g = db.save_group(&Group { name: "GitHub".into(), color: "#000".into(), ..Default::default() }).unwrap();
        let mk = |conds: Vec<(&str, &str, &str)>, all: bool| Rule {
            name: "r".into(),
            enabled: true,
            match_all: all,
            conditions: conds.into_iter().map(|(f, o, v)| Condition { field: f.into(), op: o.into(), value: v.into() }).collect(),
            group_id: Some(g),
            ..Default::default()
        };
        db.save_rule(&mk(vec![("from", "contains", "github.com")], false)).unwrap();
        let a = arrive(&db, acc, inbox, 1, &raw("GitHub <noreply@github.com>", "PR merged", "hello"));
        assert_eq!(db.message_group_ids(a).unwrap(), vec![g], "header rule");

        let g2 = db.save_group(&Group { name: "Invoices".into(), color: "#000".into(), ..Default::default() }).unwrap();
        let mut r2 = mk(vec![("body", "contains", "invoice")], false);
        r2.group_id = Some(g2);
        db.save_rule(&r2).unwrap();
        let b = arrive(&db, acc, inbox, 2, &raw("Shop <a@shop.vn>", "Your order", "Here is your invoice #12"));
        assert_eq!(db.message_group_ids(b).unwrap(), vec![g2], "body rule");

        // Mixed: subject (header) AND body (match all)
        let g3 = db.save_group(&Group { name: "Mixed".into(), color: "#000".into(), ..Default::default() }).unwrap();
        let mut r3 = mk(vec![("subject", "contains", "report"), ("body", "contains", "weekly")], true);
        r3.group_id = Some(g3);
        db.save_rule(&r3).unwrap();
        let c = arrive(&db, acc, inbox, 3, &raw("Boss <b@corp.com>", "Report", "the weekly numbers"));
        assert_eq!(db.message_group_ids(c).unwrap(), vec![g3], "mixed rule");

        // A message whose body never got indexed (e.g. too big): header rules still apply.
        let mut m = crate::parse::summarize(&raw("GitHub <ci@github.com>", "Build failed", ""), false);
        m.account_id = acc;
        m.folder_id = inbox;
        m.uid = 9;
        let big = db.insert_message(&m).unwrap();
        assert_eq!(apply_new(&db, &[big]).unwrap(), 1);
        assert_eq!(db.message_group_ids(big).unwrap(), vec![g]);
        let _ = std::fs::remove_dir_all(dir);
    }
}
