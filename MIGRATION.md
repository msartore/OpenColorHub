# NSH migration

Status: **track B (stack port) in progress**, branch `nsh/port`. The Tauri build
remains the shipping app until the parity gate at the end of this file passes.
The port lives in `nsh/` and shares `crates/colorctl-core` with the old shell.

The earlier in-place layering attempt is preserved on `nsh/layers-wip`; this
branch supersedes it. Nothing in `src/`, `src-tauri/` or `crates/` changed.

## Decision

| Question | Answer |
| --- | --- |
| Target | Native Shell (NSH) 0.5.0, Windows + Linux desktops |
| Mode | **Rust core + TSX screens** (mixed) |
| Why not TSX only | Hardware, PawnIO, HID, persistence, elevation and the service façade are already Rust and must stay native. QuickJS should only describe screens. |
| Why not Rust only | The UI is a 50 KB stylesheet and six imperative DOM screens; rewriting it in Rust builders is a second rewrite on top of the stack port. TSX keeps the existing screen logic in its own language and gives hot reload. |
| Why not keep Tauri | That was the other track. It would remove the architecture debt but leave the webview shell. The chosen outcome is a native window per platform. |
| Order | Port until the app runs and matches, ship it, then finish the Rust layer split. The migration rules forbid doing the framework port and the layer move at once. |

The mixed template is the shape NSH itself documents: a Cargo crate with a
`#[native_module]` seam, a TSX entry that imports it, and one bundle that runs
on every target.

## What exists today

| Piece | Location | Role |
| --- | --- | --- |
| Rust core | `crates/colorctl-core` | model, curves, settings, CCTL state, service façade, board guard, backends (Linux port I/O, Windows PawnIO/HID/HWiNFO, simulator), drivers, PawnIO install, autostart, elevation |
| CLI | `crates/colorctl-cli` | `colorctl`, upstream-compatible |
| Tauri shell | `src-tauri/` | 31 IPC commands, hardware probe thread, telemetry ticker, tray, single instance, manifest-required elevation, custom updater |
| Web UI | `src/` | Vite + DOM, six screens, SVG charts, demo backend |
| Tests | `cargo test` (102 core + 2 updater), headless browser demo over CDP | no TypeScript unit tests |

Known debt in the old UI, which the port removes rather than copies: screens
call the IPC singleton directly, app state is a mutable module object, and
`curves.ts` mirrors Rust rules.

## Target layout

```text
nsh/
├── nsh.json                 name, id, version, window, singleInstance, capabilities, updater
├── Cargo.toml               app crate (own workspace), depends on ../crates/colorctl-core
├── src/lib.rs               #[native_module("opencolorhub/native")] → Controller
├── App.tsx                  composition root: build deps, mount Shell
├── app/
│   ├── domain/              wire model + pure rules (fans, settings, telemetry)
│   ├── application/         ports, errors, use cases (fans, …)
│   ├── infrastructure/      native.ts: JSON → typed, the only file naming Controller
│   ├── composition/         store.ts (app-state facade), deps.ts (the root)
│   └── presentation/        format.ts (display only)
├── components/              Shell, Panel, Stat, Bar, Chip, notices
├── screens/                 Dashboard, Fans, Lighting, Settings, About; Drivers pending
├── theme.json               the old palette as shadcn/NSH tokens
└── assets/                  icon, later tray/fonts
```

Dependency rule, enforced by review until a check is added:

- `domain/` imports only `domain/`. No signals, no `nsh`, no `fetch`.
- `application/` imports `domain/`; ports speak domain types. No `nsh`, no DOM.
- `infrastructure/` imports `domain/` + `application/ports` + the generated
  `opencolorhub/native`; it parses JSON and translates errors.
- `composition/` is the only place that constructs `Controller` and the store.
- `screens/` and `components/` import `domain/`, `nsh` and the store type; they
  never import `infrastructure/` or `@tauri-apps/*`.
- `src/lib.rs` is the only file that calls hardware; TSX passes ids and numbers.

The wire model is duplicated in `nsh/app/domain/model.ts` on purpose: the Rust
`serde` types are the authority, and `native.ts` is the one seam where JSON
becomes typed values. Replacing the JSON strings with `#[derive(JsObject)]`
DTOs is a later refinement, not a parity requirement.

## The native seam

One class, `Controller`, exported through `opencolorhub/native`:

| Method | State |
| --- | --- |
| `snapshotJson()` | done |
| `settingsJson()`, `saveSettingsJson(patch)` | done |
| `aboutJson()`, `activityJson()`, `reloadStateJson()` | done |
| `setFanPercent(header, percent)` | done |
| `setFanCurve(header, pointsJson)` | done (Rust side; editor pending) |
| `applyProfile(headersJson, profile)` | done |
| `startTelemetry(callback)`, `stopTelemetry()` | done |
| `shutdown()` | done |
| `minimized` getter | done (`--minimized` autostart path) |
| channel writes, setups, drivers, PawnIO, autostart, elevation | pending |

`startTelemetry` is an **async member** so `timer` and `blocking` work: the tick
loop waits on the engine task runtime, reads the service on a blocking thread
(port I/O must not stall the UI), and posts JSON to JS. `OPENCOLORHUB_SIM=1`
forces the simulator, which is how tests and CI run without hardware.

## Tauri → NSH mapping

| Tauri | NSH |
| --- | --- |
| `#[tauri::command]` + `invoke` | `#[class] Controller` methods + generated `.nsh/types` |
| `telemetry` event + OS ticker thread | `startTelemetry` async member + `Callback<String>` |
| `backend://ready` | not needed while the probe is synchronous; if it moves to a task, a signal in the store |
| `tauri-plugin-single-instance` | `"singleInstance": true` + `Application.on("secondInstance")` |
| tray + close-to-tray | `Tray.set`; `"closeOnPress": false` + `win.on("closePress", () => win.hide())` |
| hidden window, `--minimized` | `window.defaultVisible: false` + `win.show()` unless `controller.minimized` |
| `capabilities/default.json` (`core:default` only) | enumerated `capabilities` in `nsh.json`: clipboard, `openUrl` (https/mailto), status bar. No network: the TSX makes no requests |
| CSP, no remote content | no webview at all |
| updater: bare `.exe` + `SHA256SUMS.txt` + rename/swap | `nsh/updater` + `nsh update-manifest`, signed manifest, archive replacement |
| `requireAdministrator` manifest | **no NSH equivalent** — see risks; core's `platform::request()` can relaunch with `runas` |
| NSIS/MSI/deb/AppImage | `nsh package`: Windows zip + per-user Inno installer, Linux directory/tar.gz + `.desktop` |
| paths from Tauri's `app_config_dir`/`app_data_dir` | same identifier directories, read from the environment so the constructor works before the engine runtime: `%APPDATA%\dev.msartore.opencolorhub`, XDG dirs on Linux |
| portable marker | unchanged: `opencolorhub.portable` beside the executable |
| browser demo backend (`src/mock.ts`) | headless NSH tests; a web target is unproven because the core is not built for wasm |

## Capabilities

`nsh.json` declares:

```json
"capabilities": {
  "clipboard": true,
  "openUrl": { "schemes": ["https", "mailto"] },
  "statusBar": true
}
```

- `clipboard`: the “copy” buttons on Settings/About.
- `openUrl`: the PawnIO site and release links.
- `statusBar`: the native confirm dialog sets it on mobile; desktop is a no-op.
- No `network`, no `fs`, no `process`, no `windows`, no `host`. The Rust crate's
  `ureq` traffic (drivers, updater) is native code and is not a TSX capability.

Every future grant must name the adapter that needs it in this file.

## Parity checklist

| Feature | Tauri command / behavior | NSH state |
| --- | --- | --- |
| Telemetry snapshot | `get_snapshot`, `telemetry` | done |
| Settings read/write | `get_settings`, `save_settings` | done |
| About | `get_about` | done |
| Activity | `get_activity` | done |
| Reload lighting state | `reload_state` | done |
| Fan duty | `set_fan_percent` | done |
| Fan curve | `set_fan_curve` | done |
| Fan profiles | `apply_profile` | done |
| Fan PWM | `set_fan_pwm` | pending (no UI uses it) |
| Lighting | `set_channel`, `set_all_channels`, `set_brightness`, `turn_off_channel`, `apply_led` | done |
| Setups | `list/save/apply/delete_setup` | pending |
| Drivers | `get_drivers`, `download_driver`, `downloaded_drivers`, `reveal_driver` + progress | pending |
| PawnIO | `pawnio_status`, `install_pawnio` + progress | pending |
| Elevation | `get_elevation`, `request_elevation` | done (shows state, relaunches, quits on success) |
| Autostart | `get_autostart`, `set_autostart` | done (enabling also turns on re-apply) |
| External links | `open_external` | done (`openUrl`, https grant) |
| Self-update | `update_check`, `update_install` | pending; replaced by `nsh/updater` |
| Tray, close-to-tray, single instance, `--minimized` | `tray.rs`, plugin, `lib.rs` | scaffolded |
| Keyboard shortcuts 1–6, last screen | `main.ts` | last screen persists; shortcuts pending |
| Dashboard, Fans, Lighting, Settings, About | screens | done |
| Drivers screen | screen | pending |
| Wake re-assert, suspend handling | `ResumeWatch` + ticker | pending: move the watch into the NSH tick loop |
| Windows fan control | PawnIO | **hardware gate** (elevation) |
| Packaging + updater feed | `tauri.conf.json`, release workflow | pending |

## Slice order

Each slice is one user flow through native seam → application → screen, and the
app runs after it.

- [x] **0. Scaffold** — `nsh/` app, theme, controller, generated types, build.
- [x] **1. Shell + telemetry + Dashboard** — tray/window lifecycle, close-to-tray,
      first snapshot, tick loop, live numbers, activity.
- [x] **2. Fans: duty, pump floor, profiles** — use-case guard, confirm dialog,
      toast, pump exemption, warnings.
- [x] **2b. Curve editor** — four points, drag or numeric fields, apply/revert,
      dirty flag, pump-curve refusal.
- [x] **3. Lighting** — six channel cards, global colour + brightness, 200-LED
      frame preview, per-channel apply/off.
- [x] **4. Settings + About** — backend mode, poll interval, units, accent,
      smoothing, pump binding, low-duty threshold, paths, elevation, autostart.
- [ ] **5. Setups** — fans/lighting presets, save/update/apply/delete.
- [ ] **6. Drivers** — cached-then-refreshed catalogue, grouping, firmware
      collapse, download progress, reveal in folder.
- [ ] **7. PawnIO** — status, install, installer progress, restart-as-admin path.
- [ ] **8. Updates** — `nsh/updater`, manifest publishing, progress UI, cutover.
- [ ] **9. Desktop polish** — keyboard shortcuts, last screen, window placement,
      reduced motion, high-DPI checks.
- [ ] **10. Packaging + CI** — Windows Inno/zip, Linux tarball + `.desktop`,
      Windows CI job, release assets, update feed.
- [ ] **11. Hardware verification + cutover** — real board matrix, old shell
      removed from releases, one-time installer for existing users.
- [ ] **12. Post-parity Rust layers** — extract `crates/colorctl-domain` (zero
      dependencies), `colorctl-application` (service + ports), keep
      `colorctl-core` as infrastructure. This is track A on the core, after the
      port has a stable behaviour to compare against.

## Risks and gates

1. **Elevation — release blocker on Windows.** The shipped app's manifest
   requires administrator; NSH's installer is per-user and not elevated. PawnIO
   and some HID paths may refuse a standard-integrity process. Plan: start
   unprivileged, detect with `platform::state()`, offer “Restart as
   administrator” via `platform::request()` (`ShellExecuteExW runas`), and verify
   fan control on a real board. If PawnIO needs elevation at open time, the port
   needs a privileged helper or an installer NSH does not currently produce.
2. **Single instance across integrity levels.** NSH's mutex/`WM_COPYDATA`
   hand-over has the same Windows limitation as the Tauri plugin. Every launch
   path (shortcut, autostart, elevated relaunch) must start at the same
   elevation, or the second launch exits without showing the running window.
3. **Updater cutover.** The old updater downloads a bare `OpenColorHub.exe` and
   renames it. An NSH app is a directory with its runtime, so the old updater
   cannot move users onto it. Ship a one-time installer, keep the last Tauri
   release as the fallback, and publish the NSH update feed separately.
4. **Packaging regression.** NSH produces a per-user Inno installer/zip on
   Windows and a tarball + `.desktop` on Linux, not MSI/deb/AppImage. The release
   workflow must either wrap those or the project accepts the smaller matrix.
5. **Curve editor + charts.** The DOM SVG editor is custom drag code. Rebuild it
   on `Canvas`/`PanResponder` (or four sliders + number inputs first) and keep
   the same validation and dirty-state behaviour.
6. **Browser demo.** `src/mock.ts` is a second backend with its own policy. The
   port replaces it with headless tests; a web target is only possible if
   `colorctl-core` compiles for wasm, which is untested.
7. **Data compatibility.** Settings and CCTL files must stay byte-compatible and
   in the same directories. Windows uses `%APPDATA%\dev.msartore.opencolorhub`
   for both, matching Tauri; Linux uses XDG config/data. The system-wide
   `/var/lib/colorctl/rgb.state` adoption stays in the core.
8. **Probe blocks first frame.** `Controller::new()` probes synchronously, as the
   Tauri app did behind its boot overlay. If it is noticeable, move the probe to
   a task and expose a `ready` signal; the old `backend://ready` event maps to it.

## Verification

```powershell
# cargo is installed but not on PATH in this shell
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"

nsh check nsh        # cargo check + tsc against .nsh/types
nsh build nsh        # bundle, asset and class checks
$env:OPENCOLORHUB_SIM = "1"
nsh test nsh         # Rust crate tests + TSX domain/use-case/headless screen tests
nsh run nsh          # the desktop target
nsh package nsh --target windows
```

Last run on this branch (Windows, NSH 0.5.0): `check` clean, `build` clean,
`test` 28/28 (7 headless screen tests, 11 use-case tests, 10 domain tests), and
`nsh package --target windows` produced `.nsh/out/OpenColorHub.zip` with the
release runner and the Inno script. The `-setup.exe` needs Inno Setup 6, which
was not installed in this environment; CI installs it before packaging.
`cargo test --workspace` for the old app is untouched.

The headless tests are the characterization tests for these slices: profile
tables and pump floor against the core's values, curve clamping, colour/byte
conversions, frame expansion, validation refusals, fake ports proving the use
cases clamp before the transport, and a real headless app proving fan, curve
and lighting writes reach `Service` through the controller.

## Known deviations to settle before parity

- **Accent colour** is stored but not applied at run time: NSH's theme tokens
  are compile time. Either generate the theme from `settings.accent` at startup
  or drop the preference; the Settings copy says so today.
- **The update panel** is not in Settings yet; it lands with the `nsh/updater`
  slice, together with the feed cutover.
- **Curve editor** uses a native Canvas with press/drag and numeric fields; it
  has no keyboard point selection yet (Tab reaches the fields).
- **Browser demo** is gone; the simulator (`OPENCOLORHUB_SIM=1`) and headless
  tests replace it. A web target still needs a wasm build of the core.

## Definition of done

- Every row in the parity checklist is done or explicitly dropped.
- `nsh check`, `nsh build` and `nsh test` pass on Windows and Linux; the app ran
  on a real board with PawnIO and on the simulator.
- Fan duty, curves, pump floor, lighting, wake re-assert, tray, autostart and
  update have been exercised on hardware, not only headless.
- `nsh.json` capabilities match the adapters that use them; nothing is wide open.
- The Tauri shell, its IPC, `src/`, and the old updater are removed in the same
  release that ships the NSH app; there is one source of truth.
- `MIGRATION.md` becomes the steady-state guide: how to add a screen, where a
  rule goes, how a new capability is reviewed.
