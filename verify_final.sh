#!/bin/bash
# Pre-upload verification for the full-length encode.
#
# The A/B test only covered one 60-second segment. This checks the finished file end to
# end: structural correctness (metadata, index, codec), then measured quality at several
# points across the full running time — so a demanding stretch that starves the encoder
# cannot slip through unnoticed.
#
#   ./verify_final.sh encoded.mp4                # structural checks only
#   ./verify_final.sh encoded.mp4 master.mp4     # also measures quality vs the master
#
# Without a master the structural checks still run; only the quality comparison is skipped.

set -u

BASE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FILE="${1:-}"
SRC="${2:-}"

[ -n "$FILE" ] || { echo "usage: ./verify_final.sh <encoded.mp4> [master.mp4]"; exit 1; }
[ -f "$FILE" ] || { echo "not found: $FILE"; exit 1; }
if [ -n "$SRC" ] && [ ! -f "$SRC" ]; then echo "master not found: $SRC"; exit 1; fi

echo "=============================================="
echo " Pre-upload verification"
echo " $(basename "$FILE")"
echo "=============================================="
echo

fail=0

# ---- 1. stream properties ---------------------------------------------------------
echo "--- stream ---"
ffprobe -v error -select_streams v:0 \
    -show_entries stream=codec_name,codec_tag_string,width,height,r_frame_rate,pix_fmt,bit_rate \
    -of default=nw=1 "$FILE"
dur=$(ffprobe -v error -show_entries format=duration -of default=nw=1:nk=1 "$FILE" | cut -d. -f1)
echo "duration=${dur}s"
echo "size=$(du -h "$FILE" | cut -f1)"
echo

# ---- 2. spatial metadata ----------------------------------------------------------
echo "--- 360 / stereo metadata ---"
meta=$(ffprobe -v error -select_streams v:0 -show_streams "$FILE" | grep -cE "Spherical Mapping|Stereo 3D")
if [ "$meta" = "2" ]; then
    layout=$(ffprobe -v error -select_streams v:0 -show_streams "$FILE" \
             | grep -A1 "Stereo 3D" | grep -oE "top and bottom|side by side|2D" | head -1)
    echo "OK  equirectangular + ${layout:-stereo layout} present"
else
    echo "FAIL  metadata missing — the app will show this flat. Do not upload."
    fail=1
fi
echo

# ---- 3. faststart -----------------------------------------------------------------
echo "--- index position ---"
first=$(python3 -c "
import struct
f=open('$FILE','rb'); off=0
while True:
    f.seek(off); h=f.read(16)
    if len(h)<8: break
    s=struct.unpack('>I',h[0:4])[0]; t=h[4:8].decode('latin1','replace')
    if s==1: s=struct.unpack('>Q',h[8:16])[0]
    if t in ('moov','mdat'): print(t); break
    if s<=0: break
    off+=s
")
if [ "$first" = "moov" ]; then
    echo "OK  index before media (fast start-up over CloudFront)"
else
    echo "FAIL  index after media — start-up will be slow. Do not upload."
    fail=1
fi
echo

# ---- 4. decodes cleanly end to end -------------------------------------------------
echo "--- full decode pass (this takes a few minutes) ---"
if ffmpeg -v error -i "$FILE" -f null - 2>/tmp/serene_decode_err.txt; then
    echo "OK  decodes cleanly, no corrupt frames"
else
    echo "FAIL  decode errors:"; head -5 /tmp/serene_decode_err.txt; fail=1
fi
echo

# ---- 5. quality across the whole running time --------------------------------------
# Measured through the Quest 3S eye view, same method used to pick the winner.
if [ -z "$SRC" ]; then
    echo "--- quality vs master ---"
    echo "skipped (pass a master as the second argument to measure quality)"
    echo
else
echo "--- quality vs master, sampled across the full duration ---"
VIEW="v360=e:flat:yaw=0:h_fov=96:v_fov=90:w=1832:h=1920:interp=lanczos,format=yuv420p"

# One eye of each file: the top half of a stereo-packed frame, or the whole frame if mono.
# Derived per file so this works for stereo and mono masters alike.
eye_crop () {
    local w h st
    w=$(ffprobe -v error -select_streams v:0 -show_entries stream=width  -of default=nw=1:nk=1 "$1" | tr -dc '0-9')
    h=$(ffprobe -v error -select_streams v:0 -show_entries stream=height -of default=nw=1:nk=1 "$1" | tr -dc '0-9')
    st=$(ffprobe -v error -select_streams v:0 -show_streams "$1" | grep -c "top and bottom" || true)
    if [ "${st:-0}" -gt 0 ] || [ "$w" -eq "$h" ]; then h=$((h / 2)); fi
    echo "crop=${w}:${h}:0:0"
}
CROP_A=$(eye_crop "$FILE")
CROP_B=$(eye_crop "$SRC")

printf "%-12s %-10s %-10s\n" AT PSNR_dB SSIM
for t in 60 240 420 600 780 900; do
    [ "$t" -ge "${dur:-0}" ] && continue
    chain="[0:v]${CROP_A},${VIEW}[a];[1:v]${CROP_B},${VIEW}[b]"
    p=$(ffmpeg -hide_banner -loglevel info -ss $t -t 1 -i "$FILE" -ss $t -t 1 -i "$SRC" \
        -filter_complex "${chain};[a][b]psnr" -f null - 2>&1 | grep -o "average:[0-9.]*" | head -1 | cut -d: -f2)
    s=$(ffmpeg -hide_banner -loglevel info -ss $t -t 1 -i "$FILE" -ss $t -t 1 -i "$SRC" \
        -filter_complex "${chain};[a][b]ssim" -f null - 2>&1 | grep -o "All:[0-9.]*" | head -1 | cut -d: -f2)
    printf "%-12s %-10s %-10s\n" "${t}s" "${p:-n/a}" "${s:-n/a}"
done
echo
echo "For reference the winning 60s test clip measured PSNR 46.09 / SSIM 0.9882."
echo "Any sample far below that marks a stretch the encoder found hard."
echo
fi

echo "=============================================="
if [ "$fail" = "0" ]; then
    echo " PASSED — ready to upload to serene.precipiodx.com"
else
    echo " FAILED — see above. Do not upload."
fi
echo "=============================================="
exit $fail
