# Tauri → Native Shell migration

Status: **complete in code** on branch `nsh/port`. The Tauri shell, its web
frontend and the npm/Vite build are gone; the desktop app is the Native Shell
project in [`nsh/`](nsh/README.md), sharing `crates/colorctl-core` with the
`colorctl` CLI. Hardware verification on a Colorful board is the only remaining
external check.

## Parity with `main` (v0.5.1)

The port now carries the features that landed on `main` after the fork:

- **GPU LED control** — `crates/colorctl-core/src/gpu.rs` and `gpu_catalogue.rs`,
  exposed through three `Controller` methods (`setGpuZone`, `applyGpu`,
  `syncGpuFromBoard`). The Lighting screen has collapsible Motherboard and GPU
  device groups; each GPU zone has a colour swatch, hardware-effect picker,
  brightness and Apply/Off. See [`docs/GPU-LED.md`](docs/GPU-LED.md).
- **Restore saved setup at launch / on resume** — the live setup is persisted
  after every successful write and replayed at startup (six retries while a
  controller settles). OS power notifications and the clock-gap fallback
  re-assert fans, lighting and GPU zones after a wake. Settings exposes
  *Restore saved setup at startup* and *Release fans on exit*.
- **Markdown release notes** — the update card renders the release body with
  the native `TextView`, capped at 8000 characters.

[`nsh/UI-MATCH.md`](nsh/UI-MATCH.md) records the UI fidelity contract, the
token map and the remaining deviations against the v0.5.1 screenshots.

## What changed

| Before | After |
| --- | --- |
| `src-tauri/` — 31 IPC commands, telemetry thread, tray, single instance, updater, elevation manifest | `nsh/src/lib.rs` — one `Controller` class over the same `Service` |
| `src/` — Vite + DOM screens with a browser demo backend | `nsh/screens/*.tsx` — native views, `nsh test` for headless screens |
| WebView2 runtime | no webview; native windows on Windows and Linux |
| npm, Vite, TypeScript build | `nsh check`, `nsh build`, `nsh package` |
| manifest `requireAdministrator` | in-app permission prompt, then `ShellExecuteExW runas` |
| custom updater: bare `.exe` + rename/swap | `nsh/updater` with `latest.json` |
| NSIS/MSI/deb/AppImage | Inno per-user installer + zip, Linux tarball, CLI `.deb` |

Removed: `src/`, `src-tauri/`, `index.html`, `package.json`, `package-lock.json`,
`vite.config.ts`, `tsconfig.json`, the Tauri icon/build scripts, the Tauri
release jobs and the Tauri docs/screenshots.

## Final architecture

- `crates/colorctl-core` — model, curves, CCTL state, settings, backends, board
  guard, service façade. Unchanged and shared.
- `crates/colorctl-cli` — the `colorctl` binary. Unchanged.
- `nsh/src/lib.rs` — the native seam: `snapshotJson`, `settingsJson`,
  `startTelemetry`, fan/lighting/preset/driver/PawnIO/elevation methods.
- `nsh/app/domain` — pure rules and the wire model.
- `nsh/app/application` — ports and use cases.
- `nsh/app/infrastructure/native.ts` — the one JSON → typed adapter.
- `nsh/app/composition` — app-state facade and composition root.
- `nsh/screens`, `nsh/components` — native UI.

See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for the layers, the backend
boundary and the hardware protocol notes.

## Elevation and PawnIO

The installer is per-user and intentionally does not request administrator.
Fan control on Windows needs PawnIO's driver, which a standard process may not
open. The app asks:

1. After the telemetry probe, it reads `platform::state()` and
   `platform::pawnio_status()`.
2. If PawnIO is `denied` and the app can ask, a native alert explains that
   lighting and telemetry work without elevation and offers **Restart as
   administrator** / **Continue without fan control**.
3. On consent, `platform::request()` relaunches through
   `ShellExecuteExW` + `lpVerb = "runas"`; Windows shows its own UAC prompt.
4. The old process quits, the elevated copy probes PawnIO and reads real fans.
5. The autostart entry is created with `schtasks /RL HIGHEST` on Windows and an
   XDG entry with `--minimized` on Linux, so a sign-in launch already has the
   rights fan control needs.

Settings → Privileges offers the same restart at any time. A simulated backend
never asks.

Single instance is `"singleInstance": true`; a second launch hands its arguments
over and exits. Windows blocks `WM_COPYDATA` between integrity levels, so every
launch path must use the same elevation.

## Updates

`nsh/updater` reads `"updater": { "github": "msartore/OpenColorHub" }` from
`nsh.json`. Settings → Updates checks, downloads with progress and restarts.
`release.yml` builds Windows and Linux archives, writes `latest.json` with
`nsh update-manifest`, and publishes it with the app and CLI artifacts.

Existing Tauri installs cannot update into the new shell through the old
updater; the release notes call for one manual install. After that, `latest.json`
drives updates.

## Verification

```powershell
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
nsh check nsh
nsh build nsh
$env:OPENCOLORHUB_SIM = "1"
$env:OPENCOLORHUB_DATA_DIR = Join-Path $env:TEMP "opencolorhub-nsh-tests"
$env:OPENCOLORHUB_DRIVERS_FIXTURE = Join-Path (Get-Location) "test-fixtures/drivers.json"
Remove-Item -Recurse -Force $env:OPENCOLORHUB_DATA_DIR -ErrorAction SilentlyContinue
nsh test nsh
nsh package nsh --target windows
```

- `cargo test --workspace` — core + CLI tests.
- `nsh check`, `nsh build` — clean.
- `nsh test` — **42/42**: 18 domain, 14 use-case, 10 headless screen tests. They
  cover the colour dialog writing the selected colour and the curve drag +
  **Apply curve** path.
- `nsh package --target windows` — zip, update archive and Inno installer
  (`OpenColorHub-setup.exe`); `nsh update-manifest` writes a valid `latest.json`.
- `nsh run` — checked by eye: Dashboard gauge, Fan control two-up, Lighting,
  Drivers, Settings, About, with live simulated telemetry.
- Linux: `nsh check` and `nsh build` passed in WSL Ubuntu 26.04; the WSL VM
  faulted during the headless-runner build, so the Linux `nsh test` still needs a
  healthy host or the release job.

## Remaining

- Hardware pass on a Colorful board: fan duty/curves/profiles, pump floor,
  lighting, wake re-assert, tray, autostart, PawnIO after the elevation prompt.
- First tagged NSH release (the workflow is wired but has not run).
- Linux `nsh test` on a healthy host.
- Post-parity Rust split (`crates/colorctl-domain`, `application`, adapters) as
  a separate track; the current core is one crate and already has the backend
  trait boundary.
