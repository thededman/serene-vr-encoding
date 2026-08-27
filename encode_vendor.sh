#!/bin/bash
# Batch-encode vendor VR masters to the Serene house standard.
#
#   ./encode_vendor.sh /path/to/masters/          # whole folder
#   ./encode_vendor.sh master.mp4                 # one file
#   PRESET=8 ./encode_vendor.sh /path/masters/    # ~2x faster, small quality cost
#   MAX_MBPS=45 ./encode_vendor.sh /path/masters/ # raise the streaming budget
#
# For each file: works out the right target from its actual format (vr_plan.py), encodes
# to AV1, restores the 360/stereo metadata FFmpeg strips, and writes a faststart index.
# Files it cannot handle safely are skipped and listed at the end rather than guessed at.
#
# Output: Serene-VR-Encoding/final/<name>_<WxH>_av1_<N>M.mp4

set -u

BASE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUT="$BASE/final"
PRESET="${PRESET:-6}"
MAX_MBPS="${MAX_MBPS:-35}"
# Mono content uses half the pixels of stereo, so it can afford a higher pixel density
# within the same budget. 19 px/deg matches the Quest 3S panel exactly.
PX_PER_DEG="${PX_PER_DEG:-16}"
TARGET="${1:-.}"

mkdir -p "$OUT"

if [ -d "$TARGET" ]; then
    files=$(find "$TARGET" -maxdepth 1 -type f \( -iname '*.mp4' -o -iname '*.mov' -o -iname '*.mkv' \) | sort)
else
    files="$TARGET"
fi
[ -z "$files" ] && { echo "No video files found in $TARGET"; exit 1; }

echo "=============================================="
echo " Serene house encode — AV1 preset $PRESET, max ${MAX_MBPS} Mbps"
echo "=============================================="

skipped=""; done_list=""

while IFS= read -r f; do
    [ -z "$f" ] && continue
    name=$(basename "${f%.*}")

    eval "$("$BASE/vr_plan.py" "$f" --max-mbps "$MAX_MBPS" --px-per-deg "$PX_PER_DEG" 2>/dev/null | sed 's/^/P_/')" || {
        skipped="${skipped}\n  $name — could not probe"; continue; }

    if [ "${P_STATUS:-unfit}" = "unfit" ]; then
        skipped="${skipped}\n  $name — ${P_NOTE}"
        echo; echo ">>> SKIP $name : ${P_NOTE}"
        continue
    fi

    dest="$OUT/${name}_${P_OUT_W}x${P_OUT_H}_av1_${P_MBPS}M.mp4"
    tmp="$OUT/_${name}.mp4"

    echo
    echo ">>> $name"
    echo "    ${P_WIDTH}x${P_HEIGHT} ${P_CODEC} ${P_FPS}fps ${P_LAYOUT}/${P_DEGREES}deg"
    echo "    -> ${P_OUT_W}x${P_OUT_H} AV1 @ ${P_MBPS} Mbps"
    [ -n "${P_NOTE:-}" ] && echo "    note: ${P_NOTE}"

    if [ -f "$dest" ]; then
        echo "    already encoded, skipping"
        done_list="${done_list}\n  $(basename "$dest")"
        continue
    fi

    # Colour tagging is carried across explicitly; an untagged output can shift in-headset.
    if ! ffmpeg -nostdin -hide_banner -loglevel error -stats -i "$f" \
        -vf "scale=${P_OUT_W}:${P_OUT_H}:flags=lanczos" -pix_fmt yuv420p \
        -color_primaries bt709 -color_trc bt709 -colorspace bt709 \
        -c:v libsvtav1 -preset "$PRESET" -b:v "${P_MBPS}M" -g $((P_FPS * 2)) \
        -c:a aac -b:a 192k -ac 2 \
        -movflags +faststart "$tmp" -y; then
        echo "    !! encode failed"
        skipped="${skipped}\n  $name — encode failed"
        rm -f "$tmp"; continue
    fi

    # FFmpeg drops the spatial metadata on every output; without this the app shows a
    # flat image instead of a sphere.
    if ! python3 "$BASE/inject_spatial.py" "$tmp" "$dest" --stereo="${P_STEREO_FLAG}"; then
        echo "    !! metadata injection failed"
        skipped="${skipped}\n  $name — metadata injection failed"
        continue
    fi
    rm -f "$tmp"

    n=$(ffprobe -v error -select_streams v:0 -show_streams "$dest" \
        | grep -cE "Spherical Mapping|Stereo 3D")
    if [ "${P_STEREO_FLAG}" = "mono" ]; then ok=1; else ok=2; fi
    [ "$n" -ge 1 ] && echo "    metadata OK" || echo "    !! metadata MISSING — do not upload"

    echo "    $(du -h "$dest" | cut -f1)"
    done_list="${done_list}\n  $(basename "$dest")"
done <<< "$files"

echo
echo "=============================================="
echo " Encoded:"; printf "%b\n" "$done_list"
if [ -n "$skipped" ]; then
    echo
    echo " Skipped — handle these by hand:"; printf "%b\n" "$skipped"
fi
echo
echo " Verify each before uploading:  ./verify_final.sh final/<file>"
echo "=============================================="
