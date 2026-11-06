//! `Fixatoutix.exe --selftest <report-file>`: run by the CI on the freshly built exe. It checks that the
//! Win32 side works on a real Windows (manifest DPI awareness, monitors, window enumeration, DWM, theme,
//! icon extraction) and writes a report. Exit code 0 = fine.

use crate::engine::win;
use std::fmt::Write as _;
use windows::Win32::UI::HiDpi::{
    AreDpiAwarenessContextsEqual, GetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};

struct Report {
    text: String,
    failures: i32,
}

impl Report {
    fn check(&mut self, name: &str, ok: bool, detail: String) {
        let _ = writeln!(self.text, "{} {name}: {detail}", if ok { "ok  " } else { "FAIL" });
        if !ok {
            self.failures += 1;
        }
    }

    fn info(&mut self, line: String) {
        let _ = writeln!(self.text, "info {line}");
    }
}

pub fn run(report_path: Option<&str>) -> i32 {
    let mut r = Report { text: String::new(), failures: 0 };

    r.check("version", true, format!("{} build {}", env!("CARGO_PKG_VERSION"), crate::updater::current_build()));

    let per_monitor_v2 = unsafe {
        AreDpiAwarenessContextsEqual(GetThreadDpiAwarenessContext(), DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)
            .as_bool()
    };
    r.check("manifest: Per-Monitor V2 DPI awareness", per_monitor_v2, per_monitor_v2.to_string());

    let monitors = win::monitors();
    r.check("monitors", !monitors.is_empty(), format!("{} found: {:?}", monitors.len(), monitors.first()));

    let all = win::enum_top_level();
    r.check("EnumWindows", !all.is_empty(), format!("{} top-level windows", all.len()));

    let candidates: Vec<_> = all.iter().copied().filter(|h| win::candidate(*h).is_some()).collect();
    r.info(format!("candidates: {}", candidates.len()));
    for hwnd in candidates.iter().take(3) {
        let bounds = win::frame_bounds(*hwnd);
        r.check("DWM frame bounds", bounds.is_some(), format!("{bounds:?}"));
        if let Some(info) = win::candidate(*hwnd) {
            let icon = win::icon_data_url(&info.exe_path);
            r.info(format!("icon of {}: {} chars", info.exe, icon.map(|i| i.len()).unwrap_or(0)));
        }
    }

    r.info(format!("light taskbar theme: {}", win::system_uses_light_theme()));
    let config_path = crate::config::default_path();
    r.check("config path", !config_path.as_os_str().is_empty(), config_path.display().to_string());

    if let Some(path) = report_path {
        let _ = std::fs::write(path, &r.text);
    }
    r.failures
}
