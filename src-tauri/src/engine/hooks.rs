//! `SetWinEventHook` plumbing: one thread owns the hooks and a hidden window, and forwards everything
//! to the engine worker through a channel. The callbacks do nothing but send.

use super::Msg;
use std::ffi::c_void;
use std::sync::mpsc::Sender;
use std::sync::OnceLock;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, RegisterClassExW, TranslateMessage,
    EVENT_OBJECT_CREATE, EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_SHOW, EVENT_SYSTEM_FOREGROUND,
    EVENT_SYSTEM_MINIMIZEEND,
    EVENT_SYSTEM_MOVESIZEEND, EVENT_SYSTEM_MOVESIZESTART, MSG, OBJID_WINDOW, WINDOW_EX_STYLE,
    WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS, WM_DISPLAYCHANGE, WM_DPICHANGED, WM_SETTINGCHANGE,
    WNDCLASSEXW, WS_OVERLAPPED,
};

static TX: OnceLock<Sender<Msg>> = OnceLock::new();

unsafe extern "system" fn on_event(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    id_child: i32,
    _thread: u32,
    _time: u32,
) {
    // Only whole windows, never their children (menus, carets, scrollbars...).
    if id_object != OBJID_WINDOW.0 || id_child != 0 || hwnd.0.is_null() {
        return;
    }
    if let Some(tx) = TX.get() {
        let _ = tx.send(Msg::Event { event, hwnd: hwnd.0 as isize });
    }
}

unsafe extern "system" fn sentinel_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_DISPLAYCHANGE | WM_DPICHANGED => {
            if let Some(tx) = TX.get() {
                let _ = tx.send(Msg::DisplayChanged);
            }
        }
        WM_SETTINGCHANGE => {
            // The light/dark theme switch is announced as "ImmersiveColorSet".
            let theme = lparam.0 != 0
                && unsafe { PCWSTR(lparam.0 as *const u16).to_string() }.is_ok_and(|s| s == "ImmersiveColorSet");
            if theme {
                if let Some(tx) = TX.get() {
                    let _ = tx.send(Msg::ThemeChanged);
                }
            }
        }
        _ => {}
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// Starts the hook thread. The hooks live as long as the process.
pub fn start(tx: Sender<Msg>) {
    let _ = TX.set(tx);
    std::thread::Builder::new()
        .name("fixatoutix-hooks".into())
        .spawn(|| unsafe {
            let flags = WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS;
            // (min, max) pairs: appearance, foreground, restore, drag start/end, programmatic moves.
            for (min, max) in [
                (EVENT_OBJECT_CREATE, EVENT_OBJECT_SHOW),
                (EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND),
                (EVENT_SYSTEM_MINIMIZEEND, EVENT_SYSTEM_MINIMIZEEND),
                (EVENT_SYSTEM_MOVESIZESTART, EVENT_SYSTEM_MOVESIZEEND),
                (EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_LOCATIONCHANGE),
            ] {
                let hook = SetWinEventHook(min, max, None, Some(on_event), 0, 0, flags);
                if hook.0.is_null() {
                    crate::warn!("SetWinEventHook({min:#x}..{max:#x}) failed");
                }
            }

            // Hidden top-level window: receives the broadcasts (display and theme changes).
            if let Ok(module) = GetModuleHandleW(None) {
                let class = w!("FixatoutixSentinel");
                let wc = WNDCLASSEXW {
                    cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                    lpfnWndProc: Some(sentinel_proc),
                    hInstance: module.into(),
                    lpszClassName: class,
                    ..Default::default()
                };
                RegisterClassExW(&wc);
                let _ = CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    class,
                    w!("Fixatoutix"),
                    WS_OVERLAPPED,
                    0,
                    0,
                    0,
                    0,
                    None,
                    None,
                    Some(module.into()),
                    None::<*const c_void>,
                );
            }

            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        })
        .expect("cannot start the hook thread");
}
