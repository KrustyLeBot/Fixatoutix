//! Self-update from the GitHub release `latest`, same scheme as Memento.
//!
//! CI publishes one release tagged `latest` titled "Fixatoutix build <N>" carrying `Fixatoutix.exe`.
//! The build number (the CI run number) is stamped in the exe at compile time. A newer number means an
//! update: download next to the running exe, check size + SHA-256, swap by rename, relaunch.

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const REPO_URL: &str = "https://github.com/KrustyLeBot/Fixatoutix";
const API: &str = "https://api.github.com/repos/KrustyLeBot/Fixatoutix/releases/tags/latest";

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    pub build: u64,
    pub url: String,
    pub size: u64,
    pub sha256: String,
}

/// Build number stamped by CI (`FIXATOUTIX_BUILD`); 0 for a local build, which is always older.
pub fn current_build() -> u64 {
    option_env!("FIXATOUTIX_BUILD").and_then(|b| b.trim().parse().ok()).unwrap_or(0)
}

/// One HTTP agent for the whole module; on Windows TLS goes through SChannel.
fn agent() -> ureq::Agent {
    let builder = ureq::AgentBuilder::new();
    #[cfg(windows)]
    let builder = match native_tls::TlsConnector::new() {
        Ok(connector) => builder.tls_connector(std::sync::Arc::new(connector)),
        Err(_) => builder,
    };
    builder.build()
}

pub fn user_agent() -> String {
    format!("Fixatoutix/{} build {} ({REPO_URL})", env!("CARGO_PKG_VERSION"), current_build())
}

/// The release title ends with the build number; the asset is the `.exe` (GitHub gives its SHA-256).
pub fn parse_release(data: &serde_json::Value) -> Option<Release> {
    let name = data.get("name")?.as_str()?.trim_end();
    let digits: String = name.chars().rev().take_while(|c| c.is_ascii_digit()).collect::<Vec<_>>().into_iter().rev().collect();
    let build: u64 = digits.parse().ok()?;
    for asset in data.get("assets")?.as_array()? {
        let asset_name = asset.get("name").and_then(|n| n.as_str()).unwrap_or("");
        if asset_name.to_ascii_lowercase().ends_with(".exe") {
            let digest = asset.get("digest").and_then(|d| d.as_str()).unwrap_or("");
            return Some(Release {
                build,
                url: asset.get("browser_download_url")?.as_str()?.to_string(),
                size: asset.get("size").and_then(|s| s.as_u64()).unwrap_or(0),
                sha256: digest.strip_prefix("sha256:").unwrap_or("").to_ascii_lowercase(),
            });
        }
    }
    None
}

pub fn is_newer(release: &Release, current: u64) -> bool {
    release.build > current
}

/// The published release, `Ok(None)` while the CI is re-publishing it (404).
pub fn latest() -> Result<Option<Release>, String> {
    let response = agent()
        .get(API)
        .set("Accept", "application/vnd.github+json")
        .set("User-Agent", &user_agent())
        .timeout(Duration::from_secs(15))
        .call();
    match response {
        Ok(response) => {
            let text = response.into_string().map_err(|e| e.to_string())?;
            let json: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
            Ok(parse_release(&json))
        }
        Err(ureq::Error::Status(404, _)) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

/// Downloads the new exe next to the running one (a rename cannot cross drives) and verifies it.
pub fn download(release: &Release, exe_dir: &Path, mut progress: impl FnMut(u64, u64)) -> Result<PathBuf, String> {
    let target = exe_dir.join(format!("Fixatoutix-{}.exe.download", release.build));
    let result = download_to(release, &target, &mut progress);
    if result.is_err() {
        let _ = fs::remove_file(&target);
    }
    result.map(|_| target)
}

fn download_to(release: &Release, target: &Path, progress: &mut impl FnMut(u64, u64)) -> Result<(), String> {
    let response = agent()
        .get(&release.url)
        .set("User-Agent", &user_agent())
        .timeout(Duration::from_secs(120))
        .call()
        .map_err(|e| e.to_string())?;
    let total = response
        .header("Content-Length")
        .and_then(|l| l.parse().ok())
        .unwrap_or(release.size);
    let mut reader = response.into_reader();
    let mut out = File::create(target).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    let mut done = 0u64;
    loop {
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        hasher.update(&buf[..n]);
        done += n as u64;
        progress(done, total);
    }
    out.flush().map_err(|e| e.to_string())?;
    drop(out);
    // A truncated, stale (CDN cache) or tampered download must never replace a working executable.
    if release.size != 0 && done != release.size {
        return Err(format!("incomplete download ({done} of {} bytes)", release.size));
    }
    if !release.sha256.is_empty() && format!("{:x}", hasher.finalize()) != release.sha256 {
        return Err("the downloaded file does not match the published SHA-256".into());
    }
    Ok(())
}

/// Puts `new_file` in place of `current_exe`, which may be running: Windows refuses to overwrite a
/// running exe but lets it be renamed. The old one becomes `<name>.old`, removed at the next start.
pub fn install(current_exe: &Path, new_file: &Path) -> std::io::Result<()> {
    let old = old_path(current_exe);
    if old.exists() {
        fs::remove_file(&old)?;
    }
    fs::rename(current_exe, &old)?;
    if let Err(e) = fs::rename(new_file, current_exe) {
        let _ = fs::rename(&old, current_exe); // put the working version back
        return Err(e);
    }
    Ok(())
}

fn old_path(exe: &Path) -> PathBuf {
    let mut name = exe.as_os_str().to_os_string();
    name.push(".old");
    PathBuf::from(name)
}

/// Removes the exe left behind by the last self-update; true once nothing is left.
/// Right after an update the previous process is still exiting and keeps the file locked.
pub fn cleanup_previous(exe: &Path) -> bool {
    let old = old_path(exe);
    !old.exists() || fs::remove_file(old).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn release_json(name: &str) -> serde_json::Value {
        json!({
            "name": name,
            "assets": [
                { "name": "notes.txt", "browser_download_url": "https://x/notes.txt", "size": 1 },
                { "name": "Fixatoutix.exe", "browser_download_url": "https://x/Fixatoutix.exe",
                  "size": 7_500_000, "digest": "sha256:ABCDEF0123" }
            ]
        })
    }

    #[test]
    fn parses_the_build_number_and_the_exe_asset() {
        let r = parse_release(&release_json("Fixatoutix build 42")).unwrap();
        assert_eq!(r.build, 42);
        assert_eq!(r.url, "https://x/Fixatoutix.exe");
        assert_eq!(r.size, 7_500_000);
        assert_eq!(r.sha256, "abcdef0123");
    }

    #[test]
    fn rejects_releases_without_a_number_or_an_exe() {
        assert!(parse_release(&release_json("Fixatoutix latest")).is_none());
        assert!(parse_release(&json!({ "name": "Fixatoutix build 3", "assets": [] })).is_none());
    }

    #[test]
    fn only_a_higher_build_is_newer() {
        let r = parse_release(&release_json("Fixatoutix build 10")).unwrap();
        assert!(is_newer(&r, 9));
        assert!(!is_newer(&r, 10));
        assert!(!is_newer(&r, 11));
    }

    #[test]
    fn install_swaps_and_keeps_the_old_exe() {
        let dir = std::env::temp_dir().join(format!("fixatoutix-upd-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("Fixatoutix.exe");
        let new = dir.join("Fixatoutix-2.exe.download");
        fs::write(&exe, "v1").unwrap();
        fs::write(&new, "v2").unwrap();
        install(&exe, &new).unwrap();
        assert_eq!(fs::read_to_string(&exe).unwrap(), "v2");
        assert_eq!(fs::read_to_string(old_path(&exe)).unwrap(), "v1");
        assert!(!new.exists());
        assert!(cleanup_previous(&exe));
        assert!(!old_path(&exe).exists());
    }

    #[test]
    fn a_failed_swap_restores_the_working_exe() {
        let dir = std::env::temp_dir().join(format!("fixatoutix-upd-fail-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("Fixatoutix.exe");
        fs::write(&exe, "v1").unwrap();
        assert!(install(&exe, &dir.join("missing.download")).is_err());
        assert_eq!(fs::read_to_string(&exe).unwrap(), "v1");
    }
}
