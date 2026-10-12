# Serene VR Encoding

A toolkit for preparing 360°/VR video for streaming to Meta Quest headsets — encoding to a
standard measured against the actual device rather than vendor guidance.

**Results on real content: 56 GB → 5.6 GB across four titles, at better measured image
quality than the originals.**

📖 **[Read the full write-up →](https://thededman.github.io/serene-vr-encoding/)**

---

## Why this exists

VR video looked soft in the headset. The cause turned out to be two unrelated faults in the
source files, both invisible without a headset and measurement:

- An 8K master encoded at **251 Mbps against the headset's 220 Mbps hardware decode
  ceiling**, with a nonstandard H.264 level. The decoder can't keep up, so playback falls
  back to a degraded path.
- A **mono 360° video mislabelled as top-bottom stereo**. A player honouring that tag splits
  one image in half and sends unrelated halves to each eye — permanent, unfocusable blur.
  (PSNR between the halves: 7.1 dB. Genuine stereo measures ~25 dB.)

Both are the kind of defect that ships from a vendor and survives every review that doesn't
involve putting a headset on.

## Two findings worth knowing

**Resolution is the wrong unit — use pixels per degree.** A flat 4K video puts 3840 pixels
across the screen. A 360° video spreads those same pixels around the viewer's entire head,
and the headset shows only ~96° at a time. So 4K 360° delivers **10.7 px/deg** against a
Quest 3S's ~19.1 — about half the detail the display can resolve. This is why an adaptive
ladder designed for flat video under-serves VR.

**At a fixed bitrate, more pixels make quality worse.** In a seven-way A/B on the headset,
an 8K encode at 45 Mbps scored *lowest of all seven*. Spread across 59 megapixels, every
pixel is starved. If something looks soft, the lever is bitrate or codec — not resolution.

## Quick start

```bash
brew install ffmpeg          # must include libsvtav1
./check_setup.sh             # verifies this machine can run the toolkit

./inspect_master.sh /path/to/masters/     # read-only: what are these files?
./encode_vendor.sh  /path/to/masters/     # encode the batch
./verify_final.sh   final/output.mp4      # check before uploading
```

Full walkthrough: **[HOW-TO-CONVERT-VIDEOS.md](HOW-TO-CONVERT-VIDEOS.md)**
The standard and the device limits behind it: **[docs/ENCODING-GUIDE.md](docs/ENCODING-GUIDE.md)**

## The standard

**AV1 · ~16 pixels per degree · ≤35 Mbps · faststart · spatial metadata re-injected**

| Source format | Output | Bitrate |
|---|---|---|
| 360° stereo top-bottom, 30 fps | 5760×5760 | 34 Mbps |
| 360° stereo top-bottom, 60 fps | 5152×5152 | 35 Mbps *(capped)* |
| 360° mono, 30 fps | 5760×2880 | 17 Mbps |
| VR180 stereo side-by-side, 30 fps | 5760×2880 | 17 Mbps |

Targets are derived from the decoder limits read off the device itself
(`/vendor/etc/media_codecs.xml`): max frame 8192×8192, throughput 7,776,000 blocks/s,
max bitrate 160 Mbps for AV1/HEVC. Sources already below target are never upscaled.

## What's in here

| Tool | Purpose |
|---|---|
| `check_setup.sh` | Verify the machine can run the toolkit (incl. the libsvtav1 check) |
| `inspect_master.sh` | Dry run — identify each master, show its computed target |
| `encode_vendor.sh` | Batch encode, auto-detecting each file's format |
| `encode_final.sh` | Single file at a fixed preset, incl. mezzanine presets |
| `verify_final.sh` | Pre-upload QC — metadata, index, clean decode, measured quality |
| `check_stream.sh` | Measure what a platform *actually delivers*, from its HLS manifest |
| `vr_plan.py` | Format detection and target calculation |
| `inject_spatial.py` | Restore the 360 metadata FFmpeg strips from every output |
| `play.sh` / `sideload.sh` | Push clips to a Quest and launch them from the Mac |

Plus the research harness (`encode_tests.sh`, `measure_quality.sh`,
`compare_viewports.sh`) used to derive the standard — useful if the targets ever change.

## Two gotchas this handles

**FFmpeg discards 360/stereo metadata on every output, including `-c copy`.** Without it a
player shows a flat, doubled image instead of a sphere. `inject_spatial.py` restores both
metadata generations (Google Spherical V1 XML + V2 `sv3d`/`st3d`), dependency-free.

**A square frame with no stereo tag is ambiguous** — 360° stereo top-bottom, or VR180 mono?
The tool checks declared layout against actual frame geometry and flags mismatches rather
than guessing. That check is what caught the mislabelled file above.

## Requirements

macOS with ffmpeg (including `libsvtav1`) and python3. No third-party Python packages.
`adb` is optional, for headset testing.

**Memory matters:** the encoder peaks at ~9 GB for mono and ~16 GB for stereo output.
32 GB+ is comfortable; on 16 GB, stereo encodes swap and crawl. `check_setup.sh` tells you
which your machine can handle.

## Prior art

The individual techniques here are established: viewport-based quality measurement is
standardised in JVET's 360Lib (WS-PSNR, S-PSNR, viewport PSNR), spatial metadata injection
has existing tools, and pixels-per-degree is ordinary display engineering. What this adds
is the integration — targets derived from measured device limits, format auto-detection,
metadata repair and verification gates in one pipeline that refuses to guess when a file
looks wrong.
