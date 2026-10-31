# Platform support and hardware access

## What works where

| Platform | Fan control | Lighting |
| --- | --- | --- |
| **Linux** | ✅ `/dev/port`, needs root or `CAP_SYS_RAWIO` | ✅ `/dev/hidraw` for `2F4C:1000` |
| **Windows** | ✅ PawnIO sandboxed driver · ⚪ read-only via HWiNFO when PawnIO is absent | ✅ SetupAPI + `hid.dll`, no driver |

## Why Windows needs a driver at all

`IN`/`OUT` are ring-0 instructions. Windows exposes no user-mode port broker
equivalent to Linux's `/dev/port`, so user-mode code cannot touch a Super I/O
directly. The options are a kernel driver of your own,
[WinRing0](https://nvd.nist.gov/vuln/detail/CVE-2020-14979) — which this project
will not ship, see `NOTICE.md` — or **PawnIO**: a signed driver that executes a
bytecode module whose port access is allow-listed to the selected index pair and
discovered BARs, and which never exposes physical memory.

OpenColorHub talks to it through `PawnIOLib.dll`, with the LpcIO module embedded
and passed to the driver at runtime.

**PawnIO is not bundled.** Install it separately — the LibreHardwareMonitor
installer ships it. Without it the app falls back, in order:

1. **PawnIO** — real RPM, real board temperature, real manual duty. Requires the
   driver installed and the app elevated.
2. **HWiNFO shared memory** — read-only. Needs HWiNFO64 running with
   *Settings → Safety → Shared Memory Support* enabled. Two header layouts exist
   in the wild (`__time64_t` packed or 8-byte aligned) and both are detected from
   the block itself, so a version bump does not silently break it.
   `OPENCOLORHUB_HWINFO_SHM` overrides the section name.
3. **Simulator** — persists state, evaluates curves and enforces the pump floor,
   so the app is fully usable anywhere.

The chosen backend is named in Settings and in the sidebar; nothing is silently
simulated.

### Curves on Windows are software curves

The manual-duty registers are mapped for the `0xD121` chip; the SmartFan curve
registers are not. A curve therefore lives as long as the process does, and
manual duty stays as last written after the app exits until the BIOS resets it.

### Exercising the HWiNFO reader

```bash
cargo run -p colorctl-core --example hwinfo_standin   # holds the section open
colorctl fan status                                   # reads it
```

## Lighting over HID

Lighting needs no privileged access: the RGB controller is an ordinary USB HID
device, reached through SetupAPI and `hid.dll` with no driver, service or
elevation.

`colorctl rgb probe` shows what the HID layer finds:

```
Colorful HID interfaces (VID 2F4C, PID 1000):
  MI_02  usage page 0xFF01  out  65  feat   0  <= selected  [COLORFUL]
        \\?\hid#vid_2f4c&pid_1000&mi_02#<instance>#{4d1e55b2-f16f-11cf-88cb-001111000030}
  MI_01  usage page 0xFF00  out  65  feat   0  [X570-USBHID-LED]
        \\?\hid#vid_2f4c&pid_1000&mi_01#<instance>#{4d1e55b2-f16f-11cf-88cb-001111000030}
```

MI_02 (`0xFF01`) carries LED frames. MI_01 (`0xFF00`) is the firmware-update
collection — same VID/PID, different usage page, never written to. If a firmware
revision expects feature reports instead of output reports, set
`OPENCOLORHUB_HID_TRANSPORT=feature`; the backend falls back automatically when
the output-report write fails.

## Elevation

The Windows build carries a manifest requesting `requireAdministrator`, so
Windows raises its own UAC prompt before the process starts. Every launch asks,
declining means the app does not run, and nothing is persisted.

Elevation is what makes PawnIO fan control work: `pawnio_open` returns
`ERROR_ACCESS_DENIED` to an unelevated process. RGB and HWiNFO do not need it, so
an unelevated build still drives lighting and still reads sensors when HWiNFO is
running — it just cannot touch the Super I/O.

Settings → Privileges shows the current state and offers a one-shot *Restart as
administrator*. Builds that already carry the manifest never show that button,
because they can never be running unelevated.

This is deliberately the visible path. Some vendor utilities register a hidden
scheduled task with `RunLevel::Highest` and rewrite their shortcuts, gaining
administrator rights on every launch with no consent dialog. This app will not
do that: one prompt, one decision, nothing persisted.

Trade-off: requiring elevation means the portable build also prompts for UAC. To
drop it, delete `src-tauri/app.manifest` and revert `build.rs` to
`tauri_build::build()` — at the cost of Windows fan control, which then degrades
to HWiNFO or the simulator.

## Start with the OS

**Settings → Startup** registers a sign-in entry for the current user:

* **Windows** — a Task Scheduler logon task named `OpenColorHub` with
  `RunLevel Highest`. A `HKCU\...\Run` value is simpler but starts the process
  unelevated, which for this app means no fan control and a UAC prompt at every
  sign-in anyway.
* **Linux** — `${XDG_CONFIG_HOME:-~/.config}/autostart/opencolorhub.desktop`.

Turning the switch off deletes the entry. It is created only when asked for,
named plainly in Task Scheduler, and removed by the same switch.

Enabling it also turns on **Re-apply lighting at launch**, so the saved `CCTL`
state is pushed at sign-in; otherwise the app would start and leave the lighting
dark.

## Tray

The app lives in the tray. Closing the window hides it rather than quitting —
polling continues and the lighting stays as set — and the window returns from the
tray icon (left click) or its **Open OpenColorHub** menu entry. **Quit** is the
only real exit.

A launch from the autostart entry passes `--minimized`, so signing in shows no
window at all.

## Supported boards

Two Super I/O families are in play, which is why nothing promises a single chip:

* **B650 / B840 / B850 / X870** boards ship the **NCT5584D** (`0xD42A`), which is
  what upstream verified, including the CVN B650M Gaming Frozen V14.
* The **CVN X570M GAMING PRO V14** is older and carries a **`0xD121`**
  (NCT6793D / NCT5567D-B family). Its fan registers were mapped by probing this
  board, not from a vendor document.

Lighting is the common denominator: all of these expose the same Colorful RGB
controller as HID `2F4C:1000` on usage page `0xFF01`.
