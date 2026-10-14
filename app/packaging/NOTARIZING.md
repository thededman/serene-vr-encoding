# Signing and notarizing the app

An ad-hoc signed build runs on the Mac that built it and **nowhere else** — Gatekeeper
blocks it everywhere else. To hand the app to someone, it has to be signed with a
Developer ID and notarized by Apple.

## What you need first

- An **Apple Developer account** ($99/year). First-time enrolment can take a few days, so
  start it before you need it.
- A **Developer ID Application** certificate, created in Xcode
  (Settings → Accounts → Manage Certificates → +) or on the developer portal.
- An **app-specific password** for your Apple ID (appleid.apple.com → Sign-In and
  Security → App-Specific Passwords). Not your account password.

Find your identity's exact name:

```bash
security find-identity -v -p codesigning
```

## Store the credentials once

```bash
xcrun notarytool store-credentials "serene-notary" \
  --apple-id "you@example.com" \
  --team-id "YOURTEAMID" \
  --password "abcd-efgh-ijkl-mnop"
```

## Build, sign, notarize, staple

```bash
# 1. Build signed with hardened runtime (mandatory for notarization)
./packaging/bundle.sh "Developer ID Application: Your Name (YOURTEAMID)"

# 2. Zip for submission — notarytool takes a zip, dmg or pkg, not a bare .app
cd target && ditto -c -k --keepParent "Serene VR Encoder.app" SereneVREncoder.zip

# 3. Submit and wait (usually a few minutes)
xcrun notarytool submit SereneVREncoder.zip --keychain-profile "serene-notary" --wait

# 4. Staple the ticket to the .app so it validates offline
xcrun stapler staple "Serene VR Encoder.app"
```

## Check it before sending

```bash
codesign --verify --deep --strict --verbose=2 "Serene VR Encoder.app"
spctl -a -vvv -t install "Serene VR Encoder.app"     # expect: accepted, source=Notarized
xcrun stapler validate "Serene VR Encoder.app"
```

The real test is opening it on a Mac that never built it.

## If notarization is rejected

```bash
xcrun notarytool log <submission-id> --keychain-profile "serene-notary"
```

The usual causes: hardened runtime missing (`--options runtime`), no secure timestamp
(`--timestamp`), or a nested binary signed after the outer bundle. Sign inside-out —
nested code first, the `.app` last.

## Without a Developer account

The app still works; it just can't be handed over cleanly. The recipient must
right-click → Open and confirm once, or run:

```bash
xattr -dr com.apple.quarantine "/Applications/Serene VR Encoder.app"
```

Workable for one colleague, not something to put in a README for other people.

## Note on ffmpeg

The app does not bundle ffmpeg — it calls the one on `PATH` and shows an install message
if it is missing. That keeps the bundle at ~6 MB and avoids inheriting ffmpeg's GPL terms.
If ffmpeg is ever bundled, the licensing has to be worked through first.
