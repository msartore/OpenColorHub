"""Regenerate icon PNGs from a source .ico without a resampling round-trip.

The trap this avoids: `npx tauri icon` needs a >=1024 px master, so feeding it a
256 px source means every output is 256 -> 1024 -> N, which is two resamples and
visible softness at every size. Instead:

  * sizes the .ico already contains are decoded straight from the .ico (no
    resampling at all),
  * other sizes are produced from the largest native entry in a single step.

The .ico itself is never written to — it is the user-supplied artwork and is
embedded in the executable verbatim.

Usage:  python scripts/icons.py <source.ico> [icons_dir]
"""
from __future__ import annotations

import sys
from pathlib import Path

from PIL import Image

# sizes Tauri expects, plus the Windows tile set
PNG_TARGETS = {
    "32x32.png": 32,
    "64x64.png": 64,
    "128x128.png": 128,
    "128x128@2x.png": 256,
    "icon.png": 512,
    "Square30x30Logo.png": 30,
    "Square44x44Logo.png": 44,
    "Square71x71Logo.png": 71,
    "Square89x89Logo.png": 89,
    "Square107x107Logo.png": 107,
    "Square142x142Logo.png": 142,
    "Square150x150Logo.png": 150,
    "Square284x284Logo.png": 284,
    "Square310x310Logo.png": 310,
    "StoreLogo.png": 50,
}


def main() -> int:
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    source = Path(sys.argv[1])
    out_dir = Path(sys.argv[2]) if len(sys.argv) > 2 else Path("src-tauri/icons")
    out_dir.mkdir(parents=True, exist_ok=True)

    ico = Image.open(source)
    native = sorted(ico.info.get("sizes", []))
    if not native:
        print(f"{source}: no icon entries found")
        return 1
    print(f"source {source.name}: {len(native)} entries {native}")

    # decode every native entry once
    entries = {size: ico.ico.getimage(size).convert("RGBA") for size in native}
    largest = max(native, key=lambda s: s[0])
    master = entries[largest]

    exact = resampled = 0
    for name, size in PNG_TARGETS.items():
        if (size, size) in entries:
            img = entries[(size, size)]
            exact += 1
            how = "native"
        else:
            img = master.resize((size, size), Image.LANCZOS)
            resampled += 1
            how = f"from {largest[0]}"
        img.save(out_dir / name, optimize=True)
        print(f"  {name:26} {size:>4}px  {how}")

    print(f"\n{exact} copied from native entries, {resampled} resampled in one step")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
