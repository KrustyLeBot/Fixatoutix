//! Shared application state (managed by Tauri, also read by the engine threads).

use crate::config::{self, Config};
use crate::engine::Engine;
use crate::updater::Release;
use serde::Serialize;
use std::collections::HashMap;
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

    pub fn last_foreground(&self) -> isize {
        self.foreground.load(Ordering::Relaxed)
    }

    pub fn gather(&self) {
        if let Some(engine) = self.engine.get() {
            engine.gather();
        }
    }
}
