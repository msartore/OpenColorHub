# Board auto-detection

Every screen shows which board the app thinks it is talking to, and how sure it
is.

| Confidence | Meaning | Hardware probe |
| --- | --- | --- |
| **verified** | exact match in the 35-model catalogue | allowed |
| **compatible** | Colorful board of a known generation, not catalogued | allowed, with a caution note |
| **unknown** | identity unreadable, or a Colorful chipset outside the catalogue (X570, B550, …) | allowed — the chip-ID check decides |
| **unsupported** | another vendor (ASUS, MSI, ASRock, …) | **blocked** in `auto` mode |

Sources: `/sys/class/dmi/id/*` on Linux,
`HKLM\HARDWARE\DESCRIPTION\System\BIOS` on Windows. Other platforms report
`none`. Reading needs no privileges.

## Where the verdict appears

Every telemetry snapshot carries it (`snapshot.board`), so the Dashboard header,
Settings, About and `colorctl board` agree:

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

That board is not catalogued and reads `UNKNOWN`, yet fan control works: the
Super I/O (`0xD121`) answers the probe. **Confidence gates policy, not
capability.**

## Why the guard is asymmetric

It refuses hardware only when the board is *known* to be someone else's, and
stays out of the way when it does not know. "Unsupported" guessed from a missing
catalogue entry would block working hardware; "supported" guessed from a vendor
string would poke registers on an unverified board.

`--backend hardware` overrides the guard for people who know what they are doing.
