#!/bin/bash
# Report what the Serene platform ACTUALLY delivers for a title, rather than what was
# uploaded. Answers the question the admin UI cannot: is the ladder capped at 4K?
#
#   ./check_stream.sh "https://.../playlist.m3u8"
#
# For 360 video the meaningful number is not resolution but PIXELS PER DEGREE, because the
# frame wraps the full 360 degrees and the headset shows only ~96 of them at a time.
# A Quest 3S resolves ~19.1 px/deg. Anything under ~14 will look soft no matter the bitrate.

set -u
URL="${1:-}"
[ -z "$URL" ] && { echo "usage: ./check_stream.sh <master .m3u8 URL>"; exit 1; }

echo "Fetching manifest..."
man=$(curl -fsSL "$URL") || { echo "Could not fetch. Check the URL and that it is publicly reachable."; exit 1; }

if ! grep -q "EXT-X-STREAM-INF" <<<"$man"; then
    echo
    echo "No variant streams found — this is not a master playlist."
    if grep -q "EXTINF" <<<"$man"; then
        echo "It looks like a media playlist (a single rendition). Use the master .m3u8,"
        echo "usually the URL the player is given first."
    fi
    exit 1
fi

echo
printf "%-14s %-12s %-10s %-9s %s\n" RESOLUTION BANDWIDTH CODECS PX/DEG VERDICT
printf '%.0s-' {1..70}; echo

grep "EXT-X-STREAM-INF" <<<"$man" | while IFS= read -r line; do
    res=$(sed -n 's/.*RESOLUTION=\([0-9x]*\).*/\1/p' <<<"$line")
    bw=$(sed -n 's/.*[^-]BANDWIDTH=\([0-9]*\).*/\1/p' <<<"$line" | head -1)
    cod=$(sed -n 's/.*CODECS="\([^"]*\)".*/\1/p' <<<"$line" | cut -c1-9)
    w=${res%x*}

    if [ -n "$w" ] && [ "$w" -gt 0 ] 2>/dev/null; then
        pxdeg=$(awk -v w="$w" 'BEGIN{printf "%.1f", w/360}')
        verdict=$(awk -v p="$pxdeg" 'BEGIN{
            if (p>=15) print "good";
            else if (p>=12) print "acceptable";
            else print "SOFT - below headset capability"}')
    else
        pxdeg="?"; verdict="?"
    fi

    mbps=$(awk -v b="${bw:-0}" 'BEGIN{printf "%.1f Mbps", b/1000000}')
    printf "%-14s %-12s %-10s %-9s %s\n" "${res:-?}" "$mbps" "${cod:-?}" "$pxdeg" "$verdict"
done

echo
echo "Reference: Quest 3S resolves ~19.1 px/deg. Our house standard (5760 wide) gives 16.0."
echo "A 4K rung (3840 wide) gives only 10.7 — about half what the headset can show."
