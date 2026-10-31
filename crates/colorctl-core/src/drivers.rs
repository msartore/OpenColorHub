//! Motherboard drivers and firmware, read from Colorful's public download
//! service.
//!
//! The vendor client asks a token-gated API (`igameapi.colorful.cn`) which
//! answers `{"Data":[]}` unless the request carries the client's MAC-derived
//! token — there is no anonymous path into it. The public site publishes the
//! same catalogue without credentials, keyed by product id, so this module
//! resolves a board name to that id and reads the file list from there.
//!
//! Nothing here executes what it downloads. A file is written to disk, hashed,
//! and reported; running an installer is the user's decision.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{Error, Result};

/// Public site that serves the catalogue.
const SITE: &str = "https://www.colorfulgroup.com";
/// The site's "Motherboard" model id, from the download page's model dropdown.
const MOTHERBOARD_MODEL: u32 = 84;
/// Domains the download may use. The catalogue serves files from both the
/// regional CDN (`*.colorful.cn`) and the group site itself (`colorfulgroup.com`).
/// Anything else is rejected rather than followed.
const ALLOWED_DOMAINS: [&str; 2] = ["colorful.cn", "colorfulgroup.com"];

/// What a catalogue entry is, inferred from its title and file extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriverKind {
    Bios,
    Driver,
    Utility,
    Manual,
    Other,
}

impl DriverKind {
    /// Ordering used by the UI: firmware first, then drivers, then the rest.
    fn rank(self) -> u8 {
        match self {
            DriverKind::Bios => 0,
            DriverKind::Driver => 1,
            DriverKind::Utility => 2,
            DriverKind::Manual => 3,
            DriverKind::Other => 4,
        }
    }

    /// Classifies an entry from the remote catalogue.
    ///
    /// The URL carries the vendor's own folder structure, which is more reliable
    /// than the title: `/drivers/…` is a driver even when the title is just
    /// `RLNetwork`, and `/manual/…` is always a document.
    pub fn classify(title: &str, url: &str) -> Self {
        let upper = title.to_uppercase();
        let lower_url = url.to_ascii_lowercase();

        if lower_url.contains("/manual/") || lower_url.ends_with(".pdf") {
            return DriverKind::Manual;
        }
        if lower_url.contains("/bios/") || upper.contains("BIOS") {
            return DriverKind::Bios;
        }
        if lower_url.contains("/drivers/") || lower_url.contains("/driver/") {
            return if upper.contains("IGAMECENTER") || upper.contains("UTILITY") {
                DriverKind::Utility
            } else {
                DriverKind::Driver
            };
        }
        if upper.contains("IGAMECENTER") || upper.contains("UTILITY") || upper.contains("TOOL") {
            return DriverKind::Utility;
        }
        if upper.contains("DRIVER") || upper.contains("CHIPSET") || upper.contains("AUDIO") {
            return DriverKind::Driver;
        }
        DriverKind::Other
    }
}

/// One downloadable file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DriverFile {
    pub title: String,
    pub kind: DriverKind,
    pub url: String,
    /// File name the server will hand out, already sanitised for the local FS.
    pub file_name: String,
    /// Publication date as `YYYY-MM-DD`; empty when the field was unparsable.
    pub published: String,
}

/// A board's full catalogue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverCatalogue {
    /// Board name as the site spells it, which may differ in case from SMBIOS.
    pub board: String,
    pub product_id: u32,
    pub files: Vec<DriverFile>,
}

/// The catalogue as kept on disk.
///
/// Stored as one JSON record rather than in SQLite: it is a single small list
/// keyed by board, queried only by "give me the last copy". A database would add
/// a dependency and a migration story for no gain.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CachedCatalogue {
    /// Board name as the vendor spells it.
    pub board: String,
    /// Board name as SMBIOS reported it, so a hardware change invalidates the cache.
    #[serde(default)]
    pub source_board: String,
    pub product_id: u32,
    /// When the vendor was last asked, in milliseconds since the epoch.
    pub fetched_at_ms: u64,
    pub files: Vec<DriverFile>,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|delta| delta.as_millis() as u64)
        .unwrap_or(0)
}

/// Reads the cached catalogue, or `None` when there is no usable copy.
pub fn load_cache(path: &Path) -> Option<CachedCatalogue> {
    let text = std::fs::read_to_string(path).ok()?;
    match serde_json::from_str::<CachedCatalogue>(&text) {
        Ok(cache) if !cache.files.is_empty() => Some(cache),
        _ => None,
    }
}

/// Cached copy for `board`, if it was read for the same machine.
pub fn load_cache_for(path: &Path, board: &str) -> Option<CachedCatalogue> {
    let cache = load_cache(path)?;
    cache
        .source_board
        .eq_ignore_ascii_case(board)
        .then_some(cache)
}

/// Writes the cache, replacing any previous copy atomically.
pub fn save_cache(path: &Path, cache: &CachedCatalogue) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(cache)?;
    let mut temp = path.to_path_buf().into_os_string();
    temp.push(".tmp");
    let temp = PathBuf::from(temp);
    std::fs::write(&temp, text)?;
    std::fs::rename(&temp, path)?;
    Ok(())
}

/// Fetches the catalogue and records when it was read.
pub fn fetch_and_cache(board: &str, cache_path: &Path) -> Result<CachedCatalogue> {
    let catalogue = fetch_catalogue(board)?;
    let cache = CachedCatalogue {
        board: catalogue.board,
        source_board: board.to_string(),
        product_id: catalogue.product_id,
        fetched_at_ms: now_ms(),
        files: catalogue.files,
    };
    save_cache(cache_path, &cache)?;
    Ok(cache)
}

/// Result of a completed download.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadedFile {
    pub path: PathBuf,
    pub bytes: u64,
    /// Lowercase hex SHA-256 of the bytes written.
    pub sha256: String,
}

#[derive(Debug, Deserialize)]
struct ClassEntry {
    id: String,
    #[serde(default)]
    parent_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Product {
    /// The list mixes product rows (numeric ids) with image rows (GUIDs), so
    /// the field is read as-is and non-numeric entries are dropped.
    id: serde_json::Value,
    name: String,
}

impl Product {
    fn numeric_id(&self) -> Option<u32> {
        match &self.id {
            serde_json::Value::Number(n) => n.as_u64().and_then(|v| u32::try_from(v).ok()),
            serde_json::Value::String(s) => s.parse().ok(),
            _ => None,
        }
    }
}

#[derive(Debug, Deserialize)]
struct RemoteFile {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    fileurl: Option<String>,
    #[serde(default)]
    edit_time: Option<String>,
}

/// Collapses case and runs of whitespace so `" CVN  X570M "` matches
/// `"cvn x570m"`. SMBIOS and the website disagree on both.
fn normalise(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_uppercase()
}

/// Picks the product whose name matches the board.
///
/// Exact match after normalisation wins; otherwise the longest product name the
/// board starts with, which covers boards whose SMBIOS string carries a suffix
/// the site omits. Shortest-name-first would pick `CVN X570` over
/// `CVN X570M GAMING PRO V14`, so length is what decides.
pub fn pick_product(products: &[(u32, String)], board: &str) -> Option<u32> {
    let want = normalise(board);
    if want.is_empty() {
        return None;
    }
    if let Some((id, _)) = products
        .iter()
        .find(|(_, name)| normalise(name) == want)
        .map(|(id, name)| (*id, name.clone()))
    {
        return Some(id);
    }
    products
        .iter()
        .filter(|(_, name)| {
            let candidate = normalise(name);
            want.starts_with(&candidate) || candidate.starts_with(&want)
        })
        .max_by_key(|(_, name)| normalise(name).len())
        .map(|(id, _)| *id)
}

/// Decodes `%XX` escapes without pulling in a dependency. Invalid escapes are
/// left as written rather than dropped, so a malformed name stays recognisable.
fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(value) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(value);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Derives a safe local file name from a download URL.
///
/// The name comes from the remote server, so it is treated as hostile: path
/// separators, drive letters, control characters and trailing dots are all
/// removed, and a name that reduces to `.`/`..` is rejected.
pub fn file_name(url: &str) -> Option<String> {
    let path = url.split(['?', '#']).next()?;
    let last = path.rsplit('/').next()?;
    let decoded = percent_decode(last);
    let cleaned: String = decoded
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '\\' | '|' | '?' | '*') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim_matches([' ', '.']).to_string();
    if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        return None;
    }
    // Leave room for the directory path plus a `.part` suffix.
    Some(cleaned.chars().take(120).collect())
}

/// Whether a URL may be fetched: HTTPS only, and inside Colorful's domain.
///
/// The catalogue is remote data, so the host is checked rather than trusted —
/// an entry pointing at an arbitrary server would otherwise turn this module
/// into a downloader for whatever the API returned.
pub fn is_allowed_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let host = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if host.is_empty() || host.contains('@') || host.contains(':') {
        return false; // credentials or a non-default port
    }
    ALLOWED_DOMAINS
        .iter()
        .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
}

/// Turns a catalogue reference into an absolute URL.
///
/// Older entries carry site-relative paths (`/uploads/common/...`) rather than
/// a host; they belong to the same site that served the catalogue.
pub fn absolute_url(url: &str) -> Option<String> {
    let url = url.trim();
    if url.starts_with('/') {
        // `//host/path` is protocol-relative, not a path on this site.
        if url.starts_with("//") {
            return None;
        }
        return Some(format!("{SITE}{url}"));
    }
    if url.starts_with("https://") {
        return Some(url.to_string());
    }
    None
}

/// Converts the site's `MM/DD/YYYY HH:MM:SS` to `YYYY-MM-DD`.
pub fn iso_date(edit_time: &str) -> String {
    let date = edit_time.split_whitespace().next().unwrap_or_default();
    let mut parts = date.split('/');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(m), Some(d), Some(y)) if y.len() == 4 => format!("{y}-{m:0>2}-{d:0>2}"),
        _ => String::new(),
    }
}

/// Parses the `GetProductDownload` payload into catalogue entries.
///
/// Entries with no URL, a URL outside the allowed domain, or a name that cannot
/// be made safe are dropped: a half-usable entry is worse than a missing one.
pub fn parse_files(json: &str) -> Result<Vec<DriverFile>> {
    let raw: Vec<RemoteFile> = serde_json::from_str(json)?;
    let mut files: Vec<DriverFile> = raw
        .into_iter()
        .filter_map(|entry| {
            let title = entry.title?.trim().to_string();
            let url = absolute_url(&entry.fileurl?)?;
            if title.is_empty() || !is_allowed_url(&url) {
                return None;
            }
            let file_name = file_name(&url)?;
            Some(DriverFile {
                kind: DriverKind::classify(&title, &url),
                published: iso_date(entry.edit_time.as_deref().unwrap_or_default()),
                title,
                url,
                file_name,
            })
        })
        .collect();

    files.sort_by(|a, b| {
        a.kind
            .rank()
            .cmp(&b.kind.rank())
            .then_with(|| b.published.cmp(&a.published))
            .then_with(|| a.title.cmp(&b.title))
    });
    Ok(files)
}

/// The last run of digits in a string, e.g. `1016` in `"... BIOS 1016"`.
///
/// Firmware titles are free text; the build number is the only comparable part.
pub fn firmware_number(text: &str) -> Option<u32> {
    let mut best: Option<&str> = None;
    let bytes = text.as_bytes();
    let mut start: Option<usize> = None;
    for (i, b) in bytes.iter().enumerate() {
        if b.is_ascii_digit() {
            start.get_or_insert(i);
        } else if let Some(s_idx) = start.take() {
            best = Some(&text[s_idx..i]).or(best);
        }
    }
    if let Some(s_idx) = start {
        best = Some(&text[s_idx..]).or(best);
    }
    best.and_then(|n| n.parse().ok())
}

/// Whether `title` names a firmware build newer than the installed one.
///
/// An unparsable title or an unknown installed version yields `false`: claiming
/// an update exists when it cannot be proved is worse than staying quiet.
pub fn is_newer_firmware(title: &str, installed: &str) -> bool {
    match (firmware_number(title), firmware_number(installed)) {
        (Some(candidate), Some(current)) => candidate > current,
        _ => false,
    }
}

fn http_get(agent: &ureq::Agent, url: &str) -> Result<String> {
    let response = agent
        .get(url)
        .call()
        .map_err(|e| Error::unavailable(format!("{url}: {e}")))?;
    response
        .into_string()
        .map_err(|e| Error::unavailable(format!("{url}: {e}")))
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .user_agent(concat!("OpenColorHub/", env!("CARGO_PKG_VERSION")))
        .build()
}

/// Resolves a board name to the site's product id.
///
/// The site groups boards into series; a product only appears under its own
/// series, so every class is searched rather than guessing one.
pub fn resolve_product(agent: &ureq::Agent, board: &str) -> Result<(u32, String)> {
    let classes = http_get(agent, &format!("{SITE}/Home/GetProductClassList"))?;
    let classes: Vec<ClassEntry> = serde_json::from_str(&classes)?;
    let series: Vec<String> = classes
        .into_iter()
        .filter(|c| c.parent_id.as_deref() == Some("84"))
        .map(|c| c.id)
        .collect();

    for class in series {
        let url = format!("{SITE}/Home/GetProduct?modelid={MOTHERBOARD_MODEL}&attrId={class}");
        let Ok(body) = http_get(agent, &url) else {
            continue;
        };
        let Ok(products) = serde_json::from_str::<Vec<Product>>(&body) else {
            continue;
        };
        let list: Vec<(u32, String)> = products
            .into_iter()
            .filter_map(|p| p.numeric_id().map(|id| (id, p.name)))
            .collect();
        if let Some(id) = pick_product(&list, board) {
            let name = list
                .iter()
                .find(|(pid, _)| *pid == id)
                .map(|(_, name)| name.clone())
                .unwrap_or_else(|| board.to_string());
            return Ok((id, name));
        }
    }
    Err(Error::unavailable(format!(
        "no download entries for {board} — the board is not listed on colorfulgroup.com"
    )))
}

/// Fetches the catalogue for a board.
pub fn fetch_catalogue(board: &str) -> Result<DriverCatalogue> {
    let agent = agent();
    let (product_id, name) = resolve_product(&agent, board)?;
    let url = format!("{SITE}/Home/GetProductDownload?mid={MOTHERBOARD_MODEL}&id={product_id}");
    let body = http_get(&agent, &url)?;
    let files = parse_files(&body)?;
    Ok(DriverCatalogue {
        board: name,
        product_id,
        files,
    })
}

/// Downloads one entry into `dir`, returning the path and its hash.
pub fn download(entry: &DriverFile, dir: &Path) -> Result<DownloadedFile> {
    download_with_progress(entry, dir, |_, _| {})
}

/// Downloads one entry into `dir`, reporting progress as `(received, total)`.
///
/// `total` comes from `Content-Length` and is `None` when the server omits it.
/// The temporary `.part` file keeps a half-written archive from looking like a
/// usable download; it is renamed only after the transfer completes.
pub fn download_with_progress(
    entry: &DriverFile,
    dir: &Path,
    mut on_progress: impl FnMut(u64, Option<u64>),
) -> Result<DownloadedFile> {
    if !is_allowed_url(&entry.url) {
        return Err(Error::invalid(format!(
            "refusing to download from {}",
            entry.url
        )));
    }
    std::fs::create_dir_all(dir)?;
    let dest = dir.join(&entry.file_name);
    let mut partial_os = dest.clone().into_os_string();
    partial_os.push(".part");
    let partial = PathBuf::from(partial_os);

    let agent = agent();
    let response = agent
        .get(&entry.url)
        // The download CDN rejects requests without a vendor referer (403).
        .set("Referer", SITE)
        .call()
        .map_err(|e| Error::unavailable(format!("{}: {e}", entry.url)))?;
    let total_expected = response
        .header("Content-Length")
        .and_then(|v| v.trim().parse::<u64>().ok());
    let mut reader = response.into_reader();
    let mut file = std::fs::File::create(&partial)?;
    let mut hasher = Sha256::new();
    let mut total: u64 = 0;
    let mut buf = vec![0u8; 64 * 1024];

    let result = (|| -> std::io::Result<()> {
        loop {
            let n = reader.read(&mut buf)?;
            if n == 0 {
                return Ok(());
            }
            hasher.update(&buf[..n]);
            file.write_all(&buf[..n])?;
            total += n as u64;
            on_progress(total, total_expected);
        }
    })();

    if let Err(e) = result {
        let _ = std::fs::remove_file(&partial);
        return Err(Error::Io(e));
    }
    file.flush()?;
    drop(file);
    std::fs::rename(&partial, &dest)?;

    Ok(DownloadedFile {
        path: dest,
        bytes: total,
        sha256: format!("{:x}", hasher.finalize()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILES: &str = r#"[
      {"title":"Audio Driver","fileurl":"https://endownload.colorful.cn/a/ALCHD_all.rar","edit_time":"07/28/2020 10:49:01"},
      {"title":"CVN X570M GAMING PRO V14 BIOS 1016","fileurl":"https://endownload.colorful.cn/b/CVN%20X570M%20BIOS%201016.zip","edit_time":"07/20/2026 10:49:01"},
      {"title":"Board manual","fileurl":"https://endownload.colorful.cn/c/manual.pdf","edit_time":null},
      {"title":"Tracker","fileurl":"https://evil.example.com/x.exe","edit_time":"01/01/2026 00:00:00"},
      {"title":"Plain http","fileurl":"http://endownload.colorful.cn/y.exe","edit_time":"01/01/2026 00:00:00"}
    ]"#;

    #[test]
    fn parses_and_orders_entries() {
        let files = parse_files(FILES).expect("parse");
        assert_eq!(files.len(), 3, "hostile and plain-http entries are dropped");
        assert_eq!(files[0].kind, DriverKind::Bios);
        assert_eq!(files[0].published, "2026-07-20");
        assert_eq!(files[0].file_name, "CVN X570M BIOS 1016.zip");
        assert_eq!(files[1].kind, DriverKind::Driver);
        assert_eq!(files[2].kind, DriverKind::Manual);
    }

    #[test]
    fn classifies_by_title_before_extension() {
        assert_eq!(
            DriverKind::classify("CVN BIOS 1016", "x.zip"),
            DriverKind::Bios
        );
        assert_eq!(
            DriverKind::classify("AMD Chipset Driver", "x.exe"),
            DriverKind::Driver
        );
        assert_eq!(
            DriverKind::classify("iGameCenter", "x.zip"),
            DriverKind::Utility
        );
        assert_eq!(
            DriverKind::classify("Spec sheet", "x.pdf"),
            DriverKind::Manual
        );
    }

    #[test]
    fn trusts_the_vendor_folder_over_the_title() {
        // A bare product name in /drivers/ is a driver, not "other".
        assert_eq!(
            DriverKind::classify(
                "RLNetwork",
                "https://endownload.colorful.cn/MotherBroard/Smart4/drivers/intel/i400/RLNetwork.rar"
            ),
            DriverKind::Driver
        );
        assert_eq!(
            DriverKind::classify(
                "19.10.16_19H1_WHQL_190514",
                "https://endownload.colorful.cn/x/drivers/am4/19.10.16_19H1_WHQL_190514.7z"
            ),
            DriverKind::Driver
        );
        assert_eq!(
            DriverKind::classify(
                "iGameCenter-1.0.3.1",
                "https://endownload.colorful.cn/x/drivers/intel/I500/iGameCenter.zip"
            ),
            DriverKind::Utility
        );
        assert_eq!(
            DriverKind::classify(
                "Some board",
                "https://endownload.colorful.cn/x/manual/amd/board.pdf"
            ),
            DriverKind::Manual
        );
        assert_eq!(
            DriverKind::classify(
                "Board BIOS 1016",
                "https://endownload.colorful.cn/EnDownload/MotherBroard/2026/BIOS/x.zip"
            ),
            DriverKind::Bios
        );
    }

    #[test]
    fn cache_round_trips_and_ignores_a_broken_file() {
        let dir = std::env::temp_dir().join(format!("ocb-driver-cache-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("drivers-cache.json");

        assert!(load_cache(&path).is_none(), "missing file yields nothing");

        let cache = CachedCatalogue {
            board: "CVN X570M GAMING PRO V14".to_string(),
            source_board: "CVN X570M GAMING PRO".to_string(),
            product_id: 880,
            fetched_at_ms: 1_793_450_000_000,
            files: parse_files(FILES).unwrap(),
        };
        save_cache(&path, &cache).unwrap();

        assert_eq!(load_cache(&path).expect("cached copy"), cache);
        assert!(
            !path.with_extension("json.tmp").exists(),
            "the temporary file must be renamed, not left behind"
        );

        std::fs::write(&path, "{ truncated").unwrap();
        assert!(load_cache(&path).is_none(), "a broken cache is ignored");
    }

    #[test]
    fn rejects_urls_outside_the_vendor_domain() {
        assert!(is_allowed_url("https://endownload.colorful.cn/x.zip"));
        assert!(is_allowed_url("https://colorful.cn/x.zip"));
        assert!(!is_allowed_url("http://endownload.colorful.cn/x.zip"));
        assert!(!is_allowed_url("https://evil.example.com/x.zip"));
        assert!(!is_allowed_url("https://colorful.cn.evil.com/x.zip"));
        assert!(!is_allowed_url("https://user@colorful.cn/x.zip"));
        assert!(!is_allowed_url("https://colorful.cn:8443/x.zip"));
        assert!(is_allowed_url(
            "https://www.colorfulgroup.com/uploads/x.zip"
        ));
        assert!(!is_allowed_url("https://colorfulgroup.com.evil.io/x.zip"));
    }

    #[test]
    fn resolves_site_relative_references() {
        assert_eq!(
            absolute_url("/uploads/common/2021/08/x.zip").as_deref(),
            Some("https://www.colorfulgroup.com/uploads/common/2021/08/x.zip")
        );
        assert!(absolute_url("//evil.example.com/x.zip").is_none());
        assert!(absolute_url("ftp://colorful.cn/x.zip").is_none());
        assert!(absolute_url("https://endownload.colorful.cn/x.zip").is_some());
    }

    #[test]
    fn sanitises_remote_file_names() {
        assert_eq!(
            file_name("https://a/x/CVN%20BIOS.zip").unwrap(),
            "CVN BIOS.zip"
        );
        assert_eq!(file_name("https://a/x/../../etc/passwd").unwrap(), "passwd");
        assert_eq!(file_name("https://a/x/a%00b.zip").unwrap(), "a_b.zip");
        assert!(file_name("https://a/x/").is_none());
        assert!(file_name("https://a/x/..").is_none());
    }

    #[test]
    fn matches_board_names_across_spelling() {
        let products = vec![
            (832, "CVN X570 GAMING PRO V14".to_string()),
            (880, "CVN X570M GAMING PRO V14".to_string()),
            (891, "CVN X570M GAMING FROZEN V14".to_string()),
        ];
        assert_eq!(
            pick_product(&products, "CVN X570M GAMING PRO V14"),
            Some(880)
        );
        assert_eq!(
            pick_product(&products, "  cvn   x570m gaming pro v14 "),
            Some(880)
        );
        // A suffix the site does not carry still resolves.
        assert_eq!(
            pick_product(&products, "CVN X570M GAMING PRO V14 (1030)"),
            Some(880)
        );
        assert_eq!(pick_product(&products, "BATTLE-AX B450M"), None);
    }

    #[test]
    fn compares_firmware_versions() {
        assert_eq!(
            firmware_number("CVN X570M GAMING PRO V14 BIOS 1016"),
            Some(1016)
        );
        assert_eq!(firmware_number("no digits here"), None);
        assert!(is_newer_firmware("... BIOS 1016", "1015"));
        assert!(!is_newer_firmware("... BIOS 1015", "1015"));
        assert!(!is_newer_firmware("... BIOS 1014", "1015"));
        assert!(!is_newer_firmware("... BIOS 1016", ""));
    }

    #[test]
    fn formats_dates() {
        assert_eq!(iso_date("07/20/2026 10:49:01"), "2026-07-20");
        assert_eq!(iso_date(""), "");
        assert_eq!(iso_date("not a date"), "");
    }

    /// Talks to colorfulgroup.com. Ignored by default so the suite stays
    /// offline; run with `cargo test -p colorctl-core -- --ignored live_`.
    #[test]
    #[ignore]
    fn live_catalogue_for_a_known_board() {
        let catalogue = fetch_catalogue("CVN X570M GAMING PRO V14").expect("catalogue");
        assert_eq!(catalogue.product_id, 880);
        assert!(
            catalogue.files.len() > 10,
            "expected a full catalogue, got {}",
            catalogue.files.len()
        );
        assert_eq!(catalogue.files[0].kind, DriverKind::Bios);
        assert!(catalogue.files.iter().all(|f| is_allowed_url(&f.url)));
        assert_eq!(
            catalogue.files.len(),
            21,
            "relative-path entries must survive"
        );
        assert!(
            catalogue
                .files
                .iter()
                .any(|f| f.url.starts_with("https://www.colorfulgroup.com/")),
            "site-relative entries should resolve against the group site"
        );

        // The manual is the smallest entry, so it exercises the streaming,
        // hashing and rename path without pulling a 200 MB BIOS image.
        let manual = catalogue
            .files
            .iter()
            .find(|f| f.kind == DriverKind::Manual)
            .expect("manual entry");
        let dir = std::env::temp_dir().join("opencolorhub-drivers-test");
        let done = download(manual, &dir).expect("download");
        assert_eq!(done.path, dir.join(&manual.file_name));
        assert!(
            done.bytes > 1_000,
            "suspiciously small: {} bytes",
            done.bytes
        );
        assert_eq!(done.sha256.len(), 64);
        assert!(
            !dir.join(format!("{}.part", manual.file_name)).exists(),
            "partial file left behind"
        );
        let _ = std::fs::remove_file(&done.path);
    }
}
