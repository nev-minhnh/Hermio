#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ai;
mod npu;
mod ovms;
mod driver;
mod backup;
mod llm;
mod web;
mod calendar;
mod ics;
mod commands;
mod docs;
mod db;
mod imap;
mod models;
mod net;
mod oauth;
mod parse;
mod pop3;
mod rules;
mod autoconfig;
mod smtp;
mod state;
mod notify;
mod sync;

use std::sync::Arc;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager, WindowEvent};

fn show_main(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

fn close_to_tray(app: &tauri::AppHandle) -> bool {
    app.try_state::<Arc<state::AppState>>()
        .and_then(|s| s.db.get_setting("close_to_tray").ok().flatten())
        .map(|v| v.trim_matches('"') != "false")
        .unwrap_or(true)
}

fn main() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| show_main(app)))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--hidden"]),
        ))
        .on_window_event(|w, ev| {
            if let WindowEvent::CloseRequested { api, .. } = ev {
                if w.label() == "main" && close_to_tray(w.app_handle()) {
                    api.prevent_close();
                    let _ = w.hide();
                    // In the tray: release the on-device model from the NPU to save battery.
                    npu::hibernate();
                }
            }
        })
        .setup(|app| {
            // System tray: left click opens, menu has the common actions.
            let open = MenuItem::with_id(app, "open", "Open Hermio", true, None::<&str>)?;
            let compose = MenuItem::with_id(app, "compose", "New mail", true, None::<&str>)?;
            let sync = MenuItem::with_id(app, "sync", "Sync now", true, None::<&str>)?;
            let sep = PredefinedMenuItem::separator(app)?;
            let quit = MenuItem::with_id(app, "quit", "Quit Hermio", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &compose, &sync, &sep, &quit])?;
            let mut tray = TrayIconBuilder::with_id("main")
                .tooltip("Hermio")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, ev| match ev.id.as_ref() {
                    "open" => show_main(app),
                    "compose" => {
                        show_main(app);
                        let _ = app.emit("tray-compose", ());
                    }
                    "sync" => {
                        if let Some(st) = app.try_state::<Arc<state::AppState>>() {
                            let st = st.inner().clone();
                            tauri::async_runtime::spawn(async move { sync::sync_all(st).await });
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, ev| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
                        show_main(tray.app_handle());
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;

            // Portable mode: a file named "portable" next to Hermio.exe keeps all data in .\data.
            let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf()));
            let dir = match exe_dir {
                Some(d) if d.join("portable").exists() || d.join("portable.txt").exists() => d.join("data"),
                _ => app.path().app_data_dir()?,
            };
            // First start after the rename: take over the data of the installed Hermio.
            let migrated = backup::migrate_from_hermio(&dir);
            // A restore prepared in Backup & transfer is swapped in now, before the database opens.
            let restored = backup::apply_pending(&dir);
            // Lets Windows show our toasts (also for the portable copy, which has no shortcut).
            notify::register_app_id(&app.config().identifier, &dir);
            let db = db::Db::open(&dir).map_err(|e| format!("Could not open the database: {e:#}"))?;
            let state = Arc::new(state::AppState {
                db: Arc::new(db),
                tokens: Default::default(),
                app: app.handle().clone(),
                sync_locks: Default::default(),
            });
            app.manage(state.clone());
            // A click on a new-mail toast opens Hermio on that email.
            {
                use tauri_plugin_notification::NotificationExt;
                let h = app.handle().clone();
                let _ = app.notification().on_action(move |p| notify::clicked(&h, p));
            }
            if let Some(old) = migrated {
                sync::log_plain(&state, "info", format!("Moved your data from Nelo ({}) to Hermio", old.display()));
            }
            if let Some(kept) = restored {
                let _ = state.db.set_setting("restored_note", &serde_json::to_string(&kept).unwrap_or_default());
            }
            ai::init_defaults(&state);
            npu::start_idle_watch(state.clone());
            // Learn about the NPU (device, driver, generation) in the background at start-up.
            tauri::async_runtime::spawn(async {
                npu::detect().await;
            });
            sync::start_scheduler(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::set_setting,
            commands::google_config,
            commands::set_google_config,
            commands::guess_settings,
            commands::list_accounts,
            commands::default_pop3_dir,
            commands::save_account,
            commands::google_sign_in,
            commands::test_account,
            commands::delete_account,
            commands::reorder_accounts,
            commands::relocate_pop3,
            commands::sync_now,
            commands::list_folders,
            commands::list_messages,
            commands::unread_counts,
            commands::get_message,
            commands::message_source,
            commands::set_seen,
            commands::set_flagged,
            commands::move_messages,
            commands::archive_messages,
            commands::spam_messages,
            commands::delete_messages,
            commands::create_folder,
            commands::save_attachment,
            commands::open_attachment,
            commands::suggest_contacts,
            commands::send_mail,
            commands::save_draft,
            commands::list_signatures,
            commands::save_signature,
            commands::delete_signature,
            commands::list_calendars,
            commands::set_calendar_visible,
            commands::list_events,
            commands::save_event,
            commands::delete_event,
            commands::sync_calendars,
            commands::data_dir,
            commands::open_url,
            commands::discover_account,
            commands::quick_add_account,
            commands::list_spaces,
            commands::save_space,
            commands::delete_space,
            commands::set_account_space,
            commands::list_groups,
            commands::save_group,
            commands::delete_group,
            commands::add_to_group,
            commands::remove_from_group,
            commands::message_groups,
            commands::list_rules,
            commands::save_rule,
            commands::delete_rule,
            commands::run_rules,
            commands::ai_providers,
            commands::ai_save_provider,
            commands::ai_delete_provider,
            commands::ai_models,
            commands::ai_test,
            commands::ai_run,
            commands::ai_skills,
            commands::ai_save_skill,
            commands::ai_delete_skill,
            commands::ai_chats,
            commands::ai_chat,
            commands::ai_save_chat,
            commands::ai_delete_chat,
            commands::ai_read_file,
            commands::ai_suggestions,
            commands::ai_read_bytes,
            commands::ai_install_skill_dir,
            commands::ai_cancel,
            commands::web_config,
            commands::set_web_config,
            commands::web_test,
            commands::npu_available,
            commands::npu_info,
            commands::npu_sync_models,
            commands::ai_resolve_suggestion,
            commands::ai_suggest_now,
            commands::ai_suggest_queue,
            commands::npu_install_driver,
            commands::npu_dirs,
            commands::npu_set_dirs,
            commands::npu_open_dir,
            commands::npu_redownload,
            commands::sync_log,
            commands::config_export,
            commands::data_backup,
            commands::data_stage_restore,
            commands::data_cancel_restore,
            commands::app_restart,
            commands::restored_note,
            commands::config_import,
            commands::sync_log_clear,
            commands::sync_log_open,
            commands::test_notification,
            commands::notification_pending,
            commands::schedule_send,
            commands::list_outbox,
            commands::cancel_outbox,
            commands::reschedule_outbox,
            commands::send_outbox_now,
            commands::attachment_data,
            commands::add_ics_calendar,
            commands::import_ics_file,
            commands::update_calendar,
            commands::delete_calendar,
            commands::start_hidden,
            commands::get_autostart,
            commands::set_autostart,
            commands::quit_app,
            commands::npu_catalog,
            commands::npu_status,
            commands::npu_install,
            commands::npu_pull,
            commands::npu_cancel_pull,
            commands::npu_remove,
            commands::npu_start,
            commands::npu_stop,
        ])
        .build(tauri::generate_context!())
        .expect("error while running Hermio")
        .run(|_app, event| {
            if let tauri::RunEvent::Exit = event {
                npu::shutdown();
            }
        });
}
