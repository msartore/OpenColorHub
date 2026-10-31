OpenColorHub — fan and ARGB control for Colorful motherboards
Copyright © 2026 Massimiliano Sartore

This program is free software: you can redistribute it and/or modify it under
the terms of the GNU General Public License as published by the Free Software
Foundation, version 3.

This program is distributed in the hope that it will be useful, but WITHOUT ANY
WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A
PARTICULAR PURPOSE. See the GNU General Public License for more details.

You should have received a copy of the GNU General Public License along with
this program. If not, see <https://www.gnu.org/licenses/>.

The full licence text is in LICENSE.md. It is unmodified; the SPDX identifier
for this project is GPL-3.0-only.

---

## Upstream attribution (MIT)

OpenColorHub is a rewrite of `colorctl` by Curstantine
(<https://github.com/Curstantine/colorctl>), which is distributed under the MIT
licence. Portions of this work derive from it — the CLI surface, the `CCTL`
state format, the fan-curve model and the Linux Super I/O register sequence —
and are used here under the terms below, as the MIT licence requires.

Copyright © 2026 Curstantine

> Permission is hereby granted, free of charge, to any person obtaining a copy
> of this software and associated documentation files (the "Software"), to deal
> in the Software without restriction, including without limitation the rights
> to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
> copies of the Software, and to permit persons to whom the Software is
> furnished to do so, subject to the following conditions:
>
> The above copyright notice and this permission notice shall be included in all
> copies or substantial portions of the Software.
>
> THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
> IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
> FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
> AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
> LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
> OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
> SOFTWARE.

MIT is compatible with the GPL: the combined work is distributed under
GPL-3.0-only, and the MIT notice above is retained for the derived portions.

---

## Third-party components

All dependencies are compatible with GPLv3. The ones that ship inside the
binary or the bundle:

| Component | Licence |
| --- | --- |
| Tauri 2 (`tauri`, `tauri-build`, `@tauri-apps/api`, `@tauri-apps/cli`) | MIT OR Apache-2.0 |
| `windows-sys` | MIT OR Apache-2.0 |
| `serde`, `serde_json` | MIT OR Apache-2.0 |
| `ureq`, `sha2` (self-updater) | MIT OR Apache-2.0 |
| Vite, TypeScript | MIT / Apache-2.0 |
| `LpcIO.bin` (PawnIO module, © namazso) | LGPL-2.1-or-later |

### PawnIO module

`crates/colorctl-core/resources/LpcIO.bin` is an unmodified signed module from
the PawnIO project (0.2.11), © namazso, distributed under the GNU Lesser General
Public License 2.1 or later. The full text is in
`crates/colorctl-core/resources/COPYING.LGPL-2.1`.

It is not linked into this binary: it is passed to the PawnIO kernel driver at
runtime as data, which the driver verifies and executes inside its sandbox. That
is why the LGPL's relinking requirement is satisfied by shipping the unmodified
blob — replacing it is a file swap, no recompilation.

Corresponding source: <https://github.com/namazso/PawnIO.Modules> (`LpcIO.p`).


Apache-2.0 is compatible with GPLv3 (it is not compatible with GPLv2). No
component is GPL-incompatible, and none is linked statically in a way that would
add further restrictions.

### A note on the reverse-engineered material

The iGC.Lite analysis that informed the HID framing and the HWiNFO shared-memory
layout was performed statically on a binary the author owns. No code, data or
other material from that product is included in this repository — only
independently written implementations of observable interfaces. No vendor
licence is therefore implicated.

---

## Distributing a build

GPLv3 is a copyleft licence, so shipping the installer or the portable zip to
anyone else carries an obligation with it: they must be able to get the complete
corresponding source under the same licence.

The cheapest way to satisfy that is GPLv3 §6(d) — publish the source and point
at it. If a build leaves this machine, keep the tagged source at
<https://github.com/msartore/OpenColorHub> reachable for as long as you distribute
that build, or ship the source alongside it.

What that means practically:

- Rebuilding from a modified tree and distributing it? Publish the modification.
- Publishing the binary but keeping your fork private is not permitted.
- If you build only for yourself, nothing is triggered — §6 applies to
  distribution, not to private use.

Tauri links Rust crates statically, so the "corresponding source" includes this
repository's sources and the `Cargo.lock`/`package-lock.json` needed to rebuild
the same binary. It does not include Windows or WebView2, which are separate
works the user already has.

---

## Icon set (MIT)

The navigation glyphs are Tabler Icons, inlined as path data in
`src/components/icons.ts` rather than pulled from the npm package.

<https://github.com/tabler/tabler-icons>

Copyright © 2020-2026 Paweł Kuna

> Permission is hereby granted, free of charge, to any person obtaining a copy
> of this software and associated documentation files (the "Software"), to deal
> in the Software without restriction, including without limitation the rights
> to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
> copies of the Software, and to permit persons to whom the Software is
> furnished to do so, subject to the following conditions:
>
> The above copyright notice and this permission notice shall be included in all
> copies or substantial portions of the Software.
>
> THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
> IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
> FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
> AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
> LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
> OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
> SOFTWARE.
