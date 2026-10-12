#!/bin/bash
# Objectively rank the test clips by how faithfully they reproduce the MASTER, measured
# through the headset viewport rather than on the raw equirectangular frame.
#
# Comparing full equirect frames would be misleading: the poles occupy a huge share of the
# pixels but almost none of the viewer's attention. So both clip and master are reprojected
# to the exact view one eye sees on a Quest 3S, then compared frame by frame.
#
# Both sides are compared in-process, so there is no intermediate JPEG/PNG step to add
# compression noise of its own.

set -u
BASE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

SRC="${1:-}"
[ -n "$SRC" ] || { echo "usage: ./measure_quality.sh <master.mp4> [clips-dir]"; exit 1; }
[ -f "$SRC" ] || { echo "master not found: $SRC"; exit 1; }
SRC="$(cd "$(dirname "$SRC")" && pwd)/$(basename "$SRC")"   # absolute, we cd below

cd "${2:-$BASE/tests}" || { echo "no clips dir: ${2:-$BASE/tests}"; exit 1; }
CLIP_SS=20          # seconds into the clip
SRC_SS=320          # matching point in the master (clips start at 300s)
DUR=2
VIEW="v360=e:flat:yaw=0:h_fov=96:v_fov=90:w=1832:h=1920:interp=lanczos,format=yuv420p"

printf "%-34s %-9s %-9s %-9s\n" CLIP PSNR_dB SSIM MBPS
for f in T*.mp4; do
    h=$(ffprobe -v error -select_streams v:0 -show_entries stream=height -of default=nw=1:nk=1 "$f" | tr -dc '0-9')
    w=$(ffprobe -v error -select_streams v:0 -show_entries stream=width  -of default=nw=1:nk=1 "$f" | tr -dc '0-9')
    br=$(ffprobe -v error -select_streams v:0 -show_entries stream=bit_rate -of default=nw=1:nk=1 "$f" | tr -dc '0-9')
    eye=$((h / 2))

    # Master eye crop: top half if stereo-packed (1:1 frame), whole frame if mono.
    mw=$(ffprobe -v error -select_streams v:0 -show_entries stream=width  -of default=nw=1:nk=1 "$SRC" | tr -dc '0-9')
    mh=$(ffprobe -v error -select_streams v:0 -show_entries stream=height -of default=nw=1:nk=1 "$SRC" | tr -dc '0-9')
    [ "$mw" -eq "$mh" ] && mh=$((mh / 2))
    chain="[0:v]crop=${w}:${eye}:0:0,${VIEW}[a];[1:v]crop=${mw}:${mh}:0:0,${VIEW}[b]"

    p=$(ffmpeg -hide_banner -loglevel info -ss $CLIP_SS -t $DUR -i "$f" -ss $SRC_SS -t $DUR -i "$SRC" \
        -filter_complex "${chain};[a][b]psnr" -f null - 2>&1 | grep -o "average:[0-9.]*" | head -1 | cut -d: -f2)
    s=$(ffmpeg -hide_banner -loglevel info -ss $CLIP_SS -t $DUR -i "$f" -ss $SRC_SS -t $DUR -i "$SRC" \
        -filter_complex "${chain};[a][b]ssim" -f null - 2>&1 | grep -o "All:[0-9.]*" | head -1 | cut -d: -f2)

    mb=$(awk -v b="${br:-0}" 'BEGIN{printf "%.1f", b/1000000}')
    printf "%-34s %-9s %-9s %-9s\n" "${f%.mp4}" "${p:-ERR}" "${s:-ERR}" "$mb"
done
