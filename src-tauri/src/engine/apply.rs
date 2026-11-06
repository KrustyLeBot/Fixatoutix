//! Placing a window on its rule's rectangle, with the anti-loop guard.

use super::win;
use crate::rules::Rule;
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::HWND;

/// Our own `SetWindowPos` raises LOCATIONCHANGE events: ignore them for this long.
const SELF_EVENT_WINDOW: Duration = Duration::from_millis(700);
/// At most this many corrections per window ...
const MAX_CORRECTIONS: usize = 3;
/// ... within this period, then we give up quietly (and log it).
const CORRECTION_PERIOD: Duration = Duration::from_secs(5);
/// Pixels of difference we accept without doing anything.
pub const TOLERANCE: i32 = 2;

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Placed,
    AlreadyOk,
    /// Minimized, maximized (and not forced), or gone: nothing to do and nothing to report.
    Skipped,
    /// The rule's screen is unplugged: never place a window blindly.
    MonitorMissing,
    /// UIPI: elevated window.
    ResistsAdmin,
    ResistsError(String),
    GaveUp,
}

#[derive(Default)]
struct Entry {
    ignore_until: Option<Instant>,
    corrections: VecDeque<Instant>,
    logged: bool,
}

#[derive(Default)]
pub struct AntiLoop {
    entries: HashMap<isize, Entry>,
}

impl AntiLoop {
    /// True for events caused by our own placement of this window a moment ago.
    pub fn is_self_inflicted(&self, hwnd: isize) -> bool {
        self.entries
            .get(&hwnd)
            .and_then(|e| e.ignore_until)
            .is_some_and(|until| Instant::now() < until)
    }

    /// A manual action ("Rassemblement !") starts from a clean slate.
    pub fn reset(&mut self, hwnd: isize) {
        self.entries.remove(&hwnd);
    }

    fn budget_left(&mut self, hwnd: isize) -> bool {
        let now = Instant::now();
        let entry = self.entries.entry(hwnd).or_default();
        while entry.corrections.front().is_some_and(|t| now.duration_since(*t) > CORRECTION_PERIOD) {
            entry.corrections.pop_front();
        }
        if entry.corrections.len() >= MAX_CORRECTIONS {
            if !entry.logged {
                entry.logged = true;
                crate::warn!("window {hwnd:#x} keeps moving: giving up for now");
            }
            return false;
        }
        entry.logged = false;
        true
    }

    fn record(&mut self, hwnd: isize) {
        let now = Instant::now();
        let entry = self.entries.entry(hwnd).or_default();
        entry.corrections.push_back(now);
        entry.ignore_until = Some(now + SELF_EVENT_WINDOW);
        if self.entries.len() > 512 {
            self.entries.retain(|_, e| e.ignore_until.is_some_and(|u| u > now) || !e.corrections.is_empty());
        }
    }
}

/// Is the rule's screen currently connected?
pub fn monitor_present(rule: &Rule) -> bool {
    rule.monitor.device_name.is_empty()
        || win::monitors().iter().any(|m| m.device_name.eq_ignore_ascii_case(&rule.monitor.device_name))
}

/// Places `hwnd` on the rule's rectangle. `manual` bypasses the correction budget.
pub fn apply(rule: &Rule, hwnd: HWND, guard: &mut AntiLoop, manual: bool) -> Outcome {
    let raw = win::hwnd_raw(hwnd);
    if !win::is_window(hwnd) || win::is_minimized(hwnd) {
        return Outcome::Skipped;
    }
    if win::is_maximized(hwnd) {
        if !rule.force_restore {
            return Outcome::Skipped;
        }
        win::restore(hwnd);
    }
    if !monitor_present(rule) {
        return Outcome::MonitorMissing;
    }
    if win::frame_bounds(hwnd).is_some_and(|now| now.close_to(&rule.rect, TOLERANCE)) {
        return Outcome::AlreadyOk;
    }
    if manual {
        guard.reset(raw);
    } else if !guard.budget_left(raw) {
        return Outcome::GaveUp;
    }
    let result = win::place_visible(hwnd, &rule.rect);
    guard.record(raw);
    match result {
        Ok(()) => Outcome::Placed,
        Err(win::MoveError::AccessDenied) => Outcome::ResistsAdmin,
        Err(win::MoveError::Other(message)) => Outcome::ResistsError(message),
    }
}
