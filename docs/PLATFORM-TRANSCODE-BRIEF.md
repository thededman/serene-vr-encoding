# Serene platform — VR transcode settings

**To:** Serene dev team **From:** David **Date:** 2026-08-17
**Subject:** Import transcode is capping VR video quality below what the headset can show

---

## The ask, in one line

Let VR content bypass the import transcode, or raise the ladder's top rung to
**5760×5760 AV1 at ~34 Mbps**. The current H.264/4K target delivers roughly **half** the
detail a Quest 3S can resolve, and that is what users report as "fuzzy".

---

## For project managers — summary

**The problem.** Serene's VR videos look soft in the headset, which undercuts the sense of
presence the product depends on.

**There were two separate causes, not one.**

1. *The source files.* Fixed. The masters were encoded beyond what the headset's hardware
   can decode, so playback silently fell back to a degraded path. Re-encoded to a new
   standard, verified on an actual Quest 3S, and confirmed excellent.
2. *The platform's import transcode.* **Not fixed — this needs dev work.** Serene re-encodes
   every upload into an adaptive ladder that appears to top out at 4K H.264. Because a 360°
   video wraps the whole frame around the viewer's head, a 4K rung shows only about **half**
   the detail the headset can display. No upload quality can overcome this: the pixels are
   discarded at import.

**What we are asking for.** Either a "skip transcode" option for already-optimised VR files
(the smaller change), or raise the ladder's top rung for VR content (the more thorough one).
Detailed specs below.

**What it is worth.**

| | Now | After |
|---|---|---|
| File size per title | 29 GB | **3.3 GB** |
| Storage + CloudFront egress | baseline | **−88.6%** |
| Headset image quality | soft | sharp |

So this is not a quality-versus-cost trade — it **improves quality and cuts cost at the same
time**. The saving scales across the whole content library, including the newly purchased
VR Gorilla titles.

**Risk of not doing it.** Every VR title added to the platform inherits the same ceiling, so
the library grows at a quality level below what the hardware can show — and at roughly nine
times the necessary storage and bandwidth cost.

**Effort.** The passthrough option is a small change (a per-item flag plus a conditional).
Changing the ladder is a configuration change to the transcode profile. Neither requires
new infrastructure. A reference file encoded to the proposed spec is ready for testing.

---

## Why 4K is not enough for VR (the core point)

This is the thing that makes VR different from flat video, and it is easy to miss when
reusing a standard ABR ladder.

A flat 4K video fills the screen with 3840 pixels. A **360° equirectangular** video wraps
those same 3840 pixels around the viewer's entire head — but the headset only shows about
**96°** at a time. So the viewer sees roughly a quarter of the frame, stretched across the
full display.

The meaningful unit is therefore **pixels per degree**, not resolution:

| Source | Pixels across 360° | **Pixels per degree** | vs. Quest 3S |
|---|---|---|---|
| 4K 360° | 3840 | **10.7** | **56%** — visibly soft |
| 5760 360° *(proposed)* | 5760 | **16.0** | 84% — sharp |
| Quest 3S panel | — | **19.1** | 100% |

At 10.7 px/deg the headset is displaying about half the detail it is capable of. No amount
of bitrate fixes that — the pixels are not there.

---

## Evidence

Measured on an actual enrolled Quest 3S (device `panther`), not from vendor guidance.

**1. Seven encodes A/B tested on the headset**, with PSNR/SSIM measured through the
headset's real viewport (equirectangular reprojected to a 96°×90° eye view at panel
resolution) rather than on the raw frame — measuring the raw frame is misleading, since the
poles hold many pixels but almost no viewer attention.

| Encode | Resolution | Codec | Mbps | PSNR dB | SSIM |
|---|---|---|---|---|---|
| **Winner** | 5760×5760 | **AV1** | **34** | **46.09** | **0.9882** |
| | 5760×5760 | AV1 | 25 | 45.57 | 0.9876 |
| | 5760×5760 | HEVC | 44 | 44.92 | 0.9850 |
| | 5760×5760 | HEVC | 32 | 43.12 | 0.9810 |
| | 4608×4608 | HEVC | 31 | 43.05 | 0.9814 |
| | 7680×7680 | HEVC | 45 | 42.82 | 0.9811 |

Two findings worth carrying into the ladder design:

- **AV1 beat HEVC by 1.2 dB while using 22% less bandwidth.** Quest 3S has AV1 hardware
  decode (`c2.qti.av1.decoder`, confirmed on-device). H.264 is the worst choice at these
  frame sizes.
- **At a fixed bitrate, higher resolution made quality *worse*.** The 8K encode scored
  lowest of all seven — spread across 59 MP at 45 Mbps, every pixel is starved. So the fix
  is not simply "encode bigger"; 5760×5760 is the measured optimum for this budget.

**2. Device decoder limits** (`/vendor/etc/media_codecs.xml`), which any ladder must respect:

| Decoder | Max frame | Max bitrate | Throughput |
|---|---|---|---|
| AV1 | 8192×8192 | 160 Mbps | 7,776,000 blocks/s |
| HEVC | 8192×8192 | 160 Mbps | 7,776,000 blocks/s |
| H.264 | 8192×8192 | **220 Mbps** | 7,776,000 blocks/s |

The proposed 5760×5760@30 rung uses **50%** of the throughput budget — comfortable headroom.

**3. The original master was itself out of spec** — 7680×7680 H.264 at **251 Mbps against
the 220 Mbps decoder limit**, with a nonstandard level (`level_idc=100`; the H.264 standard
stops at 6.2). That has been fixed on our side; this brief is about what happens after import.

---

## Requested changes, in priority order

**1. Add a passthrough option** — a per-item "already optimised / skip transcode" flag.
Cheapest fix, and correct in principle: pre-conformed VR masters should not be re-encoded.
Re-compressing an already-compressed delivery file costs a generation of quality for no
benefit.

**2. If the ladder must stay, raise the top rung for VR content:**

| Rung | Resolution | Codec | Bitrate | Purpose |
|---|---|---|---|---|
| 1 | 5760×5760 | **AV1** | 34 Mbps | Primary — matches headset capability |
| 2 | 4608×4608 | AV1 | 15 Mbps | Mid network |
| 3 | 3840×3840 | AV1 | 8 Mbps | Weak network fallback |

Keep top-bottom stereo packing and equirectangular projection unchanged throughout, and
preserve the spatial metadata (see below).

**3. Do not transcode VR content to H.264.** Far less efficient at these frame sizes, and it
is what caused the original problem.

**4. Confirm the transcoder can decode AV1 input.** If it cannot, tell us and we will supply
a high-bitrate HEVC mezzanine for import instead — but we need to know rather than guess.

---

## One implementation detail that silently breaks VR playback

**FFmpeg discards 360/stereo metadata on every output, including `-c copy`.** If the
transcode pipeline uses FFmpeg and does not explicitly re-inject it, output files lose the
`sv3d`/`st3d` boxes (and the older Google Spherical V1 `uuid` XML), and players fall back to
showing a flat, doubled image instead of a sphere.

Serene appears to carry projection in CMS fields (`Video Angle`, `Content Shape`,
`3D Layout`), which may mask this — but any file leaving the platform for another player
will be broken. We have a working injector (~200 lines, no dependencies) that writes both
metadata generations and can be shared.

---

## Business case

| | Current master | House standard |
|---|---|---|
| File size | 29.0 GB | **3.3 GB** |
| Bitrate | 251 Mbps | 30 Mbps |
| Headset quality | fuzzy | sharp |

**88.6% reduction in storage and CloudFront egress per title**, with *better* image quality.
The same standard applies to the whole VR Gorilla library, so the saving scales across the
catalogue.

---

## Reference file

`Londen_5760_av1_34M.mp4` — encoded to the proposed spec, verified end to end (clean decode,
correct metadata, faststart index, quality PSNR 45.8 / SSIM 0.987 sampled across the full
15:27), and confirmed excellent on a Quest 3S. Available for testing against the pipeline.

Full working notes and measurement methodology: `RESULTS.md` and `ENCODING-GUIDE.md`.
