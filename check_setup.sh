#!/bin/bash
# Verify this Mac can run the Serene VR encoding toolkit.
# Run this first — it catches the setup problems that otherwise show up mid-encode.

set -u
ok=0

say () { printf "  %-22s %s\n" "$1" "$2"; }

echo "=============================================="
echo " Serene VR toolkit — setup check"
echo "=============================================="
echo

# --- hardware ------------------------------------------------------------------------
CPU=$(sysctl -n machdep.cpu.brand_string 2>/dev/null || echo "unknown")
RAM=$(sysctl -n hw.memsize 2>/dev/null | awk '{printf "%.0f GB", $1/1024/1024/1024}')
echo "Machine"
say "CPU" "$CPU"
say "RAM" "${RAM:-unknown}"
case "$CPU" in
    *Apple*) say "encode speed" "Apple Silicon — expect ~0.3-0.5x realtime at 5760x5760" ;;
    *)       say "encode speed" "Intel — AV1 encoding will be SEVERAL TIMES slower;
                         budget hours per title, or use PRESET=8" ;;
esac
echo

# --- required ------------------------------------------------------------------------
echo "Required"
if command -v ffmpeg >/dev/null 2>&1; then
    say "ffmpeg" "$(ffmpeg -version 2>/dev/null | head -1 | cut -d' ' -f1-3)"
else
    say "ffmpeg" "MISSING  ->  brew install ffmpeg"; ok=1
fi

if command -v ffprobe >/dev/null 2>&1; then
    say "ffprobe" "found"
else
    say "ffprobe" "MISSING  ->  brew install ffmpeg"; ok=1
fi

# Every encode depends on this, and some ffmpeg builds ship without it. This is the
# single most likely reason the toolkit fails on a new machine.
if command -v ffmpeg >/dev/null 2>&1; then
    if ffmpeg -hide_banner -encoders 2>/dev/null | grep -q libsvtav1; then
        say "libsvtav1 (AV1)" "found"
    else
        say "libsvtav1 (AV1)" "MISSING — this ffmpeg cannot encode AV1.
                         Fix:  brew uninstall ffmpeg && brew install ffmpeg
                         Or use the HEVC presets instead (lower quality per bit)."
        ok=1
    fi
    if ffmpeg -hide_banner -encoders 2>/dev/null | grep -q hevc_videotoolbox; then
        say "hevc_videotoolbox" "found (hardware HEVC, used by mezzanine presets)"
    else
        say "hevc_videotoolbox" "missing — mezzanine presets unavailable"
    fi
fi

if command -v python3 >/dev/null 2>&1; then
    say "python3" "$(python3 --version 2>&1 | cut -d' ' -f2) (no extra packages needed)"
else
    say "python3" "MISSING  ->  brew install python"; ok=1
fi
echo

# --- optional ------------------------------------------------------------------------
echo "Optional — only for testing on a headset"
if command -v adb >/dev/null 2>&1; then
    n=$(adb devices 2>/dev/null | tail -n +2 | grep -c "device$")
    say "adb" "found — $n headset(s) connected"
else
    say "adb" "not installed  ->  brew install android-platform-tools
                         Only needed for sideload.sh and play.sh."
fi
echo

# --- disk ----------------------------------------------------------------------------
FREE=$(df -h . | tail -1 | awk '{print $4}')
echo "Disk"
say "free here" "$FREE"
say "guidance" "allow ~4 GB per 15-minute title, plus room for the masters"
echo

echo "=============================================="
if [ "$ok" = "0" ]; then
    echo " READY — see README.md for the workflow"
else
    echo " NOT READY — install the items marked MISSING above"
fi
echo "=============================================="
exit $ok
