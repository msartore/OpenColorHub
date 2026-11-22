//! GPU RGB control through the GPU vendor's own I²C transport.
//!
//! The wire protocol is the Nuvoton NUC121 controller ASRock puts on its
//! graphics cards, at I²C address `0x36`. It was recovered from
//! `PolychromeSYNC` v1.0.50 and independently confirmed by the OpenRGB
//! `ASRockRX9070XTGPUController`. Each zone is a single 12-byte block write:
//!
//! ```text
//! 10 00 <channel> <mode> <R> <G> <B> <speed> <brightness> <direction> 1A 00
//! ```
//!
//! The transport differs by vendor:
//!
//! * **AMD** — `ADL2_Display_WriteAndReadI2C` from `atiadlxx.dll`, line 1;
//! * **NVIDIA** — `NvAPI_I2CWriteEx` from `nvapi.dll`, port 1, non-DDC;
//! * **Intel** — `ctlI2CAccess` from `ControlLib.dll` / `igcl.dll`;
//! * **Linux** — `/dev/i2c-*` on an AMDGPU or Intel GPU adapter (`i2c-dev`).
//!
//! No kernel driver is bundled: these are the vendors' own user-mode APIs.
//! Protocol source: GPL-2.0-or-later OpenRGB controller, credited in NOTICE.md.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
// Only the Windows vendor transports cache their dynamically loaded API.
#[cfg(windows)]
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::gpu_catalogue::{self, PciId};
use crate::model::DeviceInfo;

/// I²C address of the onboard GPU RGB controller.
pub const GPU_I2C_ADDRESS: u8 = 0x36;

/// A GPU vendor, also the key for its I²C transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GpuVendor {
    Amd,
    Nvidia,
    Intel,
    Simulated,
}

impl GpuVendor {
    pub fn label(&self) -> &'static str {
        match self {
            GpuVendor::Amd => "AMD",
            GpuVendor::Nvidia => "NVIDIA",
            GpuVendor::Intel => "Intel",
            GpuVendor::Simulated => "Simulated",
        }
    }
}

/// One addressable zone on a GPU. Channel numbers are the controller's
/// hardware IDs, not an array index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GpuZone {
    pub id: &'static str,
    pub label: &'static str,
    pub channel: u8,
    pub leds: usize,
}

/// Zones shared by every controller this protocol drives.
pub const GPU_ZONES: [GpuZone; 3] = [
    GpuZone {
        id: "argb_header",
        label: "ARGB header",
        channel: 0x03,
        leds: 1,
    },
    GpuZone {
        id: "top_side",
        label: "Top / side",
        channel: 0x06,
        leds: 1,
    },
    GpuZone {
        id: "fan",
        label: "Fan",
        channel: 0x07,
        leds: 1,
    },
];

/// Hardware effect IDs, from the controller firmware / OpenRGB's mode map.
pub const GPU_MODES: [(u8, &str); 14] = [
    (0x01, "Static"),
    (0x02, "Breathing"),
    (0x03, "Strobe"),
    (0x04, "RGB cycle"),
    (0x05, "Random"),
    (0x07, "Color shift"),
    (0x08, "Visor"),
    (0x09, "Stacking"),
    (0x0A, "Fill wave"),
    (0x0B, "Traveling wave"),
    (0x0C, "Marquee color"),
    (0x0D, "Marquee random"),
    (0x0E, "Color wave"),
    (0x0F, "Rainbow"),
];

pub fn mode_label(mode: u8) -> &'static str {
    GPU_MODES
        .iter()
        .find(|(id, _)| *id == mode)
        .map(|(_, label)| *label)
        .unwrap_or("Unknown")
}

/// Persisted state for one zone. Keyed by `"<device>:<zone>"` so adding or
/// removing a GPU does not shift the other devices' settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GpuZoneState {
    pub color: [u8; 3],
    pub mode: u8,
    pub speed: u8,
    pub brightness: u8,
    pub direction: u8,
    pub enabled: bool,
}

impl Default for GpuZoneState {
    fn default() -> Self {
        Self {
            color: [255, 255, 255],
            mode: 0x01,
            speed: 0x80,
            brightness: 0xFF,
            direction: 0x00,
            enabled: true,
        }
    }
}

impl GpuZoneState {
    pub fn hex(&self) -> String {
        format!(
            "#{:02x}{:02x}{:02x}",
            self.color[0], self.color[1], self.color[2]
        )
    }
}

/// Whole lighting state, serialised to `gpu-led.state` beside `rgb.state`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct GpuLedState {
    pub enabled: bool,
    pub zones: BTreeMap<String, GpuZoneState>,
}

impl GpuLedState {
    /// Feature is on unless the user explicitly turned it off. A missing or
    /// broken file must not silently disable GPU lighting.
    fn enabled_defaults() -> Self {
        Self {
            enabled: true,
            ..Self::default()
        }
    }

    pub fn load(path: &Path) -> (Self, Option<String>) {
        match std::fs::read_to_string(path) {
            Ok(raw) => match serde_json::from_str::<Self>(&raw) {
                Ok(mut s) => {
                    if s.zones.is_empty() && !s.enabled {
                        s.enabled = true;
                    }
                    (s, None)
                }
                Err(e) => (
                    Self::enabled_defaults(),
                    Some(format!("malformed GPU state: {e}")),
                ),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Self::enabled_defaults(), None),
            Err(e) => (Self::enabled_defaults(), Some(e.to_string())),
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("state.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

/// A zone as reported to the UI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GpuZoneStatus {
    pub id: String,
    pub label: String,
    pub channel: u8,
    pub leds: usize,
    pub color: [u8; 3],
    pub hex: String,
    pub mode: u8,
    pub mode_label: String,
    pub speed: u8,
    pub brightness: u8,
    pub direction: u8,
    pub enabled: bool,
}

/// A detected GPU and its zones, as reported to the UI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GpuDeviceStatus {
    pub id: String,
    pub name: String,
    pub vendor: GpuVendor,
    pub transport: String,
    pub model: String,
    pub note: String,
    pub i2c_address: u8,
    pub max_mode: u8,
    pub zones: Vec<GpuZoneStatus>,
}

/// Everything the UI needs for the GPU section in one IPC object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GpuStatus {
    pub enabled: bool,
    pub devices: Vec<GpuDeviceStatus>,
    pub device: DeviceInfo,
}

impl GpuStatus {
    pub fn empty(reason: impl Into<String>) -> Self {
        Self {
            enabled: false,
            devices: Vec::new(),
            device: DeviceInfo::unavailable("No GPU RGB controller", "none", reason),
        }
    }
}

/// Raw I²C access. The 0x36 packet is built once, in [`build_frame`].
pub trait GpuI2c: Send {
    fn vendor(&self) -> GpuVendor;
    fn transport(&self) -> String;
    fn write_block(&mut self, addr: u8, data: &[u8]) -> Result<()>;
}

/// Builds the 12-byte controller frame.
pub fn build_frame(zone: &GpuZone, state: &GpuZoneState) -> [u8; 12] {
    let [r, g, b] = state.color;
    [
        0x10,
        0x00,
        zone.channel,
        state.mode,
        r,
        g,
        b,
        state.speed,
        state.brightness,
        state.direction,
        0x1A,
        0x00,
    ]
}

struct GpuDevice {
    id: String,
    name: String,
    vendor: GpuVendor,
    /// Model name resolved from the PCI subsystem ID.
    model: String,
    /// One-line capability note shown in the UI detail panel.
    note: String,
    i2c_address: u8,
    max_mode: u8,
    zones: Vec<GpuZone>,
    i2c: Box<dyn GpuI2c>,
}

impl GpuDevice {
    fn from_profile(
        id: String,
        vendor: GpuVendor,
        pci: Option<PciId>,
        i2c: Box<dyn GpuI2c>,
    ) -> Self {
        let profile = gpu_catalogue::profile_for(pci, vendor);
        Self {
            id,
            name: profile.model.clone(),
            vendor,
            model: profile.model,
            note: profile.note,
            i2c_address: profile.i2c_address,
            max_mode: profile.max_mode,
            zones: profile.zones.to_vec(),
            i2c,
        }
    }
}

/// Owns the detected GPU transports, the persisted state, and applies frames.
pub struct GpuLedBackend {
    devices: Vec<GpuDevice>,
    state: GpuLedState,
    state_path: PathBuf,
    info: DeviceInfo,
}

impl GpuLedBackend {
    /// Probes every available vendor transport. `simulated` skips the probe
    /// entirely and exposes one simulated card, so the demo and the test suite
    /// never touch real GPU hardware.
    pub fn detect(state_path: PathBuf, simulated: bool) -> Self {
        let (state, warning) = GpuLedState::load(&state_path);
        let devices = if simulated {
            vec![simulated_device()]
        } else {
            detect_devices()
        };
        let names: Vec<String> = devices.iter().map(|d| d.name.clone()).collect();
        let info = if devices.is_empty() {
            DeviceInfo::unavailable(
                "No GPU RGB controller",
                "AMD ADL / NVIDIA NVAPI / Intel IGCL / i2c-dev",
                warning.unwrap_or_else(|| {
                    "No GPU I²C bus with a controller at 0x36 was detected".to_string()
                }),
            )
        } else {
            let mut dev = DeviceInfo::hardware(
                if simulated {
                    "GPU RGB controllers (simulated)"
                } else {
                    "GPU RGB controllers"
                },
                format!(
                    "{} controller(s) at I²C 0x{:02X}",
                    devices.len(),
                    GPU_I2C_ADDRESS
                ),
            );
            dev.detail = names.join(", ");
            dev.simulated = simulated;
            dev
        };
        Self {
            devices,
            state,
            state_path,
            info,
        }
    }

    pub fn info(&self) -> DeviceInfo {
        self.info.clone()
    }

    pub fn state(&self) -> &GpuLedState {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut GpuLedState {
        &mut self.state
    }

    pub fn status(&self) -> GpuStatus {
        let devices = self
            .devices
            .iter()
            .map(|d| GpuDeviceStatus {
                id: d.id.clone(),
                name: d.name.clone(),
                vendor: d.vendor,
                transport: d.i2c.transport(),
                model: d.model.clone(),
                note: d.note.clone(),
                i2c_address: d.i2c_address,
                max_mode: d.max_mode,
                zones: d
                    .zones
                    .iter()
                    .map(|z| {
                        let s = self
                            .state
                            .zones
                            .get(&format!("{}:{}", d.id, z.id))
                            .copied()
                            .unwrap_or_default();
                        GpuZoneStatus {
                            id: z.id.to_string(),
                            label: z.label.to_string(),
                            channel: z.channel,
                            leds: z.leds,
                            color: s.color,
                            hex: s.hex(),
                            mode: s.mode,
                            mode_label: mode_label(s.mode).to_string(),
                            speed: s.speed,
                            brightness: s.brightness,
                            direction: s.direction,
                            enabled: s.enabled && self.state.enabled,
                        }
                    })
                    .collect(),
            })
            .collect();
        GpuStatus {
            enabled: self.state.enabled,
            devices,
            device: self.info(),
        }
    }

    /// Pushes the current state to every detected GPU. Returns zone writes.
    pub fn apply(&mut self) -> Result<usize> {
        if !self.state.enabled {
            return Ok(0);
        }
        let mut written = 0usize;
        for device in &mut self.devices {
            let address = device.i2c_address;
            for zone in device.zones.clone() {
                let key = format!("{}:{}", device.id, zone.id);
                let state = self.state.zones.get(&key).copied().unwrap_or_default();
                if !state.enabled {
                    continue;
                }
                let frame = build_frame(&zone, &state);
                device.i2c.write_block(address, &frame)?;
                written += 1;
            }
        }
        self.state.save(&self.state_path)?;
        Ok(written)
    }

    /// Re-applies after sleep. A vendor handle may have been invalidated; the
    /// transports reopen lazily where the API allows it.
    pub fn reassert(&mut self) -> Result<()> {
        self.apply()?;
        Ok(())
    }
}

fn simulated_device() -> GpuDevice {
    struct Sim;
    impl GpuI2c for Sim {
        fn vendor(&self) -> GpuVendor {
            GpuVendor::Simulated
        }
        fn transport(&self) -> String {
            "simulated".to_string()
        }
        fn write_block(&mut self, _: u8, _: &[u8]) -> Result<()> {
            Ok(())
        }
    }
    GpuDevice::from_profile("sim0".into(), GpuVendor::Simulated, None, Box::new(Sim))
}

#[cfg(windows)]
fn detect_devices() -> Vec<GpuDevice> {
    let inventory = gpu_catalogue::enumerate_displays();
    let for_vendor = |vendor: GpuVendor| -> Vec<PciId> {
        inventory
            .iter()
            .copied()
            .filter(|id| match vendor {
                GpuVendor::Amd => id.vendor == 0x1002,
                GpuVendor::Nvidia => id.vendor == 0x10DE,
                GpuVendor::Intel => id.vendor == 0x8086,
                GpuVendor::Simulated => false,
            })
            .collect()
    };
    let mut out = Vec::new();
    if let Some(d) = amd::detect(&for_vendor(GpuVendor::Amd)) {
        out.extend(d);
    }
    if let Some(d) = nvidia::detect(&for_vendor(GpuVendor::Nvidia)) {
        out.extend(d);
    }
    if let Some(d) = intel::detect(&for_vendor(GpuVendor::Intel)) {
        out.extend(d);
    }
    out
}

#[cfg(target_os = "linux")]
fn detect_devices() -> Vec<GpuDevice> {
    linux_i2c::detect(&gpu_catalogue::enumerate_displays())
}

#[cfg(not(any(windows, target_os = "linux")))]
fn detect_devices() -> Vec<GpuDevice> {
    Vec::new()
}

// ---------------------------------------------------------------------------
// Windows: AMD ADL
// ---------------------------------------------------------------------------

#[cfg(windows)]
mod amd {
    use super::*;
    use std::ffi::{c_char, c_void};
    use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

    #[repr(C)]
    struct AdlI2c {
        i_size: i32,
        i_line: i32,
        i_address: i32,
        i_offset: i32,
        i_action: i32,
        i_speed: i32,
        i_data_size: i32,
        pc_data: *const c_char,
    }
    const _: () = assert!(std::mem::size_of::<AdlI2c>() == 40);

    type MainCreate = unsafe extern "system" fn(
        unsafe extern "system" fn(i32) -> *mut c_void,
        i32,
        *mut *mut c_void,
    ) -> i32;
    type MainDestroy = unsafe extern "system" fn(*mut c_void) -> i32;
    type AdapterCount = unsafe extern "system" fn(*mut c_void, *mut i32) -> i32;
    type WriteI2c = unsafe extern "system" fn(*mut c_void, i32, *mut AdlI2c) -> i32;

    struct Api {
        _lib: *mut c_void,
        _create: MainCreate,
        destroy: MainDestroy,
        count: AdapterCount,
        write: WriteI2c,
    }
    unsafe impl Send for Api {}
    unsafe impl Sync for Api {}

    unsafe extern "system" fn alloc(size: i32) -> *mut c_void {
        unsafe extern "C" {
            fn malloc(n: usize) -> *mut c_void;
        }
        unsafe { malloc(size.max(0) as usize) }
    }

    fn api() -> Option<&'static Api> {
        static API: OnceLock<Option<Api>> = OnceLock::new();
        API.get_or_init(|| {
            for name in [
                r"C:\Windows\System32\atiadlxx.dll",
                "atiadlxx.dll",
                "atiadlxy.dll",
            ] {
                let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
                unsafe {
                    let lib = LoadLibraryW(wide.as_ptr());
                    if lib.is_null() {
                        continue;
                    }
                    let sym = |n: &str| {
                        let c: Vec<u8> = n.bytes().chain(std::iter::once(0)).collect();
                        GetProcAddress(lib, c.as_ptr()).map(|f| f as usize)
                    };
                    let (create, destroy, count, write) = (
                        sym("ADL2_Main_Control_Create"),
                        sym("ADL2_Main_Control_Destroy"),
                        sym("ADL2_Adapter_NumberOfAdapters_Get"),
                        sym("ADL2_Display_WriteAndReadI2C"),
                    );
                    if create.is_none() || destroy.is_none() || count.is_none() || write.is_none() {
                        continue;
                    }
                    return Some(Api {
                        _lib: lib,
                        destroy: std::mem::transmute::<usize, MainDestroy>(destroy.unwrap()),
                        count: std::mem::transmute::<usize, AdapterCount>(count.unwrap()),
                        write: std::mem::transmute::<usize, WriteI2c>(write.unwrap()),
                        // create is used once below, not stored
                        _create: std::mem::transmute::<usize, MainCreate>(create.unwrap()),
                    });
                }
            }
            None
        })
        .as_ref()
    }

    // One ADL context is shared by every adapter. Refcount it so it is
    // destroyed exactly once, when the last transport is dropped.
    struct AdlContext {
        api: &'static Api,
        context: *mut c_void,
    }
    // All ADL calls are serialised behind the service mutex, so sharing the
    // context across transports is safe as long as Arc can send it.
    unsafe impl Send for AdlContext {}
    unsafe impl Sync for AdlContext {}
    impl Drop for AdlContext {
        fn drop(&mut self) {
            unsafe { (self.api.destroy)(self.context) };
        }
    }

    struct Adl {
        ctx: std::sync::Arc<AdlContext>,
        adapter: i32,
    }
    unsafe impl Send for Adl {}

    impl GpuI2c for Adl {
        fn vendor(&self) -> GpuVendor {
            GpuVendor::Amd
        }
        fn transport(&self) -> String {
            format!("AMD ADL adapter {}", self.adapter)
        }
        fn write_block(&mut self, addr: u8, data: &[u8]) -> Result<()> {
            let mut i2c = AdlI2c {
                i_size: std::mem::size_of::<AdlI2c>() as i32,
                i_line: 1,
                i_address: (addr as i32) << 1,
                i_offset: 0,
                // ADL_DL_I2C_ACTIONWRITE is 0x02; 0x01 is ACTIONREAD.
                i_action: 2,
                i_speed: 100,
                i_data_size: data.len() as i32,
                pc_data: data.as_ptr() as *const c_char,
            };
            let rc = unsafe { (self.ctx.api.write)(self.ctx.context, self.adapter, &mut i2c) };
            if rc != 0 {
                Err(Error::backend(format!("ADL I²C write failed: {rc}")))
            } else {
                Ok(())
            }
        }
    }

    pub(super) fn detect(pcis: &[PciId]) -> Option<Vec<GpuDevice>> {
        let api = api()?;
        let mut context: *mut c_void = std::ptr::null_mut();
        // SAFETY: alloc returns CRT malloc'd memory, matching ADL's free().
        if unsafe { (api._create)(alloc, 1, &mut context) } != 0 || context.is_null() {
            return None;
        }
        let mut count = 0i32;
        unsafe { (api.count)(context, &mut count) };
        if count <= 0 {
            unsafe { (api.destroy)(context) };
            return None;
        }
        let ctx = std::sync::Arc::new(AdlContext { api, context });
        let mut out = Vec::new();
        // ADL reports one adapter per display, so a single GPU can appear many
        // times. Pair transports with physical cards from the PCI inventory.
        let limit = (count as usize).min(pcis.len().max(1));
        for adapter in 0..limit {
            out.push(GpuDevice::from_profile(
                format!("amd{adapter}"),
                GpuVendor::Amd,
                pcis.get(adapter).copied(),
                Box::new(Adl {
                    ctx: ctx.clone(),
                    adapter: adapter as i32,
                }),
            ));
        }
        Some(out)
    }
}

// ---------------------------------------------------------------------------
// Windows: NVIDIA NVAPI
// ---------------------------------------------------------------------------

#[cfg(windows)]
mod nvidia {
    use super::*;
    use std::ffi::c_void;
    use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

    #[repr(C)]
    struct NvI2cInfoV3 {
        version: u32,
        display_mask: u32,
        is_ddc_port: u8,
        dev_address: u8,
        _pad0: [u8; 6],
        reg_address: *mut u8,
        reg_addr_size: u32,
        _pad1: u32,
        data: *mut u8,
        size: u32,
        i2c_speed: u32,
        i2c_speed_khz: i32,
        port_id: u8,
        _pad2: [u8; 3],
        is_port_id_set: u32,
    }
    const _: () = assert!(std::mem::size_of::<NvI2cInfoV3>() == 64);

    type Init = unsafe extern "C" fn() -> i32;
    type EnumGpus = unsafe extern "C" fn(*mut *mut c_void, *mut u32) -> i32;
    type WriteEx = unsafe extern "C" fn(*mut c_void, *mut NvI2cInfoV3, *mut u32) -> i32;

    struct Api {
        _lib: *mut c_void,
        enum_gpus: EnumGpus,
        write: WriteEx,
    }
    unsafe impl Send for Api {}
    unsafe impl Sync for Api {}

    fn api() -> Option<&'static Api> {
        static API: OnceLock<Option<Api>> = OnceLock::new();
        API.get_or_init(|| {
            for name in ["nvapi64.dll", "nvapi.dll"] {
                let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
                unsafe {
                    let lib = LoadLibraryW(wide.as_ptr());
                    if lib.is_null() {
                        continue;
                    }
                    let sym = |n: &str| {
                        let c: Vec<u8> = n.bytes().chain(std::iter::once(0)).collect();
                        GetProcAddress(lib, c.as_ptr()).map(|f| f as usize)
                    };
                    let (init, enumerate, write) = (
                        sym("NvAPI_Initialize"),
                        sym("NvAPI_EnumPhysicalGPUs"),
                        sym("NvAPI_I2CWriteEx"),
                    );
                    if init.is_none() || enumerate.is_none() || write.is_none() {
                        continue;
                    }
                    let init: Init = std::mem::transmute(init.unwrap());
                    if init() != 0 {
                        continue;
                    }
                    return Some(Api {
                        _lib: lib,
                        enum_gpus: std::mem::transmute::<usize, EnumGpus>(enumerate.unwrap()),
                        write: std::mem::transmute::<usize, WriteEx>(write.unwrap()),
                    });
                }
            }
            None
        })
        .as_ref()
    }

    struct Nv {
        api: &'static Api,
        handle: *mut c_void,
    }
    unsafe impl Send for Nv {}

    impl GpuI2c for Nv {
        fn vendor(&self) -> GpuVendor {
            GpuVendor::Nvidia
        }
        fn transport(&self) -> String {
            "NVIDIA NVAPI I²C port 1".to_string()
        }
        fn write_block(&mut self, addr: u8, data: &[u8]) -> Result<()> {
            let version = ((std::mem::size_of::<NvI2cInfoV3>() as u32) << 16) | 3;
            let mut info = NvI2cInfoV3 {
                version,
                display_mask: 0,
                is_ddc_port: 0,
                dev_address: addr << 1,
                _pad0: [0; 6],
                reg_address: std::ptr::null_mut(),
                reg_addr_size: 0,
                _pad1: 0,
                data: data.as_ptr() as *mut u8,
                size: data.len() as u32,
                i2c_speed: 0xFFFF,
                i2c_speed_khz: 0, // NVAPI_I2C_SPEED_DEFAULT
                port_id: 1,
                _pad2: [0; 3],
                is_port_id_set: 1,
            };
            let mut unknown = 0u32;
            let rc = unsafe { (self.api.write)(self.handle, &mut info, &mut unknown) };
            if rc != 0 {
                Err(Error::backend(format!("NVAPI I²C write failed: {rc}")))
            } else {
                Ok(())
            }
        }
    }

    pub(super) fn detect(pcis: &[PciId]) -> Option<Vec<GpuDevice>> {
        let api = api()?;
        let mut handles = [std::ptr::null_mut(); 64];
        let mut count = 0u32;
        if unsafe { (api.enum_gpus)(handles.as_mut_ptr(), &mut count) } != 0 || count == 0 {
            return None;
        }
        let limit = (count as usize).min(pcis.len().max(1));
        Some(
            handles[..limit]
                .iter()
                .enumerate()
                .map(|(i, h)| {
                    GpuDevice::from_profile(
                        format!("nvidia{i}"),
                        GpuVendor::Nvidia,
                        pcis.get(i).copied(),
                        Box::new(Nv { api, handle: *h }),
                    )
                })
                .collect(),
        )
    }
}

// ---------------------------------------------------------------------------
// Windows: Intel IGCL
// ---------------------------------------------------------------------------

#[cfg(windows)]
mod intel {
    use super::*;
    use std::ffi::c_void;
    use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

    #[repr(C)]
    struct CtlInitArgs {
        size: u32,
        version: u8,
        _pad0: [u8; 3],
        app_version: u32,
        flags: u32,
        supported_version: u32,
        application_uid: [u8; 16],
    }
    const _: () = assert!(std::mem::size_of::<CtlInitArgs>() == 36);

    #[repr(C)]
    struct CtlI2cAccessArgs {
        size: u32,
        version: u8,
        _pad0: [u8; 3],
        data_size: u32,
        address: u32,
        op_type: u32,
        offset: u32,
        flags: u32,
        _pad1: u32,
        rad: u64,
        data: [u8; 128],
    }
    const _: () = assert!(std::mem::size_of::<CtlI2cAccessArgs>() == 168);

    type CtlInit = unsafe extern "C" fn(*mut CtlInitArgs, *mut *mut c_void) -> i32;
    type CtlClose = unsafe extern "C" fn(*mut c_void) -> i32;
    type Enumerate = unsafe extern "C" fn(*mut c_void, *mut u32, *mut *mut c_void) -> i32;
    type I2CAccess = unsafe extern "C" fn(*mut c_void, *mut CtlI2cAccessArgs) -> i32;

    struct Api {
        _lib: *mut c_void,
        _handle: *mut c_void,
        _close: CtlClose,
        enum_devices: Enumerate,
        enum_outputs: Enumerate,
        i2c: I2CAccess,
    }
    unsafe impl Send for Api {}
    unsafe impl Sync for Api {}

    fn api() -> Option<&'static Api> {
        static API: OnceLock<Option<Api>> = OnceLock::new();
        API.get_or_init(|| {
            for name in ["ControlLib.dll", "igcl.dll"] {
                let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
                unsafe {
                    let lib = LoadLibraryW(wide.as_ptr());
                    if lib.is_null() {
                        continue;
                    }
                    let sym = |n: &str| {
                        let c: Vec<u8> = n.bytes().chain(std::iter::once(0)).collect();
                        GetProcAddress(lib, c.as_ptr()).map(|f| f as usize)
                    };
                    let (init, close, ed, eo, i2c) = (
                        sym("ctlInit"),
                        sym("ctlClose"),
                        sym("ctlEnumerateDevices"),
                        sym("ctlEnumerateDisplayOutputs"),
                        sym("ctlI2CAccess"),
                    );
                    if [init, close, ed, eo, i2c].iter().any(Option::is_none) {
                        continue;
                    }
                    let init: CtlInit = std::mem::transmute(init.unwrap());
                    let mut args = CtlInitArgs {
                        size: std::mem::size_of::<CtlInitArgs>() as u32,
                        version: 0,
                        _pad0: [0; 3],
                        app_version: 0,
                        flags: 0,
                        supported_version: 0,
                        application_uid: [0; 16],
                    };
                    let mut handle: *mut c_void = std::ptr::null_mut();
                    if init(&mut args, &mut handle) != 0 || handle.is_null() {
                        continue;
                    }
                    // Keep the handle alive in the API struct through a leaked
                    // box; IGCL has no per-backend close in this transport.
                    return Some(Api {
                        _lib: lib,
                        _close: std::mem::transmute::<usize, CtlClose>(close.unwrap()),
                        enum_devices: std::mem::transmute::<usize, Enumerate>(ed.unwrap()),
                        enum_outputs: std::mem::transmute::<usize, Enumerate>(eo.unwrap()),
                        i2c: std::mem::transmute::<usize, I2CAccess>(i2c.unwrap()),
                        _handle: handle,
                    });
                }
            }
            None
        })
        .as_ref()
    }

    struct Igcl {
        api: &'static Api,
        output: *mut c_void,
    }
    unsafe impl Send for Igcl {}

    impl GpuI2c for Igcl {
        fn vendor(&self) -> GpuVendor {
            GpuVendor::Intel
        }
        fn transport(&self) -> String {
            "Intel IGCL I²C".to_string()
        }
        fn write_block(&mut self, addr: u8, data: &[u8]) -> Result<()> {
            let mut args = CtlI2cAccessArgs {
                size: std::mem::size_of::<CtlI2cAccessArgs>() as u32,
                version: 0,
                _pad0: [0; 3],
                data_size: data.len() as u32,
                address: addr as u32,
                op_type: 2, // CTL_OPERATION_TYPE_WRITE
                offset: 0,
                flags: 0,
                _pad1: 0,
                rad: 0,
                data: [0; 128],
            };
            args.data[..data.len()].copy_from_slice(data);
            let rc = unsafe { (self.api.i2c)(self.output, &mut args) };
            if rc != 0 {
                Err(Error::backend(format!("IGCL ctlI2CAccess failed: {rc:#x}")))
            } else {
                Ok(())
            }
        }
    }

    pub(super) fn detect(pcis: &[PciId]) -> Option<Vec<GpuDevice>> {
        let api = api()?;
        let mut devices = [std::ptr::null_mut(); 16];
        let mut device_count = 0u32;
        if unsafe { (api.enum_devices)(api._handle, &mut device_count, devices.as_mut_ptr()) } != 0
        {
            return None;
        }
        let mut out = Vec::new();
        for (di, device) in devices[..device_count as usize].iter().enumerate() {
            let mut outputs = [std::ptr::null_mut(); 16];
            let mut count = 0u32;
            if unsafe { (api.enum_outputs)(*device, &mut count, outputs.as_mut_ptr()) } != 0 {
                continue;
            }
            for (oi, output) in outputs[..count as usize].iter().enumerate() {
                out.push(GpuDevice::from_profile(
                    format!("intel{di}o{oi}"),
                    GpuVendor::Intel,
                    pcis.get(di).copied(),
                    Box::new(Igcl {
                        api,
                        output: *output,
                    }),
                ));
            }
        }
        Some(out)
    }
}

// ---------------------------------------------------------------------------
// Linux: /dev/i2c-* on a GPU adapter
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
mod linux_i2c {
    use super::*;
    use std::ffi::CString;

    const I2C_SLAVE: libc::c_ulong = 0x0703;

    struct LinuxI2c {
        fd: i32,
        bus: u32,
    }
    impl Drop for LinuxI2c {
        fn drop(&mut self) {
            unsafe { libc::close(self.fd) };
        }
    }
    impl GpuI2c for LinuxI2c {
        fn vendor(&self) -> GpuVendor {
            GpuVendor::Amd
        }
        fn transport(&self) -> String {
            format!("i2c-{}", self.bus)
        }
        fn write_block(&mut self, addr: u8, data: &[u8]) -> Result<()> {
            // ioctl is variadic: the variadic argument needs a concrete type.
            if unsafe { libc::ioctl(self.fd, I2C_SLAVE, addr as libc::c_ulong) } < 0 {
                return Err(Error::backend(format!("I2C_SLAVE 0x{addr:02x} failed")));
            }
            let n = unsafe { libc::write(self.fd, data.as_ptr() as *const _, data.len()) };
            if n != data.len() as isize {
                Err(Error::backend("short I²C write"))
            } else {
                Ok(())
            }
        }
    }

    pub(super) fn detect(pcis: &[PciId]) -> Vec<GpuDevice> {
        let mut out = Vec::new();
        let base = std::path::Path::new("/sys/class/i2c-dev");
        let Ok(entries) = std::fs::read_dir(base) else {
            return out;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(bus) = name
                .to_str()
                .and_then(|s| s.strip_prefix("i2c-"))
                .and_then(|s| s.parse::<u32>().ok())
            else {
                continue;
            };
            // Read the adapter name, matching OpenRGB's GPU-bus filters.
            let devname = entry.path().join("name");
            let adapter = std::fs::read_to_string(&devname).unwrap_or_default();
            let is_gpu = adapter.starts_with("AMDGPU")
                || adapter.starts_with("Synopsys DesignWare I2C adapter")
                || adapter.starts_with("NVIDIA");
            if !is_gpu {
                continue;
            }
            let path = format!("/dev/i2c-{bus}");
            let c = CString::new(path.as_bytes()).unwrap();
            let fd = unsafe { libc::open(c.as_ptr(), libc::O_RDWR) };
            if fd < 0 {
                continue;
            }
            let vendor = if adapter.starts_with("Synopsys") {
                GpuVendor::Intel
            } else if adapter.starts_with("NVIDIA") {
                GpuVendor::Nvidia
            } else {
                GpuVendor::Amd
            };
            let wanted_vendor = match vendor {
                GpuVendor::Amd => 0x1002,
                GpuVendor::Nvidia => 0x10DE,
                GpuVendor::Intel => 0x8086,
                GpuVendor::Simulated => 0,
            };
            let pci = pcis.iter().find(|p| p.vendor == wanted_vendor).copied();
            out.push(GpuDevice::from_profile(
                format!("i2c{bus}"),
                vendor,
                pci,
                Box::new(LinuxI2c { fd, bus }),
            ));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_matches_the_recovered_protocol() {
        let z = &GPU_ZONES[0];
        let s = GpuZoneState {
            color: [0x12, 0x34, 0x56],
            mode: 0x02,
            speed: 0x80,
            brightness: 0xFF,
            direction: 0x01,
            enabled: true,
        };
        assert_eq!(
            build_frame(z, &s),
            [
                0x10, 0x00, 0x03, 0x02, 0x12, 0x34, 0x56, 0x80, 0xFF, 0x01, 0x1A, 0x00
            ]
        );
    }

    #[test]
    fn dimmed_brightness_reaches_the_wire_frame() {
        let z = &GPU_ZONES[0];
        let s = GpuZoneState {
            brightness: 0x40,
            ..GpuZoneState::default()
        };
        assert_eq!(build_frame(z, &s)[8], 0x40, "byte 8 is the brightness");
    }

    #[test]
    fn zones_have_unique_channels() {
        let mut channels: Vec<u8> = GPU_ZONES.iter().map(|z| z.channel).collect();
        channels.sort_unstable();
        channels.dedup();
        assert_eq!(channels.len(), GPU_ZONES.len());
    }

    #[test]
    fn state_round_trips() {
        let dir = std::env::temp_dir().join(format!("opencolorhub-gpu-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("gpu-led.state");
        let mut state = GpuLedState {
            enabled: true,
            ..Default::default()
        };
        state
            .zones
            .insert("amd0:fan".into(), GpuZoneState::default());
        state.save(&path).unwrap();
        let (loaded, warning) = GpuLedState::load(&path);
        assert!(warning.is_none());
        assert_eq!(loaded, state);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn mode_labels_cover_all_ids() {
        for (id, _) in GPU_MODES {
            assert_ne!(mode_label(id), "Unknown");
        }
    }

    #[test]
    fn unknown_mode_has_a_label() {
        assert_eq!(mode_label(0x7F), "Unknown");
    }

    #[test]
    fn every_zone_uses_its_own_channel_in_the_frame() {
        for zone in GPU_ZONES.iter() {
            let frame = build_frame(zone, &GpuZoneState::default());
            assert_eq!(frame[2], zone.channel, "zone {}", zone.id);
        }
    }

    fn sim_backend(name: &str) -> (GpuLedBackend, PathBuf) {
        let dir = std::env::temp_dir().join(format!("och-gpu-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let backend = GpuLedBackend::detect(dir.join("gpu-led.state"), true);
        (backend, dir)
    }

    #[test]
    fn simulated_detection_is_hardware_free() {
        let (backend, _dir) = sim_backend("detect");
        let status = backend.status();
        assert!(status.enabled);
        assert!(status.device.simulated);
        assert_eq!(status.devices.len(), 1);
        assert_eq!(status.devices[0].vendor, GpuVendor::Simulated);
        assert_eq!(status.devices[0].zones.len(), GPU_ZONES.len());
    }

    #[test]
    fn applying_persists_every_enabled_zone() {
        let (mut backend, dir) = sim_backend("apply");
        let written = backend.apply().unwrap();
        assert_eq!(written, GPU_ZONES.len());
        assert!(dir.join("gpu-led.state").exists());
    }

    #[test]
    fn disabled_state_skips_writes_and_save() {
        let (mut backend, dir) = sim_backend("disabled");
        backend.state_mut().enabled = false;
        assert_eq!(backend.apply().unwrap(), 0);
        assert!(!dir.join("gpu-led.state").exists());
    }

    #[test]
    fn missing_state_file_starts_enabled() {
        let (backend, _dir) = sim_backend("missing");
        assert!(backend.state().enabled);
        assert!(backend.state().zones.is_empty());
    }

    #[test]
    fn corrupt_state_file_is_reported_with_defaults() {
        let dir = std::env::temp_dir().join(format!("och-gpu-corrupt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("gpu-led.state");
        std::fs::write(&path, "{ not json").unwrap();
        let (state, warning) = GpuLedState::load(&path);
        assert!(warning.is_some_and(|w| w.contains("malformed")));
        assert!(state.enabled);
        assert!(state.zones.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }
}
