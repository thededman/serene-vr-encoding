#!/bin/bash
# Full-length encode at the winning settings. Run this once the headset test picks a winner.
#
# DELIVERY presets — the file the headset actually plays. Small, tuned to the headset,
# and NOT suitable as input to another encoder.
#
#   ./encode_final.sh av1      master.mp4   # recommended — AV1 5760x5760 ~30 Mbps
#   ./encode_final.sh av1-low  master.mp4   # low bandwidth — AV1 5760x5760 ~25 Mbps
#   ./encode_final.sh hevc     master.mp4   # if the player cannot decode AV1 — HEVC 44 Mbps
#   ./encode_final.sh hevc-low master.mp4   # HEVC 5760x5760 32 Mbps
#
# MEZZANINE presets — for platforms that re-encode on import (e.g. the Serene admin's
# adaptive transcode). Never serve these to headsets; they exist to give the platform's
# encoder a high-quality, universally decodable source so its output does not suffer
# generation loss. Already at the right resolution and projection, so the platform
# re-compresses rather than also rescaling.
#
#   ./encode_final.sh mezzanine      master.mp4  # HEVC 5760x5760 100 Mbps — preferred
#   ./encode_final.sh mezzanine-h264 master.mp4  # H.264 150 Mbps — if HEVC is rejected
#
# Fixed 5760x5760 stereo output. For automatic per-file targets (mono, VR180, 60fps),
# use ./encode_vendor.sh instead.
#
# Output goes to Serene-VR-Encoding/final/ with spatial metadata injected and a faststart
# index, ready to upload to serene.precipiodx.com.

set -u

BASE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUT="$BASE/final"
INJ="$BASE/inject_spatial.py"

PRESET="${1:-}"
SRC="${2:-}"

if [ -z "$PRESET" ] || [ -z "$SRC" ]; then
    sed -n '2,23p' "$0" | sed 's/^# \{0,1\}//'
    exit 1
fi
[ -f "$SRC" ] || { echo "source not found: $SRC"; exit 1; }

# Output is named after the source so the preset works for any title.
STEM="$(basename "${SRC%.*}")"

case "$PRESET" in
    av1)      VCODEC=(-c:v libsvtav1 -preset 6 -b:v 45M); RES=5760; TAG="av1_34M" ;;
    av1-low)  VCODEC=(-c:v libsvtav1 -preset 6 -b:v 32M); RES=5760; TAG="av1_25M" ;;
    hevc)     VCODEC=(-c:v hevc_videotoolbox -b:v 45M -tag:v hvc1); RES=5760; TAG="hevc_45M" ;;
    hevc-low) VCODEC=(-c:v hevc_videotoolbox -b:v 32M -tag:v hvc1); RES=5760; TAG="hevc_32M" ;;
    mezzanine)      VCODEC=(-c:v hevc_videotoolbox -b:v 100M -tag:v hvc1); RES=5760; TAG="hevc_mezzanine_100M" ;;
    mezzanine-h264) VCODEC=(-c:v h264_videotoolbox -b:v 150M);             RES=5760; TAG="h264_mezzanine_150M" ;;
    *) echo "unknown preset '$PRESET' — use av1, av1-low, hevc, hevc-low, mezzanine or mezzanine-h264"; exit 1 ;;
esac

NAME="${STEM}_${RES}_${TAG}"

mkdir -p "$OUT"
cd "$OUT" || exit 1

echo "Encoding at preset '$PRESET' -> ${NAME}.mp4"
echo "Delivery presets take ~55 min; mezzanine presets ~30 min. Progress below."
echo

ffmpeg -hide_banner -loglevel error -stats \
    -i "$SRC" \
    -vf "scale=${RES}:${RES}:flags=lanczos" -pix_fmt yuv420p \
    -color_primaries bt709 -color_trc bt709 -colorspace bt709 \
    "${VCODEC[@]}" -g 60 \
    -c:a aac -b:a 192k -ac 2 \
    -movflags +faststart \
    "_${NAME}.mp4" -y || { echo "!! encode failed"; exit 1; }

echo
echo "Injecting 360/stereo metadata..."
python3 "$INJ" "_${NAME}.mp4" "${NAME}.mp4" --stereo=top-bottom && rm -f "_${NAME}.mp4"

echo
echo "=== verification ==="
ffprobe -v error -select_streams v:0 \
    -show_entries stream=codec_name,codec_tag_string,width,height,bit_rate \
    -of default=nw=1 "${NAME}.mp4"

n=$(ffprobe -v error -select_streams v:0 -show_streams "${NAME}.mp4" \
    | grep -cE "Spherical Mapping|Stereo 3D")
if [ "$n" = "2" ]; then
    echo "spatial metadata: OK (equirectangular + top-bottom)"
else
    echo "spatial metadata: !! MISSING — do not upload, the app will show it flat"
fi

# The index must sit before the media data or start-up over CloudFront suffers.
first=$(python3 -c "
import struct,sys
f=open('${NAME}.mp4','rb'); off=0
while True:
    f.seek(off); h=f.read(16)
    if len(h)<8: break
    s=struct.unpack('>I',h[0:4])[0]; t=h[4:8].decode('latin1','replace')
    if s==1: s=struct.unpack('>Q',h[8:16])[0]
    if t in ('moov','mdat'): print(t); break
    if s<=0: break
    off+=s
")
[ "$first" = "moov" ] && echo "faststart: OK (index before media)" \
                      || echo "faststart: !! index is after the media — fix before upload"

echo
ls -lh "${NAME}.mp4"
echo
echo "Ready to upload to serene.precipiodx.com"
