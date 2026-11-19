# GPU LED control

Optional control of GPU RGB zones alongside the motherboard's onboard
controller. Enable it with **Settings → Safety → GPU LED control**.

## What it drives

The supported hardware is the Nuvoton NUC121-style RGB controller that ASRock
fits to many Radeon and Intel Arc boards. It lives on the GPU's **own** I²C bus
at address **`0x36`** — not on the motherboard chipset SMBus, which is why
PawnIO's SMBus modules do not reach it.

## Card detection and per-card profiles

The app reads every display adapter's exact PCI identity (vendor, device,
subsystem vendor and subsystem device) through SetupAPI on Windows and sysfs on
Linux. The subsystem ID is looked up in a catalogue of 49 ASRock GPU models
recovered from PolychromeSYNC's `VGA_Support` table. The matched profile decides:

- the model name shown in the UI (e.g. `RX 9070 XT SL`);
- how many zones the card exposes — most cards have three, Intel Arc cards two;
- the controller I²C address (default `0x36`);
- the maximum effect ID and a capability note.

Unknown cards fall back to a vendor-generic profile, so NVIDIA and Intel cards
still get their native NVAPI / IGCL transport even when no exact model matches.
Detection is also what fixes duplicate entries: ADL reports one adapter per
display, so transports are paired with physical PCI cards instead of creating
one UI entry per monitor.

Three zones are exposed:

| Zone | Channel ID | Notes |
| --- | --- | --- |
| ARGB header | `0x03` | 5V header for **external** ARGB devices. It has no onboard LEDs, so it shows no visible change unless a strip or fans are plugged into the GPU header. |
| Top / side | `0x06` | The card's built-in top/edge lighting. |
| Fan | `0x07` | LEDs around the fans. |

It is normal for the ARGB-header zone to appear inert while Top/side and Fan respond — the header is just an output for attached hardware.

Each zone is one 12-byte block write:

```text
10 00 <channel> <mode> <R> <G> <B> <speed> <brightness> <direction> 1A 00
```

Effects: static, breathing, strobe, RGB cycle, random, color shift, visor,
stacking, two wave modes, two marquee modes, color wave and rainbow.

## Transports

No kernel driver is shipped — the app calls each GPU vendor's own user-mode
API:

| Vendor | Windows | Linux |
| --- | --- | --- |
| AMD | `atiadlxx.dll` → `ADL2_Display_WriteAndReadI2C`, I²C line 1 | `/dev/i2c-*` on an AMDGPU OEM adapter |
| NVIDIA | `nvapi.dll` → `NvAPI_I2CWriteEx`, port 1, non-DDC | `/dev/i2c-*` on an NVIDIA adapter where exposed |
| Intel | `ControlLib.dll` / `igcl.dll` → `ctlI2CAccess` | `Synopsys DesignWare I2C adapter` |

If no controller is detected, the Lighting screen simply omits the GPU section;
fan and onboard ARGB control are unaffected. Windows I²C writes need
administrator rights, which the app already requires.

## State

GPU zone colours and effects are stored in `gpu-led.state` beside `rgb.state`,
so the CLI and the desktop app share it the same way. "Sync from board" copies
the first enabled onboard channel's colour into every GPU zone.

## Limitations

- The `0x36` protocol is ASRock/Nuvoton-specific. A card from another board
  partner with a different controller is detected as a GPU but will not answer
  at that address.
- Speed and direction are written with fixed defaults (0x80 / 0) until the UI
  exposes them.
- IGCL's write path requires elevation and an active display output.
