#!/usr/bin/env python3
"""Generate the app icon as a PNG — no image libraries needed.

A 360° motif: an equirectangular band wrapped into a sphere outline, on the project's
teal. Written by hand because the alternative is adding a dependency to draw one circle.
"""
import struct, zlib, sys, math

SIZE = 1024
BG = (11, 46, 54)        # deep teal ground
RING = (79, 198, 207)    # accent
BAND = (220, 235, 237)   # light equator band


def px(x, y):
    cx = cy = SIZE / 2
    dx, dy = x - cx, y - cy
    r = math.hypot(dx, dy)
    outer, inner = SIZE * 0.36, SIZE * 0.30

    # Rounded-square background
    corner = SIZE * 0.22
    ax, ay = abs(dx), abs(dy)
    half = SIZE * 0.46
    if ax > half or ay > half:
        return (0, 0, 0, 0)
    if ax > half - corner and ay > half - corner:
        if math.hypot(ax - (half - corner), ay - (half - corner)) > corner:
            return (0, 0, 0, 0)

    # Sphere ring
    if inner <= r <= outer:
        return (*RING, 255)

    # Equator + two latitude bands, clipped to the sphere — the equirect motif
    if r < inner:
        for lat in (0.0, 0.42, -0.42):
            band_y = cy + lat * inner
            if abs(y - band_y) < SIZE * 0.018:
                return (*BAND, 255)
        # Meridian
        if abs(dx) < SIZE * 0.018:
            return (*BAND, 230)
        return (*BG, 255)

    return (*BG, 255)


def main(path):
    rows = []
    for y in range(SIZE):
        row = bytearray([0])  # filter type 0
        for x in range(SIZE):
            row += bytes(px(x + 0.5, y + 0.5))
        rows.append(bytes(row))
    raw = b"".join(rows)

    def chunk(tag, data):
        c = struct.pack(">I", len(data)) + tag + data
        return c + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)

    png = (b"\x89PNG\r\n\x1a\n"
           + chunk(b"IHDR", struct.pack(">IIBBBBB", SIZE, SIZE, 8, 6, 0, 0, 0))
           + chunk(b"IDAT", zlib.compress(raw, 9))
           + chunk(b"IEND", b""))
    open(path, "wb").write(png)
    print(f"wrote {path} ({len(png) // 1024} KB)")


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "icon.png")
