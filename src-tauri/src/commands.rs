//! Commands exposed to the panel (`window.__TAURI__.core.invoke`).

use crate::actions::{self, WindowDto};
use crate::config::Settings;
use crate::engine::win;
use crate::i18n::tr;
use crate::rules::{is_valid_regex, Mode, Rule};
use crate::state::{AppState, Status};
use crate::updater::{self, Release};
use crate::{tray, ui};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

type S<'a> = State<'a, Arc<AppState>>;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleDto {
    #[serde(flatten)]
    rule: Rule,
    status: Status,
    /// False when the rule's screen is unplugged: "Elle résiste… écran absent".
    monitor_present: bool,
    icon: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StateDto {
    settings: Settings,
    rules: Vec<RuleDto>,
    update: Option<Release>,
    version: &'static str,
    build: u64,
    autostart: bool,
    portable: bool,
    config_path: String,
    repo_url: &'static str,
}

#[tauri::command]
pub fn get_state(app: AppHandle, state: S) -> StateDto {
    let config = state.config.lock().map(|c| c.clone()).unwrap_or_default();
    let status = state.status.lock().map(|s| s.clone()).unwrap_or_default();
    let connected = win::monitors();
    let rules = config
        .rules
        .into_iter()
        .map(|rule| {
            let monitor_present = rule.monitor.device_name.is_empty()
                || connected.iter().any(|m| m.device_name.eq_ignore_ascii_case(&rule.monitor.device_name));
            RuleDto {
                status: status.get(&rule.id).copied().unwrap_or(Status::Idle),
                icon: rule.exe_path.as_deref().and_then(win::icon_data_url),
                monitor_present,
                rule,
            }
        })
        .collect();
    StateDto {
        settings: config.settings,
        rules,
        update: state.update.lock().ok().and_then(|u| u.clone()),
        version: env!("CARGO_PKG_VERSION"),
        build: updater::current_build(),
        autostart: actions::autostart_enabled(&app),
        portable: state.portable,
        config_path: state.path.display().to_string(),
        repo_url: updater::REPO_URL,
    }
}

#[tauri::command]
pub fn list_windows(state: S) -> Vec<WindowDto> {
    actions::list_windows(&state)
}

#[tauri::command]
pub fn nail_window(app: AppHandle, state: S, hwnd: i64) {
    let lang = state.language();
    let outcome = actions::nail_hwnd(&state, win::hwnd_from(hwnd as isize));
    actions::announce(&app, &state, &lang, outcome);
}

fn with_rule<R>(state: &AppState, id: &str, edit: impl FnOnce(&mut Rule) -> R) -> Result<R, String> {
    let mut config = state.config.lock().map_err(|e| e.to_string())?;
    let rule = config.rules.iter_mut().find(|r| r.id == id).ok_or("unknown rule")?;
    let result = edit(rule);
    rule.updated_at = crate::util::now_iso();
    Ok(result)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RulePatch {
    label: Option<String>,
    enabled: Option<bool>,
    mode: Option<Mode>,
    force_restore: Option<bool>,
}

#[tauri::command]
pub fn update_rule(app: AppHandle, state: S, id: String, patch: RulePatch) -> Result<(), String> {
    let enabled_now = with_rule(&state, &id, |rule| {
        if let Some(label) = patch.label.as_ref().map(|l| l.trim()).filter(|l| !l.is_empty()) {
            rule.label = label.to_string();
        }
        if let Some(mode) = patch.mode {
            rule.mode = mode;
        }
        if let Some(force) = patch.force_restore {
            rule.force_restore = force;
        }
        match patch.enabled {
            Some(enabled) => {
                let turned_on = enabled && !rule.enabled;
                rule.enabled = enabled;
                turned_on
            }
            None => false,
        }
    })?;
    state.save();
    if enabled_now || patch.mode.is_some() || patch.force_restore == Some(true) {
        state.gather();
    }
    let _ = app.emit("state-changed", ());
    Ok(())
}

/// "Éditer le match": exe is mandatory, class and title regex optional.
#[tauri::command]
pub fn set_match(
    app: AppHandle,
    state: S,
    id: String,
    exe: String,
    class_name: Option<String>,
    title_regex: Option<String>,
) -> Result<(), String> {
    let exe = exe.trim().to_string();
    if exe.is_empty() {
        return Err("exe".into());
    }
    let title_regex = title_regex.map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
    if title_regex.as_deref().is_some_and(|t| !is_valid_regex(t)) {
        return Err("regex".into());
    }
    let class_name = class_name.map(|c| c.trim().to_string()).filter(|c| !c.is_empty());
    with_rule(&state, &id, |rule| {
        rule.matcher.exe = exe;
        rule.matcher.class_name = class_name;
        rule.matcher.title_regex = title_regex;
    })?;
    state.save();
    state.gather();
    let _ = app.emit("state-changed", ());
    Ok(())
}

#[tauri::command]
pub fn delete_rule(app: AppHandle, state: S, id: String) {
    if let Ok(mut config) = state.config.lock() {
        config.rules.retain(|r| r.id != id);
    }
    if let Ok(mut status) = state.status.lock() {
        status.remove(&id);
    }
    state.save();
    let _ = app.emit("state-changed", ());
}

/// Drag and drop: the first rule that matches wins, so the order matters.
#[tauri::command]
pub fn reorder_rules(app: AppHandle, state: S, ids: Vec<String>) {
    if let Ok(mut config) = state.config.lock() {
        let mut rest = std::mem::take(&mut config.rules);
        let mut ordered = Vec::with_capacity(rest.len());
        for id in &ids {
            if let Some(i) = rest.iter().position(|r| &r.id == id) {
                ordered.push(rest.remove(i));
            }
        }
        ordered.extend(rest);
        config.rules = ordered;
    }
    state.save();
    let _ = app.emit("state-changed", ());
}

/// "Reclouer ici": takes the rectangle of the rule's open window. Returns an error code for the panel.
#[tauri::command]
pub fn reclou_rule(app: AppHandle, state: S, id: String) -> Result<(), String> {
    let lang = state.language();
    match actions::reclou_rule(&state, &id) {
        Ok(nailed) => {
            actions::announce(&app, &state, &lang, Ok(nailed));
            Ok(())
        }
        Err(code) => {
            let key = if code == "wrong_state" { "toast.resists.state" } else { "toast.resists.nowindow" };
            ui::toast(&app, "error", tr(&lang, "toast.resists"), tr(&lang, key));
            Err(code)
        }
    }
}

#[tauri::command]
pub fn set_paused(app: AppHandle, state: S, paused: bool) {
    actions::set_paused(&app, &state, paused);
}

/// "Rassemblement !"
#[tauri::command]
pub fn gather(app: AppHandle, state: S) {
    state.gather();
    let lang = state.language();
    ui::toast(&app, "ok", tr(&lang, "toast.gathered"), tr(&lang, "toast.gathered.sub"));
}

#[tauri::command]
pub fn set_language(app: AppHandle, state: S, language: String) {
    let language = if language == "en" { "en" } else { "fr" };
    if let Ok(mut config) = state.config.lock() {
        config.settings.language = language.into();
    }
    state.save();
    tray::refresh(&app);
    let _ = app.emit("state-changed", ());
}

#[tauri::command]
pub fn set_check_updates(app: AppHandle, state: S, enabled: bool) {
    if let Ok(mut config) = state.config.lock() {
        config.settings.check_updates = enabled;
    }
    state.save();
    let _ = app.emit("state-changed", ());
}

#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> Result<(), String> {
    actions::set_autostart(&app, enabled)
}

/// Registers the global "Clouer" shortcut. Replaces any previous registration.
pub fn register_shortcut(app: &AppHandle, state: &Arc<AppState>) -> Result<(), String> {
    let text = state.config.lock().map(|c| c.settings.shortcut.clone()).map_err(|e| e.to_string())?;
    let shortcut: Shortcut = text.parse().map_err(|e| format!("{e}"))?;
    let global = app.global_shortcut();
    let _ = global.unregister_all();
    let (handle, shared) = (app.clone(), state.clone());
    global
        .on_shortcut(shortcut, move |_app, _shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                actions::nail_foreground(&handle, &shared);
            }
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_shortcut(app: AppHandle, state: S, shortcut: String) -> Result<(), String> {
    let previous = state.config.lock().map(|c| c.settings.shortcut.clone()).map_err(|e| e.to_string())?;
    if let Ok(mut config) = state.config.lock() {
        config.settings.shortcut = shortcut;
    }
    if let Err(e) = register_shortcut(&app, &state) {
        if let Ok(mut config) = state.config.lock() {
            config.settings.shortcut = previous;
        }
        let _ = register_shortcut(&app, &state);
        return Err(e);
    }
    state.save();
    tray::refresh(&app);
    let _ = app.emit("state-changed", ());
    Ok(())
}

#[tauri::command]
pub fn open_config_dir(state: S) {
    let _ = std::process::Command::new("explorer.exe")
        .arg("/select,")
        .arg(&state.path)
        .spawn();
}

#[tauri::command]
pub fn open_repo() {
    let _ = std::process::Command::new("explorer.exe").arg(updater::REPO_URL).spawn();
}

/// Manual "check now". `None` when up to date.
#[tauri::command]
pub async fn check_update(app: AppHandle, state: S<'_>) -> Result<Option<Release>, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || actions::check_update(&app, &state))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn install_update(app: AppHandle, state: S<'_>) -> Result<(), String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || actions::install_update(&app, &state))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn close_panel(app: AppHandle) {
    if let Some(panel) = app.get_webview_window("panel") {
        let _ = panel.close();
    }
}
