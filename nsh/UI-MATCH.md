# NSH UI → Tauri UI match record

Baseline for the port: the Tauri app on `main` at v0.5.1 and its
`src/styles/app.css`. Target: the Native Shell port in this directory.

The current screenshots in `docs/screenshots/*.png` are captured from the
**Native Shell app itself** (simulated mode, synthetic board data) — they show
the result of this port, not the old baseline. The files that described the old
UI (`src/styles/app.css`, `src/views/settings.ts`, `src/components/icons.ts`)
live on `main` and are not present on this branch; the Tabler paths were carried
over into `components/icons.tsx`.

## Fidelity contract

| Element | Level | Decision |
| --- | --- | --- |
| Colour roles, type scale, radius, 228 px sidebar, card/chip/pill styling | Match | Tokens copied from `src/styles/app.css` into `theme.json`; no raw colours in screens except status accents. |
| Dashboard layout: notes banner, gauge + six stat tiles, fan headers + lighting cards | Match | Rebuilt in `screens/Dashboard.tsx` against the baseline screenshot. |
| Lighting: collapsible Motherboard / GPU device groups, Sync + Apply + colour chip headers | Match | `Collapsible` groups with device dots and per-zone cards; sync is a modal with colour sources. |
| GPU LED panel: title, subtitle, controller count, ARGB header / Top–side / Fan zones, effect, brightness | Match | `screens/Lighting.tsx` + `app/domain/gpu.ts` mirror the core's `GPU_MODES` and wire format. |
| Sidebar brand, nav labels/hints, bottom FANS/LIGHTING status pills | Match | `components/Shell.tsx`. |
| Settings Safety toggles (confirm, GPU LED, restore, release, reapply fallback) | Match | Exact labels/descriptions from `src/views/settings.ts`. |
| Release-note Markdown in the update card | Match | `TextView markdown=…` renders the same CommonMark subset, capped at 8000 chars. |
| Page-header action buttons: bookmark / presets / refresh | Adapt | NSH has no bookmark icon; presets and refresh use text buttons with the same placement. |
| `<details>` disclosure chevron | Adapt | Native `Collapsible` with a text chevron; same collapsed/expanded meaning. |
| Desktop `<select>` for GPU effects | Adapt | NSH `Select` with the same items and labels, native popup. |
| Hover-only affordances from the web app | Deviate | Not ported; every action has a visible press target on touch. |

## Token map

| Role (old CSS) | NSH token | Value |
| --- | --- | --- |
| `--bg` | `background` | `#07090b` |
| `--bg-panel` | `card` | `#0c1013` |
| `--bg-elev` | `popover`, `secondary`, `accent` | `#11161b` |
| `--bg-sunken` | `muted`, `sidebar` | `#090c0f` |
| `--line` | `border` | `#1a2128` |
| `--line-strong` | `input` | `#2a343d` |
| `--text` | `foreground` | `#e7eef4` |
| `--text-dim` | `muted-foreground` | `#9fb0bd` |
| `--accent` | `primary`, `ring`, `chart-1` | `#4ade80` |
| `--warn` | `chart-3`, inline accent | `#fbbf24` |
| `--danger` | `destructive`, `chart-4` | `#f87171` |
| `--info` | `chart-2`, inline accent | `#7dd3fc` |
| `--radius` | `radius` | `10px` |

Type: old 13.5 px body → NSH 13 px; old 25 px screen title → `text-[25px]`;
old 9.5 px uppercase labels → `text-[11px] uppercase` (NSH has no 9.5 px step).

## Gaps and deviations

- **Activity and backend key/value rows** remain on the Dashboard below the
  baseline content. They are additive diagnostics, not a replacement for any
  baseline element.
- **Colour control**: the Lighting screen has a single colour swatch per group
  and channel that opens a centred picker dialog. The old quick-pick preset
  swatch rows were removed — colours now come only from the picker.
- **Icon rendering**: the Tabler outline paths from `main`'s `src/components/icons.ts`
  are inlined in `components/icons.tsx` and drawn natively with `Canvas` and
  `Path2D`. The six nav glyphs, bookmark and chevron are the same paths as the
  old app; only the renderer differs (native canvas rather than DOM SVG).
- **Appearance**: `theme.json` declares the same colours for light and dark —
  the app forces the dark appearance in `nsh.json`, matching the old app's
  `color-scheme: dark`. The Settings accent is applied at runtime with
  `setTheme`, repainting every accent token (primary, ring, chart tokens, the
  status colours and sidebar-primary); the built theme is the default green.
  The only other colours on screen are in the LED preview and the picker dialog.
- **Dialogs and switches**: all custom dialogs are centred in the window
  with absolute positioning. Settings and channel toggles share one switch
  look (visible track, translated thumb) so the off state is not a bare dot.
- **Targets verified**: Windows development host (`nsh test --target windows`)
  and headless (`nsh test`). macOS, Linux, iOS and Android were not run.
