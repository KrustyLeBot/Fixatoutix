//! Thin, safe-ish wrappers over the Win32 calls the engine needs.

use crate::rules::{Rect, WinInfo};
use base64::Engine as _;
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::Mutex;
use windows::core::{w, BOOL, PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, RECT};
use windows::Win32::Graphics::Dwm::{
    DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS,
};
use windows::Win32::Graphics::Gdi::{
    DeleteObject, EnumDisplayMonitors, GetDC, GetDIBits, GetMonitorInfoW, GetObjectW, MonitorFromWindow,
    ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, HDC, HMONITOR, MONITORINFO,
    MONITORINFOEXW, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_NORMAL;
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Shell::{SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON};
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyIcon, EnumWindows, GetAncestor, GetClassNameW, GetForegroundWindow, GetIconInfo, GetWindow,
    GetWindowLongW, GetWindowRect, GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindow,
    IsWindowVisible, IsZoomed, SetWindowPos, ShowWindow, GA_ROOT, GWL_EXSTYLE, GW_OWNER, ICONINFO,
    SET_WINDOW_POS_FLAGS, SWP_NOACTIVATE, SWP_NOOWNERZORDER, SWP_NOZORDER, SW_RESTORE, WS_EX_TOOLWINDOW,
};

#[derive(Clone, Debug)]
pub struct MonitorInfo {
    pub device_name: String,
    pub work: Rect,
}

fn to_rect(r: &RECT) -> Rect {
    Rect { x: r.left, y: r.top, w: r.right - r.left, h: r.bottom - r.top }
}

fn wide_to_string(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

pub fn hwnd_from(raw: isize) -> HWND {
    HWND(raw as *mut c_void)
}

pub fn hwnd_raw(hwnd: HWND) -> isize {
    hwnd.0 as isize
}

pub fn is_window(hwnd: HWND) -> bool {
    unsafe { IsWindow(Some(hwnd)).as_bool() }
}

pub fn is_minimized(hwnd: HWND) -> bool {
    unsafe { IsIconic(hwnd).as_bool() }
}

pub fn is_maximized(hwnd: HWND) -> bool {
    unsafe { IsZoomed(hwnd).as_bool() }
}

pub fn restore(hwnd: HWND) {
    unsafe {
        let _ = ShowWindow(hwnd, SW_RESTORE);
    }
}

pub fn foreground() -> Option<HWND> {
    let hwnd = unsafe { GetForegroundWindow() };
    (!hwnd.0.is_null()).then_some(hwnd)
}

/// All top-level windows, in Z order (top first).
pub fn enum_top_level() -> Vec<HWND> {
    unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let list = unsafe { &mut *(lparam.0 as *mut Vec<HWND>) };
        list.push(hwnd);
        BOOL(1)
    }
    let mut list: Vec<HWND> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(collect), LPARAM(&mut list as *mut Vec<HWND> as isize));
    }
    list
}

fn title_of(hwnd: HWND) -> String {
    let mut buf = [0u16; 512];
    let n = unsafe { GetWindowTextW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn class_of(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { GetClassNameW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn exe_of(pid: u32) -> Option<String> {
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_FORMAT(0), PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(process);
        ok.ok()?;
        Some(String::from_utf16_lossy(&buf[..len as usize]))
    }
}

fn is_cloaked(hwnd: HWND) -> bool {
    let mut cloaked: u32 = 0;
    unsafe {
        DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, &mut cloaked as *mut u32 as *mut c_void, 4).is_ok() && cloaked != 0
    }
}

const SHELL_CLASSES: [&str; 5] =
    ["Shell_TrayWnd", "Shell_SecondaryTrayWnd", "Progman", "WorkerW", "Windows.UI.Core.CoreWindow"];

/// A real application window worth listing or placing: visible, titled, not cloaked, not a tool
/// window, not owned, not part of the shell, not ours. Returns its identity.
pub fn candidate(hwnd: HWND) -> Option<WinInfo> {
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() || is_cloaked(hwnd) {
            return None;
        }
        if GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW.0 != 0 {
            return None;
        }
        if GetAncestor(hwnd, GA_ROOT) != hwnd {
            return None;
        }
        if GetWindow(hwnd, GW_OWNER).is_ok_and(|owner| !owner.0.is_null()) {
            return None;
        }
    }
    let title = title_of(hwnd);
    if title.is_empty() {
        return None;
    }
    let class_name = class_of(hwnd);
    if SHELL_CLASSES.contains(&class_name.as_str()) {
        return None;
    }
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid == 0 || pid == std::process::id() {
        return None;
    }
    let exe_path = exe_of(pid)?;
    let exe = exe_path.rsplit(['\\', '/']).next().unwrap_or(&exe_path).to_string();
    Some(WinInfo { hwnd: hwnd_raw(hwnd), exe, exe_path, class_name, title })
}

/// Visible bounds (DWM), without the invisible Windows 11 resize borders.
pub fn frame_bounds(hwnd: HWND) -> Option<Rect> {
    let mut r = RECT::default();
    unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut r as *mut RECT as *mut c_void,
            std::mem::size_of::<RECT>() as u32,
        )
        .ok()?;
    }
    Some(to_rect(&r))
}

pub fn window_rect(hwnd: HWND) -> Option<Rect> {
    let mut r = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut r).ok()? };
    Some(to_rect(&r))
}

fn monitor_info(hmon: HMONITOR) -> Option<MonitorInfo> {
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    unsafe {
        if !GetMonitorInfoW(hmon, &mut info as *mut MONITORINFOEXW as *mut MONITORINFO).as_bool() {
            return None;
        }
    }
    Some(MonitorInfo {
        device_name: wide_to_string(&info.szDevice),
        work: to_rect(&info.monitorInfo.rcWork),
    })
}

pub fn monitor_of(hwnd: HWND) -> Option<MonitorInfo> {
    let hmon = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    monitor_info(hmon)
}

pub fn monitors() -> Vec<MonitorInfo> {
    unsafe extern "system" fn collect(hmon: HMONITOR, _: HDC, _: *mut RECT, lparam: LPARAM) -> BOOL {
        let list = unsafe { &mut *(lparam.0 as *mut Vec<MonitorInfo>) };
        if let Some(info) = monitor_info(hmon) {
            list.push(info);
        }
        BOOL(1)
    }
    let mut list: Vec<MonitorInfo> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(collect), LPARAM(&mut list as *mut Vec<MonitorInfo> as isize));
    }
    list
}

pub enum MoveError {
    /// UIPI: the target runs elevated and we do not.
    AccessDenied,
    Other(String),
}

/// Moves/resizes the window so that its VISIBLE bounds equal `target`, compensating for the invisible
/// borders (difference between `GetWindowRect` and the DWM frame bounds).
pub fn place_visible(hwnd: HWND, target: &Rect) -> Result<(), MoveError> {
    let flags: SET_WINDOW_POS_FLAGS = SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOOWNERZORDER;
    for _ in 0..2 {
        let (outer, visible) = match (window_rect(hwnd), frame_bounds(hwnd)) {
            (Some(o), Some(v)) => (o, v),
            _ => return Err(MoveError::Other("window bounds unavailable".into())),
        };
        let left = visible.x - outer.x;
        let top = visible.y - outer.y;
        let right = (outer.x + outer.w) - (visible.x + visible.w);
        let bottom = (outer.y + outer.h) - (visible.y + visible.h);
        let result = unsafe {
            SetWindowPos(
                hwnd,
                None,
                target.x - left,
                target.y - top,
                target.w + left + right,
                target.h + top + bottom,
                flags,
            )
        };
        if let Err(e) = result {
            return Err(if e.code().0 as u32 == 0x8007_0005 {
                MoveError::AccessDenied
            } else {
                MoveError::Other(e.message())
            });
        }
        // A second pass fixes windows that were resized by a DPI change when they crossed monitors.
        if frame_bounds(hwnd).is_some_and(|now| now.close_to(target, 2)) {
            break;
        }
    }
    Ok(())
}

/// True when the user runs the light Windows theme for the taskbar (tray icon must be dark).
pub fn system_uses_light_theme() -> bool {
    let mut value: u32 = 1;
    let mut size = std::mem::size_of::<u32>() as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("SystemUsesLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut value as *mut u32 as *mut c_void),
            Some(&mut size),
        )
    };
    status.0 != 0 || value != 0
}

static ICON_CACHE: Mutex<Option<HashMap<String, Option<String>>>> = Mutex::new(None);

/// The exe's icon as a `data:image/png;base64,...` URL (cached by exe path).
pub fn icon_data_url(exe_path: &str) -> Option<String> {
    if let Ok(mut guard) = ICON_CACHE.lock() {
        let cache = guard.get_or_insert_with(HashMap::new);
        if let Some(hit) = cache.get(exe_path) {
            return hit.clone();
        }
    }
    let icon = extract_icon_png(exe_path).map(|png| {
        format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png))
    });
    if let Ok(mut guard) = ICON_CACHE.lock() {
        guard.get_or_insert_with(HashMap::new).insert(exe_path.to_string(), icon.clone());
    }
    icon
}

fn extract_icon_png(exe_path: &str) -> Option<Vec<u8>> {
    let wide: Vec<u16> = exe_path.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let mut shfi = SHFILEINFOW::default();
        let got = SHGetFileInfoW(
            PCWSTR(wide.as_ptr()),
            FILE_ATTRIBUTE_NORMAL,
            Some(&mut shfi),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_LARGEICON,
        );
        if got == 0 || shfi.hIcon.is_invalid() {
            return None;
        }
        let mut info = ICONINFO::default();
        let mut result = None;
        if GetIconInfo(shfi.hIcon, &mut info).is_ok() {
            if !info.hbmColor.is_invalid() {
                let mut bmp = BITMAP::default();
                GetObjectW(
                    info.hbmColor.into(),
                    std::mem::size_of::<BITMAP>() as i32,
                    Some(&mut bmp as *mut BITMAP as *mut c_void),
                );
                let (width, height) = (bmp.bmWidth, bmp.bmHeight);
                if width > 0 && height > 0 {
                    let mut header = BITMAPINFOHEADER::default();
                    header.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
                    header.biWidth = width;
                    header.biHeight = -height; // top-down
                    header.biPlanes = 1;
                    header.biBitCount = 32;
                    let mut bmi = BITMAPINFO { bmiHeader: header, ..Default::default() };
                    let mut pixels = vec![0u8; (width * height * 4) as usize];
                    let hdc = GetDC(None);
                    let lines = GetDIBits(
                        hdc,
                        info.hbmColor,
                        0,
                        height as u32,
                        Some(pixels.as_mut_ptr() as *mut c_void),
                        &mut bmi,
                        DIB_RGB_COLORS,
                    );
                    ReleaseDC(None, hdc);
                    if lines > 0 {
                        result = encode_png(&mut pixels, width as u32, height as u32);
                    }
                }
            }
            if !info.hbmColor.is_invalid() {
                let _ = DeleteObject(info.hbmColor.into());
            }
            if !info.hbmMask.is_invalid() {
                let _ = DeleteObject(info.hbmMask.into());
            }
        }
        let _ = DestroyIcon(shfi.hIcon);
        result
    }
}

/// BGRA (as GDI gives it) to a PNG. Icons without an alpha channel are made opaque.
fn encode_png(bgra: &mut [u8], width: u32, height: u32) -> Option<Vec<u8>> {
    let has_alpha = bgra.chunks_exact(4).any(|px| px[3] != 0);
    for px in bgra.chunks_exact_mut(4) {
        px.swap(0, 2);
        if !has_alpha {
            px[3] = 255;
        }
    }
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(bgra).ok()?;
    }
    Some(out)
}
