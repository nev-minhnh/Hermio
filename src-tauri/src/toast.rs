//! Showing toast XML. On Windows through WinRT (`ToastNotificationManager`), which allows the
//! adaptive layout and buttons Hermio uses; clicks are reported to [`crate::notify::activated`]
//! while the toast is on screen. Elsewhere (development on Linux/macOS) a plain notification
//! with the given title and body is shown instead.

#[cfg(windows)]
pub fn show(app: &tauri::AppHandle, xml: String, _title: String, _body: String) {
    let app2 = app.clone();
    // WinRT objects are created on the UI thread (it's an initialised COM apartment).
    let _ = app.run_on_main_thread(move || {
        if let Err(e) = imp::show(&app2, &xml) {
            use tauri::Manager;
            if let Some(state) = app2.try_state::<std::sync::Arc<crate::state::AppState>>() {
                crate::sync::log_plain(&state, "warn", format!("Couldn't show a notification: {e}"));
            }
        }
    });
}

#[cfg(windows)]
mod imp {
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use windows::core::{IInspectable, Interface, HSTRING};
    use windows::Data::Xml::Dom::XmlDocument;
    use windows::Foundation::TypedEventHandler;
    use windows::UI::Notifications::{ToastActivatedEventArgs, ToastNotification, ToastNotificationManager};

    thread_local! {
        /// Recent toasts stay referenced so their click handlers stay alive.
        static KEEP: RefCell<VecDeque<ToastNotification>> = const { RefCell::new(VecDeque::new()) };
    }

    pub fn show(app: &tauri::AppHandle, xml: &str) -> windows::core::Result<()> {
        let doc = XmlDocument::new()?;
        doc.LoadXml(&HSTRING::from(xml))?;
        let toast = ToastNotification::CreateToastNotification(&doc)?;
        let h = app.clone();
        toast.Activated(&TypedEventHandler::<ToastNotification, IInspectable>::new(move |_, args| {
            if let Some(a) = args.as_ref() {
                if let Ok(a) = a.cast::<ToastActivatedEventArgs>() {
                    let s = a.Arguments().map(|s| s.to_string_lossy()).unwrap_or_default();
                    crate::notify::activated(&h, &s);
                }
            }
            Ok(())
        }))?;
        let notifier = ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(app.config().identifier.as_str()))?;
        notifier.Show(&toast)?;
        KEEP.with(|k| {
            let mut k = k.borrow_mut();
            k.push_back(toast);
            while k.len() > 40 {
                k.pop_front();
            }
        });
        Ok(())
    }
}

#[cfg(not(windows))]
pub fn show(app: &tauri::AppHandle, _xml: String, title: String, body: String) {
    use tauri_plugin_notification::NotificationExt;
    let _ = app.notification().builder().title(title).body(body).show();
}
