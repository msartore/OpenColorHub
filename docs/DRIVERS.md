# Drivers

The **Drivers** screen lists what Colorful publishes for the board this machine
reports, and downloads the file you pick. It never runs an installer.

---

## Where the list comes from

The vendor's own client asks `https://igameapi.colorful.cn/api/Drive/GetList` for
"the download list", and that API is **token-gated**: it answers
`{"Data":[],"Code":0,"Success":true}` — HTTP 200, no error — unless the request
carries the client's MAC-derived `token` header. There is no anonymous path into
it, and forging one would mean impersonating the vendor client.

The public site publishes the same catalogue without credentials:

| Step | Request | Purpose |
|---|---|---|
| 1 | `GET /Home/GetProductClassList` | series groups; motherboards are `parent_id = 84` |
| 2 | `GET /Home/GetProduct?modelid=84&attrId=<series>` | products per series, `{id, name}` |
| 3 | `GET /Home/GetProductDownload?mid=84&id=<product>` | the file list |

Step 2 returns image rows alongside products; their `id` is a GUID rather than a
number. They are filtered out — deserialising the whole list as numeric ids
fails on the first GUID and loses every product behind it.

Board names are matched after collapsing case and whitespace, because SMBIOS and
the website disagree on both: this machine reports `CVN X570M GAMING PRO`, the
site lists `CVN X570M GAMING PRO V14`. An exact match wins; otherwise the
longest product name the board name starts with (or that starts with the board
name) is taken.

## Rules the downloader follows

- **HTTPS only, vendor domains only.** `*.colorful.cn` (the CDN) and
  `colorfulgroup.com` (the group site, which serves older BIOS images). A
  catalogue entry pointing anywhere else is dropped rather than followed.
- **Site-relative references are resolved**, not rejected. Five older BIOS
  entries in this board's list are `/uploads/common/...` paths; dropping them
  silently lost real firmware.
- **The CDN requires a vendor `Referer`.** Without one it answers `403`, which
  looks like a dead link. The downloader sends the site origin.
- **Names come from the server and are treated as hostile**: separators, drive
  letters, control characters and trailing dots are stripped, `%XX` escapes are
  decoded, and a name that reduces to `.`/`..` is refused.
- **A `.part` file is used** and renamed only on success, so an interrupted
  transfer cannot be mistaken for a usable archive.
- **Every file is hashed** (SHA-256) as it streams, and the hash is reported. The
  catalogue publishes no checksums, so this proves what *you* received — it
  cannot prove what the vendor intended.

## What it will not do

- Run, extract or install anything. There is no `Process.Start` on this path.
- Download without a click on that specific row.
- Touch BIOS flashing. Firmware files are listed — with a `newer` badge when the
  build number beats the installed one — and the note says flashing happens in
  the BIOS, not here.

## Files

Downloads land in the app's data directory:

- installed: `%APPDATA%\dev.msartore.opencolorhub\drivers\`
- portable: `data\drivers\` next to the executable

**Show in folder** selects the file in Explorer (`explorer /select,<path>`) or
opens its directory with `xdg-open` on Linux.

## Verifying it still works

The parser and the URL rules are unit-tested offline. The live path is an
ignored test, because it needs the network:

```bash
cargo test -p colorctl-core -- --ignored live_
```

It resolves this board to product `880`, asserts 21 files with the newest
firmware first, checks that every URL is absolute and inside the vendor domains,
then downloads the manual and verifies size, hash and that no `.part` remains.
