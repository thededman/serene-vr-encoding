# Serene VR — video encoding toolkit

> **Converting videos? Start here:** [HOW-TO-CONVERT-VIDEOS.md](HOW-TO-CONVERT-VIDEOS.md) — the complete walkthrough, setup to headset check.

Prepares 360°/VR video for streaming to Meta Quest headsets through the Serene platform.

Built after Serene's VR videos were reported as "fuzzy" in the headset. The cause turned
out to be two unrelated faults in the source files, both now understood and fixable. This
toolkit encodes to a standard measured against the actual hardware — see
[`ENCODING-GUIDE.md`](docs/ENCODING-GUIDE.md) for the standard and
[`RESULTS.md`](docs/RESULTS.md) for the evidence behind it.

**Target device:** Meta Quest 3S · **Streaming budget:** ~25–35 Mbps

---

## Setup

```bash
brew install ffmpeg          # must include libsvtav1 — check_setup.sh verifies this
./check_setup.sh
```

`check_setup.sh` reports anything missing with the exact command to fix it. Only `ffmpeg`
and `python3` are required; `adb` is optional and used only for headset testing. The Python
tools need no third-party packages.

## Quick start

```bash
./inspect_master.sh /path/to/masters/     # read-only: what are these files, what will we do?
./encode_vendor.sh  /path/to/masters/     # encode the batch
./verify_final.sh   final/output.mp4      # check before uploading
```

Always run `inspect_master.sh` first. It is read-only, takes seconds, and tells you what
each file actually is and what it will become — worth knowing before starting encodes that
run for tens of minutes each.

## What each tool does

**Production pipeline**

| Tool | Purpose |
|---|---|
| `check_setup.sh` | Verify this Mac can run the toolkit |
| `inspect_master.sh` | Dry run — identify each master and show its computed target |
| `encode_vendor.sh` | Batch encode to the house standard, auto-detecting each file's format |
| `encode_final.sh` | Single file at a fixed preset, incl. **mezzanine** presets for platforms that re-encode on import |
| `verify_final.sh` | Pre-upload QC — metadata, index position, clean decode, measured quality |
| `check_stream.sh` | Measure what a platform *actually delivers*, from its HLS manifest |
| `vr_plan.py` | Format detection and target calculation (shared by the scripts above) |
| `inject_spatial.py` | Restore the 360/stereo metadata FFmpeg strips from every output |

**Headset testing** (needs `adb`)

| Tool | Purpose |
|---|---|
| `sideload.sh` | Copy clips to a connected Quest |
| `play.sh` | Launch a clip on the headset from the Mac, skipping the UI |

**Research harness** — how the standard was derived; useful if targets ever change

| Tool | Purpose |
|---|---|
| `encode_tests.sh` | Build a set of A/B test clips from one master |
| `measure_quality.sh` | Score those clips against the master through the headset viewport |
| `compare_viewports.sh` | Render the exact Quest 3S eye view from each clip for side-by-side comparison |

## The standard, in brief

**AV1 · ~16 pixels per degree · ≤35 Mbps · faststart · metadata re-injected**

| Source format | Output | Bitrate |
|---|---|---|
| 360° stereo top-bottom, 30 fps | 5760×5760 | 34 Mbps |
| 360° stereo top-bottom, 60 fps | 5152×5152 | 35 Mbps *(capped)* |
| 360° mono, 30 fps | 5760×2880 | 17 Mbps |
| VR180 stereo side-by-side, 30 fps | 5760×2880 | 17 Mbps |

Sources already below target are never upscaled. Full reasoning in
[`ENCODING-GUIDE.md`](docs/ENCODING-GUIDE.md).

### Two findings that are easy to get backwards

**Resolution is the wrong unit — use pixels per degree.** A 360° frame wraps the viewer's
whole head while the headset shows only ~96° at once. A 4K 360° video delivers 10.7 px/deg
against the Quest 3S's ~19.1 — about half the detail the display can show. This is why a
ladder designed for flat video under-serves VR.

**At a fixed bitrate, more pixels make quality *worse*.** In our A/B, an 8K encode at
45 Mbps scored lowest of seven. Spread across 59 megapixels, every pixel is starved. If
something looks soft, the lever is bitrate or codec — not resolution.

## Troubleshooting

**`libsvtav1` not found / AV1 encoding fails.** Some ffmpeg builds ship without it. Run
`./check_setup.sh`. Fix with `brew uninstall ffmpeg && brew install ffmpeg`, or use the
HEVC presets (measurably worse per bit, but functional).

**Video plays flat, or as two stacked images.** The 360 metadata is missing. FFmpeg
discards it on *every* output, including `-c copy` — that is not a bug in this toolkit but
in FFmpeg's mov muxer. `inject_spatial.py` restores it and the scripts always run it. Never
upload a file that has not been through `verify_final.sh`.

**A video looks permanently blurry and doubled.** Check whether it is mono content
mis-tagged as stereo — `inspect_master.sh` detects and reports this. A player honouring a
wrong stereo tag splits one image in half and sends unrelated halves to each eye. Note the
Serene CMS also carries a `3D Layout` field; if the app trusts that over the file, it must
be corrected there too.

**A square file is reported as needing review.** A square frame with no stereo tag is
genuinely ambiguous — it could be 360° stereo packed top-bottom, or VR180 mono. The tool
assumes top-bottom 360°, which is right at 8K sizes, and flags it. This comes up often
because *any* FFmpeg pass strips the stereo tag, so intermediate files routinely arrive
untagged. Check one frame before trusting a whole batch: if the top and bottom halves show
near-identical images, it is stereo; if they show different parts of the scene, it is mono.

**Sideloaded files don't appear on the headset.** They land in the **Gallery** app, not
**Files**. Use `./play.sh <name>` to launch one directly.

**AV1 output is below the requested bitrate.** Expected. SVT-AV1's VBR stops when it hits
its quality target, so easy content lands under budget. Not a fault — the quality target
was met with fewer bits.

**Encoding is very slow.** AV1 preset 6 runs ~0.3× realtime at 5760×5760 on Apple Silicon,
and several times slower on Intel. Use `PRESET=8 ./encode_vendor.sh ...` for roughly double
the speed at a small quality cost; keep preset 6 for flagship content.

## Related documents

- [`ENCODING-GUIDE.md`](docs/ENCODING-GUIDE.md) — the standard, the device limits, and why
- [`RESULTS.md`](docs/RESULTS.md) — the full investigation and measurements
- [`PLATFORM-TRANSCODE-BRIEF.md`](docs/PLATFORM-TRANSCODE-BRIEF.md) — the ask for the Serene
  platform, whose import transcode currently re-encodes uploads into a lower-quality ladder
