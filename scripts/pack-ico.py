#!/usr/bin/env python3
"""Packs PNG files into a multi-resolution .ico.

Windows Vista and later accept PNG-compressed entries at any size, and unlike
Pillow's own writer this keeps each entry as the exact rendering that was
produced for it instead of resampling one master.

Usage: pack-ico.py out.ico 16.png 24.png 32.png ...
"""
import struct
import sys


def main(argv):
    if len(argv) < 3:
        print(__doc__, file=sys.stderr)
        return 2
    out, sources = argv[1], argv[2:]

    entries = []
    for path in sources:
        with open(path, "rb") as fh:
            blob = fh.read()
        if blob[:8] != b"\x89PNG\r\n\x1a\n":
            raise SystemExit(f"{path}: not a PNG")
        width, height = struct.unpack(">II", blob[16:24])
        if width != height:
            raise SystemExit(f"{path}: not square ({width}x{height})")
        if width > 256:
            raise SystemExit(f"{path}: {width}px exceeds the 256px ICO limit")
        entries.append((width, blob))

    entries.sort(key=lambda e: e[0])

    header = struct.pack("<HHH", 0, 1, len(entries))
    offset = 6 + 16 * len(entries)
    directory, data = b"", b""
    for size, blob in entries:
        dim = 0 if size == 256 else size
        directory += struct.pack("<BBBBHHII", dim, dim, 0, 0, 1, 32, len(blob), offset)
        offset += len(blob)
        data += blob

    with open(out, "wb") as fh:
        fh.write(header + directory + data)

    print(f"{out}: {len(entries)} entries ({', '.join(str(e[0]) for e in entries)})")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
