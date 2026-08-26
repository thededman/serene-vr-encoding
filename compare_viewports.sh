#!/bin/bash
#   ./compare_viewports.sh [clips-dir]   (defaults to ./tests)
#
# Render the identical headset viewport from each test clip so they can be compared on the
# Mac before putting the headset on.
#
# Each output is what ONE EYE actually sees on a Quest 3S: the left eye (top half of the
# stereo frame) reprojected from equirectangular to a flat 96x90 degree view, rendered at
# the 3S panel resolution of 1832x1920. Viewed 1:1 these are a fair proxy for in-headset
# sharpness.

set -u
BASE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${1:-$BASE/tests}" || { echo "no clips dir: ${1:-$BASE/tests}"; exit 1; }
mkdir -p viewports

FOV_H=96      # Quest 3S horizontal field of view
FOV_V=90
PANEL_W=1832  # Quest 3S per-eye panel resolution
PANEL_H=1920
AT=20         # seconds into the clip

for f in T*.mp4; do
    name="${f%.mp4}"
    # Frame is top-bottom stereo, so the left eye is the top half: full width, half height.
    h=$(ffprobe -v error -select_streams v:0 -show_entries stream=height -of default=nw=1:nk=1 "$f" | tr -dc '0-9')
    w=$(ffprobe -v error -select_streams v:0 -show_entries stream=width  -of default=nw=1:nk=1 "$f" | tr -dc '0-9')
    eye_h=$((h / 2))

    for yaw in 0 90; do
        ffmpeg -hide_banner -loglevel error -ss $AT -i "$f" -frames:v 1 \
            -vf "crop=${w}:${eye_h}:0:0,v360=e:flat:yaw=${yaw}:h_fov=${FOV_H}:v_fov=${FOV_V}:w=${PANEL_W}:h=${PANEL_H}:interp=lanczos" \
            -q:v 2 "viewports/${name}_yaw${yaw}.jpg" -y
    done
    echo "  rendered $name"
done

echo
echo "Viewports in: $(pwd)/viewports"
ls -1 viewports
