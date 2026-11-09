//! Hermio's own new-mail popup: a small always-on-top window in the corner above the taskbar.
//! Windows' toasts can't show a bold first line or a tighter layout, so new mail is announced
//! here (bold subject, From / To, time), and a silent copy still goes to the notification
//! centre so it keeps the history. It never takes the focus, follows Do not disturb, and a
//! click opens the email.

use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};

pub const LABEL: &str = "notify";
const WIDTH: f64 = 372.0;
const MARGIN: f64 = 12.0;

#[derive(Clone, Serialize)]
pub struct PopupItem {
    pub key: String,
    pub subject: String,
    pub from: String,
    pub to: String,
    pub time: String,
    /// What a click opens (same format as a toast's launch argument).
    pub launch: String,
    pub from_label: String,
    pub to_label: String,
}

static QUEUE: Mutex<Vec<PopupItem>> = Mutex::new(Vec::new());

/// Windows is in Do not disturb / presentation / full-screen mode: no popup then.
#[cfg(windows)]
pub fn quiet_hours() -> bool {
    use windows_sys::Win32::UI::Shell::{SHQueryUserNotificationState, QUNS_ACCEPTS_NOTIFICATIONS};
    let mut st = 0;
    let hr = unsafe { SHQueryUserNotificationState(&mut st) };
    hr == 0 && st != QUNS_ACCEPTS_NOTIFICATIONS
}
#[cfg(not(windows))]
pub fn quiet_hours() -> bool {
    false
}

#[cfg(windows)]
fn chime() {
    use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ALIAS, SND_ASYNC, SND_NODEFAULT};
    let name: Vec<u16> = "Notification.Mail\0".encode_utf16().collect();
    unsafe {
        PlaySoundW(name.as_ptr(), std::ptr::null_mut(), SND_ALIAS | SND_ASYNC | SND_NODEFAULT);
    }
}
#[cfg(not(windows))]
fn chime() {}

/// Queue a popup and make sure the popup window exists (it picks the queue up when ready).
pub fn push(app: &AppHandle, item: PopupItem, sound: bool) {
    QUEUE.lock().unwrap().push(item);
    if sound {
        chime();
    }
    if app.get_webview_window(LABEL).is_none() {
        let built = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html#notify".into()))
            .title("Hermio")
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .resizable(false)
            .focused(false)
            .focusable(false)
            .visible(false)
            .inner_size(WIDTH, 10.0)
            .build();
        if let Err(e) = built {
            eprintln!("popup window: {e}");
        }
    }
    let _ = app.emit_to(LABEL, "popup-add", ());
}

#[tauri::command]
pub fn popup_take() -> Vec<PopupItem> {
    std::mem::take(&mut *QUEUE.lock().unwrap())
}

/// The popup page measured its content: size the window to it in the bottom-right corner of
/// the screen Hermio is on (above the taskbar), or hide it when nothing is left.
#[tauri::command]
pub fn popup_layout(app: AppHandle, height: f64) {
    let Some(w) = app.get_webview_window(LABEL) else { return };
    if height <= 0.0 {
        let _ = w.hide();
        return;
    }
    let mon = app
        .get_webview_window("main")
        .and_then(|m| m.current_monitor().ok().flatten())
        .or_else(|| w.primary_monitor().ok().flatten());
    let Some(mon) = mon else { return };
    let s = mon.scale_factor();
    let area = mon.work_area();
    let (ww, hh) = ((WIDTH * s).round() as i32, (height * s).round() as i32);
    let m = (MARGIN * s).round() as i32;
    let x = area.position.x + area.size.width as i32 - ww - m;
    let y = area.position.y + area.size.height as i32 - hh - m;
    let _ = w.set_size(PhysicalSize::new(ww as u32, hh as u32));
    let _ = w.set_position(PhysicalPosition::new(x, y));
    if !w.is_visible().unwrap_or(false) {
        let _ = w.show();
    }
    let _ = w.set_always_on_top(true);
}

/// A popup was clicked: open the email in Hermio.
#[tauri::command]
pub fn popup_open(app: AppHandle, launch: String) {
    crate::notify::activated(&app, &launch);
}
