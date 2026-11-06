//! Light rotating log in `%LOCALAPPDATA%\Fixatoutix\logs` (1 MB max, one `.1` backup).

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

const MAX_BYTES: u64 = 1_000_000;

static LOG_PATH: Mutex<Option<PathBuf>> = Mutex::new(None);

pub fn init() {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join("Fixatoutix").join("logs");
    if fs::create_dir_all(&dir).is_ok() {
        if let Ok(mut guard) = LOG_PATH.lock() {
            *guard = Some(dir.join("fixatoutix.log"));
        }
    }
}

pub fn write(level: &str, message: &str) {
    let Ok(guard) = LOG_PATH.lock() else { return };
    let Some(path) = guard.as_ref() else { return };
    if fs::metadata(path).map(|m| m.len() > MAX_BYTES).unwrap_or(false) {
        let _ = fs::rename(path, path.with_extension("log.1"));
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{} {level:<5} {message}", crate::util::now_iso());
    }
}

#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => { $crate::logger::write("INFO", &format!($($arg)*)) };
}

#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => { $crate::logger::write("WARN", &format!($($arg)*)) };
}
