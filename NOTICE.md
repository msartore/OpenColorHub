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

The upstream repository is tracked as a git submodule at
[`upstream/colorctl`](upstream/colorctl), pinned to the release whose changes
were integrated. It is a reference copy for diffing provenance, not a build
dependency: `cargo build` never reads it, and `git clone` works without
initialising it.

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
| Native Shell (`nsh`, `nsh-core`, `nsh-base`, runtime) | Apache-2.0 |
| QuickJS (bundled with Native Shell) | MIT |
| `windows-sys` | MIT OR Apache-2.0 |
| `serde`, `serde_json` | MIT OR Apache-2.0 |
| `ureq`, `sha2` | MIT OR Apache-2.0 |
| `LpcIO.bin` (PawnIO module, © namazso) | LGPL-2.1-or-later |
| Tabler Icons (navigation glyph path data) | MIT |

### Icon set (MIT)

The navigation glyphs are [Tabler Icons](https://github.com/tabler/tabler-icons),
inlined as path data in `nsh/components/icons.tsx` rather than pulled from the
npm package.

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

MIT is compatible with GPLv3; the notice is retained for the inlined paths.

### PawnIO module

`crates/colorctl-core/resources/LpcIO.bin` is an unmodified signed module from
the PawnIO project (0.2.11), Copyright © 2023 namazso <admin@namazso.eu>,
distributed under the GNU Lesser General Public License 2.1 or later. The full
text is in `crates/colorctl-core/resources/COPYING.LGPL-2.1`. Its SHA-256 is
`b3896a1cab0d808fca31fe2ebcae045d59dac690da87b17c858bb8da357eb45e`, so the
unmodified claim can be checked against the upstream module.

It is not linked into this binary: it is passed to the PawnIO kernel driver at
runtime as data, which the driver verifies and executes inside its sandbox. That
is why the LGPL's relinking requirement is satisfied by shipping the unmodified
blob — replacing it is a file swap, no recompilation.

Corresponding source: <https://github.com/namazso/PawnIO.Modules> (`LpcIO.p`).

### GPU RGB protocol sources

The GPU LED wire protocol and effect-ID map in `crates/colorctl-core/src/gpu.rs`
were recovered from the controller hardware and independently confirmed against
the OpenRGB `ASRockRX9070XTGPUController`, part of the OpenRGB project by Adam
Honse and the OpenRGB Team, <https://gitlab.com/CalcProgrammer1/OpenRGB>,
licensed GPL-2.0-or-later. The hardware-effect ID table follows OpenRGB's mode
map. OpenRGB is GPL-2.0-or-later, compatible with this project's GPL-3.0-only:
the combined work is distributed under GPLv3 and the source is recorded here.

The PCI subsystem-ID catalogue in `gpu_catalogue.rs` was transcribed from the
factual hardware-ID table published by PolychromeSYNC v1.0.50's `UpdateFW.ini`
(ASRock). Hardware IDs are factual data, not copyrightable expression; no
PolychromeSYNC code is included. The Windows GPU transports use FFI signatures
from the vendors' own user-mode APIs (ADL, NVAPI, IGCL); no vendor headers or
libraries are bundled.

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

Native Shell links Rust crates statically, so the "corresponding source"
includes this repository's sources and the `Cargo.lock` needed to rebuild the
same binary. It does not include Windows or system frameworks, which are separate
works the user already has. There is no WebView2 runtime dependency.
