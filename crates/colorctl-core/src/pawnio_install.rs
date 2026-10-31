//! Fetching and running PawnIO's official installer.
//!
//! Fan control on Windows needs PawnIO's sandboxed driver. It is a separate,
//! independently signed product from a different author, so this project does
//! not redistribute it: the installer is fetched from the project's own release
//! page, verified against a pinned SHA-256, and then run so its own UI and
//! consent prompts appear.
//!
//! The pin is deliberate. `latest` would let a future release change under a
//! build that was tested against this one, so bumping `VERSION`, `URL`, `SIZE`
//! and `SHA256` together is the only way to move forward.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::error::{Error, Result};

/// PawnIO release this build was tested against.
pub const VERSION: &str = "2.2.0";

/// Versioned asset URL — not `/latest/`, so the bytes match the pin.
pub const URL: &str =
    "https://github.com/namazso/PawnIO.Setup/releases/download/2.2.0/PawnIO_setup.exe";

/// SHA-256 published by the release, verified before anything is executed.
pub const SHA256: &str = "1f519a22e47187f70a1379a48ca604981c4fcf694f4e65b734aaa74a9fba3032";

/// Expected size, as a cheap sanity check before hashing.
pub const SIZE: u64 = 3_410_960;

/// File name used locally.
pub const FILE: &str = "PawnIO_setup.exe";

fn hash_file(path: &Path) -> Result<(String, u64)> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        total += read as u64;
    }
    Ok((format!("{:x}", hasher.finalize()), total))
}

/// Checks a downloaded installer against the pinned size and hash.
pub fn verify(path: &Path) -> Result<()> {
    let (digest, size) = hash_file(path)?;
    if size != SIZE {
        return Err(Error::backend(format!(
            "the installer is {size} bytes, expected {SIZE} — refusing to run it"
        )));
    }
    if digest != SHA256 {
        return Err(Error::backend(format!(
            "the installer's SHA-256 is {digest}, expected {SHA256} — refusing to run it"
        )));
    }
    Ok(())
}

/// Downloads the installer into `dir` and verifies it.
///
/// A file that fails verification is deleted: leaving it named `PawnIO_setup.exe`
/// invites someone to double-click it later.
pub fn download(dir: &Path, mut on_progress: impl FnMut(u64, Option<u64>)) -> Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let dest = dir.join(FILE);
    let mut partial = dest.clone().into_os_string();
    partial.push(".part");
    let partial = PathBuf::from(partial);

    let agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(120))
        .user_agent(concat!("OpenColorHub/", env!("CARGO_PKG_VERSION")))
        .build();

    let response = agent
        .get(URL)
        .call()
        .map_err(|e| Error::unavailable(format!("could not download PawnIO: {e}")))?;
    let total = response
        .header("Content-Length")
        .and_then(|value| value.trim().parse::<u64>().ok());
    let mut reader = response.into_reader();
    let mut file = std::fs::File::create(&partial)?;

    let mut buffer = vec![0u8; 64 * 1024];
    let mut received = 0u64;
    let transfer = (|| -> std::io::Result<()> {
        loop {
            let read = reader.read(&mut buffer)?;
            if read == 0 {
                return Ok(());
            }
            file.write_all(&buffer[..read])?;
            received += read as u64;
            on_progress(received, total);
        }
    })();
    if let Err(error) = transfer {
        let _ = std::fs::remove_file(&partial);
        return Err(Error::Io(error));
    }
    file.flush()?;
    drop(file);

    if let Err(error) = verify(&partial) {
        let _ = std::fs::remove_file(&partial);
        return Err(error);
    }

    std::fs::rename(&partial, &dest)?;
    Ok(dest)
}

/// Runs the installer and waits for it to finish.
///
/// No arguments are passed, so the installer shows its own window: installing a
/// kernel driver is the user's decision, not something to do quietly.
#[cfg(target_os = "windows")]
pub fn run_installer(path: &Path) -> Result<()> {
    let status = std::process::Command::new(path)
        .status()
        .map_err(|e| Error::backend(format!("could not start the installer: {e}")))?;
    if !status.success() {
        return Err(Error::backend(format!(
            "the installer exited with {status} — the driver is probably not installed"
        )));
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn run_installer(_path: &Path) -> Result<()> {
    Err(Error::unavailable(
        "PawnIO only applies to Windows; Linux uses /dev/port",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_a_file_that_is_not_the_pinned_installer() {
        let dir = std::env::temp_dir().join(format!("ocb-pawnio-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("PawnIO_setup.exe");
        std::fs::write(&path, b"not the installer").unwrap();

        let error = verify(&path).unwrap_err().to_string();
        assert!(error.contains("refusing to run"), "{error}");
    }

    /// Fetches the real installer and checks it against the pin. Ignored by
    /// default: it needs the network and 3.4 MB.
    #[test]
    #[ignore]
    fn live_download_matches_the_pin() {
        let dir = std::env::temp_dir().join(format!("ocb-pawnio-live-{}", std::process::id()));
        let path = download(&dir, |_, _| {}).expect("download");
        assert_eq!(path.file_name().unwrap(), FILE);
        verify(&path).expect("hash");
        assert_eq!(
            std::fs::metadata(&path).unwrap().len(),
            SIZE,
            "size drifted from the pin"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pin_is_a_full_sha256_and_a_sane_size() {
        assert_eq!(SHA256.len(), 64);
        assert!(SHA256.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(URL.contains(VERSION), "the URL must pin the version");
        assert!(URL.ends_with(FILE));
    }
}
