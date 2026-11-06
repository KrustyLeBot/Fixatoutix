//! Shared application state (managed by Tauri, also read by the engine threads).

use crate::config::{self, Config};
use crate::engine::Engine;
use crate::engine::win;
use crate::rules::{Rect, Rule, WinInfo};
use crate::updater::Release;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// What happened the last time the engine tried to place a rule's window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// Nothing to report (no window seen yet).
    Idle,
    Ok,
    /// The window runs elevated: "Elle résiste… lance Fixatoutix en admin".
    Admin,
    Error,
    /// Too many corrections in a few seconds: the window fights back, we stopped.
    GaveUp,
}

pub struct AppState {
    pub config: Mutex<Config>,
    pub path: PathBuf,
    pub portable: bool,
    pub status: Mutex<HashMap<String, Status>>,
    pub update: Mutex<Option<Release>>,
    pub engine: OnceLock<Engine>,
    /// When the flyout last closed because it lost focus (to swallow the tray click that caused it).
    pub panel_hidden_at: Mutex<Option<Instant>>,
    /// The last application window that was in front (the tray menu steals the focus from it).
    pub foreground: AtomicIsize,
    /// Which rule each live window has claimed (hwnd -> rule id): one rule is one window, so several
    /// windows of the same app (Edge...) can each have their own position.
    pub claims: Mutex<HashMap<isize, String>>,
}

impl AppState {
    pub fn new(config: Config, path: PathBuf) -> Self {
        let portable = path.file_name().is_some_and(|n| n == "fixatoutix.json");
        AppState {
            config: Mutex::new(config),
            path,
            portable,
            status: Mutex::new(HashMap::new()),
            update: Mutex::new(None),
            engine: OnceLock::new(),
            panel_hidden_at: Mutex::new(None),
            foreground: AtomicIsize::new(0),
            claims: Mutex::new(HashMap::new()),
        }
    }

    pub fn save(&self) {
        let snapshot = self.config.lock().map(|c| c.clone()).ok();
        if let Some(config) = snapshot {
            if let Err(e) = config::save(&self.path, &config) {
                crate::warn!("cannot save {}: {e}", self.path.display());
            }
        }
    }

    pub fn language(&self) -> String {
        self.config.lock().map(|c| c.settings.language.clone()).unwrap_or_else(|_| "fr".into())
    }

    pub fn paused(&self) -> bool {
        self.config.lock().map(|c| c.settings.paused).unwrap_or(false)
    }

    /// The rule of this window: the one it already claimed, else the free matching rule whose rectangle is
    /// closest to where the window is now (apps reopen their windows near where they were). A window
    /// with no free rule left is not ours to place.
    pub fn resolve(&self, rules: &[Rule], info: &WinInfo) -> Option<Rule> {
        let mut claims = self.claims.lock().ok()?;
        claims.retain(|h, _| win::is_window(win::hwnd_from(*h)));
        if let Some(id) = claims.get(&info.hwnd).cloned() {
            if let Some(rule) = rules.iter().find(|r| r.id == id && r.enabled && r.matches(info)) {
                return Some(rule.clone());
            }
            claims.remove(&info.hwnd);
        }
        let here = win::frame_bounds(win::hwnd_from(info.hwnd));
        let best = {
            let taken: HashSet<&String> = claims.values().collect();
            rules
                .iter()
                .filter(|r| r.enabled && r.matches(info) && !taken.contains(&r.id))
                .min_by_key(|r| distance(here.as_ref(), &r.rect))
                .cloned()
        }?;
        claims.insert(info.hwnd, best.id.clone());
        Some(best)
    }

    pub fn claim(&self, hwnd: isize, rule_id: &str) {
        if let Ok(mut claims) = self.claims.lock() {
            claims.retain(|_, id| id != rule_id);
            claims.insert(hwnd, rule_id.to_string());
        }
    }

    pub fn claimed_rule(&self, hwnd: isize) -> Option<String> {
        self.claims.lock().ok()?.get(&hwnd).cloned()
    }

    pub fn claimed_window(&self, rule_id: &str) -> Option<isize> {
        let claims = self.claims.lock().ok()?;
        claims
            .iter()
            .find(|(h, id)| id.as_str() == rule_id && win::is_window(win::hwnd_from(**h)))
            .map(|(h, _)| *h)
    }

    pub fn release_rule(&self, rule_id: &str) {
        if let Ok(mut claims) = self.claims.lock() {
            claims.retain(|_, id| id != rule_id);
        }
    }

    pub fn last_foreground(&self) -> isize {
        self.foreground.load(Ordering::Relaxed)
    }

    pub fn gather(&self) {
        if let Some(engine) = self.engine.get() {
            engine.gather();
        }
    }
}

fn distance(here: Option<&Rect>, target: &Rect) -> i64 {
    match here {
        Some(h) => {
            ((h.x - target.x).abs() + (h.y - target.y).abs() + (h.w - target.w).abs() + (h.h - target.h).abs()) as i64
        }
        None => 0,
    }
}
