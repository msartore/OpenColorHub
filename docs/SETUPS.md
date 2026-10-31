# Presets

A **preset** is a saved half of the machine you can restore in one click. Fan
presets live on the **Fan control** screen and lighting presets on the
**Lighting** screen, each above that screen's controls.

The split is deliberate: a fan preset never writes lighting and a lighting
preset never writes fans, so recalling a quiet fan curve at night cannot dim the
case, and picking a colour cannot spin the fans up.

---

## What a preset stores

Outcomes, not commands:

| Kind | Captured |
|---|---|
| Fan preset | For each header, either the manual duty (`percent`) or the four-point SmartFan curve it was driven by |
| Lighting preset | For each channel, colour, brightness and on/off |

Applying a lighting preset stages every channel and pushes a single 200-LED
frame, so it costs one HID write rather than six.

Neither kind stores: the active backend, the poll interval, temperature units,
tray/autostart preferences, or anything about the board. Those belong to
Settings and stay put when a preset is applied.

## Where it lives

```
%APPDATA%\dev.msartore.opencolorhub\setups.json     (installed)
data\setups.json                                    (portable)
```

```json
{
  "version": 2,
  "setups": [
    {
      "id": "s1a191fb2170",
      "name": "Amber 80%",
      "kind": "lighting",
      "saved_at_ms": 1793450516848,
      "fans": [],
      "channels": [{ "channel": "led", "color": [251, 191, 36], "brightness": 80, "enabled": true }]
    }
  ]
}
```

- Only the half named by `kind` is populated; the other array is empty and
  ignored when applying.
- Writes go to `setups.json.tmp` and are renamed into place, so an interrupted
  save cannot leave a half-parsed file.
- A file that fails to parse is **kept**, not overwritten: the store starts empty
  and the panel shows the parse error instead of silently discarding your
  presets.
- `version` is the schema version. A version 1 file — one entry holding both
  halves — is split on load into one fan preset and one lighting preset under the
  same name, and rewritten as version 2.

## Actions

| Action | What it does |
|---|---|
| **Save fan preset** / **Save lighting preset** | Captures that half as it is running now, under the name you type |
| **Apply** | Replays that half onto the hardware |
| **Update** | Overwrites that preset with what is running now |
| **Delete** | Two clicks: the first arms it, the second removes it (disarms itself after 4 s) |

Names are free text, up to 48 characters. Two presets may share a name; ids are
what identify them, so renaming is "save under a new name, delete the old one".
The same name may appear in both lists — that is how you keep "Night" as a fan
profile and "Night" as a lighting profile.

## Limits worth knowing

- A preset covers one half only. To restore a whole machine state, apply one of
  each.
- Applying a curve to a board whose SmartFan registers are unmapped fails that
  header with an explanation rather than silently doing nothing (the same guard
  as the Fan control screen).
- Presets are per machine. There is no export/import yet.
