#!/usr/bin/env python3
"""
vr_plan.py — work out how a VR master should be encoded for the Serene pipeline.

Probes one file, identifies its projection and stereo layout, and computes the target
resolution and bitrate for the house standard (see ENCODING-GUIDE.md). Prints shell-
evaluable KEY=VALUE lines so both inspect_master.sh and encode_vendor.sh can share
exactly this logic rather than each reimplementing it.

    ./vr_plan.py master.mp4
    ./vr_plan.py master.mp4 --max-mbps 45
    ./vr_plan.py master.mp4 --px-per-deg 19   # mono content has headroom; 19 matches the panel

STATUS is one of:
    ok      — encode with the values given
    review  — encodable, but something needs a human decision (NOTE says what)
    unfit   — cannot be handled automatically; NOTE says why
"""

import json
import shlex
import subprocess
import sys

# --- Limits measured off the Quest 3S (/vendor/etc/media_codecs.xml, device "panther") ---
BLOCKS_PER_SEC = 7_776_000          # decoder throughput, 16x16 blocks
SAFETY = 0.80                       # a saturated decoder drops frames rather than erroring
MAX_PIXEL_RATE = BLOCKS_PER_SEC * 256 * SAFETY

SPEC_MAX_PIXELS = 35_651_584        # AV1/HEVC Level 6.2 max luma per frame
DEVICE_MAX_DIM = 8192

# --- House standard, validated by the 7-way A/B test (see RESULTS.md) ---
TARGET_PX_PER_DEG = 16              # Quest 3S resolves ~19; 16 is the measured sweet spot
BITS_PER_PIXEL = 0.0346             # from the winning encode: 34.4 Mbps at 5760x5760@30
DEFAULT_MAX_MBPS = 35
MIN_MBPS = 8


def probe(path):
    out = subprocess.run(
        ["ffprobe", "-v", "error", "-select_streams", "v:0",
         "-show_streams", "-print_format", "json", path],
        capture_output=True, text=True)
    if out.returncode != 0:
        raise SystemExit(f"ffprobe failed on {path}")
    st = json.loads(out.stdout)["streams"][0]

    num, _, den = st.get("r_frame_rate", "30/1").partition("/")
    fps = round(float(num) / float(den or 1))

    stereo, projection = "mono", None
    for sd in st.get("side_data_list", []):
        t = sd.get("side_data_type", "")
        if t == "Stereo 3D":
            stereo = sd.get("type", "mono")
        elif t == "Spherical Mapping":
            projection = sd.get("projection")

    return {
        "width": int(st["width"]),
        "height": int(st["height"]),
        "fps": fps,
        "codec": st.get("codec_name", "?"),
        "pix_fmt": st.get("pix_fmt", "?"),
        "bitrate": int(st.get("bit_rate") or 0),
        "stereo": stereo,
        "projection": projection,
    }


def align(n, mult=16):
    """Round to a multiple of the macroblock size, minimum one block."""
    return max(mult, int(round(n / mult)) * mult)


def identify(info):
    """Work out the stereo layout and how much of the sphere one eye covers.

    Returns (layout, eye_w, eye_h, h_degrees, note). Coverage is inferred from the
    per-eye aspect ratio: a 2:1 eye is a full 360x180 equirect, a 1:1 eye is 180x180.
    """
    w, h, stereo = info["width"], info["height"], info["stereo"]

    if "top" in stereo:                     # "top and bottom"
        layout, eye_w, eye_h = "tb", w, h // 2
    elif "side" in stereo:                  # "side by side"
        layout, eye_w, eye_h = "sbs", w // 2, h
    else:
        layout, eye_w, eye_h = "mono", w, h

    if eye_h == 0:
        return layout, eye_w, eye_h, 0, "could not determine eye dimensions"

    ratio = eye_w / eye_h

    # A square frame carrying NO stereo tag is genuinely ambiguous: it is either 360
    # stereo packed top-bottom (the standard 8K VR layout) or a VR180 mono frame. Any
    # FFmpeg pass strips the stereo tag, so intermediate files routinely arrive like
    # this. Top-bottom 360 is overwhelmingly the common case at these sizes -- guessing
    # VR180 mono instead silently halves the resolution and mangles the projection.
    if layout == "mono" and abs(ratio - 1.0) < 0.05:
        return ("tb", eye_w, eye_h // 2, 360,
                f"{w}x{h} is square with no stereo tag (ambiguous). ASSUMING 360 STEREO "
                "TOP-BOTTOM, the usual layout at this size. If it is really VR180 mono, "
                "the output will be wrong -- check one frame before trusting a batch")


    # Sanity-check the declared stereo layout against the geometry. A correct per-eye frame
    # is 2:1 (360x180) or 1:1 (180x180). Anything wildly outside that means the stereo tag
    # is wrong -- most often a mono equirect exported with a spurious stereo flag. Trusting
    # such a tag makes a player split one image in half and send unrelated halves to each
    # eye, which looks like permanent blur. Treat the frame as mono and say so loudly.
    if layout != "mono" and (ratio > 3.0 or ratio < 0.6):
        whole = w / h if h else 0
        if 1.7 < whole < 2.3:
            return ("mono", w, h, 360,
                    f"file declares {stereo!r} stereo, but that would make each eye "
                    f"{eye_w}x{eye_h} ({ratio:.1f}:1). The full frame is {w}x{h} ({whole:.1f}:1), "
                    "a normal mono 360 equirect. TREATING AS MONO -- the stereo tag is wrong "
                    "and will cause blur if a player honours it")
        return (layout, eye_w, eye_h, 360,
                f"per-eye aspect {ratio:.1f}:1 is not valid for {stereo!r} stereo; check manually")

    if ratio > 1.6:
        return layout, eye_w, eye_h, 360, ""
    if ratio < 1.3:
        return layout, eye_w, eye_h, 180, ""
    return layout, eye_w, eye_h, 360, f"unusual per-eye aspect {ratio:.2f}, assuming 360"


def plan(path, max_mbps=DEFAULT_MAX_MBPS, px_per_deg=TARGET_PX_PER_DEG):
    info = probe(path)
    layout, src_eye_w, src_eye_h, degrees, note = identify(info)
    notes = [note] if note else []
    status = "review" if any(k in note for k in
        ("TREATING AS MONO", "check manually", "ASSUMING 360 STEREO")) else "ok"

    if info["projection"] is None:
        notes.append("no 360 metadata in source; assuming equirectangular")
        status = "review"
    elif info["projection"] != "equirectangular":
        return dict(info, status="unfit", note=f"projection is {info['projection']}, "
                    "not equirectangular — handle manually", layout=layout,
                    out_w=0, out_h=0, mbps=0, degrees=degrees)

    if info["fps"] > 60:
        return dict(info, status="unfit", note=f"{info['fps']} fps exceeds what the budget "
                    "supports — request a 30 fps master", layout=layout,
                    out_w=0, out_h=0, mbps=0, degrees=degrees)

    if "10" in info["pix_fmt"] or "12" in info["pix_fmt"]:
        notes.append(f"source is {info['pix_fmt']} (10-bit/HDR); output forced to 8-bit")
        status = "review"

    # --- ideal per-eye size at the target pixel density ---
    eye_w = align(px_per_deg * degrees)
    eye_h = align(eye_w / 2 if degrees == 360 else eye_w)

    # Never upscale — a vendor master smaller than target stays at its own size.
    if src_eye_w < eye_w:
        eye_w, eye_h = align(src_eye_w), align(src_eye_h)
        notes.append(f"source is only {src_eye_w}px per eye, below the {px_per_deg} "
                     "px/deg target; keeping source resolution")

    out_w, out_h = (eye_w, eye_h * 2) if layout == "tb" else \
                   (eye_w * 2, eye_h) if layout == "sbs" else (eye_w, eye_h)

    # --- clamp to the three hard limits, preserving aspect ratio ---
    for limit, label in ((SPEC_MAX_PIXELS, "codec level"),
                         (MAX_PIXEL_RATE / info["fps"], "decoder throughput")):
        if out_w * out_h > limit:
            f = (limit / (out_w * out_h)) ** 0.5
            out_w, out_h = align(out_w * f), align(out_h * f)
            notes.append(f"reduced to {out_w}x{out_h} to stay within {label} at "
                         f"{info['fps']} fps")

    if max(out_w, out_h) > DEVICE_MAX_DIM:
        f = DEVICE_MAX_DIM / max(out_w, out_h)
        out_w, out_h = align(out_w * f), align(out_h * f)
        notes.append(f"reduced to {out_w}x{out_h} to stay within the 8192px device limit")

    # --- bitrate: hold bits-per-pixel constant, then apply the streaming budget ---
    mbps = BITS_PER_PIXEL * out_w * out_h * info["fps"] / 1_000_000
    if mbps > max_mbps:
        notes.append(f"ideal bitrate {mbps:.0f} Mbps exceeds the {max_mbps} Mbps budget; "
                     "capped, so this will be slightly softer than the 30 fps standard")
        mbps = max_mbps
        status = "review"
    mbps = max(MIN_MBPS, round(mbps))

    stereo_flag = {"tb": "top-bottom", "sbs": "left-right"}.get(layout, "mono")

    return dict(info, status=status, note="; ".join(notes), layout=layout,
                out_w=out_w, out_h=out_h, mbps=mbps, degrees=degrees,
                stereo_flag=stereo_flag)


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    max_mbps = DEFAULT_MAX_MBPS
    px_per_deg = TARGET_PX_PER_DEG
    for i, a in enumerate(sys.argv):
        if a == "--max-mbps":
            max_mbps = float(sys.argv[i + 1])
        elif a == "--px-per-deg":
            px_per_deg = float(sys.argv[i + 1])
    if not args:
        raise SystemExit(__doc__)

    p = plan(args[0], max_mbps, px_per_deg)
    # Shell-quoted: several values contain spaces ("top and bottom", note text), which
    # would otherwise break the caller's eval.
    for k in ("status", "width", "height", "fps", "codec", "pix_fmt", "stereo",
              "projection", "layout", "degrees", "out_w", "out_h", "mbps",
              "stereo_flag", "note"):
        print(f"{k.upper()}={shlex.quote(str(p.get(k, '')))}")


if __name__ == "__main__":
    main()
