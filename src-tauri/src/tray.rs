//! Notification-area icon: two states (on guard / at the banquet), light and dark variants, and the
//! native right-click menu.

use crate::engine::win;
use crate::i18n::tr;
use crate::state::AppState;
use crate::{actions, ui};
use std::sync::Arc;
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Position, Size};

const TRAY_ID: &str = "main";

const GUARD_DARK_TASKBAR: &[u8] = include_bytes!("../icons/tray-guard-dark.png");
const GUARD_LIGHT_TASKBAR: &[u8] = include_bytes!("../icons/tray-guard-light.png");
const BANQUET_DARK_TASKBAR: &[u8] = include_bytes!("../icons/tray-banquet-dark.png");
const BANQUET_LIGHT_TASKBAR: &[u8] = include_bytes!("../icons/tray-banquet-light.png");

fn icon_for(paused: bool) -> Option<Image<'static>> {
    let light = win::system_uses_light_theme();
    let bytes = match (paused, light) {
        (false, true) => GUARD_LIGHT_TASKBAR,
        (false, false) => GUARD_DARK_TASKBAR,
        (true, true) => BANQUET_LIGHT_TASKBAR,
        (true, false) => BANQUET_DARK_TASKBAR,
    };
    Image::from_bytes(bytes).ok()
}

fn build_menu(app: &AppHandle, state: &AppState) -> tauri::Result<Menu<tauri::Wry>> {
    let lang = state.language();
    let (paused, shortcut) = state
        .config
        .lock()
        .map(|c| (c.settings.paused, c.settings.shortcut.clone()))
        .unwrap_or((false, String::new()));
    let has_update = state.update.lock().map(|u| u.is_some()).unwrap_or(false);

    let menu = Menu::new(app)?;
    menu.append(&MenuItem::with_id(app, "open", tr(&lang, "tray.open"), true, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "nail", tr(&lang, "tray.nail"), true, Some(shortcut.as_str()))?)?;
    menu.append(&MenuItem::with_id(app, "gather", tr(&lang, "tray.gather"), true, None::<&str>)?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    let guard_label = if paused { tr(&lang, "tray.banquet") } else { tr(&lang, "tray.guard") };
    menu.append(&CheckMenuItem::with_id(app, "guard", guard_label, true, !paused, None::<&str>)?)?;
    menu.append(&CheckMenuItem::with_id(
        app,
        "autostart",
        tr(&lang, "tray.autostart"),
        true,
        actions::autostart_enabled(app),
        None::<&str>,
    )?)?;
    if has_update {
        menu.append(&PredefinedMenuItem::separator(app)?)?;
        menu.append(&MenuItem::with_id(app, "update", tr(&lang, "tray.update"), true, None::<&str>)?)?;
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(app, "quit", tr(&lang, "tray.quit"), true, None::<&str>)?)?;
    Ok(menu)
}

pub fn setup(app: &AppHandle, state: &Arc<AppState>) -> tauri::Result<()> {
    let click_state = state.clone();
    let menu_state = state.clone();
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&build_menu(app, state)?)
        .show_menu_on_left_click(false)
        .tooltip(tr(&state.language(), "tip.guard"))
        .on_tray_icon_event(move |tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, rect, .. } =
                event
            {
                let (x, y) = match rect.position {
                    Position::Physical(p) => (p.x as f64, p.y as f64),
                    Position::Logical(p) => (p.x, p.y),
                };
                let (w, h) = match rect.size {
                    Size::Physical(s) => (s.width as f64, s.height as f64),
                    Size::Logical(s) => (s.width, s.height),
                };
                ui::toggle_panel(tray.app_handle(), &click_state, Some(ui::Anchor { x, y, w, h }));
            }
        })
        .on_menu_event(move |app, event| on_menu(app, &menu_state, event.id().as_ref()));
    if let Some(icon) = icon_for(state.paused()) {
        builder = builder.icon(icon);
    }
    builder.build(app)?;
    Ok(())
}

fn on_menu(app: &AppHandle, state: &Arc<AppState>, id: &str) {
    match id {
        "open" => ui::show_panel(app, state, None),
        "nail" => actions::nail_foreground(app, state),
        "gather" => {
            state.gather();
            let lang = state.language();
            ui::toast(app, "ok", tr(&lang, "toast.gathered"), tr(&lang, "toast.gathered.sub"));
        }
        "guard" => actions::set_paused(app, state, !state.paused()),
        "autostart" => {
            let want = !actions::autostart_enabled(app);
            if actions::set_autostart(app, want).is_err() {
                let lang = state.language();
                ui::toast(app, "error", tr(&lang, "toast.resists"), tr(&lang, "toast.autostart.fail"));
                refresh(app);
            }
        }
        "update" => {
            ui::show_panel(app, state, None);
            let (app, state) = (app.clone(), state.clone());
            std::thread::spawn(move || {
                if let Err(e) = actions::install_update(&app, &state) {
                    crate::warn!("update failed: {e}");
                }
            });
        }
        "quit" => app.exit(0),
        _ => {}
    }
}

/// Re-reads the state: icon (guard/banquet, light/dark), tooltip and menu.
pub fn refresh(app: &AppHandle) {
    let Some(state) = app.try_state::<Arc<AppState>>() else { return };
    let Some(tray) = app.tray_by_id(TRAY_ID) else { return };
    let paused = state.paused();
    if let Some(icon) = icon_for(paused) {
        let _ = tray.set_icon(Some(icon));
    }
    let lang = state.language();
    let _ = tray.set_tooltip(Some(tr(&lang, if paused { "tip.banquet" } else { "tip.guard" })));
    if let Ok(menu) = build_menu(app, &state) {
        let _ = tray.set_menu(Some(menu));
    }
}
