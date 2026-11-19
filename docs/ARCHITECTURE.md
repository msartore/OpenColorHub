# Architecture

## Why a workspace

Upstream `colorctl`: one Linux binary mixing hardware access, business rules
(curves, profiles, pump safety) and the CLI. Reuse from a desktop shell meant
linking the CLI crate (`clap` in the GUI) or copy-pasting register writes. Split
instead:

| Crate | Depends on | Responsibility |
| --- | --- | --- |
| `colorctl-core` | `serde`, `thiserror`, `libc` (Linux), `windows-sys` (Windows) | model, curves, CCTL state, GPU state and model catalogue, settings, backends, board guard, elevation, service façade |
| `colorctl-cli` | core + `clap` | `colorctl` binary, upstream-compatible syntax |
| `src-tauri` (`opencolorhub`) | core + `tauri` + `ureq` | IPC commands, telemetry thread, self-updater, window |
| `src/` (TypeScript) | `@tauri-apps/api` | UI, browser demo backend |

`colorctl-core` builds and tests pass on Windows and Linux; the platform boundary
is the trait pair (`FanBackend`, `LedBackend`) plus the self-contained
`GpuLedBackend`, not `#[cfg]` through the UI.

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

* `backend::linux::{LinuxFan, LinuxLed}` — `/dev/port` register I/O for the
  NCT5584D, hidraw frame writer; upstream register sequence.
* `backend::windows::{pawnio, fan}` — **PawnIO**, a signed driver running a
  bytecode module (`LpcIO.bin`) whose port access is allow-listed to the selected
  index pair and discovered BARs. Real RPM, temperature, manual duty; curves in
  software (below).
* `backend::windows::{hid, led}` — SetupAPI + `hid.dll` report I/O: lighting
  without driver or elevation.
* `backend::windows::hwinfo` — read-only telemetry from HWiNFO's shared-memory
  block when PawnIO is absent. Writes refused.
* `backend::sim::{SimFan, SimLed}` — deterministic synthetic telemetry plus a
  real `CCTL` state file on disk.
* `gpu::GpuLedBackend` — a third, optional backend for GPU RGB. It owns the
  detected vendor transports and their state; `gpu_catalogue` resolves a card's
  PCI subsystem ID to a per-model profile (zones, I²C address, effects). The
  transports call vendor libraries dynamically (`atiadlxx.dll`, `nvapi.dll`,
  `ControlLib.dll`/`igcl.dll`) or `/dev/i2c-*`; no kernel module or bundled
  driver is involved. The 0x36 wire format is in `gpu::build_frame`.

`PawnIo` resolves the five exports with `LoadLibraryW`/`GetProcAddress` and
degrades to "not installed" rather than failing the build; `LpcIo` implements the
sandbox contract (slot select, BAR discovery, index/data access). The driver
enforces the port allow-list — hence this backend instead of WinRing0.

Both families build packets in `backend::frame`: one 200-LED wire format.

`backend::build(mode, …)` picks the pair. `Auto` probes and falls back per
subsystem, recording *why* in `Capabilities::notes`; Windows fan order is
PawnIO → HWiNFO → simulator. `Hardware` never falls back — it returns an
`Unsupported*` backend with the underlying error.

## Curves on Windows are software curves

`0xD121` has manual-duty registers but no known SmartFan curve layout. `PawnIoFan`
keeps the curve in memory, re-evaluates it on every read, and writes duty when the
target moves by three or more steps.

* a curve lives as long as the process;
* manual duty remains after exit until the BIOS resets the chip;
* `supports_curves()` is `true`, but nothing is written to the curve registers.

## State and persistence

* **Lighting** — 35-byte `CCTL` blob: magic, version, 6 × (R, G, B, brightness,
  enabled). Identical to upstream, so `/var/lib/colorctl/rgb.state` keeps working
  and CLI and GUI share a file; the system path is adopted only when writable.
  `RgbState::to_leds()` expands it to the 200-LED frame.
* **Settings** — `<app config>/settings.json`, atomic write (temp file + rename);
  unknown or corrupt files degrade to defaults with a warning.
* **Active setup** — `<state dir>/active-setup.json`: the duties and curves the
  backends are actually driving plus the lighting frame on the controller.
  Rewritten after every successful hardware write and replayed at launch, so a
  shutdown, restart, update or elevation relaunch restores what the user left.
  Sits beside `rgb.state`, so the CLI and the app share it when they share that
  file. `restore_setup_on_startup` (default on) can disable the replay;
  `release_fans_on_exit` (default off) decides whether exit hands fans to the
  chip's SmartFan.
* **GPU** — `<state dir>/gpu-led.state`, JSON keyed by
  `"<device>:<zone>"` so adding or removing a card does not shift the other
  devices' colours. Restored with the active setup and re-applied on resume.
* **Simulator** — `sim-fan.json` beside it, preserved across restarts.
* **Portable mode** — an `opencolorhub.portable` marker next to the executable
  redirects config and data into `config/` and `data/`.

## The service façade

`Service` enforces what no frontend may bypass:

* pump duty floor (60%) on manual writes and curves;
* curve validation (4 points, strictly ascending, 0–100%);
* optional EMA smoothing of temperature/RPM;
* an activity ring buffer in the UI;
* GPU zone writes, board-to-GPU colour sync and the `gpu_led_enabled` toggle,
  holding the detected transports behind the service mutex;
* `ActionResult { snapshot, message }` per mutation, so the UI never re-reads
  stale state. The GPU mutations are `set_gpu_zone`, `apply_gpu` and
  `sync_gpu_from_board`; all GPU state is inside `Snapshot.gpu_led`.

## Elevation

`src-tauri/app.manifest` sets
`requestedExecutionLevel level="requireAdministrator"`, wired in `build.rs`
through `tauri_build::WindowsAttributes`: UAC runs before the process starts and
declining means the app does not run.

`colorctl-core::platform` reads the state via `OpenProcessToken` +
`GetTokenInformation(TokenElevation)` and can relaunch through `ShellExecuteExW`
with `lpVerb = "runas"`, re-quoting the command line. `ERROR_CANCELLED` (1223) is
a decision, not an error.

The vendor utility in `iGC.Lite-RE/` instead registers a hidden scheduled task
with `RunLevel::Highest` and rewrites shortcuts. This app will not.

## Updates

`src-tauri/src/update.rs`, without the Tauri updater plugin (signing keys,
`latest.json` per release, installer re-run):

1. `GET https://api.github.com/repos/msartore/OpenColorHub/releases/latest`.
2. Compare `tag_name` with `CARGO_PKG_VERSION` (numeric, tolerant of `v` and
   `-beta` suffixes).
3. If newer and the release carries `OpenColorHub.exe`, offer it in a modal.
4. Download to `<data>/update/`, emitting `update://progress`, then verify
   against `SHA256SUMS.txt` from the same release.
5. Swap: rename the running image to `OpenColorHub.old.exe`, copy the new binary
   into place, `app.restart()`. The leftover is deleted on the next launch, and a
   failed copy rolls the rename back.

`restart()` matters: the old process holds the WebView2 profile lock, and racing
it produces a window that never appears.

## Tray and window lifecycle

`src-tauri/src/tray.rs`: tray icon with **Open** / **Quit**, left click toggles
the window, `CloseRequested` prevented so the title-bar X hides rather than exits.

The window is `visible: false` in `tauri.conf.json`, shown only without
`--minimized` (the autostart flag) — no empty-frame flash at sign-in, as
`show()`-then-`hide()` would produce.

Only one process owns the hardware. `tauri-plugin-single-instance` registers
first: a second launch forwards its command line and exits before its own setup —
no second tray icon, no second claim on the Super I/O port or HID interface. The
running instance treats the hand-over as "show yourself" and calls
`tray::reveal`, unless the forwarded line carries `--minimized`.

The hand-over is `WM_COPYDATA` to a hidden plugin window, and Windows blocks that
message between integrity levels. Every launch path must start at the same
elevation — what the `requireAdministrator` manifest buys. Without it the build
runs `asInvoker`: a click while the elevated instance is up finds the mutex, fails
to deliver the hand-over, and exits.

The lock is a named mutex created in the plugin's setup; `RunEvent::Exit`
releases it, and `App::run` dispatches that event before `process::restart`, so a
self-update is not mistaken for a rival instance. The plugin is pinned below 2.5,
whose minimum Tauri (2.12) is newer than this crate's.

Tauri's built-in tray is used rather than `tray-icon` + `winit`: a second event
loop competes with the webview for the message pump.

## Threading and IPC

One `Mutex<Inner>` guards the service; port I/O and the HID frame are global. A
dedicated OS thread ticks `Service::snapshot()` every `poll_interval_ms` and emits
`telemetry`, keeping the webview out of polling and blocking I/O off the async
runtime. The same thread catches hardware the machine forgot: a tick well past its
interval means suspend, a controller that stops answering means re-enumeration.
Either re-applies the lost lighting and fan settings
(`Service::reassert_after_wake`) before the next snapshot.

Commands are thin: lock, call the service, map the error to a `String` the UI
shows verbatim. `save_settings` rebuilds backends only when the mode, state path
or GPU toggle changed, then pushes a fresh snapshot. `set_gpu_zone`, `apply_gpu`
and `sync_gpu_from_board` expose the GPU backend with the same `ActionResult`
pattern as the board mutations.

`capabilities/default.json` grants only `core:default` — no filesystem, shell or
HTTP plugins. The webview cannot open `/dev/port` or write next to the executable.

## Frontend

No framework: a DOM helper (`el`, `svg`) and views that own their nodes and
update in place — telemetry arrives every second, and re-rendering would steal
focus from sliders and colour pickers mid-drag.

* `main.ts` — state, hash router, toast stack, keyboard shortcuts (⌘/Ctrl+1…5).
* `views/*` — one module per screen; each exposes `{ el, update(snapshot) }`.
* `update.ts` — release check, modal, download progress; release notes are
  rendered from Markdown by `markdown.ts`, a DOM-only parser (no `innerHTML`)
  supporting headings, tables, lists, quotes, code and links.
* `views/leds.ts` — Lighting is two expandable groups (Motherboard / GPU), each
  with one main colour and a header-level **Sync** button that opens a modal to
  pick the source colour. Expanding reveals the channel / GPU-zone cards; GPU
  cards use the same card structure as board channels.
* `components/icons.ts` — Tabler glyphs, including the group chevron and the
  bookmark used by the preset buttons.
* `components/charts.ts` — SVG temperature gauge, draggable 4-point curve editor,
  200-LED frame preview.
* `mock.ts` — same command names and data shapes as the Rust side, so
  `npm run dev` runs the whole UI without a Rust toolchain, headlessly verifiable.

Sparse by design: no status bar, no per-screen subtitle, no register or bank
identifiers. Diagnostics sit behind one collapsible dashboard line.

## Board detection and the hardware guard

`board::detect()` reads SMBIOS once, at service construction:

* Linux — eight files under `/sys/class/dmi/id` (no privileges);
* Windows — `HKLM\HARDWARE\DESCRIPTION\System\BIOS`, parsed from `reg query`
  output, so no extra crate.

`board::classify()` is pure and holds all policy:

* names normalised (case, whitespace, `(ATX)` hints, `V14`/`V14A`/`V15`
  revisions) before comparison; prefix match for marketing suffixes;
* exact catalogue hit → **verified**;
* Colorful board, supported generation → **compatible**;
* Colorful board on a *legacy* chipset (X570, B550, B450, …) → **unknown**: the
  Super I/O probe is allowed, because the chip-ID check is the authority — the
  path the CVN X570M here takes;
* any other vendor → **unsupported**; `backend::build` then refuses the hardware
  backend in `auto` mode.

The verdict rides in `Snapshot.board`, so every IPC response and the CLI carry the
same facts. `Service::with_board` injects a known identity for tests and the CLI's
`--backend simulated`.

## Hardware protocol notes

**Fan channels.** `crates/colorctl-core/src/chipmap.rs` names and numbers the six
channels — SYS 1, CPU 2, AUX0 3, AUX1 8, AUX2 9, AUX3 10 — Colorful's own
`NCT5567D_B_Service` assignment. Both backends read that table; headers are named
after the channels, not a silkscreen guess, since the vendor service never calls
one `AIO_PUMP`.

* HWM base `0x0A30` on this board, index/data at `base+0x05` / `base+0x06`;
* bank select is register `0x4E` written *through* the index/data pair
  (`base+0x4E` is not the bank register and is denied by the sandbox);
* PWM command at register `0x09` of that bank, mode bit at `0x02`;
* duty read-back in bank 0 at `0x01, 0x03, 0x11, 0x13, 0x15, 0x17`;
* tachometers: bank 6 at `0x56 + 2i` on Windows (measured here — 3640 rpm idle,
  4426 under load), bank 4 at `0xC0 + 2i` on Linux (the vendor's layout, not yet
  measured on that chip);
* a duty write is verified against the same bank's `0x09`, not the bank-0 table,
  so the check does not reuse the slot order it checks;
* on exit the desktop app writes `0x02 = 0x40` for every channel it put in manual
  mode, handing control back to the chip's SmartFan curve. The CLI does not: it
  writes a duty and exits on purpose;
* a channel bound as a pump in Settings is floored at 60% for duty and curves;
  none is bound by default, because this board's table names no pump channel.

**Fans, Linux (NCT5584D, chip `0xD42A`).** Index/data pairs probed at 0x4E/0x4F
then 0x2E/0x2F; chip ID validated before any write. Hardware-monitor base from LDN
0x0B: index = base+5, data = base+6. Manual mode is `0x02 = 0x00` plus duty in
`0x09`; the Linux backend can also program SmartFan points (`0x66 = 0xAA`,
`0x03 = 0x01`, `0x04 = 0x01`, `0x02 = 0x40`, four temperatures in `0x21..0x24`,
four duties in `0x27..0x2A`). Windows evaluates curves in software.

Bank 0 registers `0x00`/`0x02` masquerade as RPM (`0x04A0` = reserved byte + duty
`0xA0` = 63%), and every bank reads identically when the bank write is silently
dropped. Identify sensors by response to load, not plausibility. Read-back
catches a wrong register but not a rotated bank→header mapping — only ears on the
fans settle that.

**Lighting.** HID `2F4C:1000`, usage page `0xFF01`. Ten 65-byte packets (report
ID, `01 00 88`, packet index, 20 × RGB) then a flush packet with index `0xFF`.
Some drivers reject the 65-byte form with `EINVAL`; the first write probes, the
rest follow whichever worked.

## Packaging

* `scripts/icons.py` regenerates the PNG set without a resampling round-trip:
  sizes the source `.ico` already contains are decoded from it, only the rest are
  resampled (`npx tauri icon` needs a ≥1024 master; a 256 px source produced every
  size as 256 → 1024 → N and went soft).
* `scripts/portable.ps1` stages the portable zip and writes the marker file.
* `tauri.conf.json` builds NSIS, MSI and the Linux bundles; the release workflow
  stages everything flat into `dist/`, adds `SHA256SUMS.txt` and the bare
  `OpenColorHub.exe` the updater downloads.

## Testing strategy

* `cargo test` — 185 core tests plus 2 updater tests (7 CLI, 3 desktop):
  state round-trip against a byte-exact fixture, curve interpolation and
  validation, pump floors, LED range coverage, settings clamping, sim
  persistence, GPU frame construction, simulated GPU service mutations, PCI
  catalogue lookup and fallback, version comparison.
* `npm run build` — `tsc --noEmit` then a Vite build; the type layer is the
  contract between Rust serde representations and the UI.
* Headless UI verification — the browser demo is driven over CDP (click, read
  DOM, screenshot).
* Desktop verification — the built binary runs with
  `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=…` and
  `__COMPAT_LAYER=RunAsInvoker` (the manifest demands elevation and a UAC click
  cannot be automated), then over CDP; `sim-fan.json`, `rgb.state` and
  `settings.json` prove the Rust side wrote them.

## Empty and error states

A blank panel cannot distinguish "nothing to show" from a failed read.
`src/components/state.ts` renders both in one shape: a large Tabler glyph, a
title, a sentence on what happened, optional mono detail, and an action when
retrying makes sense.

- `state--empty` is neutral: no tach, no lighting channels, no catalogue entries,
  an unidentifiable board.
- `state--error` turns the glyph red and carries the backend's own message
  (`snapshot.read_error`, `led_device.error`, the fetch error), so the detail is
  quotable in a bug report.
- `stateSlot()` hides the content, shows the state, clears it when data returns —
  the pattern in Fan control, Lighting and the Dashboard.
