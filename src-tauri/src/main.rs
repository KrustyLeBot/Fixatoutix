//! Fixatoutix — Par Toutatis, tes fenêtres ne bougeront plus !
//!
//! Lives in the notification area only: no taskbar button, a flyout panel on tray click, a global
//! shortcut to nail the active window, and an engine that puts nailed windows back where they belong.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod logger;
mod rules;
mod updater;
mod util;

#[cfg(windows)]
mod actions;
#[cfg(windows)]
mod commands;
#[cfg(windows)]
mod engine;
#[cfg(windows)]
mod i18n;
#[cfg(windows)]
mod selftest;
#[cfg(windows)]
mod state;
#[cfg(windows)]
mod tray;
#[cfg(windows)]
mod ui;

#[cfg(not(windows))]
fn main() {
    eprintln!("Fixatoutix only runs on Windows 11.");
}

#[cfg(windows)]
fn main() {
    use std::sync::Arc;
    use tauri::{Manager, RunEvent};

    let args: Vec<String> = std::env::args().collect();
    let value_of = |flag: &str| args.iter().position(|a| a == flag).map(|i| args.get(i + 1).cloned());

    if let Some(report) = value_of("--selftest") {
        std::process::exit(selftest::run(report.as_deref()));
    }
    // Started by the previous version after a self-update: let it exit first, otherwise the
    // single-instance guard would hand control back to the dying process.
    let after_update = value_of("--after-update");
    if let Some(Some(pid)) = &after_update {
        if let Ok(pid) = pid.parse::<u32>() {
            wait_for_exit(pid);
        }
    }

    logger::init();
    info!("Fixatoutix {} build {} starting", env!("CARGO_PKG_VERSION"), updater::current_build());

    let background = args.iter().any(|a| a == "--background");
    let path = config::default_path();
    let loaded = config::load(&path);
    let recovered = loaded.recovered;
    if recovered {
        warn!("config unreadable, backed up and reset");
    }
    let state = Arc::new(state::AppState::new(loaded.config, path));

    let app = tauri::Builder::default()
        // Must be the first plugin: a second launch opens the panel of the running instance.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(state) = app.try_state::<Arc<state::AppState>>() {
                ui::show_panel(app, &state, None);
            }
        }))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--background"]),
        ))
        .manage(state.clone())
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::list_windows,
            commands::nail_window,
            commands::update_rule,
            commands::set_match,
            commands::delete_rule,
            commands::reorder_rules,
            commands::reclou_rule,
            commands::set_paused,
            commands::gather,
            commands::set_language,
            commands::set_check_updates,
            commands::set_autostart,
            commands::set_shortcut,
            commands::open_config_dir,
            commands::open_repo,
            commands::check_update,
            commands::install_update,
            commands::close_panel,
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            let lang = state.language();

            let engine = engine::start(handle.clone(), state.clone());
            let _ = state.engine.set(engine);

            tray::setup(&handle, &state)?;
            if let Err(e) = commands::register_shortcut(&handle, &state) {
                warn!("cannot register the global shortcut: {e}");
                ui::toast(&handle, "error", i18n::tr(&lang, "toast.resists"), i18n::tr(&lang, "toast.resists.shortcut"));
            }

            // The exe may have been moved: rewrite the autostart entry with the current path.
            if actions::autostart_enabled(&handle) {
                let _ = actions::set_autostart(&handle, true);
            }

            // The previous exe is still locked for a moment right after an update.
            std::thread::spawn(|| {
                if let Ok(exe) = std::env::current_exe() {
                    for _ in 0..40 {
                        if updater::cleanup_previous(&exe) {
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(500));
                    }
                }
            });
            actions::spawn_update_checker(handle.clone(), state.clone());

            if recovered {
                ui::toast(&handle, "error", i18n::tr(&lang, "toast.config"), i18n::tr(&lang, "toast.config.sub"));
            } else if after_update.is_some() {
                ui::toast(&handle, "ok", i18n::tr(&lang, "toast.updated"), &format!("build {}", updater::current_build()));
            }
            if !background {
                ui::show_panel(&handle, &state, None);
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("cannot build the Fixatoutix application");

    app.run(|_app, event| {
        // The panel and toasts come and go: only an explicit exit (tray "Quitter", update) ends the app.
        if let RunEvent::ExitRequested { api, code, .. } = event {
            if code.is_none() {
                api.prevent_exit();
            }
        }
    });
}

#[cfg(windows)]
fn wait_for_exit(pid: u32) {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE};
    unsafe {
        if let Ok(process) = OpenProcess(PROCESS_SYNCHRONIZE, false, pid) {
            WaitForSingleObject(process, 15_000);
            let _ = CloseHandle(process);
        }
    }
}
