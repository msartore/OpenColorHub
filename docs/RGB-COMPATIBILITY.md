# RGB compatibility

The lighting controller is a **USB HID device, not a Super I/O block**: Colorful
boards expose vendor `0x2F4C`; the AMD boards this project targets use product
`0x1000` on usage page `0xFF01`.

SMBIOS identity ([board detection](BOARD-DETECTION.md)) gates *fan* control, not
RGB. Board model, chipset and Super I/O change neither detection nor frame
writing.

## How the controller is identified

| Field | Value |
| --- | --- |
| Vendor ID | `0x2F4C` (Colorful) |
| Product ID | `0x1000` on the AMD boards (`0x1003`–`0x2002` on Intel boards) |
| Usage page | `0xFF01`, usage `0x0001` |
| Report | 65-byte output reports, report ID `0x00` |

**No model whitelist** — matching Colorful's own driver: `iGameMBoard.dll`
dispatches on USB product ID alone, and OpenRGB registers the same detector
(`REGISTER_HID_DETECTOR_PU(…, 0x2F4C, 0x1000, 0xFF01, 0x0001)`). Libraries
examined: `iGameMBoard.dll`, `iGameCenter.Hardware` and `iGameAPI.dll` from the
iGame Center Lite installer; the analysis was published with upstream
[`colorctl`](https://github.com/Curstantine/colorctl) `COMPATIBILITY.md` and is
reproduced as it applies to this app.

## ODM families behind the model list

Colorful does not design every board it sells; `iGameMBoard.dll` classes group by
original design manufacturer:

| Prefix in the driver | ODM | Platforms |
| --- | --- | --- |
| `JW_*` | JieWei / JGINYUE | Colorful's AMD AM4/AM5 boards |
| `CQ_*` | ChangQing | Mainstream and budget Intel boards |
| `ECS_*` | Elitegroup (ECS) | Older Intel boards |
| `WB_*` | WanBao | Select Intel boards |

## Product ID catalogue

Seventeen product IDs have a dispatch routine in `iGameMBoard.dll`. Only
`0x1000` is on the AMD path this project drives; Intel entries are listed so a
controller on another board is identified, not mistaken for unsupported.

| PID | Driver class | Platform |
| --- | --- | --- |
| **`0x1000`** | **`iGameMBoard_JW_X570`** | **AMD AM5 and AM4 — the one this app drives** |
| `0x1003` | `iGameMBoard_Colorful_B460_A12` | Intel B460 |
| `0x1004` | `iGameMBoard_ECS_B560` | Intel B560 (ECS) |
| `0x1005` | `iGameMBoard_ECS_Z490` | Intel Z490 (ECS) |
| `0x1006` | `iGameMBoard_ECS_Z390` | Intel Z390 (ECS) |
| `0x1007` | `iGameMBoard_JW_B560I` | Intel B560 mini-ITX |
| `0x1013`, `0x1015` | `iGameMBoard_Colorful_Z590` | Intel Z590 |
| `0x1017` | `iGameMBoard_B760W4` | Intel B760 |
| `0x1018` | `iGameMBoard_WB_B660` | Intel B660 (WanBao) |
| `0x1019`–`0x1021` | `iGameMBoard_CQ_2025_*` | 2025/2026 Intel revisions (ChangQing) |
| `0x1022` | `iGameMBoard_Colorful_2025_1124_0x1022` | Late-2025 Intel refresh |
| `0x1023` | `iGameMBoard_CQ_Z990_0x1023` | Z890 / Z990 class |
| `0x2000` | `iGameMBoard_CQ_H610` | Intel H610 |
| `0x2002` | `iGameMBoard_CQ_HuoShen` | FireGod / special editions |

## Boards that share the `0x1000` endpoint

Identical AMD firmware and the same 200-LED protocol. The list is upstream's, not
a hardware test of every SKU; what is verified here is the endpoint and payload.

**AM5 (B650 / B840 / B850 / X870)**

- CVN: B650 GAMING FROZEN V14, B650M GAMING FROZEN V14, B850 GAMING PRO WIFI7 V14,
  B850M ARK FROZEN V14, B850M GAMING FROZEN V14, B850I FROZEN WIFI V14,
  X870 GAMING FROZEN V14
- iGame: B850I MINI OC V14
- COLORFIRE: B650M-MEOW WIFI ORANGE, B850M-A MEOW WIFI ORANGE,
  B850M-MEOW WIFI7 V14
- BATTLE-AX: B650A-GHA WIFI V14, B650M-PLUS V14/V15/WIFI V15,
  B650M-WHITE WIFI V14/V15, B650M-A PLUS V14, B650M-D PRO V14,
  B650M-E WIFI V14, B650M-E PRO V14, B650M-GHA WIFI V14, B650M-T V14,
  B850M-PLUS PRO WIFI V14, B850M-PLUS WIFI V14, B850M-PLUS S WIFI7 V14,
  B850M-GHA WIFI V14, B850M-T WIFI V14, B850M-E WIFI V14,
  B840M-D PRO V14, B840M-GHA WIFI V14, X870A-GHA WIFI V14
- iCafe: B650M-G DELUXE V14/V14A/V15, B650M-G PLUS DELUXE V14,
  B850M-G DELUXE V14A/V15

**AM4 (A620 / A520 / X570)**

- BATTLE-AX A620M-GHA WIFI V14, A620M-D PRO V14, A620AM-GHA, A620AM-D PRO,
  A520M-T WIFI V15
- CVN X570 GAMING PRO V14, X570 FROZEN V14

## The frame

`crates/colorctl-core/src/backend/frame.rs` is the implementation of record; both
backends build packets there.

```text
offset 0        report ID (0x00)
offset 1..=3    magic 0x01 0x00 0x88
offset 4        packet index 0..=9, or 0xFF for the commit
offset 5..     20 × (R, G, B)
```

A full update is **ten 65-byte packets of twenty LEDs** plus a commit packet that
latches the frame. Bytes past a channel's last LED stay zero, as the firmware
does for unpopulated headers.

## Channel and header map

The controller accepts all 200 LEDs; a board with fewer headers has no traces
from the unused channels. Setting colour on an empty channel is harmless, not an
error.

| Channel | Label | Official ID | LED range | LEDs |
| --- | --- | --- | --- | --- |
| `led` | Onboard shroud + I/O cover | `0` (`MBoard_LED`) | `0..18` | 18 |
| `rgb1` | `12V_1` — 12V RGB header 1 | `121` (`12V_1`) | `18..19` | 1 |
| `rgb2` | `12V_2` — 12V RGB header 2 | `122` (`12V_2`) | `19..20` | 1 |
| `argb1` | `5V_1` — 5V ARGB header 1 | `51` (`5V_1`) | `20..80` | 60 |
| `argb2` | `5V_2` — 5V ARGB header 2 | `52` (`5V_2`) | `80..140` | 60 |
| `argb3` | `5V_3` — 5V ARGB header 3 | `53` (`5V_3`) | `140..200` | 60 |

The driver provisions eight 5V headers (`51`–`58`) and two 12V; the AMD boards
this app supports expose the first three 5V and both 12V. A 12V header is a
single zone driven by one LED entry — G-R-B is not addressable.

## Comparison with OpenRGB

| | Colorful's driver | OpenRGB | This app |
| --- | --- | --- | --- |
| Detection | VID/PID + usage page | same | same (`2f4c:1000`, `0xFF01`) |
| Model whitelist | none | none | none |
| Device name | generic | DMI query | SMBIOS, from `board.rs` |
| Channel buffer | 200 LEDs fixed | 6 zones into the same frame | 200 LEDs fixed |
| ARGB sizing | 60 max per header | user-configurable 0–60 | 60 fixed |
| Platform scope | 17 PIDs, Intel + AMD | AMD `0x1000` | AMD `0x1000` |
| State | vendor config | per-app | `CCTL` blob, upstream-compatible |

Same wire format in all three; the differences are naming, persistence and how
much of the header count is exposed.

## What "supported" means for RGB

Lighting works wherever the controller enumerates: open the device, write the
frame, read state back from the `CCTL` file. Fan control is narrower — it needs
the Super I/O to answer the chip-ID probe; see
[board detection](BOARD-DETECTION.md) and [platform notes](PLATFORM.md).

`/dev/hidraw` access comes from the udev rule the `.deb` packages install;
[platform notes](PLATFORM.md) has the rule and manual steps for AppImage and
tarball builds.

## Licence and provenance

Tables adapted from upstream [`colorctl`](https://github.com/Curstantine/colorctl)
(MIT), whose `COMPATIBILITY.md` documents the decompilation of Colorful's iGame
Center libraries. This repository is GPL-3.0-only; attribution in `NOTICE.md`.
