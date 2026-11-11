# Drivers

The **Drivers** screen lists what Colorful publishes for the board this machine
reports and downloads the file you pick. It never runs an installer.

---

## Where the list comes from

The vendor client asks `https://igameapi.colorful.cn/api/Drive/GetList`; that API
is **token-gated**, answering `{"Data":[],"Code":0,"Success":true}` — HTTP 200,
no error — unless the request carries the client's MAC-derived `token` header.
There is no anonymous path, and forging one means impersonating the vendor
client.

The public site publishes the same catalogue without credentials:

| Step | Request | Purpose |
|---|---|---|
| 1 | `GET /Home/GetProductClassList` | series groups; motherboards are `parent_id = 84` |
| 2 | `GET /Home/GetProduct?modelid=84&attrId=<series>` | products per series, `{id, name}` |
| 3 | `GET /Home/GetProductDownload?mid=84&id=<product>` | the file list |

Step 2 returns image rows with GUID `id`s; they are filtered out — deserialising
the list as numeric ids fails on the first GUID and loses every product behind
it.

Board names are matched after collapsing case and whitespace: SMBIOS reports
`CVN X570M GAMING PRO`, the site lists `CVN X570M GAMING PRO V14`. Exact match
wins; otherwise the longest product name the board name starts with (or that
starts with the board name).

## Rules the downloader follows

- **HTTPS, vendor domains only.** `*.colorful.cn` (CDN) and `colorfulgroup.com`
  (older BIOS images). Anything else is dropped.
- **Site-relative references are resolved**: five older BIOS entries are
  `/uploads/common/...`; dropping them lost real firmware.
- **The CDN requires a vendor `Referer`** or answers `403`; the downloader sends
  the site origin.
- **Server names are hostile**: separators, drive letters, control characters and
  trailing dots stripped, `%XX` decoded, `.`/`..` refused.
- **A `.part` file is used**, renamed only on success.
- **Every file is hashed** (SHA-256) while streaming. The catalogue publishes no
  checksums, so the hash proves what *you* received, not what the vendor
  intended.

## What it will not do

- Run, extract or install anything. No `Process.Start` on this path.
- Download without a click on that row.
- Touch BIOS flashing. Firmware is listed — with a `newer` badge when the build
  number beats the installed one — and the note says flashing happens in the
  BIOS.

## Caching

Cached in the app's data directory as `drivers-cache.json`: one JSON record, not
SQLite (a single list keyed by board, read only as "the last copy").

- The cached copy renders **immediately** — 1 ms versus ~1.2 s for the network —
  then refreshes in the background.
- The cache remembers its SMBIOS board, so swapped hardware invalidates it.
- Failed refresh with a cache: list plus warning. Without: the error alone.
- **Metadata only**: titles, kinds, URLs, dates. Driver files are downloaded only
  on request.

## What Colorful publishes

21 entries in four groups for this board, classified from the vendor's folder
names (`/bios/`, `/drivers/`, `/manual/`) before title keywords:

| Group | Count | Examples |
|---|---|---|
| Firmware | 13 | BIOS 1003 → 1016 |
| Drivers | 6 | AMD chipset 2026-03, B550 chipset, Win11 chipset + integrated graphics, audio, LAN (`RLNetwork`, `19.10.16_19H1_WHQL`) |
| Utilities | 1 | iGameCenter installer |
| Manuals | 1 | board manual (PDF) |

## Files

Downloads land in the app's data directory:

- installed: `%APPDATA%\dev.msartore.opencolorhub\drivers\`
- portable: `data\drivers\` next to the executable

**Show in folder** selects the file in Explorer (`explorer /select,<path>`) or
opens its directory with `xdg-open` on Linux.

## Verifying it still works

The parser and URL rules are unit-tested offline. The live path is an ignored
test (it needs the network):

```bash
cargo test -p colorctl-core -- --ignored live_
```

It resolves this board to product `880`, asserts 21 files with the newest
firmware first, checks every URL is absolute and inside the vendor domains, then
downloads the manual and verifies size, hash and no remaining `.part`.
