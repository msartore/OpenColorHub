//! Motherboard auto-detection.
//!
//! Everything downstream of this module assumes a Colorful board with the
//! NCT5584D Super I/O and the onboard HID RGB controller. This module reads the
//! platform's SMBIOS/DMI view of the board, matches it against the verified
//! catalogue, and reports how confident that match is — so the app can refuse
//! to poke registers on an unrelated machine instead of guessing.
//!
//! Detection sources:
//!
//! * Linux — `/sys/class/dmi/id/*` (world-readable, no root needed).
//! * Windows — `HKLM\HARDWARE\DESCRIPTION\System\BIOS` via `reg query`.
//!
//! Every source funnels into [`classify`], which is pure and unit-tested.

use std::path::Path;
// Used by the Windows registry reader, which is compiled on every platform so
// its parser stays testable — hence no cfg gate here.
use std::process::Command;

use serde::{Deserialize, Serialize};

/// Board families that shipped with the NCT5584D + Colorful RGB controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoardFamily {
    Cvn,
    IGame,
    Colorfire,
    BattleAx,
    ICafe,
}

impl BoardFamily {
    pub fn id(&self) -> &'static str {
        match self {
            BoardFamily::Cvn => "CVN",
            BoardFamily::IGame => "iGame",
            BoardFamily::Colorfire => "COLORFIRE",
            BoardFamily::BattleAx => "BATTLE-AX",
            BoardFamily::ICafe => "iCafe",
        }
    }
}

/// One verified board model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoardEntry {
    pub name: &'static str,
    pub family: BoardFamily,
    /// Chipset generation, e.g. `B650`, `B850`, `X870`.
    pub series: &'static str,
}

/// Catalogue extracted from the vendor software (see `COMPATIBILITY.md`).
pub const BOARDS: &[BoardEntry] = &[
    // --- B850 / B840 / X870 (AM5, NCT5584D) ---
    BoardEntry {
        name: "CVN B850 GAMING PRO WIFI7",
        family: BoardFamily::Cvn,
        series: "B850",
    },
    BoardEntry {
        name: "CVN B850M ARK FROZEN",
        family: BoardFamily::Cvn,
        series: "B850",
    },
    BoardEntry {
        name: "CVN B850M GAMING FROZEN",
        family: BoardFamily::Cvn,
        series: "B850",
    },
    BoardEntry {
        name: "CVN B850I FROZEN WIFI",
        family: BoardFamily::Cvn,
        series: "B850",
    },
    BoardEntry {
        name: "CVN B850I GAMING FROZEN",
        family: BoardFamily::Cvn,
        series: "B850",
    },
    BoardEntry {
        name: "CVN X870 GAMING FROZEN",
        family: BoardFamily::Cvn,
        series: "X870",
    },
    BoardEntry {
        name: "iGame B850I MINI OC",
        family: BoardFamily::IGame,
        series: "B850",
    },
    BoardEntry {
        name: "COLORFIRE B850M-A MEOW WIFI ORANGE",
        family: BoardFamily::Colorfire,
        series: "B850",
    },
    BoardEntry {
        name: "COLORFIRE B850M-MEOW WIFI7",
        family: BoardFamily::Colorfire,
        series: "B850",
    },
    BoardEntry {
        name: "BATTLE-AX B850M-PLUS PRO WIFI",
        family: BoardFamily::BattleAx,
        series: "B850",
    },
    BoardEntry {
        name: "BATTLE-AX B850M-PLUS WIFI",
        family: BoardFamily::BattleAx,
        series: "B850",
    },
    BoardEntry {
        name: "BATTLE-AX B850M-PLUS S WIFI7",
        family: BoardFamily::BattleAx,
        series: "B850",
    },
    BoardEntry {
        name: "BATTLE-AX B850M-GHA WIFI",
        family: BoardFamily::BattleAx,
        series: "B850",
    },
    BoardEntry {
        name: "BATTLE-AX B850M-T WIFI",
        family: BoardFamily::BattleAx,
        series: "B850",
    },
    BoardEntry {
        name: "BATTLE-AX B850M-E WIFI",
        family: BoardFamily::BattleAx,
        series: "B850",
    },
    BoardEntry {
        name: "BATTLE-AX B840M-D PRO",
        family: BoardFamily::BattleAx,
        series: "B840",
    },
    BoardEntry {
        name: "BATTLE-AX B840M-GHA WIFI",
        family: BoardFamily::BattleAx,
        series: "B840",
    },
    BoardEntry {
        name: "BATTLE-AX X870A-GHA WIFI",
        family: BoardFamily::BattleAx,
        series: "X870",
    },
    BoardEntry {
        name: "iCafe B850M-G DELUXE",
        family: BoardFamily::ICafe,
        series: "B850",
    },
    // --- B650 (AM5, NCT5584D) ---
    BoardEntry {
        name: "CVN B650 GAMING FROZEN",
        family: BoardFamily::Cvn,
        series: "B650",
    },
    BoardEntry {
        name: "CVN B650M GAMING FROZEN",
        family: BoardFamily::Cvn,
        series: "B650",
    },
    BoardEntry {
        name: "COLORFIRE B650M-MEOW WIFI ORANGE",
        family: BoardFamily::Colorfire,
        series: "B650",
    },
    BoardEntry {
        name: "BATTLE-AX B650A-GHA WIFI",
        family: BoardFamily::BattleAx,
        series: "B650",
    },
    BoardEntry {
        name: "BATTLE-AX B650M-PLUS",
        family: BoardFamily::BattleAx,
        series: "B650",
    },
    BoardEntry {
        name: "BATTLE-AX B650M-PLUS WIFI",
        family: BoardFamily::BattleAx,
        series: "B650",
    },
    BoardEntry {
        name: "BATTLE-AX B650M-WHITE WIFI",
        family: BoardFamily::BattleAx,
        series: "B650",
    },
    BoardEntry {
        name: "BATTLE-AX B650M-A PLUS",
        family: BoardFamily::BattleAx,
        series: "B650",
    },
    BoardEntry {
        name: "BATTLE-AX B650M-D PRO",
        family: BoardFamily::BattleAx,
        series: "B650",
    },
    BoardEntry {
        name: "BATTLE-AX B650M-E WIFI",
        family: BoardFamily::BattleAx,
        series: "B650",
    },
    BoardEntry {
        name: "BATTLE-AX B650M-E PRO",
        family: BoardFamily::BattleAx,
        series: "B650",
    },
    BoardEntry {
        name: "BATTLE-AX B650M-GHA WIFI",
        family: BoardFamily::BattleAx,
        series: "B650",
    },
    BoardEntry {
        name: "BATTLE-AX B650M-T",
        family: BoardFamily::BattleAx,
        series: "B650",
    },
    BoardEntry {
        name: "BATTLE-AX B650M-T WIFI",
        family: BoardFamily::BattleAx,
        series: "B650",
    },
    BoardEntry {
        name: "iCafe B650M-G DELUXE",
        family: BoardFamily::ICafe,
        series: "B650",
    },
    BoardEntry {
        name: "iCafe B650M-PLUS DELUXE",
        family: BoardFamily::ICafe,
        series: "B650",
    },
];

/// Series strings the NCT5584D was shipped on.
pub const SUPPORTED_SERIES: &[&str] = &["A620", "B650", "B840", "B850", "X670", "X870"];

/// Colorful chipsets that predate the NCT5584D generation. A board from one of
/// these is still ours — so it is not "foreign" — but its Super I/O is a
/// different part, and the probe (not the catalogue) has to decide.
pub const LEGACY_SERIES: &[&str] = &[
    "A320", "B350", "X370", "B450", "X470", "A520", "B550", "X570",
];

/// How strongly the detected board matches the catalogue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoardConfidence {
    /// Exact catalogue match — upstream lists this model as tested.
    Verified,
    /// Colorful board of a known family/series but not an exact catalogue entry.
    Compatible,
    /// Could not read the identity at all.
    Unknown,
    /// A board from another vendor: fan/LED control must not touch hardware.
    Unsupported,
}

impl BoardConfidence {
    pub fn label(&self) -> &'static str {
        match self {
            BoardConfidence::Verified => "verified",
            BoardConfidence::Compatible => "compatible",
            BoardConfidence::Unknown => "unknown",
            BoardConfidence::Unsupported => "unsupported",
        }
    }

    /// True unless the board is confirmed to belong to another vendor.
    ///
    /// `Unknown` still allows the probe: the Super I/O chip ID is the final
    /// authority and it refuses to write on anything but an NCT5584D.
    pub fn allows_hardware(&self) -> bool {
        !self.is_foreign()
    }

    /// True when the board is confirmed *not* ours (as opposed to merely unknown).
    pub fn is_foreign(&self) -> bool {
        matches!(self, BoardConfidence::Unsupported)
    }
}

/// Raw SMBIOS identity of the board.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct BoardIdentity {
    pub vendor: String,
    pub name: String,
    pub version: String,
    pub bios_vendor: String,
    pub bios_version: String,
    pub system_vendor: String,
    pub system_product: String,
}

impl BoardIdentity {
    /// `Colorful CVN B650M GAMING FROZEN V14` — trimmed, no empty parts.
    pub fn display_name(&self) -> String {
        let vendor = self.vendor.trim();
        let name = self.name.trim();
        match (vendor.is_empty(), name.is_empty()) {
            (false, false) => format!("{vendor} {name}"),
            (true, false) => name.to_string(),
            (false, true) => vendor.to_string(),
            (true, true) => "unknown board".to_string(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.vendor.trim().is_empty() && self.name.trim().is_empty()
    }
}

/// Result of detection: identity, catalogue match and human notes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoardInfo {
    pub identity: Option<BoardIdentity>,
    pub confidence: BoardConfidence,
    pub family: Option<String>,
    pub series: Option<String>,
    /// Catalogue entry that matched exactly.
    pub matched: Option<String>,
    pub catalog_size: usize,
    /// `dmi`, `registry`, `demo` or `none`.
    pub source: String,
    pub notes: Vec<String>,
}

impl Default for BoardInfo {
    fn default() -> Self {
        Self::unknown("none", "board identity was not read")
    }
}

impl BoardInfo {
    pub fn unknown(source: &str, note: &str) -> Self {
        Self {
            identity: None,
            confidence: BoardConfidence::Unknown,
            family: None,
            series: None,
            matched: None,
            catalog_size: BOARDS.len(),
            source: source.to_string(),
            notes: vec![note.to_string()],
        }
    }

    pub fn display_name(&self) -> String {
        self.identity
            .as_ref()
            .map(|i| i.display_name())
            .unwrap_or_else(|| "unidentified board".to_string())
    }
}

// ---------------------------------------------------------------- matching --

/// Uppercases, collapses whitespace and drops revision suffixes so that
/// `"CVN B650M Gaming Frozen V14"` and `"CVN B650M GAMING FROZEN V14 (mATX)"`
/// compare equal to the catalogue entry.
pub fn normalize(name: &str) -> String {
    let mut text = name.trim().to_uppercase();

    // Drop parenthetical hints such as "(ATX)" / "(mATX)".
    if let Some(index) = text.find('(') {
        text.truncate(index);
    }

    let mut words: Vec<&str> = text
        .split_whitespace()
        .filter(|word| !matches!(*word, "/" | "|" | "REV" | "VER"))
        .collect();
    // Trailing revision tokens: V14, V14A, V15, V1, REV 1.0, VER 1.0 …
    while let Some(last) = words.last() {
        let token = *last;
        let revision = (token.starts_with('V')
            && token.len() <= 5
            && token[1..].chars().all(|c| c.is_ascii_alphanumeric())
            && token[1..]
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_digit()))
            || token == "REV"
            || (token.starts_with("REV") && token.len() <= 6);
        if revision {
            words.pop();
        } else {
            break;
        }
    }
    words.join(" ")
}

fn looks_like_colorful(identity: &BoardIdentity) -> bool {
    let haystack = format!(
        "{} {} {}",
        identity.vendor, identity.system_vendor, identity.system_product
    )
    .to_uppercase();
    ["COLORFUL", "COLORFIRE", "七彩虹", "SEVEN-COLOR"]
        .iter()
        .any(|needle| haystack.contains(needle))
        || BOARDS
            .iter()
            .any(|entry| normalize(&identity.name).starts_with(&normalize(entry.name)))
}

fn series_of(name: &str) -> Option<String> {
    let normalised = normalize(name);
    let words: Vec<&str> = normalised.split([' ', '-', '_']).collect();
    SUPPORTED_SERIES
        .iter()
        .chain(LEGACY_SERIES.iter())
        .find(|series| words.iter().any(|word| word.starts_with(**series)))
        .map(|series| series.to_string())
}

/// Matches an identity against the catalogue. Pure: all platform detection
/// funnels through here, which is what the tests exercise.
pub fn classify(identity: Option<BoardIdentity>, source: &str) -> BoardInfo {
    let Some(identity) = identity.filter(|i| !i.is_empty()) else {
        return BoardInfo::unknown(source, "SMBIOS did not report a board name");
    };

    let key = normalize(&identity.name);
    let exact = BOARDS
        .iter()
        .find(|entry| normalize(entry.name) == key)
        .or_else(|| {
            // The firmware sometimes appends a marketing suffix (WIFI7, plus
            // revisions we already stripped), so accept a prefix match too.
            BOARDS.iter().find(|entry| {
                let candidate = normalize(entry.name);
                key.starts_with(&candidate) || candidate.starts_with(&key)
            })
        });

    let series = exact
        .map(|entry| entry.series.to_string())
        .or_else(|| series_of(&identity.name));
    let colorful = looks_like_colorful(&identity);
    let legacy = series
        .as_deref()
        .is_some_and(|s| LEGACY_SERIES.contains(&s));
    let same_generation = series
        .as_deref()
        .is_none_or(|s| SUPPORTED_SERIES.contains(&s));

    let mut notes = Vec::new();
    let (confidence, family, matched) = if let Some(entry) = exact {
        notes.push(format!(
            "{} matched the verified catalogue ({} series).",
            entry.name, entry.series
        ));
        (
            BoardConfidence::Verified,
            Some(entry.family.id().to_string()),
            Some(entry.name.to_string()),
        )
    } else if colorful && legacy {
        notes.push(format!(
            "{} is a Colorful board from the {} generation, which predates the NCT5584D. The Super I/O probe decides whether fan control works; nothing is written until the chip ID matches.",
            identity.display_name(),
            series.as_deref().unwrap_or("legacy")
        ));
        (BoardConfidence::Unknown, None, None)
    } else if colorful && same_generation {
        notes.push(
            "Colorful board that is not in the verified catalogue — registers are shared across the family, but test fan curves carefully."
                .to_string(),
        );
        (BoardConfidence::Compatible, None, None)
    } else if colorful {
        notes.push(
            "Colorful board on an unfamiliar chipset — the Super I/O probe decides before anything is written."
                .to_string(),
        );
        (BoardConfidence::Unknown, None, None)
    } else {
        notes.push(format!(
            "{} does not look like a Colorful board; hardware control is disabled to avoid writing to an unrelated Super I/O.",
            identity.display_name()
        ));
        (BoardConfidence::Unsupported, None, None)
    };

    if series.is_none() && confidence != BoardConfidence::Unsupported {
        notes.push(
            "Chipset series could not be derived from the board name; the Super I/O probe decides."
                .to_string(),
        );
    }

    if !identity.bios_version.trim().is_empty() {
        notes.push(format!("BIOS {}", identity.bios_version.trim()));
    }

    BoardInfo {
        identity: Some(identity),
        confidence,
        family,
        series,
        matched,
        catalog_size: BOARDS.len(),
        source: source.to_string(),
        notes,
    }
}

// -------------------------------------------------------------- detection --

/// Detects the board on the current platform.
pub fn detect() -> BoardInfo {
    #[cfg(target_os = "linux")]
    {
        detect_from_dmi(Path::new("/sys/class/dmi/id"))
    }

    #[cfg(target_os = "windows")]
    {
        detect_from_registry()
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        BoardInfo::unknown("none", "no board detection for this platform")
    }
}

/// Linux: DMI strings live in sysfs and are readable without privileges.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn detect_from_dmi(dir: &Path) -> BoardInfo {
    let read = |name: &str| {
        std::fs::read_to_string(dir.join(name))
            .map(|value| value.trim().to_string())
            .unwrap_or_default()
    };

    let identity = BoardIdentity {
        vendor: read("board_vendor"),
        name: read("board_name"),
        version: read("board_version"),
        bios_vendor: read("bios_vendor"),
        bios_version: read("bios_version"),
        system_vendor: read("sys_vendor"),
        system_product: read("product_name"),
    };

    if identity.is_empty() {
        return BoardInfo::unknown(
            "dmi",
            &format!(
                "no DMI board identity under {} (container or hidden firmware table?)",
                dir.display()
            ),
        );
    }

    classify(Some(identity), "dmi")
}

/// Windows: `reg query` on the SMBIOS mirror key. Parsed by a separate
/// function so it is testable without a registry.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub fn detect_from_registry() -> BoardInfo {
    const KEY: &str = r"HKLM\HARDWARE\DESCRIPTION\System\BIOS";

    let output = Command::new("reg").args(["query", KEY]).output();
    let Ok(output) = output else {
        return BoardInfo::unknown("registry", "could not run `reg query` to read SMBIOS data");
    };
    if !output.status.success() {
        return BoardInfo::unknown("registry", "SMBIOS registry key is not readable");
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let pairs = parse_reg_query(&text);
    let get = |key: &str| {
        pairs
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(key))
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };

    let identity = BoardIdentity {
        vendor: get("BaseBoardManufacturer"),
        name: get("BaseBoardProduct"),
        version: get("BaseBoardVersion"),
        bios_vendor: get("BIOSVendor"),
        bios_version: get("BIOSVersion"),
        system_vendor: get("SystemManufacturer"),
        system_product: get("SystemProductName"),
    };

    if identity.is_empty() {
        return BoardInfo::unknown("registry", "SMBIOS registry key had no board values");
    }

    classify(Some(identity), "registry")
}

/// Parses `reg query` output lines of the form
/// `    BaseBoardProduct    REG_SZ    CVN B650M GAMING FROZEN V14`.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub fn parse_reg_query(text: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("HKEY") || trimmed.starts_with("ERROR") {
            continue;
        }
        let Some((name, rest)) = trimmed.split_once("REG_SZ") else {
            continue;
        };
        let name = name.trim();
        let value = rest.trim();
        if name.is_empty() {
            continue;
        }
        pairs.push((name.to_string(), value.to_string()));
    }
    pairs
}

/// Demo identities used by the browser mock and the CLI `--backend simulated`
/// path so screenshots and tests show a realistic board.
pub fn demo_board() -> BoardInfo {
    classify(
        Some(BoardIdentity {
            vendor: "Colorful Technology And Development Co.,LTD".to_string(),
            name: "CVN B650M GAMING FROZEN V14".to_string(),
            version: "1.0".to_string(),
            bios_vendor: "American Megatrends International, LLC.".to_string(),
            bios_version: "1.0.7 (demo)".to_string(),
            system_vendor: "Colorful Technology And Development Co.,LTD".to_string(),
            system_product: "CVN B650M GAMING FROZEN V14".to_string(),
        }),
        "demo",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(vendor: &str, name: &str) -> BoardIdentity {
        BoardIdentity {
            vendor: vendor.to_string(),
            name: name.to_string(),
            ..BoardIdentity::default()
        }
    }

    #[test]
    fn normalization_strips_revisions_and_hints() {
        assert_eq!(
            normalize("CVN B650M Gaming Frozen V14"),
            "CVN B650M GAMING FROZEN"
        );
        assert_eq!(
            normalize("CVN B650 GAMING FROZEN V14 (ATX)"),
            "CVN B650 GAMING FROZEN"
        );
        assert_eq!(
            normalize("iCafe B850M-G DELUXE V14A"),
            "ICAFE B850M-G DELUXE"
        );
        assert_eq!(
            normalize("BATTLE-AX B650M-T V14 / V15"),
            "BATTLE-AX B650M-T"
        );
        assert_eq!(normalize(" "), "");
    }

    #[test]
    fn exact_match_is_verified() {
        let info = classify(
            Some(identity(
                "Colorful Technology And Development Co.,LTD",
                "CVN B650M GAMING FROZEN V14",
            )),
            "dmi",
        );
        assert_eq!(info.confidence, BoardConfidence::Verified);
        assert_eq!(info.matched.as_deref(), Some("CVN B650M GAMING FROZEN"));
        assert_eq!(info.family.as_deref(), Some("CVN"));
        assert_eq!(info.series.as_deref(), Some("B650"));
        assert!(info.confidence.allows_hardware());
        assert!(!info.confidence.is_foreign());
        assert!(info.notes.iter().any(|n| n.contains("verified catalogue")));
    }

    #[test]
    fn revision_suffix_still_matches_catalogue() {
        for detected in [
            "CVN B650M GAMING FROZEN",
            "cvn b650m gaming frozen v15",
            "CVN B650M GAMING FROZEN V14 (mATX)",
        ] {
            let info = classify(Some(identity("Colorful", detected)), "dmi");
            assert_eq!(info.confidence, BoardConfidence::Verified, "{detected}");
        }
    }

    #[test]
    fn unknown_colorful_board_is_compatible() {
        let info = classify(
            Some(identity("Colorful Technology", "CVN B900X UNRELEASED V1")),
            "dmi",
        );
        assert_eq!(info.confidence, BoardConfidence::Compatible);
        assert!(info.matched.is_none());
        assert!(info.confidence.allows_hardware());
        assert!(info.series.is_none());
    }

    #[test]
    fn foreign_board_is_unsupported() {
        let info = classify(
            Some(identity(
                "ASUSTeK COMPUTER INC.",
                "ROG STRIX B650E-F GAMING WIFI",
            )),
            "dmi",
        );
        assert_eq!(info.confidence, BoardConfidence::Unsupported);
        assert!(!info.confidence.allows_hardware());
        assert!(info.confidence.is_foreign());
        assert!(
            info.notes
                .iter()
                .any(|n| n.contains("does not look like a Colorful board"))
        );
        // The series is still parsed: it tells support why it was rejected.
        assert_eq!(info.series.as_deref(), Some("B650"));
    }

    #[test]
    fn legacy_colorful_board_is_unknown_but_not_foreign() {
        // A real board: Colorful CVN X570M GAMING PRO (read from the registry on
        // a Windows host). Ours, but a different Super I/O generation.
        let info = classify(
            Some(identity(
                "Colorful Technology And Development Co.,LTD",
                "CVN X570M GAMING PRO",
            )),
            "registry",
        );
        assert_eq!(info.confidence, BoardConfidence::Unknown);
        assert!(!info.confidence.is_foreign());
        assert_eq!(info.series.as_deref(), Some("X570"));
        assert!(
            info.notes
                .iter()
                .any(|n| n.contains("predates the NCT5584D")),
            "notes: {:?}",
            info.notes
        );
        // The probe still decides, so the backend may try.
        assert!(info.confidence.allows_hardware());
    }

    #[test]
    fn empty_identity_is_unknown_not_unsupported() {
        let info = classify(Some(BoardIdentity::default()), "dmi");
        assert_eq!(info.confidence, BoardConfidence::Unknown);
        assert!(!info.confidence.is_foreign());
        let info = classify(None, "registry");
        assert_eq!(info.confidence, BoardConfidence::Unknown);
        assert_eq!(info.source, "registry");
    }

    #[test]
    fn dmi_directory_is_parsed() {
        let dir = std::env::temp_dir().join(format!("boardutil-dmi-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("board_vendor"),
            "Colorful Technology And Development Co.,LTD\n",
        )
        .unwrap();
        std::fs::write(dir.join("board_name"), "CVN B850M GAMING FROZEN V14\n").unwrap();
        std::fs::write(dir.join("bios_version"), "2.0.3\n").unwrap();
        std::fs::write(dir.join("product_name"), "CVN B850M GAMING FROZEN V14\n").unwrap();

        let info = detect_from_dmi(&dir);
        assert_eq!(info.confidence, BoardConfidence::Verified);
        assert_eq!(info.matched.as_deref(), Some("CVN B850M GAMING FROZEN"));
        assert_eq!(info.series.as_deref(), Some("B850"));
        assert_eq!(info.source, "dmi");
        assert!(info.display_name().contains("Colorful"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_dmi_directory_is_reported_cleanly() {
        let info = detect_from_dmi(Path::new("Z:/definitely/not/here"));
        assert_eq!(info.confidence, BoardConfidence::Unknown);
        assert!(info.notes[0].contains("no DMI board identity"));
    }

    #[test]
    fn registry_output_is_parsed() {
        let sample = "\r\nHKEY_LOCAL_MACHINE\\HARDWARE\\DESCRIPTION\\System\\BIOS\r\n\
            \x20   BaseBoardManufacturer    REG_SZ    Colorful Technology And Development Co.,LTD\r\n\
            \x20   BaseBoardProduct    REG_SZ    CVN B650M GAMING FROZEN V14\r\n\
            \x20   BaseBoardVersion    REG_SZ    V14\r\n\
            \x20   BIOSVersion    REG_SZ    1.0.7\r\n\
            \r\n";
        let pairs = parse_reg_query(sample);
        assert_eq!(pairs.len(), 4);
        assert_eq!(pairs[1].0, "BaseBoardProduct");
        assert_eq!(pairs[1].1, "CVN B650M GAMING FROZEN V14");
        assert_eq!(pairs[3].1, "1.0.7");
    }

    #[test]
    fn every_catalogue_entry_normalizes_and_stays_unique() {
        let mut seen = std::collections::HashSet::new();
        for entry in BOARDS {
            let key = normalize(entry.name);
            assert!(!key.is_empty(), "{} normalised to nothing", entry.name);
            assert!(seen.insert(key.clone()), "duplicate catalogue key: {key}");
            assert!(
                SUPPORTED_SERIES.contains(&entry.series),
                "{} has an unknown series {}",
                entry.name,
                entry.series
            );
        }
        assert!(BOARDS.len() >= 30, "catalogue unexpectedly small");
    }

    #[test]
    fn demo_board_is_verified() {
        let info = demo_board();
        assert_eq!(info.confidence, BoardConfidence::Verified);
        assert_eq!(info.source, "demo");
        assert!(info.display_name().contains("CVN B650M"));
    }

    #[test]
    fn serializes_for_the_frontend() {
        let json = serde_json::to_string(&demo_board()).unwrap();
        assert!(json.contains("\"confidence\":\"verified\""));
        assert!(json.contains("\"series\":\"B650\""));
        assert!(json.contains("\"source\":\"demo\""));
    }
}
