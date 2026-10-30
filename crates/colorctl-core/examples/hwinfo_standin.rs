//! Publishes a synthetic HWiNFO shared-memory section, for testing the
//! telemetry backend on a machine where HWiNFO is not installed.
//!
//! ```text
//! cargo run -p colorctl-core --example hwinfo_standin --release
//! colorctl fan status
//! ```
//!
//! It writes `Global\HWiNFO_SENSORS_SHARED_MEM2` with a plausible sensor set
//! (one board temperature, five fans) and holds it open until Ctrl-C. Nothing
//! else on the system reads that section, so it is safe to leave running; the
//! backend simply reports these numbers instead of the simulator's.
//!
//! Windows only — HWiNFO does not exist elsewhere.

#[cfg(windows)]
fn main() {
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Memory::{
        CreateFileMappingW, FILE_MAP_WRITE, MapViewOfFile, PAGE_READWRITE, UnmapViewOfFile,
    };

    const HEADER_LEN: usize = 48;
    const SENSOR_SIZE: usize = 264;
    const READING_SIZE: usize = 316;
    const TEMP: u32 = 1;
    const FAN: u32 = 3;

    let sensors: [&str; 1] = ["Nuvoton NCT5584D"];
    let readings: [(&str, &str, u32, f64); 6] = [
        ("System", "°C", TEMP, 34.0),
        ("CPU Fan", "RPM", FAN, 1180.0),
        ("CHA Fan 1", "RPM", FAN, 890.0),
        ("CHA Fan 2", "RPM", FAN, 0.0),
        ("CHA Fan 3", "RPM", FAN, 760.0),
        ("AIO Pump", "RPM", FAN, 2100.0),
    ];

    let sensor_offset = HEADER_LEN;
    let reading_offset = sensor_offset + SENSOR_SIZE * sensors.len();
    let total = reading_offset + READING_SIZE * readings.len();

    let mut buf = vec![0u8; total];
    buf[0..4].copy_from_slice(b"HWiN");
    buf[4..8].copy_from_slice(&2u32.to_le_bytes());
    buf[8..12].copy_from_slice(&7u32.to_le_bytes());

    let base = 16; // 8-byte aligned poll_time layout
    let put = |buf: &mut Vec<u8>, at: usize, v: u32| {
        buf[at..at + 4].copy_from_slice(&v.to_le_bytes());
    };
    put(&mut buf, base, sensor_offset as u32);
    put(&mut buf, base + 4, SENSOR_SIZE as u32);
    put(&mut buf, base + 8, sensors.len() as u32);
    put(&mut buf, base + 12, reading_offset as u32);
    put(&mut buf, base + 16, READING_SIZE as u32);
    put(&mut buf, base + 20, readings.len() as u32);

    for (i, name) in sensors.iter().enumerate() {
        let s = sensor_offset + i * SENSOR_SIZE;
        buf[s + 8..s + 8 + name.len()].copy_from_slice(name.as_bytes());
    }

    for (i, (label, unit, kind, value)) in readings.iter().enumerate() {
        let s = reading_offset + i * READING_SIZE;
        let e = &mut buf[s..s + READING_SIZE];
        e[0..4].copy_from_slice(&kind.to_le_bytes());
        e[4..8].copy_from_slice(&0u32.to_le_bytes());
        let lo = READING_SIZE - 304;
        e[lo..lo + label.len()].copy_from_slice(label.as_bytes());
        let uo = READING_SIZE - 48;
        e[uo..uo + unit.len()].copy_from_slice(unit.as_bytes());
        let vo = READING_SIZE - 32;
        e[vo..vo + 8].copy_from_slice(&value.to_le_bytes());
    }

    let name: Vec<u16> = r"Local\HWiNFO_SENSORS_SHARED_MEM2"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    // SAFETY: paging-file backed section of a known size; `name` outlives the call.
    let handle = unsafe {
        CreateFileMappingW(
            INVALID_HANDLE_VALUE,
            std::ptr::null(),
            PAGE_READWRITE,
            0,
            total as u32,
            name.as_ptr(),
        )
    };
    if handle.is_null() {
        eprintln!(
            "CreateFileMappingW failed: {}",
            std::io::Error::last_os_error()
        );
        std::process::exit(1);
    }
    if unsafe { GetLastError() } == 183 {
        eprintln!("a real HWiNFO section already exists — refusing to overwrite it");
        std::process::exit(2);
    }

    // SAFETY: writable view of the section we just created.
    let view = unsafe { MapViewOfFile(handle, FILE_MAP_WRITE, 0, 0, total) };
    if view.Value.is_null() {
        eprintln!("MapViewOfFile failed");
        std::process::exit(1);
    }
    // SAFETY: `view` covers `total` bytes; `buf` is exactly that long.
    unsafe {
        std::ptr::copy_nonoverlapping(buf.as_ptr(), view.Value as *mut u8, total);
    }

    println!(
        "published {} sensors / {} readings into Local\\HWiNFO_SENSORS_SHARED_MEM2 ({total} bytes)",
        sensors.len(),
        readings.len()
    );
    println!("holding it open — press Ctrl-C to remove");

    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }

    // Unreachable, but keeps the cleanup honest if the loop ever exits.
    #[allow(unreachable_code)]
    unsafe {
        UnmapViewOfFile(view);
        CloseHandle(handle);
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("hwinfo_standin is Windows-only");
}
