//! Read-only fan and temperature telemetry from HWiNFO's shared-memory block.
//!
//! Windows gives user mode no way to touch the Super I/O (see the README), so a
//! kernel driver is the only route to *writing* fan speeds. Reading is a
//! different problem: HWiNFO keeps a polled snapshot of every sensor in a named
//! shared-memory section, and any process can map it read-only.
//!
//! That is exactly what Colorful's own client does — `iGameAPI.HWiNFO.dll`
//! loads `HWiNFO64.dll` and exposes `Init` / `GetXml` / `GetSensorCount` /
//! `ParseSensors` over it (`iGC.Lite-RE/13-real-data-paths.md` §13.4). This
//! module reimplements the underlying interface instead of shipping the vendor
//! wrapper.
//!
//! ## The block
//!
//! ```text
//! header   signature 'HWiN' | version | revision | poll_time |
//!          offset/size/count of the sensor array |
//!          offset/size/count of the reading array
//! sensors  [id | instance | name_orig[128] | name_user[128]] *
//! readings [type | sensor_index | reading_id |
//!           label_orig[128] | label_user[128] | unit[16] |
//!           value, value_min, value_max, value_avg : f64] *
//! ```
//!
//! Two header layouts exist in the wild: `__time64_t poll_time` is 8-byte
//! aligned on some builds (48-byte header) and 4-byte packed on others
//! (44-byte header). [`parse_header`] tries both and keeps the one whose
//! offsets actually land inside the mapping, so neither is hardcoded.
//!
//! Reading elements are parsed **from the end**: the four trailing doubles are
//! fixed, so `value` sits at `stride - 32` regardless of the struct's packing.

use std::ffi::c_void;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::System::Memory::{
    FILE_MAP_READ, MEMORY_BASIC_INFORMATION, MapViewOfFile, OpenFileMappingW, UnmapViewOfFile,
    VirtualQuery,
};

use crate::backend::FanBackend;
use crate::curve::FanCurve;
use crate::error::{Error, Result};
use crate::model::{DeviceInfo, FanHeader, FanMode, FanSample, Telemetry};

/// `'HWiN'` little-endian.
const SIGNATURE: u32 = u32::from_le_bytes(*b"HWiN");

/// Section names to try, most specific first.
///
/// HWiNFO creates a `Global\` section (it runs elevated). The `Local\` entry is
/// a same-session fallback: it costs nothing when HWiNFO is present, and it lets
/// the backend be exercised on a machine without HWiNFO and without
/// `SeCreateGlobalPrivilege`. `OPENCOLORHUB_HWINFO_SHM` overrides the lot.
const SECTION_NAMES: [&str; 3] = [
    r"Global\HWiNFO_SENSORS_SHARED_MEM2",
    r"Global\HWiNFO_SENSORS_SHARED_MEM",
    r"Local\HWiNFO_SENSORS_SHARED_MEM2",
];

const SECTION_ENV: &str = "OPENCOLORHUB_HWINFO_SHM";

/// `SENSOR_READING_TYPE` values this backend cares about.
const READING_TEMP: u32 = 1;
const READING_FAN: u32 = 3;

/// Guard against a corrupt header making us allocate for millions of entries.
const MAX_ELEMENTS: usize = 8192;

/// Where the arrays live inside the mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub version: u32,
    pub revision: u32,
    pub sensor_offset: usize,
    pub sensor_size: usize,
    pub sensor_count: usize,
    pub reading_offset: usize,
    pub reading_size: usize,
    pub reading_count: usize,
}

/// One sensor reading.
#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    /// `SENSOR_READING_TYPE`: 1 = temperature, 3 = fan, and others we ignore.
    pub kind: u32,
    pub sensor_index: u32,
    pub label: String,
    pub unit: String,
    pub value: f64,
}

impl Reading {
    pub fn is_temp(&self) -> bool {
        self.kind == READING_TEMP
    }

    pub fn is_fan(&self) -> bool {
        self.kind == READING_FAN
    }
}

/// Parses the header, accepting either `poll_time` alignment.
pub fn parse_header(buf: &[u8]) -> Option<Header> {
    if buf.len() < 44 {
        return None;
    }
    let sig = read_u32(buf, 0)?;
    if sig != SIGNATURE {
        return None;
    }
    let version = read_u32(buf, 4)?;
    let revision = read_u32(buf, 8)?;

    // poll_time occupies 8 bytes; the fields after it start at 12 (packed) or
    // 16 (8-byte aligned).
    for poll_time_end in [12usize, 16usize] {
        let Some(header) = header_after(buf, poll_time_end, version, revision) else {
            continue;
        };
        return Some(header);
    }
    None
}

fn header_after(buf: &[u8], base: usize, version: u32, revision: u32) -> Option<Header> {
    let sensor_offset = read_u32(buf, base)? as usize;
    let sensor_size = read_u32(buf, base + 4)? as usize;
    let sensor_count = read_u32(buf, base + 8)? as usize;
    let reading_offset = read_u32(buf, base + 12)? as usize;
    let reading_size = read_u32(buf, base + 16)? as usize;
    let reading_count = read_u32(buf, base + 20)? as usize;

    // A sensor element is id + instance + two 128-byte names; a reading element
    // adds a 16-byte unit and four doubles on top of that.
    if sensor_size < 264 || reading_size < 316 {
        return None;
    }
    if sensor_count == 0 || reading_count == 0 {
        return None;
    }
    if sensor_count > MAX_ELEMENTS || reading_count > MAX_ELEMENTS {
        return None;
    }

    let sensors_end = sensor_offset.checked_add(sensor_size.checked_mul(sensor_count)?)?;
    let readings_end = reading_offset.checked_add(reading_size.checked_mul(reading_count)?)?;
    if sensors_end > buf.len() || readings_end > buf.len() {
        return None;
    }

    Some(Header {
        version,
        revision,
        sensor_offset,
        sensor_size,
        sensor_count,
        reading_offset,
        reading_size,
        reading_count,
    })
}

/// Parses every reading in the block.
pub fn parse_readings(buf: &[u8], header: &Header) -> Vec<Reading> {
    let mut out = Vec::with_capacity(header.reading_count);
    for index in 0..header.reading_count {
        let start = header.reading_offset + index * header.reading_size;
        let stride = header.reading_size;
        let Some(element) = buf.get(start..start + stride) else {
            break;
        };

        // Trailing four doubles are fixed: value, min, max, avg.
        let Some(value) = read_f64(element, stride - 32) else {
            continue;
        };
        // Working backwards from the doubles: unit[16], label_user[128], label_orig[128].
        let unit = read_cstr(element, stride - 48, 16);
        let label_user = read_cstr(element, stride - 176, 128);
        let label_orig = read_cstr(element, stride - 304, 128);
        let label = if label_user.is_empty() {
            label_orig
        } else {
            label_user
        };

        let Some(kind) = read_u32(element, 0) else {
            continue;
        };
        let sensor_index = read_u32(element, 4).unwrap_or(0);

        out.push(Reading {
            kind,
            sensor_index,
            label,
            unit,
            value,
        });
    }
    out
}

fn read_u32(buf: &[u8], offset: usize) -> Option<u32> {
    let bytes = buf.get(offset..offset + 4)?;
    Some(u32::from_le_bytes(bytes.try_into().ok()?))
}

fn read_f64(buf: &[u8], offset: usize) -> Option<f64> {
    let bytes = buf.get(offset..offset + 8)?;
    Some(f64::from_le_bytes(bytes.try_into().ok()?))
}

fn read_cstr(buf: &[u8], offset: usize, len: usize) -> String {
    let Some(bytes) = buf.get(offset..offset + len) else {
        return String::new();
    };
    let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).trim().to_string()
}

// ---------------------------------------------------------------- mapping --

/// A mapped view of HWiNFO's section.
pub struct SharedMemory {
    handle: HANDLE,
    view: *mut c_void,
    len: usize,
    pub name: String,
}

// SAFETY: the view is read-only and only exposed through `&[u8]`.
unsafe impl Send for SharedMemory {}

impl SharedMemory {
    /// Opens and maps the first section that exists and is readable.
    pub fn open() -> Result<Self> {
        let mut last_error = String::from("HWiNFO shared memory is not available");

        let mut candidates: Vec<String> = Vec::new();
        if let Ok(custom) = std::env::var(SECTION_ENV) {
            candidates.push(custom);
        }
        candidates.extend(SECTION_NAMES.iter().map(|n| n.to_string()));

        for name in candidates {
            let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
            // SAFETY: `wide` is NUL-terminated and outlives the call.
            let handle = unsafe { OpenFileMappingW(FILE_MAP_READ, 0, wide.as_ptr()) };
            if handle.is_null() {
                last_error = format!(
                    "{name} is not present ({})",
                    std::io::Error::last_os_error()
                );
                continue;
            }

            // SAFETY: a live handle from OpenFileMappingW; length 0 maps the
            // whole section.
            let mapped = unsafe { MapViewOfFile(handle, FILE_MAP_READ, 0, 0, 0) };
            if mapped.Value.is_null() {
                // SAFETY: `handle` was produced by OpenFileMappingW and is owned here.
                unsafe {
                    CloseHandle(handle);
                }
                last_error = format!("{name} could not be mapped");
                continue;
            }

            let len = region_size(mapped.Value);
            if len < 44 {
                // SAFETY: both resources are live and owned here.
                unsafe {
                    UnmapViewOfFile(mapped);
                    CloseHandle(handle);
                }
                last_error = format!("{name} is only {len} bytes");
                continue;
            }

            return Ok(Self {
                handle,
                view: mapped.Value,
                len,
                name,
            });
        }

        Err(Error::unavailable(format!(
            "HWiNFO shared memory not found — {last_error}. Start HWiNFO64 and enable \
             Settings → Safety → Shared Memory Support."
        )))
    }

    pub fn bytes(&self) -> &[u8] {
        // SAFETY: the view is mapped for `len` bytes and is only read here.
        unsafe { std::slice::from_raw_parts(self.view as *const u8, self.len) }
    }
}

impl Drop for SharedMemory {
    fn drop(&mut self) {
        // SAFETY: both resources were produced here and are released once.
        unsafe {
            UnmapViewOfFile(
                windows_sys::Win32::System::Memory::MEMORY_MAPPED_VIEW_ADDRESS { Value: self.view },
            );
            CloseHandle(self.handle);
        }
    }
}

fn region_size(base: *mut c_void) -> usize {
    let mut info: MEMORY_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: `base` is the start of a mapped view; `info` is a live buffer of
    // the size we pass.
    let written = unsafe {
        VirtualQuery(
            base as *const c_void,
            &mut info,
            std::mem::size_of::<MEMORY_BASIC_INFORMATION>(),
        )
    };
    if written == 0 { 0 } else { info.RegionSize }
}

// ---------------------------------------------------------------- backend --

/// Maps HWiNFO's sensor block onto the five fan headers.
pub struct HwinfoFan {
    shm: SharedMemory,
    device: DeviceInfo,
}

impl HwinfoFan {
    /// True when a usable HWiNFO block is present.
    pub fn available() -> bool {
        Self::open().is_ok()
    }

    pub fn open() -> Result<Self> {
        let shm = SharedMemory::open()?;
        let bytes = shm.bytes();
        let header = parse_header(bytes).ok_or_else(|| {
            Error::backend(
                "HWiNFO shared memory has an unrecognised layout — update HWiNFO, or this \
                 version is newer than the parser",
            )
        })?;
        if parse_readings(bytes, &header).is_empty() {
            return Err(Error::backend(
                "HWiNFO shared memory is present but contains no readings yet",
            ));
        }

        let device = DeviceInfo::hardware(
            "HWiNFO sensors",
            format!(
                "Read-only telemetry from {} (v{}.{})",
                shm.name, header.version, header.revision
            ),
        )
        .with_path(shm.name.clone());

        Ok(Self { shm, device })
    }

    /// The parsed snapshot, for diagnostics.
    pub fn readings(&self) -> Option<(Header, Vec<Reading>)> {
        let bytes = self.shm.bytes();
        let header = parse_header(bytes)?;
        let readings = parse_readings(bytes, &header);
        Some((header, readings))
    }
}

impl FanBackend for HwinfoFan {
    fn info(&self) -> DeviceInfo {
        self.device.clone()
    }

    fn read(&mut self) -> Result<Telemetry> {
        let bytes = self.shm.bytes();
        let header = parse_header(bytes)
            .ok_or_else(|| Error::backend("HWiNFO shared memory layout changed while running"))?;
        let readings = parse_readings(bytes, &header);

        let temperature_c = pick_temperature(&readings).map(|v| v as f32);

        let mut fans = Vec::new();
        for header in FanHeader::all() {
            let Some(reading) = pick_fan(&readings, *header) else {
                continue;
            };
            let rpm = reading.value.clamp(0.0, u16::MAX as f64).round() as u16;
            fans.push(FanSample::new(*header, rpm, 0, FanMode::Unknown));
        }

        if fans.is_empty() && temperature_c.is_none() {
            return Err(Error::backend(
                "HWiNFO is running but exposes no fan or temperature readings — enable the \
                 sensor groups for the motherboard in HWiNFO",
            ));
        }

        Ok(Telemetry {
            temperature_source: Some("HWiNFO shared memory".to_string()),
            temperature_c,
            fans,
        })
    }

    fn set_pwm(&mut self, _header: FanHeader, _pwm: u8) -> Result<()> {
        Err(Error::unavailable(WRITE_REFUSAL))
    }

    fn set_curve(&mut self, _header: FanHeader, _curve: &FanCurve) -> Result<()> {
        Err(Error::unavailable(WRITE_REFUSAL))
    }

    fn supports_curves(&self) -> bool {
        false
    }
}

const WRITE_REFUSAL: &str = "Fan speeds cannot be changed on Windows: the Super I/O needs raw \
                             I/O ports, which requires a kernel driver. HWiNFO provides read-only \
                             telemetry.";

/// Chooses the board temperature: prefer the motherboard/system sensor, then a
/// chipset sensor, then any CPU package temperature.
pub fn pick_temperature(readings: &[Reading]) -> Option<f64> {
    let temps: Vec<&Reading> = readings.iter().filter(|r| r.is_temp()).collect();
    for needle in ["system", "motherboard", "mainboard", "chipset"] {
        if let Some(found) = temps.iter().find(|r| label_matches(&r.label, needle)) {
            return Some(found.value);
        }
    }
    for needle in ["cpu", "package", "tctl"] {
        if let Some(found) = temps.iter().find(|r| label_matches(&r.label, needle)) {
            return Some(found.value);
        }
    }
    temps.first().map(|r| r.value)
}

/// Maps one of our five headers onto an HWiNFO fan label.
pub fn pick_fan(readings: &[Reading], header: FanHeader) -> Option<&Reading> {
    let fans: Vec<&Reading> = readings.iter().filter(|r| r.is_fan()).collect();
    let wanted = match header {
        FanHeader::Cpu => &["cpu fan", "cpufan", "cpu_fan", "cpu opt"][..],
        FanHeader::ChaFan1 => &[
            "cha fan 1",
            "chafan1",
            "cha_fan1",
            "sys fan 1",
            "system fan 1",
        ],
        FanHeader::ChaFan2 => &[
            "cha fan 2",
            "chafan2",
            "cha_fan2",
            "sys fan 2",
            "system fan 2",
        ],
        FanHeader::ChaFan3 => &[
            "cha fan 3",
            "chafan3",
            "cha_fan3",
            "sys fan 3",
            "system fan 3",
        ],
        FanHeader::Pump => &["pump", "aio"],
    };
    for needle in wanted {
        if let Some(found) = fans.iter().find(|r| label_matches(&r.label, needle)) {
            return Some(found);
        }
    }
    None
}

fn label_matches(label: &str, needle: &str) -> bool {
    label.to_ascii_lowercase().contains(needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a synthetic block matching one of the two header layouts.
    fn block(aligned_poll_time: bool) -> Vec<u8> {
        let header_len = if aligned_poll_time { 48 } else { 44 };
        let sensor_size = 264usize;
        let reading_size = 316usize;
        let sensor_count = 1usize;
        let reading_count = 6usize;

        let sensor_offset = header_len;
        let reading_offset = sensor_offset + sensor_size * sensor_count;
        let total = reading_offset + reading_size * reading_count;
        let mut buf = vec![0u8; total];

        buf[0..4].copy_from_slice(b"HWiN");
        buf[4..8].copy_from_slice(&2u32.to_le_bytes());
        buf[8..12].copy_from_slice(&7u32.to_le_bytes());

        let base = if aligned_poll_time { 16 } else { 12 };
        buf[base..base + 4].copy_from_slice(&(sensor_offset as u32).to_le_bytes());
        buf[base + 4..base + 8].copy_from_slice(&(sensor_size as u32).to_le_bytes());
        buf[base + 8..base + 12].copy_from_slice(&(sensor_count as u32).to_le_bytes());
        buf[base + 12..base + 16].copy_from_slice(&(reading_offset as u32).to_le_bytes());
        buf[base + 16..base + 20].copy_from_slice(&(reading_size as u32).to_le_bytes());
        buf[base + 20..base + 24].copy_from_slice(&(reading_count as u32).to_le_bytes());

        let readings: [(&str, &str, u32, f64); 6] = [
            ("System", "°C", READING_TEMP, 34.0),
            ("CPU Fan", "RPM", READING_FAN, 1180.0),
            ("CHA Fan 1", "RPM", READING_FAN, 890.0),
            ("CHA Fan 2", "RPM", READING_FAN, 0.0),
            ("CHA Fan 3", "RPM", READING_FAN, 760.0),
            ("AIO Pump", "RPM", READING_FAN, 2100.0),
        ];

        for (i, (label, unit, kind, value)) in readings.iter().enumerate() {
            let start = reading_offset + i * reading_size;
            let e = &mut buf[start..start + reading_size];
            e[0..4].copy_from_slice(&kind.to_le_bytes());
            e[4..8].copy_from_slice(&0u32.to_le_bytes());
            let lo = reading_size - 304;
            e[lo..lo + label.len()].copy_from_slice(label.as_bytes());
            let uo = reading_size - 48;
            e[uo..uo + unit.len()].copy_from_slice(unit.as_bytes());
            let vo = reading_size - 32;
            e[vo..vo + 8].copy_from_slice(&value.to_le_bytes());
        }

        buf
    }

    #[test]
    fn parses_the_packed_header_layout() {
        let header = parse_header(&block(false)).expect("packed layout");
        assert_eq!(header.version, 2);
        assert_eq!(header.revision, 7);
        assert_eq!(header.reading_count, 6);
        assert_eq!(header.reading_size, 316);
    }

    #[test]
    fn parses_the_aligned_header_layout() {
        let header = parse_header(&block(true)).expect("aligned layout");
        assert_eq!(header.reading_count, 6);
        assert_eq!(header.sensor_count, 1);
    }

    #[test]
    fn rejects_a_foreign_signature() {
        let mut buf = block(false);
        buf[0..4].copy_from_slice(b"JUNK");
        assert!(parse_header(&buf).is_none());
    }

    #[test]
    fn rejects_arrays_that_overflow_the_mapping() {
        let mut buf = block(false);
        // Claim a huge reading count that cannot fit.
        let base = 12;
        buf[base + 20..base + 24].copy_from_slice(&90_000u32.to_le_bytes());
        assert!(
            parse_header(&buf).is_none(),
            "must not accept a bogus count"
        );
    }

    #[test]
    fn reads_labels_units_and_values() {
        let buf = block(false);
        let header = parse_header(&buf).unwrap();
        let readings = parse_readings(&buf, &header);
        assert_eq!(readings.len(), 6);
        assert_eq!(readings[0].label, "System");
        assert_eq!(readings[0].unit, "°C");
        assert_eq!(readings[0].value, 34.0);
        assert!(readings[1].is_fan());
        assert_eq!(readings[1].label, "CPU Fan");
        assert_eq!(readings[1].value, 1180.0);
    }

    #[test]
    fn maps_every_header_onto_a_reading() {
        let buf = block(false);
        let header = parse_header(&buf).unwrap();
        let readings = parse_readings(&buf, &header);

        let cpu = pick_fan(&readings, FanHeader::Cpu).unwrap();
        assert_eq!(cpu.value, 1180.0);
        let cha1 = pick_fan(&readings, FanHeader::ChaFan1).unwrap();
        assert_eq!(cha1.value, 890.0);
        let cha2 = pick_fan(&readings, FanHeader::ChaFan2).unwrap();
        assert_eq!(cha2.value, 0.0, "a stopped fan is still a reading");
        let pump = pick_fan(&readings, FanHeader::Pump).unwrap();
        assert_eq!(pump.value, 2100.0);
    }

    #[test]
    fn temperature_prefers_the_system_sensor() {
        let buf = block(false);
        let header = parse_header(&buf).unwrap();
        let readings = parse_readings(&buf, &header);
        assert_eq!(pick_temperature(&readings), Some(34.0));
    }

    #[test]
    fn unknown_labels_yield_no_match_rather_than_a_guess() {
        let buf = block(false);
        let header = parse_header(&buf).unwrap();
        let mut readings = parse_readings(&buf, &header);
        readings.retain(|r| !label_matches(&r.label, "cha fan 3"));
        assert!(pick_fan(&readings, FanHeader::ChaFan3).is_none());
    }

    /// End-to-end: create a real shared-memory section, publish a synthetic
    /// block into it, and read it back through the backend.
    ///
    /// Uses the `Local\` name: creating a `Global\` section needs
    /// `SeCreateGlobalPrivilege`, which an unelevated test process lacks. The
    /// reader prefers `Global\` and falls back to `Local\`, so this exercises
    /// the same code path HWiNFO does.
    #[test]
    fn maps_a_live_section_end_to_end() {
        use windows_sys::Win32::Foundation::{GetLastError, HANDLE};
        use windows_sys::Win32::System::Memory::{
            CreateFileMappingW, FILE_MAP_WRITE, PAGE_READWRITE,
        };

        const ERROR_ALREADY_EXISTS: u32 = 183;
        const TEST_SECTION: &str = r"Local\HWiNFO_SENSORS_SHARED_MEM2";

        let payload = block(true);
        let size = payload.len() as u32;
        let wide: Vec<u16> = TEST_SECTION
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();

        // SAFETY: INVALID_HANDLE_VALUE plus a non-zero size asks for a
        // paging-file backed section; `wide` is NUL-terminated and outlives the call.
        let handle: HANDLE = unsafe {
            CreateFileMappingW(
                windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE,
                std::ptr::null(),
                PAGE_READWRITE,
                0,
                size,
                wide.as_ptr(),
            )
        };
        assert!(
            !handle.is_null(),
            "could not create the test section: {}",
            std::io::Error::last_os_error()
        );
        // Never write into a section we did not create.
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            eprintln!("skipping: a section with that name already exists");
            unsafe {
                CloseHandle(handle);
            }
            return;
        }

        // SAFETY: writing the synthetic block into our own writable view.
        let writable = unsafe { MapViewOfFile(handle, FILE_MAP_WRITE, 0, 0, size as usize) };
        assert!(!writable.Value.is_null(), "MapViewOfFile failed");
        unsafe {
            std::ptr::copy_nonoverlapping(
                payload.as_ptr(),
                writable.Value as *mut u8,
                payload.len(),
            );
        }

        let mut fan = HwinfoFan::open().expect("backend should open our section");
        let telemetry = fan.read().expect("readings should parse");

        assert_eq!(telemetry.temperature_c, Some(34.0));
        assert_eq!(telemetry.fans.len(), 5, "all five headers resolve");

        let cpu = telemetry
            .fans
            .iter()
            .find(|f| f.header == FanHeader::Cpu)
            .expect("cpu fan");
        assert_eq!(cpu.rpm, 1180);

        let pump = telemetry
            .fans
            .iter()
            .find(|f| f.header == FanHeader::Pump)
            .expect("pump");
        assert_eq!(pump.rpm, 2100);

        assert!(!fan.supports_curves(), "HWiNFO is telemetry only");
        assert!(
            fan.set_pwm(FanHeader::Cpu, 128).is_err(),
            "writes must be refused"
        );

        // SAFETY: release both resources before the backing buffer is dropped.
        unsafe {
            UnmapViewOfFile(writable);
            CloseHandle(handle);
        }
    }
}
