# Platform support and hardware access

## What works where

| Platform | Fan control | Lighting |
| --- | --- | --- |
| **Linux** | ✅ `/dev/port`, needs root or `CAP_SYS_RAWIO` | ✅ `/dev/hidraw` for `2F4C:1000` |
| **Windows** | ✅ PawnIO sandboxed driver · ⚪ read-only via HWiNFO when PawnIO is absent | ✅ SetupAPI + `hid.dll`, no driver |

## Lighting without root

`/dev/hidraw*` is root-owned, mode `0600`: a fresh build finds the controller but
is refused on open. Both `.deb` packages ship a udev rule granting the user at
the seat an ACL on the controller's nodes:

```udev
KERNEL=="hidraw*", ATTRS{idVendor}=="2f4c", ATTRS{idProduct}=="1000", TAG+="uaccess"
```

`uaccess`, not `MODE="0666"`: unprivileged for the logged-in user only. Root is
unaffected; fan control is untouched (`/dev/port` is a different device).

The `.deb` installs `/usr/lib/udev/rules.d/99-opencolorhub.rules` (desktop) or
`99-colorctl.rules` (CLI) and reloads udev from `postinst` — no reboot. Portable
tarballs ship it as `colorctl.rules`
([`packaging/colorctl.rules`](../packaging/colorctl.rules)); AppImage or source
builds install it by hand:

```bash
sudo cp colorctl.rules /etc/udev/rules.d/99-opencolorhub.rules
sudo udevadm control --reload-rules
sudo udevadm trigger --subsystem-match=hidraw
```

Without it an unprivileged run is refused on open: `--backend hardware` reports
lighting unavailable, `auto` falls back to the simulator with a note naming the
error.

## Why Windows needs a driver at all

`IN`/`OUT` are ring-0. Windows has no user-mode port broker like `/dev/port`.
Options: your own kernel driver, [WinRing0](https://nvd.nist.gov/vuln/detail/CVE-2020-14979)
- deliberately not shipped - or **PawnIO**: a signed driver running a
bytecode module whose port access is allow-listed to the selected index pair and
discovered BARs, never exposing physical memory.

OpenColorHub uses `PawnIOLib.dll`, with the LpcIO module embedded and passed to
the driver at runtime.

**Temperature** comes from the register Colorful's client reads: on the
NCT5567D-B (`0xD121`) iGC.Lite calls `GetTemp(1, 80)` — bank 1, register `0x50` —
as the X axis of its fan curves. OpenColorHub reads that first, then falls back
to bank-0 Nuvoton sensors (`0x27`, `0x73`, `0x75`, `0x77`) where `0x50` is
unwired. The Dashboard names the source ("board thermistor", "HWiNFO shared
memory", "synthetic") and keeps `bank 1 · 0x50` in the gauge tooltip. Values
outside 5–110 °C are rejected.

**Two temperatures.** The Dashboard shows both the **motherboard** (board
thermistor, bank 1 `0x50`) and the **CPU**. The CPU reading comes directly from
the Nuvoton's `CPUTIN` sensor (bank 0 `0x75`, half-degree bit in `0x76`) over
the same PawnIO connection as fan control — **no HWiNFO required**. Boards that
leave CPUTIN unwired fall back to HWiNFO shared memory (CPU Package, Tctl/Tdie,
Core 0, or a custom `cpu@` label) when it is running. Fan curves keep using the
board temperature — the value Colorful's own client drives them from — so the
CPU readout never changes control behaviour. The simulator shows a synthetic
CPU value for the demo.

**Empty headers are hidden.** A header that never reports a tach is an empty
socket, not a fan at 0 rpm: its card is dropped and the screen says *"No more
fans detected — N of 6 headers are in use."* Latched — once spun, a header keeps
its card at 0%. Average duty and total rpm count only existing headers.

**PawnIO is not bundled.** On launch and on Fan control the app fetches
`PawnIO_setup.exe` from the project's GitHub release (currently 2.2.0), verifies
a pinned SHA-256, and runs it so its installer UI and UAC prompt appear. A hash
mismatch deletes it and refuses. Driver present but process unelevated: offers
*Restart as administrator*.

Fallback order:

1. **PawnIO** — real RPM, board temperature, manual duty; needs driver and
   elevation.
2. **HWiNFO shared memory** — read-only; HWiNFO64 running with *Settings → Safety
   → Shared Memory Support*. Two header layouts (`__time64_t` packed or 8-byte
   aligned) are detected from the block. `OPENCOLORHUB_HWINFO_SHM` overrides the
   section name.
3. **Simulator** — persists state, evaluates curves, enforces the pump floor.

The chosen backend is named in Settings and the sidebar; nothing is silently
simulated.

### Curves on Windows are software curves

Manual-duty registers are mapped for `0xD121`; the SmartFan curve registers are
not. A curve lives as long as the process, and manual duty stays as last written
after exit until the BIOS resets it.

### Exercising the HWiNFO reader

```bash
cargo run -p colorctl-core --example hwinfo_standin   # holds the section open
colorctl fan status                                   # reads it
```

## Lighting over HID

No privileged access: an ordinary USB HID device via SetupAPI + `hid.dll`, no
driver, service or elevation.

`colorctl rgb probe` shows what the HID layer finds:

```
Colorful HID interfaces (VID 2F4C, PID 1000):
  MI_02  usage page 0xFF01  out  65  feat   0  <= selected  [COLORFUL]
        \\?\hid#vid_2f4c&pid_1000&mi_02#<instance>#{4d1e55b2-f16f-11cf-88cb-001111000030}
  MI_01  usage page 0xFF00  out  65  feat   0  [X570-USBHID-LED]
        \\?\hid#vid_2f4c&pid_1000&mi_01#<instance>#{4d1e55b2-f16f-11cf-88cb-001111000030}
```

MI_02 (`0xFF01`) carries LED frames. MI_01 (`0xFF00`) is the firmware-update
collection — same VID/PID, never written to. Feature-report firmware:
`OPENCOLORHUB_HID_TRANSPORT=feature`; the backend falls back automatically when
the output-report write fails.

## Elevation

The installer is per-user and does not request administrator, so the app starts
standard and can be installed without a UAC prompt. PawnIO's driver still needs
elevation for fan control; RGB and HWiNFO work without it.

When PawnIO is installed but `pawnio_status()` reports `denied`, the app asks
once at startup:

* **Continue without fan control** leaves lighting, settings and telemetry
  working.
* **Restart as administrator** calls `platform::request()`. Because the app is
  single-instance, a `runas` copy launched while this one still holds the mutex
  would be forwarded to it and exit — Windows blocks that `WM_COPYDATA` handoff
  across the medium→high integrity boundary. `request()` therefore hands the
  launch to a hidden helper that waits for the current PID to exit, then raises
  the UAC prompt (`Start-Process -Verb RunAs`) so the elevated copy starts as the
  only instance and probes PawnIO again. Declining relaunches the standard app.

Settings → Privileges shows the current state and offers the same restart. The
autostart task runs at `RunLevel Highest`, so a sign-in launch already has the
rights.

Because Windows blocks `WM_COPYDATA` between integrity levels, every launch path
must use the same elevation for the single-instance hand-over to work. A normal
double launch is unaffected; the elevated relaunch quits before the new copy
takes over.

There is no manifest and no persisted "always elevated" choice: the app asks,
UAC answers, and nothing is written.

## Start with the OS

**Settings → Startup** registers a sign-in entry for the current user:

* **Windows** — Task Scheduler logon task `OpenColorHub` with `RunLevel Highest`.
  A `HKCU\...\Run` value starts unelevated: no fan control and a UAC prompt at
  sign-in anyway.
* **Linux** — `${XDG_CONFIG_HOME:-~/.config}/autostart/opencolorhub.desktop`.

The switch off deletes the entry, which is created only when asked for.

Enabling it also turns on **Re-apply lighting at launch**, pushing the saved
`CCTL` state at sign-in.

## Waking from sleep

Suspend power-cycles the USB bus, so the RGB controller returns on its own
pattern, and reloads the Super I/O from firmware defaults, putting every header
back under BIOS fan control. Neither chip remembers what this app wrote.

Two checks:

* Every tick asks the open controller whether it still answers. A resume, hub
  power dip or controller reset invalidates the handle silently, caught on the
  next second.
* A tick more than half a second beyond its poll interval means the process was
  suspended; it measures wall clock.

Either re-pushes the lighting frame and rewrites every duty or curve this session
set, including the manual-mode bit, because the chip returned to automatic. A
re-enumerating controller refuses the frame; the write is retried for a few
seconds before it is reported.

Only what the session applied is restored. Untouched lighting is left alone; a
header nobody set keeps its firmware behaviour. Success appears as a `wake` entry
in the Dashboard activity log.

Waking is separate from **Re-apply lighting at launch**: that switch covers a
fresh start; a wake is always re-applied.

## Tray

Closing the window hides it — polling continues and the lighting stays as set —
and the window returns from the tray icon (left click) or **Open OpenColorHub**.
**Quit** is the only real exit.

An autostart launch passes `--minimized`: no window at sign-in.

A second launch hands its command line to the running instance and exits; that
instance brings its window forward. A `--minimized` hand-over asks for nothing.

## Supported boards

* **B650 / B840 / B850 / X870** boards ship the **NCT5584D** (`0xD42A`), verified
  upstream, including the CVN B650M Gaming Frozen V14.
* The **CVN X570M GAMING PRO V14** carries a **`0xD121`** (NCT6793D /
  NCT5567D-B family). Its fan registers were mapped by probing this board, not
  from a vendor document.

Lighting is the common denominator: all expose the same Colorful RGB controller
as HID `2F4C:1000` on usage page `0xFF01`.

## Settling the channel map

`chipmap.rs` uses Colorful's own assignment — SYS 1, CPU 2, AUX0 3, AUX1 8,
AUX2 9, AUX3 10 — and names headers after the channels. Earlier builds shipped
five headers (CPU, CHA_FAN1-3, AIO_PUMP) one row early: "CPU_FAN" wrote bank 1,
which the vendor calls SYS, and bank 10 was never touched.

A pump is a Settings → Safety setting; the bound channel is floored at 60% for
manual duty and curves; none is bound by default.

Open: which *physical* header answers to each channel, and which tachometer
window is real:

| | Windows (measured here) | Vendor's service |
|---|---|---|
| Tachometers | bank 6, `0x56 + 2i` | bank 4, `0xC0 + 2i`, MSB first |
| Temperature | bank 1 `0x50`, then bank 0 | bank 1, `0x50` |

`colorctl fan probe` prints every register both layouts use, from an elevated
shell:

```
colorctl.exe fan probe          # table
colorctl.exe fan probe --json   # machine-readable
```

It writes nothing. Run it twice — idle, then with a manual duty on one channel —
and the registers that move belong to that fan. Label slots from movement, never
channel order. Until then the channel-to-silkscreen mapping is unverified and RGB
is the only fully tested path.
