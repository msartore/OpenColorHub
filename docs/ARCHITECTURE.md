# Architecture

## Why a workspace

The upstream `colorctl` is a single Linux binary that mixes three concerns:
hardware access, business rules (curves, profiles, pump safety) and the CLI.
Reusing any of that from a desktop shell would have meant either linking the
CLI crate (dragging `clap` into the GUI) or copy-pasting register writes. The
workspace splits the concerns instead:

| Crate | Depends on | Responsibility |
| --- | --- | --- |
| `colorctl-core` | `serde`, `thiserror`, `libc` (Linux), `windows-sys` (Windows) | model, curves, CCTL state, settings, backends, board guard, elevation, service façade |
| `colorctl-cli` | core + `clap` | `colorctl` binary, upstream-compatible syntax |
| `src-tauri` (`opencolorhub`) | core + `tauri` + `ureq` | IPC commands, telemetry thread, self-updater, window |
| `src/` (TypeScript) | `@tauri-apps/api` | UI, browser demo backend |

`colorctl-core` compiles and its tests pass on Windows and Linux. That
is what "multi-platform ready" means here: the platform boundary is one trait
pair (`FanBackend`, `LedBackend`), not `#[cfg]` scattered through the UI.

## The backend boundary

```rust
pub trait FanBackend: Send {
    fn info(&self) -> DeviceInfo;
    fn read(&mut self) -> Result<Telemetry>;
    fn set_pwm(&mut self, header: FanHeader, pwm: u8) -> Result<()>;
    fn set_curve(&mut self, header: FanHeader, curve: &FanCurve) -> Result<()>;
}

pub trait LedBackend: Send {
    fn info(&self) -> DeviceInfo;
    fn state(&self) -> &RgbState;
    fn state_mut(&mut self) -> &mut RgbState;
    fn apply(&mut self) -> Result<()>;
    fn state_path(&self) -> Option<PathBuf>;
}
```

Implementations:

* `backend::linux::{LinuxFan, LinuxLed}` — `/dev/port` register I/O for the
  NCT5584D and the hidraw frame writer, ported from upstream with the same
  register sequence.
* `backend::windows::{pawnio, fan}` — fan control through **PawnIO**, a signed
  driver that executes a bytecode module (`LpcIO.bin`) whose port access is
  allow-listed to the selected index pair and discovered BARs. Real RPM, real
  temperature, real manual duty. Curves are evaluated in software (below).
* `backend::windows::{hid, led}` — SetupAPI enumeration plus `hid.dll` report
  I/O for the RGB controller, so lighting works on Windows with no driver and no
  elevation.
* `backend::windows::hwinfo` — read-only fan and temperature telemetry from
  HWiNFO's shared-memory block, used when PawnIO is absent. Writes are refused.
* `backend::sim::{SimFan, SimLed}` — deterministic synthetic telemetry plus a
  real `CCTL` state file on disk.

`backend::windows::pawnio` wraps the driver and the module in two types: `PawnIo`
resolves the five exports with `LoadLibraryW`/`GetProcAddress` and degrades to
"not installed" rather than failing the build, while `LpcIo` implements the
sandbox contract (slot select, BAR discovery, Super I/O index/data access). The
port allow-list is enforced by the driver, not by us, which is the entire reason
this backend exists instead of WinRing0.

Both hardware families build their packets in `backend::frame`, so the 200-LED
wire format has exactly one definition and cannot drift between platforms.

`backend::build(mode, …)` decides which pair to use. `Auto` probes hardware and
falls back per subsystem, recording *why* in `Capabilities::notes`. On Windows
the fan order is PawnIO → HWiNFO → simulator, and the notes say which one won, so
the UI never has to guess. `Hardware` never falls back — it hands back an
`Unsupported*` backend that reports the underlying error, so a missing device can
never masquerade as a working one.

## Curves on Windows are software curves

The `0xD121` Super I/O has mapped manual-duty registers but no known SmartFan
curve layout, so `PawnIoFan` keeps the curve in memory, evaluates it against the
latest temperature on every read, and writes duty when the target moves by three
or more steps. Consequences worth knowing, and documented in the README:

* a curve lives as long as the process does;
* manual duty remains in effect after exit until the BIOS resets the chip;
* `supports_curves()` is `true`, because the user-visible behaviour is a working
  curve — but nothing is written to the curve registers.

## State and persistence

* **Lighting** — 35-byte `CCTL` blob: magic, version, then 6 × (R, G, B,
  brightness, enabled). Identical to upstream, so an existing
  `/var/lib/colorctl/rgb.state` keeps working, and the CLI and GUI can share a
  file. `RgbState::to_leds()` expands it to the 200-LED frame.
* **Settings** — JSON at `<app config>/settings.json`, written atomically
  (temp file + rename). Unknown/corrupt files degrade to defaults with a
  warning instead of blocking startup.
* **Simulator** — `sim-fan.json` next to it, so a restart preserves the
  simulated configuration.
* **Portable mode** — an `opencolorhub.portable` marker next to the executable
  redirects config and data into `config/` and `data/` beside it, so one binary
  serves both installed and portable use.

## The service façade

`Service` owns the backends and enforces the rules that must not be bypassed by
any frontend:

* pump duty floor (60%) on manual writes and curves;
* curve validation (4 points, strictly ascending, 0–100%);
* optional EMA smoothing of temperature/RPM;
* an activity ring buffer surfaced in the UI;
* `ActionResult { snapshot, message }` for every mutation, so the UI never
  re-reads stale state.

## Elevation

The Windows executable carries `src-tauri/app.manifest` with
`requestedExecutionLevel level="requireAdministrator"`, wired in `build.rs`
through `tauri_build::WindowsAttributes`. Windows therefore raises UAC before
the process starts; declining means the app does not run.

`colorctl-core::platform` detects the current state (`OpenProcessToken` +
`GetTokenInformation(TokenElevation)`) and can relaunch itself through
`ShellExecuteExW` with `lpVerb = "runas"`, re-quoting the original command line.
`ERROR_CANCELLED` (1223) is treated as a decision rather than an error: the
caller stays put and says so.

This is deliberately the visible path. The vendor utility analysed in
`iGC.Lite-RE/` instead registers a hidden scheduled task with `RunLevel::Highest`
and rewrites its shortcuts — administrator rights on every launch, no consent
dialog. This app will not do that.

## Updates

`src-tauri/src/update.rs` implements a self-updater without the Tauri updater
plugin, which would require signing keys, a `latest.json` per release and an
installer re-run:

1. `GET https://api.github.com/repos/msartore/OpenColorHub/releases/latest`.
2. Compare `tag_name` with `CARGO_PKG_VERSION` (numeric, tolerant of `v` and
   `-beta` suffixes).
3. If newer and the release carries `OpenColorHub.exe`, offer it in a modal.
4. Download to `<data>/update/`, emitting `update://progress`, then verify
   against `SHA256SUMS.txt` from the same release.
5. Swap: rename the running image to `OpenColorHub.old.exe`, copy the new binary
   into place, `app.restart()`. The leftover is deleted on the next launch, and a
   failed copy rolls the rename back.

`restart()` rather than a manual spawn matters: the old process still holds the
WebView2 profile lock, and racing it produces a window that never appears.

## Tray and window lifecycle

`src-tauri/src/tray.rs` owns the shell behaviour: a tray icon with
**Open** / **Quit**, left click toggling the window, and `CloseRequested`
prevented so the title-bar X hides the window instead of ending the process.

The window is declared `visible: false` in `tauri.conf.json` and only shown when
the process was *not* started with `--minimized` — the flag the autostart entry
passes. Declaring it hidden avoids the flash of an empty frame at sign-in that
`show()`-then-`hide()` would produce.

This uses Tauri's built-in tray rather than the `tray-icon` + `winit` recipe
found in guides: Tauri already runs an event loop, and a second one competes with
the webview for the message pump.

## Threading and IPC

One `Mutex<Inner>` guards the service, because port I/O and the HID frame are
global resources. A dedicated OS thread ticks `Service::snapshot()` every
`poll_interval_ms` and emits a `telemetry` event; the webview never polls. This
keeps blocking I/O off the async runtime entirely.

Commands are thin: lock, call the service, map the error to a `String` that the
UI displays verbatim. `save_settings` rebuilds the backends only when the
backend mode or state path actually changed, then pushes a fresh snapshot.

`capabilities/default.json` grants only `core:default` — no filesystem, shell or
HTTP plugins. All privileged work happens in Rust; the webview cannot open
`/dev/port` or write next to the executable even if it wanted to.

## Frontend

No framework: a small DOM helper (`el`, `svg`) plus views that own their nodes
and update them in place. That matters because telemetry arrives every second —
re-rendering would steal focus from sliders and colour pickers mid-drag.

* `main.ts` — state, hash router, toast stack, keyboard shortcuts (⌘/Ctrl+1…5).
* `views/*` — one module per screen; each exposes `{ el, update(snapshot) }`.
* `update.ts` — release check, modal, download progress.
* `components/charts.ts` — SVG temperature gauge, draggable 4-point curve
  editor, 200-LED frame preview.
* `mock.ts` — a browser backend implementing the same command names and data
  shapes as the Rust side. `npm run dev` therefore runs the entire UI with no
  Rust toolchain, which is also how the UI is verified headlessly.

The interface is intentionally sparse: no persistent status bar, no explanatory
subtitle on every screen, no register or bank identifiers. Diagnostics live
behind a single collapsible line on the dashboard.

## Board detection and the hardware guard

`board::detect()` reads the platform's SMBIOS view once, at service
construction:

* Linux — eight files under `/sys/class/dmi/id` (no privileges needed);
* Windows — the `HKLM\HARDWARE\DESCRIPTION\System\BIOS` registry mirror,
  parsed from `reg query` output so no extra crate is required;

`board::classify()` is pure and carries all the policy, which is why the tests
can cover it exhaustively without hardware:

* names are normalised (case, whitespace, `(ATX)` hints, `V14`/`V14A`/`V15`
  revisions) before comparison, and a prefix match tolerates marketing suffixes;
* exact catalogue hit → **verified**;
* Colorful board on a supported generation → **compatible**;
* Colorful board on a *legacy* chipset (X570, B550, B450, …) → **unknown**: the
  Super I/O probe is still allowed, because the chip ID check is the authority —
  this is the path the CVN X570M in this repository takes;
* any other vendor → **unsupported**, and `backend::build` then refuses to open
  the hardware backend at all in `auto` mode.

The verdict rides along in `Snapshot.board`, so every IPC response and the CLI
carry the same facts the guard used. `Service::with_board` allows tests and the
CLI's `--backend simulated` path to inject a known identity.

## Hardware protocol notes

**Fans, Linux (NCT5584D, chip `0xD42A`).** Super I/O index/data pairs are probed
at 0x4E/0x4F then 0x2E/0x2F; the chip ID is validated before any write. The
hardware-monitor base comes from LDN 0x0B, giving index = base+5 and data =
base+6. Per-header banks: CPU 2, CHA_FAN1 1, CHA_FAN2 10, CHA_FAN3 3, AIO_PUMP 11.
Manual mode is register `0x02 = 0x00` plus duty in `0x09`; SmartFan writes
`0x66 = 0xAA`, `0x03 = 0x01`, `0x04 = 0x01`, `0x02 = 0x40`, then four
temperatures in `0x21..0x24` and four duties in `0x27..0x2A`. Tachometers live in
bank 4 at 0xC0/0xC2/0xC4/0xCA/0xCE.

**Fans, Windows (NCT6793D / NCT5567D-B family, chip `0xD121`).** Measured, not
assumed — the register map came from an elevated probe written for this board:

* HWM base `0x0A30`, index/data at `base+0x05` / `base+0x06`;
* bank select is register `0x4E` written *through* the index/data pair
  (`base+0x4E` is not the bank register and is denied by the sandbox);
* tachometer bank 6, registers `0x56 + 2i`;
* PWM duty read-back in bank 0 at `0x01, 0x03, 0x11, 0x13, 0x15`;
* PWM command banks 1, 2, 3, 8, 9 at register `0x09`;
* manual-mode bit at register `0x02`.

Traps that cost time: bank 0 registers `0x00`/`0x02` masquerade as RPM
(`0x04A0` = reserved byte + duty `0xA0` = 63%), and every bank reads identically
when the bank write is silently dropped. Sensors are therefore identified by how
they respond to load, not by whether the value looks plausible. Writes are
verified by read-back, which catches a wrong register but not a rotated
bank→header mapping — only ears on the actual fans can settle that.

**Lighting.** The controller enumerates as HID `2F4C:1000` with usage page
`0xFF01`. The frame is ten 65-byte packets (report ID, `01 00 88`, packet index,
20 × RGB) followed by a flush packet with index `0xFF`. Some drivers reject the
65-byte form with `EINVAL`; the first write probes and the rest follow whichever
worked, which is the only reliable behaviour across kernels.

## Packaging

* `scripts/icons.py` regenerates the PNG set without a resampling round-trip:
  sizes the source `.ico` already contains are decoded straight out of it and
  only the rest are resampled. `npx tauri icon` needs a ≥1024 master, so feeding
  it a 256 px source produced every size as 256 → 1024 → N and went soft.
* `scripts/portable.ps1` stages the portable zip and writes the marker file.
* `tauri.conf.json` builds NSIS, MSI and the Linux bundles; the release workflow
  stages everything flat into `dist/`, adds `SHA256SUMS.txt` and the bare
  `OpenColorHub.exe` the updater downloads.

## Testing strategy

* `cargo test` — 102 core tests plus 2 updater tests: state round-trip against a
  byte-exact fixture, curve interpolation and validation, pump floors, LED range
  coverage, settings clamping, sim persistence, version comparison.
* `npm run build` — `tsc --noEmit` then a Vite build; the type layer is the
  contract between Rust serde representations and the UI.
* Headless UI verification — the browser demo is driven over CDP (click, read
  DOM, screenshot) so every screen is exercised without a human.
* Desktop verification — the built binary is launched with
  `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=…` and
  `__COMPAT_LAYER=RunAsInvoker` (the manifest demands elevation and a UAC click
  cannot be automated), then driven over CDP; the resulting `sim-fan.json`,
  `rgb.state` and `settings.json` are inspected to prove the Rust side really
  wrote them.

## Empty and error states

Every screen can end up with nothing to show or with a failed read, and a blank
panel cannot tell those apart. `src/components/state.ts` renders both in one
shape — a large Tabler glyph, a title, a sentence saying what happened, optional
technical detail in mono, and an action when retrying makes sense.

- `state--empty` is neutral: no fans reporting a tach, no lighting channels, no
  catalogue entries, a board SMBIOS could not identify.
- `state--error` turns the glyph red and carries the backend's own message
  (`snapshot.read_error`, `led_device.error`, the fetch error) rather than a
  paraphrase, so the detail line is always something a bug report can quote.
- `stateSlot()` wraps the pattern used by Fan control, Lighting and the
  Dashboard: hide the real content, show the state in its place, clear it when
  data returns.
