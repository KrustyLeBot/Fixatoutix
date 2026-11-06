//! Settings + rules on disk. Portable (`fixatoutix.json` next to the exe) or `%APPDATA%\Fixatoutix\config.json`.

use crate::rules::Rule;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const SCHEMA_VERSION: u32 = 1;
pub const DEFAULT_SHORTCUT: &str = "Ctrl+Alt+F";

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub shortcut: String,
    /// `fr` (default) or `en`.
    pub language: String,
    /// Au banquet: nothing is applied.
    pub paused: bool,
    /// Look for a newer release on GitHub (the only network access of the app).
    pub check_updates: bool,
    /// Build number of the last release we announced, so the toast is shown once per build.
    pub announced_build: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            shortcut: DEFAULT_SHORTCUT.into(),
            language: "fr".into(),
            paused: false,
            check_updates: true,
            announced_build: 0,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    pub schema: u32,
    pub settings: Settings,
    pub rules: Vec<Rule>,
}

impl Default for Config {
    fn default() -> Self {
        Config { schema: SCHEMA_VERSION, settings: Settings::default(), rules: Vec::new() }
    }
}

pub struct Loaded {
    pub config: Config,
    /// The file was unreadable: it was copied to `.bak` and we started from scratch.
    pub recovered: bool,
}

/// `fixatoutix.json` next to the exe wins (portable mode), otherwise `%APPDATA%\Fixatoutix\config.json`.
pub fn resolve_path(exe: Option<&Path>, appdata: Option<&Path>) -> PathBuf {
    if let Some(dir) = exe.and_then(Path::parent) {
        let portable = dir.join("fixatoutix.json");
        if portable.is_file() {
            return portable;
        }
    }
    appdata
        .map(|p| p.join("Fixatoutix").join("config.json"))
        .or_else(|| exe.and_then(Path::parent).map(|d| d.join("fixatoutix.json")))
        .unwrap_or_else(|| PathBuf::from("fixatoutix.json"))
}

pub fn default_path() -> PathBuf {
    let exe = std::env::current_exe().ok();
    let appdata = std::env::var_os("APPDATA").map(PathBuf::from);
    resolve_path(exe.as_deref(), appdata.as_deref())
}

pub fn load(path: &Path) -> Loaded {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(_) => return Loaded { config: Config::default(), recovered: false },
    };
    match serde_json::from_str::<Config>(&text) {
        Ok(config) => Loaded { config, recovered: false },
        Err(_) => {
            let _ = fs::copy(path, path.with_extension("json.bak"));
            Loaded { config: Config::default(), recovered: true }
        }
    }
}

/// Atomic write: temporary file in the same folder, then rename over the target.
pub fn save(path: &Path, config: &Config) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_string_pretty(config).map_err(std::io::Error::other)?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json)?;
    fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{Match, Mode, MonitorRef, Rect};

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("fixatoutix-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn missing_file_gives_defaults() {
        let dir = scratch("missing");
        let loaded = load(&dir.join("config.json"));
        assert!(!loaded.recovered);
        assert_eq!(loaded.config.settings.shortcut, "Ctrl+Alt+F");
        assert_eq!(loaded.config.settings.language, "fr");
        assert!(loaded.config.rules.is_empty());
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = scratch("roundtrip");
        let path = dir.join("sub").join("config.json");
        let mut config = Config::default();
        config.rules.push(Rule {
            id: "1".into(),
            enabled: true,
            label: "Bloc-notes".into(),
            matcher: Match { exe: "notepad.exe".into(), class_name: None, title_regex: None },
            rect: Rect { x: 0, y: 0, w: 960, h: 1040 },
            monitor: MonitorRef { device_name: "\\\\.\\DISPLAY1".into(), work_area: None },
            mode: Mode::Lock,
            force_restore: false,
            exe_path: None,
            created_at: "t".into(),
            updated_at: "t".into(),
        });
        save(&path, &config).unwrap();
        assert!(!path.with_extension("json.tmp").exists());
        let loaded = load(&path);
        assert_eq!(loaded.config.rules.len(), 1);
        assert_eq!(loaded.config.rules[0].label, "Bloc-notes");
    }

    #[test]
    fn corrupt_file_is_backed_up_and_reset() {
        let dir = scratch("corrupt");
        let path = dir.join("config.json");
        fs::write(&path, "{ not json").unwrap();
        let loaded = load(&path);
        assert!(loaded.recovered);
        assert!(loaded.config.rules.is_empty());
        assert_eq!(fs::read_to_string(path.with_extension("json.bak")).unwrap(), "{ not json");
    }

    #[test]
    fn portable_file_next_to_the_exe_wins() {
        let dir = scratch("portable");
        let exe = dir.join("Fixatoutix.exe");
        let appdata = dir.join("appdata");
        assert_eq!(resolve_path(Some(&exe), Some(&appdata)), appdata.join("Fixatoutix").join("config.json"));
        fs::write(dir.join("fixatoutix.json"), "{}").unwrap();
        assert_eq!(resolve_path(Some(&exe), Some(&appdata)), dir.join("fixatoutix.json"));
    }

    #[test]
    fn partial_files_fill_in_defaults() {
        let config: Config = serde_json::from_str(r#"{"settings":{"language":"en"}}"#).unwrap();
        assert_eq!(config.settings.language, "en");
        assert_eq!(config.settings.shortcut, "Ctrl+Alt+F");
        assert!(config.settings.check_updates);
    }
}
