# OpenColorHub — NSH port

The Native Shell port lives here, beside the shipping Tauri app. It shares
`../crates/colorctl-core`; nothing in `../src/` or `../src-tauri/` is used.

See [`../MIGRATION.md`](../MIGRATION.md) for the plan, the parity checklist and
the remaining slices. In short:

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

Dashboard and Fan control are ported. The other four screens show a truthful
placeholder until their slice lands.
