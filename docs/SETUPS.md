# Setups

A **setup** is a saved combination of fan and lighting settings you can restore
in one click. The screen lives under **Setups** in the rail.

---

## What a setup stores

Outcomes, not commands:

| Part | Captured |
|---|---|
| Fans | For each header, either the manual duty (`percent`) or the four-point SmartFan curve it was driven by |
| Lighting | For each channel, colour, brightness and on/off |

Applying one writes the fans first, then all lighting channels, and pushes a
single 200-LED frame — so a setup costs one HID write, not six.

It does **not** store: the active backend, the poll interval, temperature
units, tray/autostart preferences, or anything about the board. Those belong to
Settings and stay put when a setup is applied.

## Where it lives

```
%APPDATA%\dev.msartore.opencolorhub\setups.json     (installed)
data\setups.json                                    (portable)
```

```json
{
  "version": 1,
  "setups": [
    {
      "id": "s1a191fb2170",
      "name": "Amber 80%",
      "saved_at_ms": 1793450516848,
      "fans": [{ "header": "cpu", "percent": 55, "curve": null }],
      "channels": [{ "channel": "led", "color": [251, 191, 36], "brightness": 80, "enabled": true }]
    }
  ]
}
```

- Writes go to `setups.json.tmp` and are renamed into place, so an interrupted
  save cannot leave a half-parsed file.
- A file that fails to parse is **kept**, not overwritten: the store starts empty
  and the screen shows the parse error instead of silently discarding your
  setups.
- `version` is the schema version. It only changes if a field changes meaning.

## Actions

| Action | What it does |
|---|---|
| **Save current state** | Captures what is running now under the name you type |
| **Apply** | Replays that state onto the hardware |
| **Update** | Overwrites that setup with what is running now |
| **Delete** | Two clicks: the first arms it, the second removes it (disarms itself after 4 s) |

Names are free text, up to 48 characters. Two setups may share a name; ids are
what identify them, so renaming is "save under a new name, delete the old one".

## Limits worth knowing

- A setup is applied **as a whole** — there is no "apply only the fans".
- Applying a curve to a board whose SmartFan registers are unmapped fails that
  header with an explanation rather than silently doing nothing (the same guard
  as the Fan control screen).
- Setups are per machine. There is no export/import yet.
