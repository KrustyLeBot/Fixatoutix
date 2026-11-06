//! What the user can do: nail a window, list windows, pause, install an update.
//! Shared by the global shortcut, the tray menu and the panel commands.

use crate::engine::win;
use crate::i18n::{nailed_variant, tr};
use crate::rules::{Match, Mode, MonitorRef, Rule};
use crate::state::{AppState, Status};
use crate::util::{label_from_exe, now_iso};
use crate::{ui, updater};
use serde::Serialize;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use windows::Win32::Foundation::HWND;

pub struct Nailed {
    pub label: String,
    pub title: String,
    pub created: bool,
}

pub enum NailError {
    /// Not an application window (desktop, taskbar, tool window, ourselves...).
    NotAWindow,
    /// Minimized or maximized: there is no meaningful rectangle to remember.
    WrongState,
}

/// Creates the rule of `hwnd`, or updates its rectangle when it is already nailed ("Reclouer ici").
pub fn nail_hwnd(state: &AppState, hwnd: HWND) -> Result<Nailed, NailError> {
    let info = win::candidate(hwnd).ok_or(NailError::NotAWindow)?;
    if win::is_minimized(hwnd) || win::is_maximized(hwnd) {
        return Err(NailError::WrongState);
    }
    let rect = win::frame_bounds(hwnd).ok_or(NailError::NotAWindow)?;
    let monitor = win::monitor_of(hwnd);
    let monitor_ref = MonitorRef {
        device_name: monitor.as_ref().map(|m| m.device_name.clone()).unwrap_or_default(),
        work_area: monitor.as_ref().map(|m| m.work),
    };
    let now = now_iso();
    let (label, created, rule_id) = {
        let mut config = state.config.lock().map_err(|_| NailError::NotAWindow)?;
        // The rule this window already claimed, else a matching rule nobody has claimed (the window opened
        // while paused...). Otherwise this is another window of the same app: it gets its own rule.
        let claimed = state.claimed_rule(info.hwnd);
        let index = claimed
            .and_then(|id| config.rules.iter().position(|r| r.id == id && r.matches(&info)))
            .or_else(|| {
                let taken: std::collections::HashSet<String> =
                    state.claims.lock().map(|c| c.values().cloned().collect()).unwrap_or_default();
                config.rules.iter().position(|r| r.matches(&info) && !taken.contains(&r.id))
            });
        if let Some(index) = index {
            let rule = &mut config.rules[index];
            rule.rect = rect;
            rule.monitor = monitor_ref;
            rule.exe_path = Some(info.exe_path.clone());
            rule.updated_at = now;
            (rule.label.clone(), false, rule.id.clone())
        } else {
            let rule = Rule {
                id: uuid::Uuid::new_v4().to_string(),
                enabled: true,
                label: unique_label(&config.rules, &label_from_exe(&info.exe)),
                matcher: Match { exe: info.exe.clone(), class_name: None, title_regex: None },
                rect,
                monitor: monitor_ref,
                mode: Mode::Lock,
                force_restore: false,
                exe_path: Some(info.exe_path.clone()),
                created_at: now.clone(),
                updated_at: now,
            };
            let result = (rule.label.clone(), true, rule.id.clone());
            config.rules.push(rule);
            result
        }
    };
    state.save();
    state.claim(info.hwnd, &rule_id);
    if let Ok(mut status) = state.status.lock() {
        status.insert(rule_id, Status::Ok);
    }
    crate::info!("nailed '{}' ({}) at {},{} {}x{}", label, info.title, rect.x, rect.y, rect.w, rect.h);
    Ok(Nailed { label, title: info.title, created })
}

/// Hotkey / tray: the window in front, or the last application window that was.
pub fn nail_foreground(app: &AppHandle, state: &Arc<AppState>) {
    let lang = state.language();
    let candidates = [win::foreground(), Some(win::hwnd_from(state.last_foreground()))];
    let target = candidates.into_iter().flatten().find(|h| win::candidate(*h).is_some());
    let outcome = match target {
        Some(hwnd) => nail_hwnd(state, hwnd),
        None => Err(NailError::NotAWindow),
    };
    announce(app, state, &lang, outcome);
}

pub fn announce(app: &AppHandle, state: &Arc<AppState>, lang: &str, outcome: Result<Nailed, NailError>) {
    match outcome {
        Ok(n) => {
            let title = if n.created { nailed_variant(lang) } else { tr(lang, "toast.renailed") };
            ui::toast(app, "ok", title, &format!("{} — {}", n.label, n.title));
            let _ = app.emit("state-changed", ());
            let _ = state; // nothing else to refresh
        }
        Err(NailError::NotAWindow) => ui::toast(app, "error", tr(lang, "toast.resists"), tr(lang, "toast.resists.window")),
        Err(NailError::WrongState) => ui::toast(app, "error", tr(lang, "toast.resists"), tr(lang, "toast.resists.state")),
    }
}

/// "Reclouer ici" from the panel: take the rectangle of the rule's open window.
pub fn reclou_rule(state: &AppState, id: &str) -> Result<Nailed, String> {
    let rule = state
        .config
        .lock()
        .map_err(|e| e.to_string())?
        .rules
        .iter()
        .find(|r| r.id == id)
        .cloned()
        .ok_or("unknown rule")?;
    let hwnd = state
        .claimed_window(&rule.id)
        .map(win::hwnd_from)
        .or_else(|| {
            win::enum_top_level().into_iter().find(|h| {
                win::candidate(*h).is_some_and(|info| rule.matches(&info) && state.claimed_rule(info.hwnd).is_none())
            })
        })
        .ok_or("no_window")?;
    state.claim(win::hwnd_raw(hwnd), &rule.id);
    nail_hwnd(state, hwnd).map_err(|e| match e {
        NailError::NotAWindow => "no_window".to_string(),
        NailError::WrongState => "wrong_state".to_string(),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowDto {
    pub hwnd: i64,
    pub title: String,
    pub exe: String,
    pub label: String,
    pub icon: Option<String>,
    pub pinned: bool,
}

/// Visible application windows for the "Clouer !" picker, in Z order.
pub fn list_windows(state: &AppState) -> Vec<WindowDto> {
    win::enum_top_level()
        .into_iter()
        .filter_map(win::candidate)
        .map(|info| WindowDto {
            hwnd: info.hwnd as i64,
            label: label_from_exe(&info.exe),
            icon: win::icon_data_url(&info.exe_path),
            pinned: state.claimed_rule(info.hwnd).is_some(),
            title: info.title,
            exe: info.exe,
        })
        .collect()
}

/// En garde / Au banquet.
pub fn set_paused(app: &AppHandle, state: &Arc<AppState>, paused: bool) {
    if let Ok(mut config) = state.config.lock() {
        config.settings.paused = paused;
    }
    state.save();
    if !paused {
        state.gather();
    }
    crate::tray::refresh(app);
    let _ = app.emit("state-changed", ());
}

pub fn set_autostart(app: &AppHandle, enable: bool) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    let manager = app.autolaunch();
    let result = if enable { manager.enable() } else { manager.disable() };
    result.map_err(|e| e.to_string())?;
    crate::tray::refresh(app);
    let _ = app.emit("state-changed", ());
    Ok(())
}

pub fn autostart_enabled(app: &AppHandle) -> bool {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch().is_enabled().unwrap_or(false)
}

/// Looks for a newer release. `Ok(Some)` when one is available (and remembers it).
pub fn check_update(app: &AppHandle, state: &Arc<AppState>) -> Result<Option<updater::Release>, String> {
    let latest = updater::latest()?;
    let newer = latest.filter(|r| updater::is_newer(r, updater::current_build()));
    if let Ok(mut slot) = state.update.lock() {
        *slot = newer.clone();
    }
    if let Some(release) = &newer {
        crate::info!("update available: build {}", release.build);
        let announce = state
            .config
            .lock()
            .map(|mut c| {
                let fresh = c.settings.announced_build < release.build;
                if fresh {
                    c.settings.announced_build = release.build;
                }
                fresh
            })
            .unwrap_or(false);
        if announce {
            state.save();
            let lang = state.language();
            ui::toast(app, "info", tr(&lang, "toast.update"), tr(&lang, "toast.update.sub"));
        }
    }
    crate::tray::refresh(app);
    let _ = app.emit("state-changed", ());
    Ok(newer)
}

/// Downloads the pending update, swaps the exe and restarts. Blocking: run it off the UI thread.
pub fn install_update(app: &AppHandle, state: &Arc<AppState>) -> Result<(), String> {
    let release = state.update.lock().map_err(|e| e.to_string())?.clone().ok_or("no update pending")?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = exe.parent().ok_or("exe has no folder")?.to_path_buf();
    crate::info!("installing build {}", release.build);
    let progress_app = app.clone();
    let file = updater::download(&release, &dir, move |done, total| {
        let _ = progress_app.emit("update-progress", (done, total));
    })?;
    updater::install(&exe, &file).map_err(|e| format!("cannot replace the exe: {e}"))?;
    relaunch(&exe)?;
    app.exit(0);
    Ok(())
}

fn relaunch(exe: &std::path::Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    std::process::Command::new(exe)
        .arg("--after-update")
        .arg(std::process::id().to_string())
        .current_dir(exe.parent().unwrap_or(std::path::Path::new(".")))
        .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("cannot restart: {e}"))
}

/// Background loop: first check shortly after start, then every 6 hours.
pub fn spawn_update_checker(app: AppHandle, state: Arc<AppState>) {
    std::thread::Builder::new()
        .name("fixatoutix-update".into())
        .spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(25));
            loop {
                let enabled = state.config.lock().map(|c| c.settings.check_updates).unwrap_or(false);
                if enabled {
                    if let Err(e) = check_update(&app, &state) {
                        crate::warn!("update check failed: {e}");
                    }
                }
                std::thread::sleep(std::time::Duration::from_secs(6 * 3600));
            }
        })
        .ok();
}

/// "Edge", then "Edge 2", "Edge 3"...: each nailed window of an app has its own entry.
fn unique_label(rules: &[Rule], base: &str) -> String {
    if !rules.iter().any(|r| r.label == base) {
        return base.to_string();
    }
    (2..)
        .map(|n| format!("{base} {n}"))
        .find(|candidate| !rules.iter().any(|r| &r.label == candidate))
        .unwrap_or_else(|| base.to_string())
}
