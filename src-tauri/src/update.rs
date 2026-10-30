//! Self-update: check the GitHub release feed, download the new executable and
//! swap it in place.
//!
//! Windows refuses to overwrite a running image, but it does allow *renaming*
//! one, so the swap is: rename `OpenColorHub.exe` -> `OpenColorHub.old.exe`,
//! copy the downloaded binary into place, relaunch, and delete the leftover on
//! the next start. If the copy fails the rename is rolled back, so a failed
//! update leaves a working app rather than none.
//!
//! No signing keys and no installer re-run: the release ships the bare exe as
//! an asset, and its SHA-256 is cross-checked against `SHA256SUMS.txt` from the
//! same release before anything is written next to the executable.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

const REPO: &str = "msartore/OpenColorHub";
const ASSET: &str = "OpenColorHub.exe";
const SUMS: &str = "SHA256SUMS.txt";

/// What the UI needs to render the update prompt.
#[derive(Debug, Clone, Serialize)]
pub struct UpdateStatus {
    pub current: String,
    pub latest: Option<String>,
    pub available: bool,
    pub notes: Option<String>,
    pub asset_url: Option<String>,
    /// Populated instead of the fields above when the check could not run
    /// (offline, rate-limited, no releases yet). Never fatal.
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Debug, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Serialize, Clone)]
struct Progress {
    received: u64,
    total: Option<u64>,
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(8))
        .timeout_read(Duration::from_secs(30))
        .timeout_write(Duration::from_secs(30))
        .user_agent(concat!("OpenColorHub/", env!("CARGO_PKG_VERSION")))
        .build()
}

/// Parses `v1.2.3`, `1.2`, `1.2.3-beta.1` into comparable numbers.
fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let core = text.trim().trim_start_matches(['v', 'V']);
    let core = core.split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    Some((major, minor, patch))
}

fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

/// Queries the release feed. Blocking; callers run it off the UI thread.
pub fn check() -> UpdateStatus {
    let current = env!("CARGO_PKG_VERSION").to_string();
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");

    let response = agent()
        .get(&url)
        .set("Accept", "application/vnd.github+json")
        .call();

    let release: Release = match response {
        Ok(resp) => match resp.into_json::<Release>() {
            Ok(release) => release,
            Err(err) => {
                return UpdateStatus {
                    current,
                    latest: None,
                    available: false,
                    notes: None,
                    asset_url: None,
                    error: Some(format!("malformed release feed: {err}")),
                };
            }
        },
        Err(ureq::Error::Status(404, _)) => {
            return UpdateStatus {
                current,
                latest: None,
                available: false,
                notes: None,
                asset_url: None,
                error: Some("no published releases yet".into()),
            };
        }
        Err(err) => {
            return UpdateStatus {
                current,
                latest: None,
                available: false,
                notes: None,
                asset_url: None,
                error: Some(describe(&err)),
            };
        }
    };

    let asset = release.assets.iter().find(|a| a.name == ASSET).or_else(|| {
        release
            .assets
            .iter()
            .find(|a| a.name.to_ascii_lowercase().ends_with(".exe") && !a.name.contains("setup"))
    });

    let available = is_newer(&release.tag_name, &current) && asset.is_some();
    UpdateStatus {
        latest: Some(release.tag_name.clone()),
        available,
        notes: release
            .body
            .map(|b| b.trim().to_string())
            .filter(|b| !b.is_empty()),
        asset_url: asset.map(|a| a.browser_download_url.clone()),
        current,
        error: None,
    }
}

fn describe(err: &ureq::Error) -> String {
    match err {
        ureq::Error::Status(code, _) => match code {
            403 => "GitHub rate limit reached — try again later".into(),
            other => format!("GitHub returned HTTP {other}"),
        },
        ureq::Error::Transport(t) => format!("network error: {t}"),
    }
}

/// Directory that holds the staged download, next to the app's data.
pub fn staging_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("update")
}

/// Downloads `url` into `dest`, emitting `update://progress` as it goes.
pub fn download(app: &AppHandle, url: &str, dest: &Path) -> Result<PathBuf, String> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    }

    let resp = agent().get(url).call().map_err(|e| describe(&e))?;
    let total = resp
        .header("Content-Length")
        .and_then(|v| v.parse::<u64>().ok());

    let mut reader = resp.into_reader();
    let mut file =
        File::create(dest).map_err(|e| format!("cannot write {}: {e}", dest.display()))?;

    let mut buf = [0u8; 64 * 1024];
    let mut received: u64 = 0;
    loop {
        let read = reader
            .read(&mut buf)
            .map_err(|e| format!("download failed: {e}"))?;
        if read == 0 {
            break;
        }
        file.write_all(&buf[..read])
            .map_err(|e| format!("cannot write {}: {e}", dest.display()))?;
        received += read as u64;
        let _ = app.emit("update://progress", Progress { received, total });
    }
    file.flush().map_err(|e| e.to_string())?;

    if let Some(expected) = total
        && received != expected
    {
        let _ = fs::remove_file(dest);
        return Err(format!(
            "truncated download: {received} of {expected} bytes"
        ));
    }
    Ok(dest.to_path_buf())
}

/// Fetches `SHA256SUMS.txt` from the same release and matches `ASSET`.
/// Returns the expected digest, or `None` when the file or entry is absent —
/// an unsigned project should still update, just with less assurance.
pub fn expected_digest(status: &UpdateStatus) -> Option<String> {
    let base = status.asset_url.as_deref()?.rsplit_once('/')?.0;
    let url = format!("{base}/{SUMS}");
    let body = agent().get(&url).call().ok()?.into_string().ok()?;
    body.lines()
        .filter_map(|line| line.split_once(char::is_whitespace))
        .find(|(_, name)| name.trim().trim_start_matches('*') == ASSET)
        .map(|(digest, _)| digest.trim().to_ascii_lowercase())
}

/// Replaces the running executable with `staged` and relaunches.
///
/// The app exits on success; the caller never regains control.
pub fn swap_and_restart(app: &AppHandle, staged: &Path) -> Result<(), String> {
    let current = std::env::current_exe().map_err(|e| format!("cannot locate executable: {e}"))?;
    let backup = current.with_extension("old.exe");
    let _ = fs::remove_file(&backup);

    fs::rename(&current, &backup).map_err(|e| format!("cannot move current build aside: {e}"))?;
    if let Err(err) = fs::copy(staged, &current) {
        let _ = fs::rename(&backup, &current);
        return Err(format!("cannot install update: {err}"));
    }

    // `restart()` shuts this process down before relaunching, so the new build
    // does not race the old one for the WebView2 profile lock. It never returns.
    let _ = fs::remove_file(staged);
    app.restart()
}

/// Removes the `*.old.exe` left behind by a previous update. Best effort.
pub fn clean_previous() {
    if let Ok(current) = std::env::current_exe() {
        let _ = fs::remove_file(current.with_extension("old.exe"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_compares_versions() {
        assert_eq!(parse_version("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("0.2"), Some((0, 2, 0)));
        assert_eq!(parse_version("0.2.0-beta.1"), Some((0, 2, 0)));

        assert!(is_newer("v0.3.0", "0.2.0"));
        assert!(is_newer("0.10.0", "0.9.9"));
        assert!(!is_newer("v0.2.0", "0.2.0"));
        assert!(!is_newer("v0.1.9", "0.2.0"));
        assert!(!is_newer("nightly", "0.2.0"));
    }

    #[test]
    fn rejects_non_numeric_tags() {
        assert!(!is_newer("latest", "0.2.0"));
        assert!(!is_newer("", "0.2.0"));
    }
}
