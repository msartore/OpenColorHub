//! IPC for the self-updater.

use tauri::{AppHandle, State};

use crate::state::AppState;
use crate::update::{self, UpdateStatus};

/// Cheap enough to call on launch: one unauthenticated GitHub API request.
#[tauri::command]
pub async fn update_check() -> UpdateStatus {
    tauri::async_runtime::spawn_blocking(update::check)
        .await
        .unwrap_or_else(|_| UpdateStatus {
            current: env!("CARGO_PKG_VERSION").to_string(),
            latest: None,
            available: false,
            notes: None,
            asset_url: None,
            error: Some("update check failed".into()),
        })
}

/// Downloads the release asset, verifies it, swaps it in and relaunches.
///
/// Returns only on failure — success exits the process.
#[tauri::command]
pub async fn update_install(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let status = update::check();
    if !status.available {
        return Err(status.error.unwrap_or_else(|| "already up to date".into()));
    }
    let url = status
        .asset_url
        .clone()
        .ok_or("release has no executable asset")?;

    let data_dir = state.data_dir();
    let staged = update::staging_dir(&data_dir).join("OpenColorHub.exe");

    let handle = app.clone();
    let dest = staged.clone();
    let path = tauri::async_runtime::spawn_blocking(move || update::download(&handle, &url, &dest))
        .await
        .map_err(|_| "download thread panicked".to_string())??;

    if let Some(expected) = update::expected_digest(&status) {
        let actual = sha256_file(&path)?;
        if actual != expected {
            let _ = std::fs::remove_file(&path);
            return Err("downloaded build does not match the published checksum".into());
        }
    }

    update::swap_and_restart(&app, &path)
}

fn sha256_file(path: &std::path::Path) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher).map_err(|e| e.to_string())?;
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>())
}
