# Serene VR — house encoding standard

How to prepare purchased VR footage (VR Gorilla or any vendor) for streaming to Quest 3S
via S3 + CloudFront. Derived from measurements on the actual headset and a 7-way A/B test,
not from vendor guidance — see `RESULTS.md` for the evidence.

---

## The short version

```bash
./encode_vendor.sh /path/to/vendor/masters/
```

Detects each file's format, encodes to the house standard, restores the 360 metadata,
and verifies the result. Anything unusual is flagged rather than guessed at.

**House standard: AV1, ~16 pixels per degree, ≤35 Mbps, faststart, metadata re-injected.**

---

## Why these settings

Three hard limits govern everything, all measured off the Quest 3S itself
(`/vendor/etc/media_codecs.xml`, device `panther`):

| Limit | Value | What it means |
|---|---|---|
| Max frame size | 8192×8192 | Rarely the binding constraint |
| **Decoder throughput** | **7,776,000 blocks/s** (16×16 blocks) | **Usually the binding constraint** |
| Max bitrate | AV1/HEVC 160 Mbps, H.264 220 Mbps | Only H.264 masters get close |

Throughput is the one that bites. In pixels: **W × H × fps ≤ 1,990,656,000**. We target
**80% of that** for safety margin, since a saturated decoder drops frames rather than
failing cleanly — which looks like judder or fuzz, not an error message.

A fourth, softer limit: **AV1/HEVC Level 6.2 caps a frame at 35,651,584 pixels.** The Quest
will decode beyond it, but staying inside keeps files portable to other players and future
devices. It costs nothing at our resolutions.

### Codec: AV1

Tested head to head at matched bitrates. AV1 beat HEVC by 1.2 dB PSNR *while using 22%
less bandwidth*, and beat it decisively on the headset. Quest 3S has AV1 hardware decode
(`c2.qti.av1.decoder`). H.264 is not a candidate — it is far less efficient at these frame
sizes and is what caused the original problem.

### Resolution: ~16 pixels per degree

Quest 3S resolves about **19 px/deg** (1832 px across ~96°). Encoding beyond that wastes
bitrate on detail the optics cannot show.

**The key finding, which is counterintuitive: at a fixed bitrate, more pixels make things
worse.** An 8K test encode at 45 Mbps scored *worst of all seven* — spread that thin, every
pixel is starved. A 4608² encode at the same bitrate as a 5760² one was indistinguishable.
Within a fixed budget you are bitrate-limited, not resolution-limited. Do not "fix"
softness by raising resolution.

16 px/deg is the sweet spot: close enough to panel limit that residual softness is
compression, low enough that bitrate goes to quality rather than pixel count.

### Bitrate: 0.035 bits per pixel, capped at 35 Mbps

The validated setting (5760²@30, 34 Mbps) works out to **0.0346 bits/pixel**. Scale that by
pixel rate to hold quality constant across different formats — then cap at the 35 Mbps
streaming budget.

---

## Target resolutions by source format

Per eye, both give 16 px/deg — the layouts differ, the perceived sharpness does not.

| Source format | Output | Bitrate | Throughput used |
|---|---|---|---|
| 360° stereo, top-bottom, 30 fps | **5760×5760** | 34 Mbps | 50% |
| 360° stereo, top-bottom, 60 fps | **5152×5152** | 35 Mbps *(capped)* | 80% |
| 360° mono, 30 fps | **5760×2880** | 17 Mbps | 25% |
| VR180 stereo, side-by-side, 30 fps | **5760×2880** | 17 Mbps | 25% |
| VR180 stereo, side-by-side, 60 fps | **5760×2880** | 34 Mbps | 50% |

Sources already below the target are never upscaled — a 3840×3840 master stays at its own
resolution, since inventing pixels only costs bitrate.

### The 60 fps problem — read before ordering 60 fps content

360° stereo at 60 fps is genuinely expensive. Holding quality constant would need
**~55 Mbps**, well past the 35 Mbps budget. So 60 fps forces a trade:

- **Drop to 5120×5120 and accept the 35 Mbps cap** (what the script does) — mildly softer
  than the 30 fps equivalent.
- **Raise the budget to ~55 Mbps** — only if site Wi-Fi reliably supports it.
- **Prefer 30 fps masters** where the vendor offers a choice. For calm, contemplative
  content — which is most of what Serene shows — 30 fps costs little and buys a lot of
  sharpness.

VR180 does not have this problem: it covers a quarter of the sphere, so the same px/deg
costs far fewer pixels. **VR180 at 60 fps fits the budget at full quality** (5760×2880,
34 Mbps, half the decoder's throughput) — whereas 360° at 60 fps does not. If a vendor
offers the same scene in VR180, it is the better buy for streaming.

---

## Two things that are easy to get wrong

**1. FFmpeg silently discards the 360 metadata.** On every output, including `-c copy`.
Without it the app shows two stacked flat images instead of a sphere. `inject_spatial.py`
restores it (both the V1 XML and V2 `sv3d`/`st3d` forms). The scripts always run it — never
upload a file that has not been through it.

**2. The index must come before the media.** Vendor masters often have `moov` at the end
(the original London file had it 29 GB in). Served as a direct MP4, the player must fetch
the tail before it can start. `-movflags +faststart` fixes it; `verify_final.sh` checks it.

---

## Workflow for a new vendor delivery

```bash
# 1. Inspect what arrived — do not assume the vendor's stated format
./inspect_master.sh /path/to/masters/

# 2. Encode the batch (AV1 preset 6; ~2-3x realtime per file)
./encode_vendor.sh /path/to/masters/

# 3. Verify before uploading
./verify_final.sh final/<name>.mp4

# 4. Spot-check one file on the headset in the Serene app itself
```

Step 4 matters: everything else confirms the *headset* can decode the file. Only playing it
in Serene confirms the *app's player* does. They are different questions.

### Encoding time

SVT-AV1 preset 6 runs at roughly **0.3× realtime** at 5760² on the M5 Pro — about 50
minutes per 15-minute video, and it saturates the CPU. For a large batch, either run
overnight or use `PRESET=8 ./encode_vendor.sh ...`, which is roughly twice as fast for a
small quality cost. Reserve preset 6 for flagship content.

---

## When something does not fit

The script flags rather than guesses if it sees: an unrecognised projection (cubemap,
fisheye, EAC), 10-bit or HDR source, frame rates above 60, or missing spatial metadata.
Handle those by hand — a wrong assumption about projection produces a file that plays but
looks subtly wrong in the headset, which is worse than an obvious failure.

**Always keep the vendor master.** It is the source for any re-encode when targets change.
