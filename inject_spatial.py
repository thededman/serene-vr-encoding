#!/usr/bin/env python3
"""
inject_spatial.py — restore 360/VR spatial metadata that FFmpeg strips.

FFmpeg drops the Google Spherical metadata on every MP4 it writes (verified: even
`-c copy` loses it). Without it a VR player treats the file as a flat 2D video and the
top-bottom stereo halves show as two stacked images.

This writes BOTH metadata generations so any player can find one it understands:

  * Spherical Video V1  — an XML blob in a `uuid` box on the video trak. This is what the
    Serene source master uses, so it is what the Serene app is known to read.
  * Spherical Video V2  — `sv3d` (projection) + `st3d` (stereo mode) boxes inside the
    video sample entry. Modern players prefer this; older ones ignore it harmlessly.

Usage:
    python3 inject_spatial.py INPUT.mp4 OUTPUT.mp4 [--stereo top-bottom|left-right|mono]

No third-party dependencies.
"""

import os
import struct
import sys

SPHERICAL_V1_UUID = bytes.fromhex("ffcc8263f8554a938814587a02521fdd")

# Containers whose payload is just more boxes, so we can recurse straight in.
CONTAINERS = {b"moov", b"trak", b"mdia", b"minf", b"stbl", b"edts", b"udta"}

STEREO_MODES = {"mono": 0, "top-bottom": 1, "left-right": 2}


# --------------------------------------------------------------------------------------
# Box primitives
# --------------------------------------------------------------------------------------

def box(box_type: bytes, payload: bytes) -> bytes:
    """Build a plain MP4 box."""
    return struct.pack(">I", len(payload) + 8) + box_type + payload


def full_box(box_type: bytes, payload: bytes, version: int = 0, flags: int = 0) -> bytes:
    """Build a FullBox (box with a version byte and 3 flag bytes)."""
    return box(box_type, struct.pack(">B3s", version, flags.to_bytes(3, "big")) + payload)


def iter_boxes(buf: bytes, start: int = 0, end: int = None):
    """Yield (type, header_size, box_start, box_size) for each box in a byte range."""
    if end is None:
        end = len(buf)
    pos = start
    while pos + 8 <= end:
        size = struct.unpack(">I", buf[pos:pos + 4])[0]
        btype = buf[pos + 4:pos + 8]
        header = 8
        if size == 1:
            size = struct.unpack(">Q", buf[pos + 8:pos + 16])[0]
            header = 16
        elif size == 0:
            size = end - pos
        if size < header or pos + size > end:
            break
        yield btype, header, pos, size
        pos += size


def find_box(buf: bytes, path, start: int = 0, end: int = None):
    """Walk a slash-separated box path. Returns (box_start, box_size, header_size)."""
    if end is None:
        end = len(buf)
    target, rest = path[0], path[1:]
    for btype, header, pos, size in iter_boxes(buf, start, end):
        if btype != target:
            continue
        if not rest:
            return pos, size, header
        # stsd is a FullBox with an entry count before its children.
        child_start = pos + header + (8 if target == b"stsd" else 0)
        return find_box(buf, rest, child_start, pos + size)
    return None


# --------------------------------------------------------------------------------------
# Metadata builders
# --------------------------------------------------------------------------------------

def build_v1_uuid(width: int, height: int, stereo: str) -> bytes:
    """Spherical Video V1: RDF/XML inside a uuid box.

    Field values mirror the Serene master exactly (full-frame crop, no partial pano) so
    the app sees the same shape it already handles, just at a new resolution.
    """
    stereo_tag = ""
    if stereo != "mono":
        stereo_tag = f"<GSpherical:StereoMode>{stereo}</GSpherical:StereoMode>"

    xml = (
        '<?xml version="1.0"?>'
        '<rdf:SphericalVideo '
        'xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#" '
        'xmlns:GSpherical="http://ns.google.com/videos/1.0/spherical/">'
        "<GSpherical:Spherical>true</GSpherical:Spherical>"
        "<GSpherical:Stitched>true</GSpherical:Stitched>"
        "<GSpherical:StitchingSoftware>Serene VR pipeline</GSpherical:StitchingSoftware>"
        "<GSpherical:ProjectionType>equirectangular</GSpherical:ProjectionType>"
        f"{stereo_tag}"
        f"<GSpherical:FullPanoWidthPixels>{width}</GSpherical:FullPanoWidthPixels>"
        f"<GSpherical:FullPanoHeightPixels>{height}</GSpherical:FullPanoHeightPixels>"
        f"<GSpherical:CroppedAreaImageWidthPixels>{width}</GSpherical:CroppedAreaImageWidthPixels>"
        f"<GSpherical:CroppedAreaImageHeightPixels>{height}</GSpherical:CroppedAreaImageHeightPixels>"
        "<GSpherical:CroppedAreaLeftPixels>0</GSpherical:CroppedAreaLeftPixels>"
        "<GSpherical:CroppedAreaTopPixels>0</GSpherical:CroppedAreaTopPixels>"
        "</rdf:SphericalVideo>"
    ).encode("utf-8")

    return box(b"uuid", SPHERICAL_V1_UUID + xml)


def build_sv3d() -> bytes:
    """Spherical Video V2 projection box: full equirectangular sphere, no pose offset."""
    svhd = full_box(b"svhd", b"Serene VR pipeline\x00")
    prhd = full_box(b"prhd", struct.pack(">iii", 0, 0, 0))          # yaw, pitch, roll
    equi = full_box(b"equi", struct.pack(">IIII", 0, 0, 0, 0))      # full sphere bounds
    return box(b"sv3d", svhd + box(b"proj", prhd + equi))


def build_st3d(stereo: str) -> bytes:
    """Spherical Video V2 stereo box."""
    return full_box(b"st3d", struct.pack(">B", STEREO_MODES[stereo]))


# --------------------------------------------------------------------------------------
# moov surgery
# --------------------------------------------------------------------------------------

def video_trak_range(moov: bytes):
    """Return (start, size) of the trak whose handler is 'vide'.

    `moov` is the whole moov box, so skip its own header to reach the child boxes.
    """
    moov_header = 16 if struct.unpack(">I", moov[0:4])[0] == 1 else 8
    for btype, header, pos, size in iter_boxes(moov, moov_header):
        if btype != b"trak":
            continue
        hdlr = find_box(moov, [b"mdia", b"hdlr"], pos + header, pos + size)
        if hdlr:
            h_start, _, h_header = hdlr
            # hdlr payload: version/flags(4) + pre_defined(4) + handler_type(4)
            if moov[h_start + h_header + 8:h_start + h_header + 12] == b"vide":
                return pos, size
    raise SystemExit("error: no video track found")


def patch_sizes(moov: bytearray, ancestors, delta: int) -> None:
    """Grow every ancestor box header by delta (all are 32-bit sizes in practice)."""
    for pos in ancestors:
        size = struct.unpack(">I", moov[pos:pos + 4])[0]
        if size == 1:
            big = struct.unpack(">Q", moov[pos + 8:pos + 16])[0]
            moov[pos + 8:pos + 16] = struct.pack(">Q", big + delta)
        else:
            moov[pos:pos + 4] = struct.pack(">I", size + delta)


def shift_chunk_offsets(moov: bytearray, delta: int) -> None:
    """moov grew and sits before mdat, so every chunk offset must move by the same delta."""
    for btype, header, pos, size in iter_boxes(bytes(moov)):
        if btype != b"moov":
            continue
        _shift_recursive(moov, pos + header, pos + size, delta)


def _shift_recursive(moov: bytearray, start: int, end: int, delta: int) -> None:
    for btype, header, pos, size in iter_boxes(bytes(moov), start, end):
        if btype in CONTAINERS:
            _shift_recursive(moov, pos + header, pos + size, delta)
        elif btype in (b"stco", b"co64"):
            body = pos + header + 4                      # skip version/flags
            count = struct.unpack(">I", moov[body:body + 4])[0]
            p = body + 4
            width = 4 if btype == b"stco" else 8
            fmt = ">I" if btype == b"stco" else ">Q"
            for _ in range(count):
                val = struct.unpack(fmt, moov[p:p + width])[0]
                moov[p:p + width] = struct.pack(fmt, val + delta)
                p += width


def inject(src: str, dst: str, stereo: str) -> None:
    size_on_disk = os.path.getsize(src)

    # Read the top-level layout without loading the whole (possibly multi-GB) file.
    with open(src, "rb") as f:
        top = []
        pos = 0
        while pos < size_on_disk:
            f.seek(pos)
            hdr = f.read(16)
            if len(hdr) < 8:
                break
            bsize = struct.unpack(">I", hdr[0:4])[0]
            btype = hdr[4:8]
            if bsize == 1:
                bsize = struct.unpack(">Q", hdr[8:16])[0]
            elif bsize == 0:
                bsize = size_on_disk - pos
            if bsize <= 0:
                break
            top.append((btype, pos, bsize))
            pos += bsize

        moov_entry = next((t for t in top if t[0] == b"moov"), None)
        mdat_entry = next((t for t in top if t[0] == b"mdat"), None)
        if moov_entry is None:
            raise SystemExit("error: no moov box found")

        f.seek(moov_entry[1])
        moov = bytearray(f.read(moov_entry[2]))

    trak_start, trak_size = video_trak_range(bytes(moov))

    # --- walk down to the sample entry, recording every ancestor on the way ------------
    # All of these enclose the insertion point, so each one's size must grow with it.
    ancestors = [trak_start]
    scan_start, scan_end = trak_start + 8, trak_start + trak_size
    for name in (b"mdia", b"minf", b"stbl", b"stsd"):
        found = find_box(bytes(moov), [name], scan_start, scan_end)
        if not found:
            raise SystemExit(f"error: no {name.decode()} box found")
        b_start, b_size, b_header = found
        ancestors.append(b_start)
        # stsd is a FullBox carrying an entry count ahead of its children.
        scan_start = b_start + b_header + (8 if name == b"stsd" else 0)
        scan_end = b_start + b_size

    entry = next(iter_boxes(bytes(moov), scan_start, scan_end), None)
    if entry is None:
        raise SystemExit("error: no sample entry found")
    _, entry_header, entry_start, entry_size = entry
    ancestors.append(entry_start)

    # VisualSampleEntry: 6 reserved + 2 data_ref_index + 16 predefined/reserved,
    # then width and height as 16-bit values.
    vpos = entry_start + entry_header + 24
    width, height = struct.unpack(">HH", moov[vpos:vpos + 4])

    # --- V2 boxes go inside the sample entry ------------------------------------------
    v2 = build_sv3d() + build_st3d(stereo)
    moov[entry_start + entry_size:entry_start + entry_size] = v2
    patch_sizes(moov, ancestors, len(v2))
    trak_size += len(v2)

    # --- V1 uuid box is appended as a direct child of the trak ------------------------
    v1 = build_v1_uuid(width, height, stereo)
    moov[trak_start + trak_size:trak_start + trak_size] = v1
    patch_sizes(moov, [trak_start], len(v1))

    # moov itself grew by everything we added.
    delta = len(v2) + len(v1)
    patch_sizes(moov, [0], delta)

    # If moov precedes mdat (faststart layout), media data slid forward by delta.
    if mdat_entry and moov_entry[1] < mdat_entry[1]:
        shift_chunk_offsets(moov, delta)

    # --- write out, streaming mdat rather than buffering it ---------------------------
    with open(src, "rb") as fin, open(dst, "wb") as fout:
        for btype, bpos, bsize in top:
            if btype == b"moov":
                fout.write(moov)
                continue
            fin.seek(bpos)
            remaining = bsize
            while remaining:
                chunk = fin.read(min(8 << 20, remaining))
                if not chunk:
                    break
                fout.write(chunk)
                remaining -= len(chunk)

    print(f"  injected {width}x{height} equirectangular / {stereo} -> {os.path.basename(dst)}")


def main() -> None:
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    stereo = "top-bottom"
    for a in sys.argv[1:]:
        if a.startswith("--stereo"):
            stereo = a.split("=", 1)[1] if "=" in a else "top-bottom"
    if len(args) != 2:
        raise SystemExit(__doc__)
    if stereo not in STEREO_MODES:
        raise SystemExit(f"error: --stereo must be one of {list(STEREO_MODES)}")
    inject(args[0], args[1], stereo)


if __name__ == "__main__":
    main()
