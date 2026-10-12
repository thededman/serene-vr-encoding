#!/bin/bash
# Copy the A/B test clips onto a Quest 3S over USB.
#
# Requires adb (Android platform tools). If it is missing:
#     brew install android-platform-tools
#
# On the headset, developer mode must be on (Meta Horizon phone app ->
# Headset Settings -> Developer Mode) and the "Allow USB debugging" prompt accepted
# while wearing it.
#
# No adb? Plug the Quest in, accept the file-access prompt in the headset, and drag the
# tests/ folder across with Android File Transfer instead.

set -u

BASE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TESTS="${1:-$BASE/tests}"
DEST="/sdcard/Movies/SereneTests"

if ! command -v adb >/dev/null 2>&1; then
    echo "adb not found. Install with:  brew install android-platform-tools"
    exit 1
fi

echo "Waiting for headset (put it on and accept the USB debugging prompt)..."
adb wait-for-device

devices=$(adb devices | grep -c "device$")
if [ "$devices" -eq 0 ]; then
    echo "No authorised device. Check the headset for the USB debugging prompt."
    exit 1
fi

adb shell mkdir -p "$DEST"

total=$(ls -1 "$TESTS"/T*.mp4 2>/dev/null | wc -l | tr -d ' ')
if [ "$total" = "0" ]; then
    echo "No test clips found in $TESTS — run encode_tests.sh first."
    exit 1
fi

echo "Copying $total clips to $DEST ..."
n=0
for f in "$TESTS"/T*.mp4; do
    n=$((n + 1))
    echo "[$n/$total] $(basename "$f")  ($(du -h "$f" | cut -f1))"
    adb push "$f" "$DEST/" || echo "  !! push failed for $(basename "$f")"
done

echo
echo "Done. On the headset the clips are under Movies/SereneTests."
adb shell ls -lh "$DEST"
