# Architecture

## Why a workspace

Upstream `colorctl` was one Linux binary mixing hardware access, business rules
(curves, profiles, pump safety) and the CLI. Reuse from a desktop shell meant
linking the CLI crate (`clap` in the GUI) or copy-pasting register writes. Split
instead:

| Crate / directory | Depends on | Responsibility |
| --- | --- | --- |
| `colorctl-core` | `serde`, `thiserror`, `libc` (Linux), `windows-sys` (Windows) | model, curves, CCTL state, settings, backends, board guard, elevation, service façade |
| `colorctl-cli` | core + `clap` | `colorctl` binary, upstream-compatible syntax |
| `nsh/` | core + `nsh-core` + `nsh-base` | the Native Shell app: one Rust `Controller`, TSX screens |
| `nsh/app/domain` | nothing | pure wire model and rules (curves, colours, settings, presets, drivers) |
| `nsh/app/application` | domain | ports and use cases |
| `nsh/app/infrastructure` | application + the generated native module | one JSON → typed adapter |
| `nsh/app/composition` | infrastructure + application | the app-state facade and the composition root |

`colorctl-core` builds and tests on Windows and Linux; the platform boundary is
one trait pair (`FanBackend`, `LedBackend`), not `#[cfg]` through the UI.

## The app shell

The desktop app is a normal Native Shell project. `nsh.json` names the crate and
the window; `App.tsx` mounts one component tree. There is no webview:
`View`, `Text`, `Scroll`, `Canvas` and `nsh/base` controls are native views on
each platform, and the bundle runs the Rust `Controller` directly.

`src/lib.rs` exports one `#[class] Controller` through
`#[native_module("opencolorhub/native")]`. TSX can call:

- JSON reads: `snapshotJson`, `settingsJson`, `aboutJson`, `activityJson`,
  `listSetups`, `driversJson`, `pawnioStatusJson`, `elevationJson`, …
- writes with typed scalar arguments: fan duty/curves/profiles, channel
  colours, brightness, presets, settings, autostart, elevation.
- one streaming loop: `startTelemetry(callback)`, an async member that uses
  `timer` and `blocking` and posts a JSON snapshot each poll.

The generated `.nsh/types` keeps TSX checked against those signatures. DTO
shapes stay in `colorctl-core`; `nsh/app/infrastructure/native.ts` parses each
JSON answer once, so no screen sees a raw string.

## Dependency rules inside `nsh/`

- `domain/` imports only `domain/`: no signals, no `nsh`, no I/O.
- `application/` imports `domain/`; ports speak domain types.
- `infrastructure/` is the only place that names `Controller`; it maps JSON and
  turns strings into `AppError`.
- `composition/` constructs the adapter, the store and the commands. Screens
  never build a port.
- `screens/` and `components/` read the store and call commands; they never
  import `infrastructure/` or a native module.

The one deliberate duplication is the wire model (`nsh/app/domain/model.ts`):
the Rust `serde` types are the authority, and the adapter is the seam.

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

`PawnIo` resolves the five exports with `LoadLibraryW`/`GetProcAddress` and
degrades to "not installed" rather than failing the build; `LpcIo` implements
the sandbox contract (slot select, BAR discovery, index/data access). The driver
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

The TSX curve editor is a native `Canvas` drawing with press/drag editing; the
same four-point validation and pump floor live in `domain/fan.ts` for preview,
and `Service` re-checks every write.

## State and persistence

* **Lighting** — 35-byte `CCTL` blob: magic, version, 6 × (R, G, B, brightness,
  enabled). Identical to upstream, so `/var/lib/colorctl/rgb.state` keeps working
  and CLI and GUI share a file; the system path is adopted only when writable.
  `RgbState::to_leds()` expands it to the 200-LED frame.
* **Settings** — `<app config>/settings.json`, atomic write (temp file + rename);
  unknown or corrupt files degrade to defaults with a warning.
* **Setups** — `<data>/setups.json`, versioned, fan and lighting presets in one
  file but never mixed on apply.
* **Simulator** — `sim-fan.json` beside it, preserved across restarts.
* **Portable mode** — an `opencolorhub.portable` marker next to the executable
  redirects config and data into `config/` and `data/`.

The NSH `Controller` reads the same identifier directories as the previous shell:
`%APPDATA%\dev.msartore.opencolorhub` on Windows, XDG config/data on Linux.

## The service façade

`Service` enforces what no frontend may bypass:

* pump duty floor (60%) on manual writes and curves;
* curve validation (4 points, strictly ascending, 0–100%);
* optional EMA smoothing of temperature/RPM;
* an activity ring buffer in the UI;
* `ActionResult { snapshot, message }` per mutation, so the UI never re-reads
  stale state.

## Elevation and PawnIO

The installer is per-user and deliberately does **not** request administrator;
`nsh package` has no `requireAdministrator` manifest. Fan control on Windows needs
PawnIO's driver, which a standard process may not open.

The app therefore asks at startup, after the telemetry probe:

1. `platform::state()` reports `{ elevated, can_request, detail }`.
2. If PawnIO is `denied` and the app can ask, a native alert says fan control
   needs administrator rights.
3. On consent, `platform::request()` relaunches the executable with
   `ShellExecuteExW` + `lpVerb = "runas"`. Windows shows its own UAC prompt; a
   declined UAC is a decision, not an error.
4. The process quits so the elevated copy takes over; the elevated copy probes
   PawnIO again and reads real fans. Lighting, settings and telemetry work
   without elevation.

The autostart entry is created with `schtasks /RL HIGHEST` (Windows) or an XDG
`.desktop` entry with `--minimized` (Linux), so a sign-in launch already has the
rights fan control needs. `--minimized` keeps the window hidden until the tray
icon is used.

Single instance is `nsh.json` `"singleInstance": true`: a second launch hands its
arguments over and exits; the running instance shows itself unless the launch came
from autostart. Windows blocks `WM_COPYDATA` between integrity levels, so every
launch path must use the same elevation; the elevated relaunch quits the old
process first.

## Updates

`nsh/updater` in the app:

1. `nsh.json` names the source: `"updater": { "github": "msartore/OpenColorHub" }`.
2. Settings → Updates calls `checkForUpdate()`; a packaged app gets the release's
   version, notes and download.
3. `downloadUpdate()` reports bytes, and `installAndRestart()` replaces the app
   and restarts it. The archive and manifest are verified by NSH; no custom
   rename-and-swap exists.

The release workflow writes `latest.json` with `nsh update-manifest` from the
Windows zip and the Linux tarball, and uploads it beside them. A development run
returns `null` from the check, so the panel says update checks need a packaged
build. Existing Tauri installs cannot move onto this app through the old updater;
the release notes call for one manual install.

## Tray and window lifecycle

`Tray.set` gives an icon with **Open** and **Quit**; a left click focuses the
window. The main window starts hidden (`"defaultVisible": false`), is shown
unless `--minimized`, and its close button hides it to the tray
(`"closeOnPress": false` + `closePress` → `hide`). The window placement is saved
in `Storage` and restored on launch.

## Threading and the native seam

One `Mutex<Inner>` guards the service; port I/O and the HID frame are global. The
TSX `Controller` owns the same lock and shares it with blocking work:

* `startTelemetry` waits on the engine task runtime, reads the service through
  `blocking` (port I/O never stalls the UI), and posts JSON to JS.
* The loop also watches the gap between ticks. A gap well past the interval is a
  suspend; fans and lighting are re-applied with retries and the outcome is noted
  in the activity list.
* Writes run on the engine thread through the same lock; a class method returns
  `Result<String, String>`, which TSX sees as a value or a rejected promise.

Commands are thin: lock, call the service, map the error to a message the UI
shows verbatim. `saveSettingsJson` rebuilds backends only when the mode or state
path changed.

## Frontend

TSX screens in `nsh/screens` keep their own nodes and update signals in place:

* `App.tsx` — composition root, tray/window lifecycle, shortcuts, elevation
  prompt, last screen.
* `components/Shell.tsx` — sidebar, scrolling content, toasts.
* `components/CurveChart.tsx` — native Canvas four-point editor.
* `screens/*` — Dashboard, Fan control, Lighting, Drivers, Settings, About.
* `app/presentation/layout.ts` — width breakpoints: wide (≥1200), medium
  (900–1199), narrow; the sidebar, stat tiles, channel cards, fan cards, settings
  columns and dialogs follow them.

Sparse by design: no status bar, no per-screen subtitle, no register or bank
identifiers. Diagnostics sit behind one collapsible dashboard line. Empty and
error states share one shape: a title, a sentence on what happened, optional mono
detail, and an action when retrying makes sense.

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
  Super I/O probe is allowed, because the chip-ID check is the authority;
* any other vendor → **unsupported**; `backend::build` then refuses the hardware
  backend in `auto` mode.

The verdict rides in `Snapshot.board`, so every command and the CLI carry the
same facts. `Service::with_board` injects a known identity for tests and the CLI's
`--backend simulated`.

## Hardware protocol notes

**Fan channels.** `crates/colorctl-core/src/chipmap.rs` names and numbers the six
channels — SYS 1, CPU 2, AUX0 3, AUX1 8, AUX2 9, AUX3 10 — Colorful's own
`NCT5567D_B_Service` assignment. Both backends read that table; headers are named
after the channels, not a silkscreen guess.

* HWM base `0x0A30` on this board, index/data at `base+0x05` / `base+0x06`;
* bank select is register `0x4E` written *through* the index/data pair;
* PWM command at register `0x09` of that bank, mode bit at `0x02`;
* duty read-back in bank 0 at `0x01, 0x03, 0x11, 0x13, 0x15, 0x17`;
* tachometers: bank 6 at `0x56 + 2i` on Windows, bank 4 at `0xC0 + 2i` on Linux;
* a duty write is verified against the same bank's `0x09`, not the bank-0 table;
* on exit the app writes `0x02 = 0x40` for every channel it put in manual mode,
  handing control back to the chip's SmartFan curve. The CLI does not;
* a channel bound as a pump in Settings is floored at 60% for duty and curves.

**Fans, Linux (NCT5584D, chip `0xD42A`).** Index/data pairs probed at 0x4E/0x4F
then 0x2E/0x2F; chip ID validated before any write. Hardware-monitor base from LDN
0x0B: index = base+5, data = base+6. Manual mode is `0x02 = 0x00` plus duty in
`0x09`; the Linux backend can also program SmartFan points.

**Lighting.** HID `2F4C:1000`, usage page `0xFF01`. Ten 65-byte packets (report
ID, `01 00 88`, packet index, 20 × RGB) then a flush packet with index `0xFF`.
Some drivers reject the 65-byte form with `EINVAL`; the first write probes, the
rest follow whichever worked.

## Packaging

* `nsh package nsh --target windows` writes the app directory, a zip, an update
  archive and an Inno Setup installer. The installer is per-user and needs no
  administrator.
* `nsh package nsh --target linux` writes the app directory, a tarball and a
  `.desktop` entry.
* `release.yml` builds both, generates `latest.json` and publishes them with the
  CLI's `.deb` and tarballs.
* `scripts/set-version.mjs` writes the tag into the workspace, `nsh/nsh.json` and
  the NSH crate.

## Testing strategy

* `cargo test` — 102 core tests plus 2 updater tests: state round-trip against a
  byte-exact fixture, curve interpolation and validation, pump floors, LED range
  coverage, settings clamping, sim persistence, version comparison.
* `nsh check` — Cargo check plus TSX type-check against `.nsh/types`.
* `nsh test` — 42 TSX tests: 18 domain, 14 use-case (fake ports), 10 headless
  screen tests. They run with `OPENCOLORHUB_SIM=1`; a scratch
  `OPENCOLORHUB_DATA_DIR`; and a driver fixture, so no real settings or network
  are touched.
* `nsh run` — the desktop target with live simulated telemetry.
* Desktop verification on hardware uses the packaged app, the simulator as a
  fallback, and the files the Rust side writes (`settings.json`, `rgb.state`,
  `sim-fan.json`) as evidence.

## Empty and error states

A blank panel cannot distinguish "nothing to show" from a failed read. Panels
render both in one shape: a title, a sentence on what happened, optional mono
detail, and an action when retrying makes sense.

- empty is neutral: no tach, no lighting channels, no catalogue entries, an
  unidentifiable board.
- error carries the backend's own message (`snapshot.read_error`,
  `led_device.error`, the fetch error), so the detail is quotable in a bug report.
