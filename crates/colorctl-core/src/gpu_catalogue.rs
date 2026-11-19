//! GPU model catalogue keyed by PCI subsystem ID.
//!
//! The mappings come from PolychromeSYNC v1.0.50's `UpdateFW.ini`
//! `[VGA_Support]` table. A key is `(subsystem_device << 16) | subsystem_vendor`,
//! e.g. `0x51031849` = subsystem device `0x5103`, vendor `0x1849` (ASRock).
//!
//! Matching a card to a profile lets the rest of the app pick the right
//! transport, zone layout and effect set instead of treating every GPU alike.

use crate::gpu::{GPU_ZONES, GpuVendor};

/// PCI identity of one display adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PciId {
    pub vendor: u16,
    pub device: u16,
    pub sub_vendor: u16,
    pub sub_device: u16,
}

impl PciId {
    /// The key used by the PolychromeSYNC catalogue.
    pub fn catalogue_key(&self) -> u32 {
        ((self.sub_device as u32) << 16) | self.sub_vendor as u32
    }
}

/// How a detected card should be driven.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuProfile {
    /// Human model name, e.g. `RX 9070 XT Steel Legend`.
    pub model: String,
    pub vendor: GpuVendor,
    /// I²C address of its RGB controller.
    pub i2c_address: u8,
    /// Zones this card exposes.
    pub zones: &'static [crate::gpu::GpuZone],
    /// Highest effect ID the firmware accepts.
    pub max_mode: u8,
    /// One-line note shown in the UI detail panel.
    pub note: String,
}

const ASROCK_VENDOR: u16 = 0x1849;

/// (catalogue key, model name) pairs from `UpdateFW.ini`.
const CATALOGUE: &[(u32, &str)] = &[
    (0x51031849, "RX 5700 XT Taichi"),
    (0x51041849, "RX 5700 XT Taichi"),
    (0x51111849, "RX 5700 PG"),
    (0x51121849, "RX 5700 PG"),
    (0x51161849, "RX 5500 PG"),
    (0x50431849, "RX 580 PGU"),
    (0x50551849, "RX 590 GME PG"),
    (0x51211849, "RX 5600 PGD2"),
    (0x51221849, "RX 5600 PGD3"),
    (0x52011849, "RX 6800 XT Taichi"),
    (0x52021849, "RX 6800 XT PG"),
    (0x52031849, "RX 6800 CLP"),
    (0x52041849, "RX 6800 PG"),
    (0x52061849, "RX 6700 XT PGD"),
    (0x52121849, "RX 6900 XT PG"),
    (0x52381849, "RX 6900 XT PG"),
    (0x52131849, "RX 6900 OCF"),
    (0x52141849, "RX 6900 OCF"),
    (0x52201849, "RX 6600 XT PGD"),
    (0x52341849, "RX 6650 XT PGD"),
    (0x52321849, "RX 6750 XT PGD"),
    (0x52301849, "RX 6950 OCF"),
    (0x52311849, "RX 6950 OCF"),
    (0x60011849, "Arc A770"),
    (0x60101849, "Arc A770"),
    (0x60201849, "Arc B580 Steel Legend"),
    (0x53011849, "RX 7900 XTX AQUA"),
    (0x53021849, "RX 7900 XTX Taichi"),
    (0x53041849, "RX 7900 XTX PG"),
    (0x53071849, "RX 7900 XT Taichi"),
    (0x53081849, "RX 7900 XT PG"),
    (0x53171849, "RX 7900 XT PG"),
    (0x53111849, "RX 7600 PG"),
    (0x53211849, "RX 7600 PG"),
    (0x53241849, "RX 7600 PG"),
    (0x52391849, "RX 7600 SL"),
    (0x53121849, "RX 7600 SL"),
    (0x53221849, "RX 7700 XT SL"),
    (0x53271849, "RX 7700 XT SL"),
    (0x53311849, "RX 7700 XT SL"),
    (0x53181849, "RX 7700 XT SL"),
    (0x53191849, "RX 7700 XT SL"),
    (0x54051849, "RX 9070 SL"),
    (0x54121849, "RX 9070 GRE SLD"),
    (0x54031849, "RX 9070 XT SL"),
    (0x54011849, "RX 9070 XT TC"),
    (0x54141849, "RX 9070 XT SLD"),
    (0x54071849, "RX 9060 XT SL"),
    (0x54161849, "RX 9060 XT SL"),
];

/// Look up a card by its PCI identity, falling back to a vendor-generic
/// profile so an unknown board still gets a usable transport.
pub fn profile_for(pci: Option<PciId>, vendor: GpuVendor) -> GpuProfile {
    let model = pci.and_then(|p| {
        CATALOGUE
            .iter()
            .find(|(key, _)| *key == p.catalogue_key())
            .map(|(_, name)| *name)
    });
    let is_arc = model.is_some_and(|name| name.contains("Arc"));

    if let (Some(pci), Some(name)) = (pci, model) {
        return GpuProfile {
            model: name.to_string(),
            vendor,
            i2c_address: 0x36,
            zones: if is_arc { &GPU_ZONES[1..] } else { &GPU_ZONES },
            max_mode: 0x0F,
            note: if pci.sub_vendor == ASROCK_VENDOR {
                "ASRock Nuvoton RGB controller · verified profile".to_string()
            } else {
                "Matched vendor profile".to_string()
            },
        };
    }

    let generic_vendor = vendor.label();
    GpuProfile {
        model: pci
            .map(|p| format!("{generic_vendor} GPU {:04X}:{:04X}", p.device, p.sub_device))
            .unwrap_or_else(|| format!("{generic_vendor} GPU (unidentified)")),
        vendor,
        i2c_address: 0x36,
        zones: &GPU_ZONES,
        max_mode: 0x0F,
        note: "No exact catalogue match — using the generic 0x36 profile".to_string(),
    }
}

/// Enumerate display adapters and read their exact PCI IDs.
#[cfg(windows)]
pub fn enumerate_displays() -> Vec<PciId> {
    use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
        DIGCF_PRESENT, GUID_DEVCLASS_DISPLAY, SP_DEVINFO_DATA, SPDRP_HARDWAREID,
        SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo, SetupDiGetClassDevsW,
        SetupDiGetDeviceRegistryPropertyW,
    };

    unsafe {
        let set = SetupDiGetClassDevsW(
            &GUID_DEVCLASS_DISPLAY,
            std::ptr::null(),
            std::ptr::null_mut(),
            DIGCF_PRESENT,
        );
        if set == -1 || set == 0 {
            return Vec::new();
        }
        let mut out = Vec::new();
        let mut index = 0u32;
        loop {
            let mut info: SP_DEVINFO_DATA = std::mem::zeroed();
            info.cbSize = std::mem::size_of::<SP_DEVINFO_DATA>() as u32;
            if SetupDiEnumDeviceInfo(set, index, &mut info) == 0 {
                break;
            }
            index += 1;
            let mut buffer = [0u16; 512];
            let ok = SetupDiGetDeviceRegistryPropertyW(
                set,
                &info,
                SPDRP_HARDWAREID,
                std::ptr::null_mut(),
                buffer.as_mut_ptr() as *mut u8,
                (buffer.len() * 2) as u32,
                std::ptr::null_mut(),
            );
            if ok == 0 {
                continue;
            }
            let len = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
            let text = String::from_utf16_lossy(&buffer[..len]);
            if let Some(id) = parse_hardware_id(&text) {
                out.push(id);
            }
        }
        SetupDiDestroyDeviceInfoList(set);
        out
    }
}

#[cfg(windows)]
fn parse_hardware_id(text: &str) -> Option<PciId> {
    let upper = text.to_ascii_uppercase();
    let hex_at = |key: &str, digits: usize| -> Option<u32> {
        let start = upper.find(key)? + key.len();
        u32::from_str_radix(upper.get(start..start + digits)?, 16).ok()
    };
    let vendor = hex_at("VEN_", 4)? as u16;
    let device = hex_at("DEV_", 4)? as u16;
    let subsys = upper.find("SUBSYS_").and_then(|p| {
        let start = p + 7;
        Some((
            u32::from_str_radix(upper.get(start..start + 4)?, 16).ok()? as u16,
            u32::from_str_radix(upper.get(start + 4..start + 8)?, 16).ok()? as u16,
        ))
    });
    let (sub_device, sub_vendor) = subsys.unwrap_or((0, 0));
    Some(PciId {
        vendor,
        device,
        sub_vendor,
        sub_device,
    })
}

/// Enumerate display adapters from sysfs.
#[cfg(target_os = "linux")]
pub fn enumerate_displays() -> Vec<PciId> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir("/sys/bus/pci/devices") else {
        return out;
    };
    for entry in entries.flatten() {
        let base = entry.path();
        let class = std::fs::read_to_string(base.join("class")).unwrap_or_default();
        if !class.trim_start_matches("0x").starts_with("03") {
            continue;
        }
        let read_u16 = |name: &str| -> Option<u16> {
            let raw = std::fs::read_to_string(base.join(name)).ok()?;
            u16::from_str_radix(raw.trim().trim_start_matches("0x"), 16).ok()
        };
        if let (Some(vendor), Some(device)) = (read_u16("vendor"), read_u16("device")) {
            out.push(PciId {
                vendor,
                device,
                sub_vendor: read_u16("subsystem_vendor").unwrap_or(0),
                sub_device: read_u16("subsystem_device").unwrap_or(0),
            });
        }
    }
    out
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn enumerate_displays() -> Vec<PciId> {
    Vec::new()
}

/// All catalogue entries, for a UI "supported cards" list.
pub fn models() -> Vec<(&'static str, GpuVendor)> {
    CATALOGUE
        .iter()
        .map(|(_, name)| {
            let vendor = if name.starts_with("Arc") {
                GpuVendor::Intel
            } else {
                GpuVendor::Amd
            };
            (*name, vendor)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogue_key_is_subsystem_device_then_vendor() {
        let id = PciId {
            vendor: 0x1002,
            device: 0x73BF,
            sub_vendor: 0x1849,
            sub_device: 0x5301,
        };
        assert_eq!(id.catalogue_key(), 0x53011849);
        let profile = profile_for(Some(id), GpuVendor::Amd);
        assert_eq!(profile.model, "RX 7900 XTX AQUA");
        assert_eq!(profile.i2c_address, 0x36);
        assert_eq!(profile.zones.len(), 3);
    }

    #[test]
    fn arc_cards_use_fewer_zones() {
        let id = PciId {
            vendor: 0x8086,
            device: 0x56C0,
            sub_vendor: 0x1849,
            sub_device: 0x6001,
        };
        let profile = profile_for(Some(id), GpuVendor::Intel);
        assert_eq!(profile.model, "Arc A770");
        assert_eq!(profile.zones.len(), 2);
    }

    #[test]
    fn unknown_cards_get_a_generic_profile() {
        let id = PciId {
            vendor: 0x10DE,
            device: 0x2484,
            sub_vendor: 0x1462,
            sub_device: 0x1234,
        };
        let profile = profile_for(Some(id), GpuVendor::Nvidia);
        assert!(profile.model.contains("NVIDIA GPU"));
        assert!(profile.note.contains("generic"));
    }

    #[test]
    fn catalogue_has_the_expected_size() {
        assert_eq!(CATALOGUE.len(), 49);
    }

    #[test]
    fn catalogue_keys_are_unique() {
        let mut keys: Vec<u32> = CATALOGUE.iter().map(|(key, _)| *key).collect();
        keys.sort_unstable();
        let before = keys.len();
        keys.dedup();
        assert_eq!(keys.len(), before, "duplicate subsystem ID in catalogue");
    }

    #[test]
    fn every_catalogue_entry_resolves_to_a_profile() {
        for (key, expected) in CATALOGUE {
            let id = PciId {
                vendor: 0x1002,
                device: 0,
                sub_vendor: (*key & 0xFFFF) as u16,
                sub_device: (*key >> 16) as u16,
            };
            let profile = profile_for(Some(id), GpuVendor::Amd);
            assert_eq!(&profile.model, expected, "key {key:#010X}");
            assert!(!profile.zones.is_empty());
            assert_eq!(profile.i2c_address, 0x36);
            assert!(profile.max_mode >= 0x0F);
        }
    }

    #[test]
    fn asrock_matches_get_a_verified_note() {
        let id = PciId {
            vendor: 0x1002,
            device: 0x744C,
            sub_vendor: 0x1849,
            sub_device: 0x5403,
        };
        let profile = profile_for(Some(id), GpuVendor::Amd);
        assert_eq!(profile.model, "RX 9070 XT SL");
        assert!(profile.note.contains("verified"));
    }

    #[test]
    fn arc_b580_uses_two_zones() {
        let id = PciId {
            vendor: 0x8086,
            device: 0xE20B,
            sub_vendor: 0x1849,
            sub_device: 0x6020,
        };
        let profile = profile_for(Some(id), GpuVendor::Intel);
        assert_eq!(profile.model, "Arc B580 Steel Legend");
        assert_eq!(profile.zones.len(), 2);
    }

    #[test]
    fn generic_profile_works_without_any_pci_id() {
        let profile = profile_for(None, GpuVendor::Nvidia);
        assert!(profile.model.contains("unidentified"));
        assert_eq!(profile.zones.len(), crate::gpu::GPU_ZONES.len());
        assert!(profile.note.contains("generic"));
    }

    #[test]
    fn models_are_classified_by_vendor() {
        let all = models();
        assert_eq!(all.len(), CATALOGUE.len());
        assert!(
            all.iter()
                .any(|(name, vendor)| name.contains("Arc") && *vendor == GpuVendor::Intel)
        );
        assert!(
            all.iter()
                .any(|(name, vendor)| name.starts_with("RX") && *vendor == GpuVendor::Amd)
        );
    }
}
