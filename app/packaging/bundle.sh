#!/bin/bash
# Build Serene VR Encoder.app
#
#   ./packaging/bundle.sh                  ad-hoc signed — runs on THIS Mac only
#   ./packaging/bundle.sh "Developer ID Application: Name (TEAMID)"
#                                          signed for distribution; notarize separately
#
# Gatekeeper blocks an ad-hoc-signed app on any other machine. To give this to someone
# else it must be signed with a Developer ID and notarized — see NOTARIZING.md.

set -euo pipefail
cd "$(dirname "$0")/.."

APP="Serene VR Encoder.app"
IDENTITY="${1:-}"
OUT="target/$APP"

echo "Building release binary…"
cargo build --release --quiet

echo "Assembling bundle…"
rm -rf "$OUT"
mkdir -p "$OUT/Contents/MacOS" "$OUT/Contents/Resources"
cp target/release/serene-vr-encoder "$OUT/Contents/MacOS/"

# PNG -> iconset -> icns. macOS wants every size baked in.
ICONSET="$(mktemp -d)/AppIcon.iconset"
mkdir -p "$ICONSET"
for s in 16 32 128 256 512; do
    sips -z $s $s packaging/icon.png --out "$ICONSET/icon_${s}x${s}.png" >/dev/null 2>&1
    d=$((s * 2))
    sips -z $d $d packaging/icon.png --out "$ICONSET/icon_${s}x${s}@2x.png" >/dev/null 2>&1
done
iconutil -c icns "$ICONSET" -o "$OUT/Contents/Resources/AppIcon.icns"

cat > "$OUT/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>               <string>Serene VR Encoder</string>
    <key>CFBundleDisplayName</key>        <string>Serene VR Encoder</string>
    <key>CFBundleIdentifier</key>         <string>com.thededman.serene-vr-encoder</string>
    <key>CFBundleVersion</key>            <string>0.1.0</string>
    <key>CFBundleShortVersionString</key> <string>0.1.0</string>
    <key>CFBundleExecutable</key>         <string>serene-vr-encoder</string>
    <key>CFBundleIconFile</key>           <string>AppIcon</string>
    <key>CFBundlePackageType</key>        <string>APPL</string>
    <key>LSMinimumSystemVersion</key>     <string>11.0</string>
    <key>NSHighResolutionCapable</key>    <true/>
    <key>CFBundleDocumentTypes</key>
    <array>
      <dict>
        <key>CFBundleTypeName</key>      <string>Video</string>
        <key>CFBundleTypeRole</key>      <string>Editor</string>
        <key>LSItemContentTypes</key>
        <array><string>public.movie</string><string>public.mpeg-4</string></array>
      </dict>
    </array>
</dict>
</plist>
PLIST

if [ -n "$IDENTITY" ]; then
    echo "Signing with Developer ID (hardened runtime)…"
    # Hardened runtime is mandatory for notarization.
    codesign --force --deep --options runtime --timestamp \
             --sign "$IDENTITY" "$OUT"
    codesign --verify --deep --strict --verbose=2 "$OUT"
    echo
    echo "Signed. Next: notarize it — see packaging/NOTARIZING.md"
else
    echo "Ad-hoc signing (this Mac only)…"
    codesign --force --deep --sign - "$OUT"
    echo
    echo "NOTE: ad-hoc signed. This will run here, but Gatekeeper will block it on any"
    echo "      other Mac. Pass a Developer ID identity to sign for distribution."
fi

echo
echo "Built: $(pwd)/$OUT"
du -sh "$OUT" | awk '{print "Size:  "$1}'
