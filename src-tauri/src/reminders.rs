//! Calendar reminders: 12 h, 6 h, 4 h, 2 h, 1 h, 30, 15 and 10 minutes before each timed event.
//!
//! Each step is announced once (a toast with "Skip remaining reminders", plus a card in the
//! window). Steps already passed when Hermio first sees an event (it was off, the event was just
//! added) collapse into one reminder. "Skip" silences the rest for that occurrence.

use crate::models::Event;
use crate::state::AppState;
use std::sync::Arc;
use std::time::Duration;

/// Lead times, in seconds.
pub const LEADS: [i64; 8] = [12 * 3600, 6 * 3600, 4 * 3600, 2 * 3600, 3600, 30 * 60, 15 * 60, 10 * 60];

/// Identifies one occurrence: moving the event makes it a new one (reminders start over).
pub fn key(e: &Event) -> String {
    let id = if e.remote_id.is_empty() { e.id.to_string() } else { e.remote_id.clone() };
    format!("{}:{}@{}", e.calendar_id, id, e.start_ts)
}

/// The step that is due when `left` seconds remain: the smallest lead time ≥ `left`.
pub fn due(left: i64) -> Option<i64> {
    LEADS.iter().copied().filter(|l| *l >= left).min()
}

pub fn start(state: Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        // Let the first calendar sync land.
        tokio::time::sleep(Duration::from_secs(20)).await;
        loop {
            if crate::notify::setting_bool(&state, "reminders", true) {
                tick(&state);
            }
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    });
}

fn tick(state: &AppState) {
    let now = chrono::Utc::now().timestamp();
    let Ok(events) = state.db.upcoming_events(now, LEADS[0] + 60) else { return };
    for e in events {
        let left = e.start_ts - now;
        let Some(lead) = due(left) else { continue };
        let k = key(&e);
        let (fired, skipped) = state.db.reminder(&k).ok().flatten().unwrap_or((0, false));
        if skipped || (fired != 0 && lead >= fired) {
            continue;
        }
        if state.db.set_reminder(&k, e.start_ts, Some(lead), None).is_err() {
            continue;
        }
        let last = lead == *LEADS.last().unwrap();
        crate::notify::reminder(state, &e, left, &k, last);
        state.emit(
            "reminder",
            serde_json::json!({
                "key": k, "event_id": e.id, "title": e.title, "location": e.location, "meet_link": e.meet_link,
                "start_ts": e.start_ts, "end_ts": e.end_ts, "left": left, "last": last, "color": e.color,
            }),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn due_steps() {
        assert_eq!(due(13 * 3600), None);
        assert_eq!(due(11 * 3600), Some(12 * 3600));
        assert_eq!(due(3 * 3600), Some(4 * 3600));
        assert_eq!(due(3600), Some(3600));
        assert_eq!(due(20 * 60), Some(30 * 60));
        assert_eq!(due(5 * 60), Some(10 * 60));
    }
}
