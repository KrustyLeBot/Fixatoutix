//! The engine: Win32 event hooks feed a single worker thread that debounces events, finds the rule of
//! each window and places it.

pub mod apply;
pub mod hooks;
pub mod win;

use crate::rules::{Mode, Rule};
use crate::state::{AppState, Status};
use apply::{AntiLoop, Outcome};
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use windows::Win32::UI::WindowsAndMessaging::{
    EVENT_OBJECT_CREATE, EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_SHOW, EVENT_SYSTEM_FOREGROUND,
    EVENT_SYSTEM_MINIMIZEEND,
    EVENT_SYSTEM_MOVESIZEEND, EVENT_SYSTEM_MOVESIZESTART,
};

/// Wait before looking at a freshly shown window (its title and class settle a moment after).
const APPEAR_DELAY: Duration = Duration::from_millis(200);
/// Second look for rules that also match on the title.
const APPEAR_RETRY_DELAY: Duration = Duration::from_millis(1300);
const RESTORE_DELAY: Duration = Duration::from_millis(100);
const MOVE_END_DELAY: Duration = Duration::from_millis(150);
/// LOCATIONCHANGE debounce (programmatic moves, lock mode).
const LOCATION_DEBOUNCE: Duration = Duration::from_millis(300);
/// Screens need a moment to settle after a configuration change.
const DISPLAY_SETTLE: Duration = Duration::from_millis(1200);

pub enum Msg {
    Event { event: u32, hwnd: isize },
    DisplayChanged,
    ThemeChanged,
    /// "Rassemblement !": enumerate every window and apply every rule now.
    Gather,
}

#[derive(Clone)]
pub struct Engine {
    tx: Sender<Msg>,
}

impl Engine {
    pub fn gather(&self) {
        let _ = self.tx.send(Msg::Gather);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Appear { retry: bool },
    /// Debounced programmatic move: only the Menhir mode reacts.
    Location,
    /// End of a user drag: only the Menhir mode reacts.
    MoveEnd,
}

struct Pending {
    due: Instant,
    kind: Kind,
}

pub fn start(app: AppHandle, state: Arc<AppState>) -> Engine {
    let (tx, rx) = mpsc::channel::<Msg>();
    hooks::start(tx.clone());
    std::thread::Builder::new()
        .name("fixatoutix-engine".into())
        .spawn(move || worker(app, state, rx))
        .expect("cannot start the engine thread");
    Engine { tx }
}

fn worker(app: AppHandle, state: Arc<AppState>, rx: mpsc::Receiver<Msg>) {
    let mut pending: HashMap<isize, Pending> = HashMap::new();
    let mut dragging: HashSet<isize> = HashSet::new();
    let mut guard = AntiLoop::default();
    let mut display_due: Option<Instant> = None;

    // First pass at startup.
    gather(&app, &state, &mut guard);

    loop {
        let next_due = pending.values().map(|p| p.due).chain(display_due).min();
        let timeout = next_due
            .map(|due| due.saturating_duration_since(Instant::now()))
            .unwrap_or(Duration::from_secs(3600));
        match rx.recv_timeout(timeout) {
            Ok(Msg::Event { event, hwnd }) => {
                on_event(&state, &guard, &mut pending, &mut dragging, event, hwnd);
            }
            Ok(Msg::DisplayChanged) => display_due = Some(Instant::now() + DISPLAY_SETTLE),
            Ok(Msg::ThemeChanged) => crate::tray::refresh(&app),
            Ok(Msg::Gather) => gather(&app, &state, &mut guard),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }

        let now = Instant::now();
        if display_due.is_some_and(|due| due <= now) {
            display_due = None;
            crate::info!("display configuration changed: re-evaluating all rules");
            gather(&app, &state, &mut guard);
        }
        let due: Vec<(isize, Kind)> =
            pending.iter().filter(|(_, p)| p.due <= now).map(|(h, p)| (*h, p.kind)).collect();
        for (hwnd, kind) in due {
            pending.remove(&hwnd);
            if let Some(retry_at) = process(&app, &state, &mut guard, hwnd, kind) {
                pending.insert(hwnd, Pending { due: retry_at, kind: Kind::Appear { retry: true } });
            }
        }
    }
}

fn on_event(
    state: &AppState,
    guard: &AntiLoop,
    pending: &mut HashMap<isize, Pending>,
    dragging: &mut HashSet<isize>,
    event: u32,
    hwnd: isize,
) {
    let now = Instant::now();
    match event {
        EVENT_SYSTEM_FOREGROUND => {
            if win::candidate(win::hwnd_from(hwnd)).is_some() {
                state.foreground.store(hwnd, std::sync::atomic::Ordering::Relaxed);
            }
        }
        EVENT_OBJECT_CREATE | EVENT_OBJECT_SHOW => {
            pending.insert(hwnd, Pending { due: now + APPEAR_DELAY, kind: Kind::Appear { retry: false } });
        }
        EVENT_SYSTEM_MINIMIZEEND => {
            pending.insert(hwnd, Pending { due: now + RESTORE_DELAY, kind: Kind::Appear { retry: true } });
        }
        EVENT_SYSTEM_MOVESIZESTART => {
            dragging.insert(hwnd);
            pending.remove(&hwnd); // never touch a window during the drag
        }
        EVENT_SYSTEM_MOVESIZEEND => {
            dragging.remove(&hwnd);
            pending.insert(hwnd, Pending { due: now + MOVE_END_DELAY, kind: Kind::MoveEnd });
        }
        EVENT_OBJECT_LOCATIONCHANGE => {
            if dragging.contains(&hwnd) || guard.is_self_inflicted(hwnd) || state.paused() {
                return;
            }
            // Cheap pre-filter: this event is very frequent, so only bother when a Menhir exists.
            let has_lock = state
                .config
                .lock()
                .map(|c| c.rules.iter().any(|r| r.enabled && r.mode == Mode::Lock))
                .unwrap_or(false);
            if !has_lock {
                return;
            }
            // A debounce: each new event pushes the deadline back, unless an appearance is waiting.
            let keep = pending.get(&hwnd).is_some_and(|p| matches!(p.kind, Kind::Appear { .. } | Kind::MoveEnd));
            if !keep {
                pending.insert(hwnd, Pending { due: now + LOCATION_DEBOUNCE, kind: Kind::Location });
            }
        }
        _ => {}
    }
}

/// Handles one due window. Returns a time to look again (title-dependent rules whose window is not
/// ready yet).
fn process(app: &AppHandle, state: &AppState, guard: &mut AntiLoop, raw: isize, kind: Kind) -> Option<Instant> {
    if state.paused() {
        return None;
    }
    let hwnd = win::hwnd_from(raw);
    if !win::is_window(hwnd) {
        return None;
    }
    let info = win::candidate(hwnd)?;
    let rules: Vec<Rule> = state.config.lock().ok()?.rules.clone();
    let Some(rule) = state.resolve(&rules, &info) else {
        // The exe is known but the title/class did not match yet: the window may still be setting up.
        let may_match_later = matches!(kind, Kind::Appear { retry: false })
            && rules.iter().any(|r| r.enabled && r.matcher.exe.eq_ignore_ascii_case(&info.exe));
        return may_match_later.then(|| Instant::now() + APPEAR_RETRY_DELAY);
    };
    if matches!(kind, Kind::Location | Kind::MoveEnd) && rule.mode != Mode::Lock {
        return None;
    }
    let outcome = apply::apply(&rule, hwnd, guard, false);
    record(app, state, &rule, &info.title, outcome);
    None
}

fn record(app: &AppHandle, state: &AppState, rule: &Rule, title: &str, outcome: Outcome) {
    let status = match outcome {
        Outcome::Placed => {
            crate::info!("placed '{}' ({}) at {},{} {}x{}", rule.label, title, rule.rect.x, rule.rect.y, rule.rect.w, rule.rect.h);
            Status::Ok
        }
        Outcome::AlreadyOk => Status::Ok,
        Outcome::Skipped => return,
        Outcome::MonitorMissing => return, // reported live by get_state
        Outcome::ResistsAdmin => {
            crate::warn!("'{}' resists: elevated window", rule.label);
            Status::Admin
        }
        Outcome::ResistsError(message) => {
            crate::warn!("'{}' resists: {message}", rule.label);
            Status::Error
        }
        Outcome::GaveUp => Status::GaveUp,
    };
    let changed = state
        .status
        .lock()
        .map(|mut map| map.insert(rule.id.clone(), status) != Some(status))
        .unwrap_or(false);
    if changed {
        let _ = app.emit("state-changed", ());
    }
}

/// Enumerates every window and applies every rule (startup, back from the banquet, display change,
/// "Rassemblement !").
fn gather(app: &AppHandle, state: &AppState, guard: &mut AntiLoop) {
    if state.paused() {
        return;
    }
    let Some(rules) = state.config.lock().ok().map(|c| c.rules.clone()) else { return };
    if rules.iter().all(|r| !r.enabled) {
        return;
    }
    for hwnd in win::enum_top_level() {
        let Some(info) = win::candidate(hwnd) else { continue };
        let Some(rule) = state.resolve(&rules, &info) else { continue };
        let outcome = apply::apply(&rule, hwnd, guard, true);
        record(app, state, &rule, &info.title, outcome);
    }
    let _ = app.emit("state-changed", ());
}
