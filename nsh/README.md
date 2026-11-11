# OpenColorHub — Native Shell app

The desktop app. It shares `../crates/colorctl-core` with the `colorctl` CLI;
there is no Tauri shell and no webview.

See [`../MIGRATION.md`](../MIGRATION.md) for the conversion record and the
remaining hardware verification. In short:

```powershell
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"   # cargo is installed but may be off PATH
nsh check nsh
nsh build nsh
$env:OPENCOLORHUB_SIM = "1"
$env:OPENCOLORHUB_DATA_DIR = Join-Path $env:TEMP "opencolorhub-nsh-tests"
$env:OPENCOLORHUB_DRIVERS_FIXTURE = Join-Path (Get-Location) "nsh/test-fixtures/drivers.json"
Remove-Item -Recurse -Force $env:OPENCOLORHUB_DATA_DIR -ErrorAction SilentlyContinue
nsh test nsh
nsh run nsh
```

`nsh` 0.5.0 is the toolchain this branch was built with. `OPENCOLORHUB_SIM=1`
forces the core's simulator so tests never touch a fan or an LED;
`OPENCOLORHUB_DATA_DIR` and `OPENCOLORHUB_DRIVERS_FIXTURE` keep a headless run
away from the user's settings and the vendor site.

Layout:

| Path | What it is |
| --- | --- |
| `src/lib.rs` | the only Rust that touches hardware; one `Controller` class |
| `app/domain/` | pure rules and the wire model |
| `app/application/` | ports and use cases |
| `app/infrastructure/` | the JSON ⇄ typed native adapter |
| `app/composition/` | the app-state facade and the composition root |
| `screens/`, `components/` | TSX |

All six screens are native: Dashboard (gauge and sparkline), Fan control with
the four-point curve editor, Lighting with a 200-LED preview and colour presets,
Drivers, Settings and About. The layout follows `app/presentation/layout.ts`
(wide, medium, narrow).
