//! The two kinds of web windows: the flyout panel and the little toast. Both are created on demand and
//! destroyed when done, so an idle Fixatoutix holds no WebView2 at all (CPU 0 %, a few MB of RAM).

use crate::state::AppState;
use crate::util::url_encode;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::window::{Effect, EffectsBuilder};
use tauri::{
    AppHandle, Manager, PhysicalPosition, PhysicalRect, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
    WindowEvent,
};

pub const PANEL_WIDTH: f64 = 360.0;
pub const PANEL_HEIGHT: f64 = 480.0;
const TOAST_WIDTH: f64 = 380.0;
const TOAST_HEIGHT: f64 = 96.0;
const TOAST_LIFETIME: Duration = Duration::from_millis(3600);
/// A tray click this soon after the flyout closed itself is the click that closed it.
const BLUR_CLICK_GUARD: Duration = Duration::from_millis(350);
/// Ignore focus losses right after opening (Windows sometimes bounces the focus).
const OPEN_GRACE: Duration = Duration::from_millis(400);

/// Where the tray icon is, in physical pixels.
#[derive(Clone, Copy)]
pub struct Anchor {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

pub fn toggle_panel(app: &AppHandle, state: &Arc<AppState>, anchor: Option<Anchor>) {
    if let Some(panel) = app.get_webview_window("panel") {
        let _ = panel.close();
        return;
    }
    let just_closed = state
        .panel_hidden_at
        .lock()
        .ok()
        .and_then(|t| *t)
        .is_some_and(|t| t.elapsed() < BLUR_CLICK_GUARD);
    if !just_closed {
        show_panel(app, state, anchor);
    }
}

pub fn show_panel(app: &AppHandle, state: &Arc<AppState>, anchor: Option<Anchor>) {
    if let Some(panel) = app.get_webview_window("panel") {
        let _ = panel.show();
        let _ = panel.set_focus();
        return;
    }
    let built = WebviewWindowBuilder::new(app, "panel", WebviewUrl::App("index.html".into()))
        .title("Fixatoutix")
        .inner_size(PANEL_WIDTH, PANEL_HEIGHT)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .decorations(false)
        .shadow(true)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .disable_drag_drop_handler() // lets the list reorder by HTML5 drag and drop
        .effects(EffectsBuilder::new().effect(Effect::Mica).build())
        .build();
    let panel = match built {
        Ok(panel) => panel,
        Err(e) => {
            crate::warn!("cannot create the panel: {e}");
            return;
        }
    };
    position_panel(app, &panel, anchor);

    let opened = Instant::now();
    let handle = app.clone();
    let state = state.clone();
    panel.on_window_event(move |event| match event {
        WindowEvent::Focused(false) if opened.elapsed() > OPEN_GRACE => {
            if let Ok(mut at) = state.panel_hidden_at.lock() {
                *at = Some(Instant::now());
            }
            if let Some(panel) = handle.get_webview_window("panel") {
                let _ = panel.close();
            }
        }
        _ => {}
    });
    let _ = panel.show();
    let _ = panel.set_focus();
}

fn work_area_at(app: &AppHandle, x: f64, y: f64) -> Option<(PhysicalRect<i32, u32>, f64)> {
    let monitor = app
        .monitor_from_point(x, y)
        .ok()
        .flatten()
        .or_else(|| app.primary_monitor().ok().flatten())?;
    Some((*monitor.work_area(), monitor.scale_factor()))
}

/// Above the tray icon (below it when the taskbar is at the top), never outside the work area.
fn position_panel(app: &AppHandle, panel: &WebviewWindow, anchor: Option<Anchor>) {
    let (probe_x, probe_y) = anchor.map(|a| (a.x + a.w / 2.0, a.y + a.h / 2.0)).unwrap_or((0.0, 0.0));
    let Some((work, _)) = work_area_at(app, probe_x, probe_y).or_else(|| work_area_at(app, 0.0, 0.0)) else {
        return;
    };
    // Move onto the right screen first so the window takes that screen's DPI, then read its real size.
    let _ = panel.set_position(PhysicalPosition::new(work.position.x + 40, work.position.y + 40));
    let (width, height) = panel
        .outer_size()
        .map(|s| (s.width as f64, s.height as f64))
        .unwrap_or((PANEL_WIDTH, PANEL_HEIGHT));
    let scale = panel.scale_factor().unwrap_or(1.0);
    let margin = 12.0 * scale;
    let (left, top) = (work.position.x as f64, work.position.y as f64);
    let (right, bottom) = (left + work.size.width as f64, top + work.size.height as f64);

    let (mut x, mut y) = match anchor {
        Some(a) => {
            let x = a.x + a.w / 2.0 - width / 2.0;
            let y = if a.y > (top + bottom) / 2.0 { a.y - height - 8.0 * scale } else { a.y + a.h + 8.0 * scale };
            (x, y)
        }
        None => (right - width - margin, bottom - height - margin),
    };
    x = x.clamp(left + margin, (right - width - margin).max(left + margin));
    y = y.clamp(top + margin, (bottom - height - margin).max(top + margin));
    let _ = panel.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
}

static TOAST_COUNTER: AtomicU32 = AtomicU32::new(0);

/// A discreet toast in the bottom-right corner. `kind`: `ok`, `error` or `info`.
pub fn toast(app: &AppHandle, kind: &str, title: &str, sub: &str) {
    let app = app.clone();
    let url = format!("toast.html?k={}&t={}&s={}", url_encode(kind), url_encode(title), url_encode(sub));
    let handle = app.clone();
    let _ = handle.run_on_main_thread(move || {
        for (label, window) in app.webview_windows() {
            if label.starts_with("toast-") {
                let _ = window.close();
            }
        }
        let label = format!("toast-{}", TOAST_COUNTER.fetch_add(1, Ordering::Relaxed));
        let built = WebviewWindowBuilder::new(&app, &label, WebviewUrl::App(url.into()))
            .title("Fixatoutix")
            .inner_size(TOAST_WIDTH, TOAST_HEIGHT)
            .resizable(false)
            .decorations(false)
            .shadow(false)
            .transparent(true)
            .always_on_top(true)
            .skip_taskbar(true)
            .focusable(false)
            .focused(false)
            .visible(false)
            .build();
        let window = match built {
            Ok(window) => window,
            Err(e) => {
                crate::warn!("cannot create a toast: {e}");
                return;
            }
        };
        if let Some((work, _)) = work_area_at(&app, 0.0, 0.0) {
            // Park it on the primary screen first so the size below uses that screen's DPI.
            let _ = window.set_position(PhysicalPosition::new(work.position.x, work.position.y));
            let scale = window.scale_factor().unwrap_or(1.0);
            let (w, h) = window
                .outer_size()
                .map(|s| (s.width as f64, s.height as f64))
                .unwrap_or((TOAST_WIDTH * scale, TOAST_HEIGHT * scale));
            let x = work.position.x as f64 + work.size.width as f64 - w - 8.0 * scale;
            let y = work.position.y as f64 + work.size.height as f64 - h - 8.0 * scale;
            let _ = window.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
        }
        let _ = window.show();
        let closer = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(TOAST_LIFETIME);
            let target = closer.clone();
            let _ = closer.run_on_main_thread(move || {
                if let Some(window) = target.get_webview_window(&label) {
                    let _ = window.close();
                }
            });
        });
    });
}
