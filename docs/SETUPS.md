# Presets

A **preset** is a saved half of the machine you can restore in one click. Fan and
lighting presets live behind the **save icon** in the header of the **Fan
control** and **Lighting** screens; it opens a dialog with the list, the actions
and the save form.

A fan preset never writes lighting and a lighting preset never writes fans:
recalling a quiet fan curve cannot dim the case, and picking a colour cannot spin
the fans up.

---

## What a preset stores

Outcomes, not commands:

| Kind | Captured |
|---|---|
| Fan preset | For each header, either the manual duty (`percent`) or the four-point SmartFan curve it was driven by |
| Lighting preset | For each channel, colour, brightness and on/off |

Applying a lighting preset stages every channel and pushes a single 200-LED
frame: one HID write, not six.

Neither kind stores the active backend, poll interval, temperature units,
tray/autostart preferences or anything about the board. Those belong to Settings.

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
  ignored.
- Writes go to `setups.json.tmp` and are renamed into place, so an interrupted
  save cannot leave a half-parsed file.
- A file that fails to parse is **kept**: the store starts empty and the panel
  shows the parse error instead of discarding your presets.
- `version` is the schema version. A version 1 file — one entry holding both
  halves — is split on load into one fan preset and one lighting preset under the
  same name, and rewritten as version 2.

## Actions

The dialog opens from the header icon and closes on Escape, a click outside the
card, or the close `×`. Applying a preset also closes it.

| Action | What it does |
|---|---|
| **Save fan preset** / **Save lighting preset** | Captures that half as it is running now, under the name you type |
| **Apply** | Replays that half onto the hardware and closes the dialog |
| **Update** | Overwrites that preset with what is running now |
| **Delete** | Two clicks: the first arms it, the second removes it (disarms itself after 4 s) |

Names are free text, up to 48 characters. Two presets may share a name; ids
identify them, so renaming is "save under a new name, delete the old one". The
same name may appear in both lists — "Night" as a fan profile and as a lighting
profile.

## Limits worth knowing

- A preset covers one half only. To restore a whole machine state, apply one of
  each.
- Applying a curve to a board whose SmartFan registers are unmapped fails that
  header with an explanation rather than doing nothing (the same guard as the Fan
  control screen).
- Presets are per machine. There is no export/import yet.
