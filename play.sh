#!/bin/bash
# Play a test clip on the headset directly, skipping the Files/Gallery UI entirely.
#
#   ./play.sh T6           # play the recommended London clip
#   ./play.sh animals_6848 # any substring of the filename works
#   ./play.sh T3        # switch to the HEVC fallback
#   ./play.sh           # list what is on the headset
#
# Sideloaded videos land in the Gallery app (com.oculus.hzosgallery), NOT the Files app —
# which is why they appear "missing" when browsing Files. This looks the clip up by name
# in the media database and fires it straight at the player.

set -u
export PATH="$PATH:/opt/homebrew/bin"

if ! command -v adb >/dev/null 2>&1; then
    echo "adb not found. Install with:  brew install android-platform-tools"
    exit 1
fi

# Pull ids and names as two parallel lists (the content command cannot project both).
ids=$(adb shell 'content query --uri content://media/external/video/media --projection _id' 2>/dev/null \
      | sed -n 's/.*_id=\([0-9]*\).*/\1/p')
names=$(adb shell 'content query --uri content://media/external/video/media --projection _display_name' 2>/dev/null \
      | sed -n 's/.*_display_name=\(.*\)$/\1/p' | tr -d '\r')

if [ -z "$ids" ]; then
    echo "No videos found on the headset. Is it connected and unlocked?"
    exit 1
fi

if [ $# -eq 0 ]; then
    echo "Clips on the headset:"
    paste <(echo "$names") <(echo "$ids") | while IFS=$'\t' read -r n i; do
        printf "   %-34s (id %s)\n" "$n" "$i"
    done
    echo
    echo "Play one with:  ./play.sh T6"
    exit 0
fi

want="$1"
# Substring match, case-insensitive — so "animals", "6848" or "T6" all work.
line=$(paste <(echo "$names") <(echo "$ids") | grep -i -- "$want" | head -1)

if [ -z "$line" ]; then
    echo "No clip matching '$want'. Run ./play.sh with no arguments to list them."
    exit 1
fi

name=$(echo "$line" | cut -f1)
id=$(echo "$line" | cut -f2)

echo "Playing: $name"
adb shell am start -a android.intent.action.VIEW \
    -d "content://media/external/video/media/${id}" -t video/mp4 >/dev/null 2>&1

echo "Put the headset on — it should be playing in the Gallery viewer."
echo "If it opens flat instead of as a 360 sphere, the player is ignoring the"
echo "spatial metadata; note that in RESULTS.md, it matters for the app test."
