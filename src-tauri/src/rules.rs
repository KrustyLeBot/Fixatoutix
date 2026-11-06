//! Rule model and matching. Pure logic: no Win32 here, so it is unit-tested everywhere.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    /// True when every edge is within `tolerance` pixels of `other`.
    pub fn close_to(&self, other: &Rect, tolerance: i32) -> bool {
        (self.x - other.x).abs() <= tolerance
            && (self.y - other.y).abs() <= tolerance
            && (self.w - other.w).abs() <= tolerance
            && (self.h - other.h).abs() <= tolerance
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct MonitorRef {
    pub device_name: String,
    #[serde(default)]
    pub work_area: Option<Rect>,
}

/// `lock` = Menhir (put back after every move), `restore` = Retour au village (only when it reappears).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Lock,
    Restore,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Match {
    pub exe: String,
    #[serde(default)]
    pub class_name: Option<String>,
    #[serde(default)]
    pub title_regex: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    pub id: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    pub label: String,
    #[serde(rename = "match")]
    pub matcher: Match,
    pub rect: Rect,
    pub monitor: MonitorRef,
    #[serde(default)]
    pub mode: Mode,
    /// Restore a maximized window before placing it (default off: maximized windows are left alone).
    #[serde(default)]
    pub force_restore: bool,
    /// Full path of the exe the rule was made from (only used to show its icon).
    #[serde(default)]
    pub exe_path: Option<String>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
}

fn yes() -> bool {
    true
}

/// What the engine knows about a top-level window.
#[derive(Clone, Debug, Default)]
pub struct WinInfo {
    pub hwnd: isize,
    /// File name only, e.g. `Spotify.exe`.
    pub exe: String,
    /// Full path of the executable (used for the icon).
    pub exe_path: String,
    pub class_name: String,
    pub title: String,
}

static REGEX_CACHE: Mutex<Option<HashMap<String, Option<Regex>>>> = Mutex::new(None);

/// Compiles (and caches) a title regex. `None` when the pattern is invalid.
pub fn cached_regex(pattern: &str) -> Option<Regex> {
    let mut guard = REGEX_CACHE.lock().ok()?;
    let cache = guard.get_or_insert_with(HashMap::new);
    cache
        .entry(pattern.to_string())
        .or_insert_with(|| Regex::new(pattern).ok())
        .clone()
}

pub fn is_valid_regex(pattern: &str) -> bool {
    Regex::new(pattern).is_ok()
}

impl Rule {
    /// exe (case-insensitive) + className if set + titleRegex if set.
    pub fn matches(&self, info: &WinInfo) -> bool {
        if !self.matcher.exe.eq_ignore_ascii_case(&info.exe) {
            return false;
        }
        if let Some(class) = self.matcher.class_name.as_deref().filter(|c| !c.is_empty()) {
            if class != info.class_name {
                return false;
            }
        }
        if let Some(pattern) = self.matcher.title_regex.as_deref().filter(|p| !p.is_empty()) {
            match cached_regex(pattern) {
                Some(re) if re.is_match(&info.title) => {}
                _ => return false,
            }
        }
        true
    }
}

/// First enabled rule that matches wins.
pub fn find_rule<'a>(rules: &'a [Rule], info: &WinInfo) -> Option<&'a Rule> {
    rules.iter().find(|r| r.enabled && r.matches(info))
}

/// First rule (enabled or not) that matches: used to decide between "create" and "reclouer".
pub fn find_rule_any<'a>(rules: &'a [Rule], info: &WinInfo) -> Option<&'a Rule> {
    rules.iter().find(|r| r.matches(info))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(exe: &str, class: Option<&str>, title: Option<&str>) -> Rule {
        Rule {
            id: "r".into(),
            enabled: true,
            label: "t".into(),
            matcher: Match {
                exe: exe.into(),
                class_name: class.map(String::from),
                title_regex: title.map(String::from),
            },
            rect: Rect { x: 0, y: 0, w: 960, h: 1040 },
            monitor: MonitorRef::default(),
            mode: Mode::Lock,
            force_restore: false,
            exe_path: None,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    fn win(exe: &str, class: &str, title: &str) -> WinInfo {
        WinInfo { exe: exe.into(), class_name: class.into(), title: title.into(), ..Default::default() }
    }

    #[test]
    fn exe_is_case_insensitive() {
        assert!(rule("Spotify.exe", None, None).matches(&win("spotify.EXE", "x", "y")));
        assert!(!rule("Spotify.exe", None, None).matches(&win("notepad.exe", "x", "y")));
    }

    #[test]
    fn class_and_title_narrow_the_match() {
        let r = rule("Discord.exe", Some("Chrome_WidgetWin_1"), Some("^Discord"));
        assert!(r.matches(&win("Discord.exe", "Chrome_WidgetWin_1", "Discord | #general")));
        assert!(!r.matches(&win("Discord.exe", "Other", "Discord")));
        assert!(!r.matches(&win("Discord.exe", "Chrome_WidgetWin_1", "Settings")));
    }

    #[test]
    fn invalid_regex_never_matches() {
        assert!(!rule("a.exe", None, Some("(")).matches(&win("a.exe", "c", "t")));
    }

    #[test]
    fn empty_optional_fields_are_ignored() {
        assert!(rule("a.exe", Some(""), Some("")).matches(&win("a.exe", "c", "t")));
    }

    #[test]
    fn first_enabled_rule_wins() {
        let mut first = rule("a.exe", None, None);
        first.id = "first".into();
        first.enabled = false;
        let mut second = rule("a.exe", None, None);
        second.id = "second".into();
        let rules = vec![first, second];
        let w = win("a.exe", "c", "t");
        assert_eq!(find_rule(&rules, &w).unwrap().id, "second");
        assert_eq!(find_rule_any(&rules, &w).unwrap().id, "first");
    }

    #[test]
    fn rect_tolerance() {
        let a = Rect { x: 0, y: 0, w: 100, h: 100 };
        assert!(a.close_to(&Rect { x: 2, y: -2, w: 98, h: 102 }, 2));
        assert!(!a.close_to(&Rect { x: 3, y: 0, w: 100, h: 100 }, 2));
    }

    #[test]
    fn rule_json_matches_the_spec_shape() {
        let json = r#"{"id":"1","enabled":true,"label":"Spotify","match":{"exe":"Spotify.exe"},
            "rect":{"x":0,"y":0,"w":960,"h":1040},"monitor":{"deviceName":"\\\\.\\DISPLAY1"},"mode":"restore"}"#;
        let r: Rule = serde_json::from_str(json).unwrap();
        assert_eq!(r.mode, Mode::Restore);
        assert_eq!(r.matcher.exe, "Spotify.exe");
        assert!(!r.force_restore);
    }
}
