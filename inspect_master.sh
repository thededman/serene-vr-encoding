#!/bin/bash
# Inspect vendor masters and report what each one is and how it will be encoded.
# Read-only — run this before committing to a batch of long encodes.
#
#   ./inspect_master.sh /path/to/masters/
#   ./inspect_master.sh one_file.mp4

set -u
BASE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TARGET="${1:-.}"

if [ -d "$TARGET" ]; then
    files=$(find "$TARGET" -maxdepth 1 -type f \( -iname '*.mp4' -o -iname '*.mov' -o -iname '*.mkv' \) | sort)
else
    files="$TARGET"
fi

[ -z "$files" ] && { echo "No video files found in $TARGET"; exit 1; }

printf "%-38s %-13s %-5s %-8s %-6s %-13s %-7s %s\n" \
    FILE SOURCE FPS LAYOUT FOV "-> OUTPUT" MBPS STATUS
printf '%.0s-' {1..118}; echo

review=0; unfit=0
while IFS= read -r f; do
    [ -z "$f" ] && continue
    eval "$("$BASE/vr_plan.py" "$f" 2>/dev/null | sed 's/^/P_/')" || {
        printf "%-38s  could not probe\n" "$(basename "$f" | cut -c1-37)"; continue; }

    name=$(basename "$f")
    [ ${#name} -gt 37 ] && name="${name:0:34}..."

    if [ "${P_STATUS:-}" = "unfit" ]; then
        out="-"; mbps="-"; unfit=$((unfit+1))
    else
        out="${P_OUT_W}x${P_OUT_H}"; mbps="${P_MBPS}"
        [ "${P_STATUS:-}" = "review" ] && review=$((review+1))
    fi

    printf "%-38s %-13s %-5s %-8s %-6s %-13s %-7s %s\n" \
        "$name" "${P_WIDTH}x${P_HEIGHT}" "${P_FPS}" "${P_LAYOUT}" "${P_DEGREES}" \
        "$out" "$mbps" "${P_STATUS}"

    [ -n "${P_NOTE:-}" ] && echo "      note: ${P_NOTE}"
done <<< "$files"

echo
[ "$unfit"  -gt 0 ] && echo "$unfit file(s) cannot be handled automatically — see notes above."
[ "$review" -gt 0 ] && echo "$review file(s) need a decision — see notes above."
echo "Encode with:  ./encode_vendor.sh $TARGET"
