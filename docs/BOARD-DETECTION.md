# Board auto-detection

Every screen shows which board the app thinks it is talking to, and how sure it
is about that.

| Confidence | Meaning | Hardware probe |
| --- | --- | --- |
| **verified** | exact match in the 35-model catalogue | allowed |
| **compatible** | Colorful board of a known generation, not catalogued | allowed, with a caution note |
| **unknown** | identity unreadable, or a Colorful chipset outside the catalogue (X570, B550, …) | allowed — the chip-ID check decides |
| **unsupported** | another vendor (ASUS, MSI, ASRock, …) | **blocked** in `auto` mode |

Sources: `/sys/class/dmi/id/*` on Linux and
`HKLM\HARDWARE\DESCRIPTION\System\BIOS` on Windows. Other platforms report
`none`. Reading needs no privileges on either.

## Where the verdict appears

The verdict travels in every telemetry snapshot (`snapshot.board`), so it is
shown identically on the Dashboard header, in Settings, on the About screen and
in `colorctl board`:

```console
$ colorctl board
Motherboard: Colorful Technology And Development Co.,LTD CVN X570M GAMING PRO
  Board rev:   V14
  BIOS: American Megatrends International, LLC. 1015
  System:      CVN X570M GAMING PRO
  Detected by: registry
  Support: UNKNOWN (35 of 35 catalogue boards)
  · ... is a Colorful board from the X570 generation, which predates the
    NCT5584D. The Super I/O probe decides whether fan control works; nothing is
    written until the chip ID matches.
  · BIOS 1015
  Hardware: Super I/O probe decides at runtime
```

That board is not in the catalogue, so it reads `UNKNOWN` — and fan control still
works, because the Super I/O on this X570 board (`0xD121`) answers the probe.
**The confidence level gates policy, not capability.**

## Why the guard is asymmetric

It is deliberately conservative in one direction only: it refuses hardware when
the board is *known* to be someone else's, and stays out of the way when it
simply does not know. Guessing "unsupported" from a missing catalogue entry would
block working hardware; guessing "supported" from a Colorful vendor string would
poke registers on a board nobody has verified.

`--backend hardware` overrides the guard for people who know what they are doing.
