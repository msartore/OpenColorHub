# OpenColorHub

![OpenColorHub](docs/assets/banner.png)

Fan and ARGB control for Colorful AMD motherboards — a Tauri 2 desktop app and a
`colorctl` CLI sharing one hardware core.

It drives the two blocks those boards ship with: the **Nuvoton Super I/O** (fan
headers, 4-point curves, tachometers, board temperature) and the **Colorful RGB
controller** (200 LEDs across six channels: onboard shroud, two 12V RGB headers,
three 5V ARGB headers).

The board is identified from SMBIOS before anything is written, and matched
against a verified catalogue. A machine that is not a Colorful board never has
its Super I/O touched. → [Board detection](docs/BOARD-DETECTION.md)

It is a desktop rewrite of [`colorctl`](https://github.com/Curstantine/colorctl):
the register programming, curve tables and `CCTL` state format stay
byte-compatible, so a machine can alternate between the CLI and this app without
losing its lighting state.

## Platforms

| Platform | Fans | Lighting |
| --- | --- | --- |
| **Linux** | ✅ `/dev/port` — root or `CAP_SYS_RAWIO` | ✅ `/dev/hidraw` |
| **Windows** | ✅ PawnIO sandboxed driver · ⚪ read-only via HWiNFO without it | ✅ SetupAPI + `hid.dll`, no driver |

Windows needs a signed driver to broker `IN`/`OUT`; PawnIO is **not bundled**,
but the app offers to install it: on launch (and from the Fan control screen) it
fetches PawnIO's own installer from the project's releases, checks it against a
pinned SHA-256 and runs it so its UI and UAC prompt appear. Nothing is installed
silently. Without the driver the app falls back to HWiNFO telemetry and then the
simulator, both labelled. → [Platform notes](docs/PLATFORM.md)

## Screenshots

| | |
| --- | --- |
| ![Dashboard](docs/screenshots/dashboard.png) | ![Fan control](docs/screenshots/fans.png) |
| **Dashboard** — gauge with session history, six live metrics, fan and channel detail | **Fan control** — per-header curves, presets, pump floor |
| ![Lighting](docs/screenshots/leds.png) | ![Drivers](docs/screenshots/drivers.png) |
| **Lighting** — six channels, 200-LED frame | **Drivers** — vendor downloads, nothing executed |
| ![Setups](docs/screenshots/setups.png) | ![About](docs/screenshots/about.png) |
| **Setups** — saved fan + lighting combinations | **About** — the board this build detected |

Settings covers backend selection, elevation, units, safety confirmations, poll
interval, startup with the OS and updates. The app lives in the tray: closing the
window hides it, **Quit** is the only real exit.

## Install

Windows installers, a portable zip and Linux packages are on the
[releases page](https://github.com/msartore/OpenColorHub/releases). The portable
zip keeps settings and state in `config/` and `data/` beside the executable.

From source:

```bash
npm install
npm run tauri dev      # hot reload
npm run tauri build    # bundles in target/release/bundle
npm run dev            # browser demo, no Rust toolchain needed
cargo test             # core + CLI unit tests
```

On Windows use `pwsh scripts/build-release.ps1 -Portable` for a shippable build:
it strips absolute source paths from the binary and refuses to package a stale
executable. Real hardware on Linux needs privileges:

```bash
sudo ./target/release/opencolorhub        # or set CAP_SYS_RAWIO on the binary
```

## Documentation

| | |
| --- | --- |
| [Architecture](docs/ARCHITECTURE.md) | Crates, backend boundary, protocols, verification |
| [Platform notes](docs/PLATFORM.md) | Driver rationale, fallbacks, elevation, startup, tray |
| [Board detection](docs/BOARD-DETECTION.md) | Confidence levels, sources, the hardware guard |
| [Updates](docs/UPDATES.md) | Self-updater, release process, integrity caveats |
| [Drivers](docs/DRIVERS.md) | Board download catalogue, URL rules, why nothing is executed |
| [Setups](docs/SETUPS.md) | Saved fan + lighting combinations, file format, limits |
| [NOTICE.md](NOTICE.md) | Licensing of this project and everything it embeds |

## Safety

* **The pump is protected** — curves below 60% are rejected, manual duty clamped,
  and the built-in profiles use Colorful's pump table.
* **Curves are validated before they reach the chip** — four points, strictly
  ascending temperatures, 0–100% duty, enforced in the UI, the CLI and the service.
* **The chip is identified first** — an unrecognised Super I/O aborts before any
  register write (`0xD42A` via `/dev/port`, the `0xD1xx` family via PawnIO).

Writing fan curves on a board you have not verified is at your own risk.

## Licence

GNU General Public License v3.0 — SPDX `GPL-3.0-only`. Full text in
[`LICENSE.md`](LICENSE.md); what it means in practice, plus the MIT attribution
for the upstream `colorctl` portions and the LGPL notice for the embedded PawnIO
module, is in [`NOTICE.md`](NOTICE.md).

No warranty. This software writes to Super I/O and USB HID hardware; if that
damages anything, the licence says the authors are not liable.
