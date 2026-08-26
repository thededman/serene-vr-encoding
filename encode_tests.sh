#!/bin/bash
# Build the six A/B test clips for on-headset comparison (Quest 3S).
# Each clip: same 60s segment, faststart, spatial metadata re-injected.

set -u

BASE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

SRC="${1:-}"
[ -n "$SRC" ] || { echo "usage: ./encode_tests.sh <master.mp4> [out-dir]"; exit 1; }
[ -f "$SRC" ] || { echo "master not found: $SRC"; exit 1; }
SRC="$(cd "$(dirname "$SRC")" && pwd)/$(basename "$SRC")"   # absolute, we cd below

OUT="${2:-$BASE/tests}"
INJ="$BASE/inject_spatial.py"

SS=300      # 05:00 — moving bus, building detail, signage text, high-contrast edges
DUR=60

mkdir -p "$OUT"
cd "$OUT" || exit 1

# Colour tagging must be carried across explicitly; the source is bt709 throughout and an
# untagged output can shift colour in the headset.
COLOR="-color_primaries bt709 -color_trc bt709 -colorspace bt709"
AUDIO="-c:a aac -b:a 192k -ac 2"

finish () {   # $1 = temp file, $2 = final name
    if [ ! -s "$1" ]; then
        echo "!! FAILED to encode $2"
        return 1
    fi
    python3 "$INJ" "$1" "$2" --stereo=top-bottom && rm -f "$1"
}

echo "=============================================="
echo " Serene VR — A/B test clip build"
echo " segment ${SS}s +${DUR}s"
echo "=============================================="

# ---- T0: control, today's file, stream-copied -------------------------------------
echo; echo ">>> T0  control 7680x7680 H.264 ~250 Mbps (copy)"
ffmpeg -hide_banner -loglevel error -stats \
    -ss $SS -t $DUR -i "$SRC" -c copy -movflags +faststart \
    _t0.mp4 -y
finish _t0.mp4 T0_control_7680_h264_250M.mp4

# ---- T1: primary candidate ---------------------------------------------------------
echo; echo ">>> T1  5760x5760 HEVC 32 Mbps   [primary candidate]"
ffmpeg -hide_banner -loglevel error -stats \
    -ss $SS -t $DUR -i "$SRC" \
    -vf "scale=5760:5760:flags=lanczos" -pix_fmt yuv420p $COLOR \
    -c:v hevc_videotoolbox -b:v 32M -g 60 -tag:v hvc1 \
    $AUDIO -movflags +faststart _t1.mp4 -y
finish _t1.mp4 T1_5760_hevc_32M.mp4

# ---- T2: same budget, lower resolution ---------------------------------------------
echo; echo ">>> T2  4608x4608 HEVC 32 Mbps   [res vs bitrate trade]"
ffmpeg -hide_banner -loglevel error -stats \
    -ss $SS -t $DUR -i "$SRC" \
    -vf "scale=4608:4608:flags=lanczos" -pix_fmt yuv420p $COLOR \
    -c:v hevc_videotoolbox -b:v 32M -g 60 -tag:v hvc1 \
    $AUDIO -movflags +faststart _t2.mp4 -y
finish _t2.mp4 T2_4608_hevc_32M.mp4

# ---- T3: bitrate headroom probe ----------------------------------------------------
echo; echo ">>> T3  5760x5760 HEVC 45 Mbps   [is 32M starving T1?]"
ffmpeg -hide_banner -loglevel error -stats \
    -ss $SS -t $DUR -i "$SRC" \
    -vf "scale=5760:5760:flags=lanczos" -pix_fmt yuv420p $COLOR \
    -c:v hevc_videotoolbox -b:v 45M -g 60 -tag:v hvc1 \
    $AUDIO -movflags +faststart _t3.mp4 -y
finish _t3.mp4 T3_5760_hevc_45M.mp4

# ---- T4: codec efficiency at the same budget ---------------------------------------
echo; echo ">>> T4  5760x5760 AV1 32 Mbps    [codec efficiency]"
ffmpeg -hide_banner -loglevel error -stats \
    -ss $SS -t $DUR -i "$SRC" \
    -vf "scale=5760:5760:flags=lanczos" -pix_fmt yuv420p $COLOR \
    -c:v libsvtav1 -preset 6 -b:v 32M -g 60 \
    $AUDIO -movflags +faststart _t4.mp4 -y
finish _t4.mp4 T4_5760_av1_32M.mp4

# ---- T5: does the 3S decode out-of-spec 8K at all? ---------------------------------
echo; echo ">>> T5  7680x7680 HEVC 45 Mbps   [out-of-spec L6.3 probe]"
ffmpeg -hide_banner -loglevel error -stats \
    -ss $SS -t $DUR -i "$SRC" \
    -pix_fmt yuv420p $COLOR \
    -c:v hevc_videotoolbox -b:v 45M -g 60 -tag:v hvc1 \
    $AUDIO -movflags +faststart _t5.mp4 -y
finish _t5.mp4 T5_7680_hevc_45M.mp4

echo; echo "=============================================="
echo " Done."
ls -lh "$OUT"/T*.mp4
