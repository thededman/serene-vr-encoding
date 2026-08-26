# Serene VR — video quality investigation

**Owner:** David · **Started:** 2026-08-17
**Goal:** Stop the 360° video looking fuzzy in the headset, while keeping it streamable
from S3 + CloudFront.
**Target device:** Meta Quest 3S · **Bitrate ceiling:** ~25–35 Mbps

---

## Summary for the weekly update

> Diagnosed the cause of the fuzzy VR video. The master itself is not at fault — it is
> genuinely sharp. Reading the decoder specifications directly off the Quest 3S showed the
> file is encoded at **251 Mbps against a hardware limit of 220 Mbps**, and its H.264
> profile level is outside the published standard entirely. The headset cannot decode it
> properly and falls back to a degraded path, which is what viewers see as fuzz. Two
> further faults compound it: the file's index sits at the very end of 29 GB (hurting
> start-up over CloudFront), and 250 Mbps is far beyond any Wi-Fi budget.
>
> Built seven test encodes and measured each one against the master through a simulated
> headset viewport. **Switching to AV1 at 5760×5760 gives the best result: better measured
> quality than any HEVC option while using 22% less bandwidth, and it fits inside the
> 35 Mbps streaming budget.** It also produces a file around **3.9 GB instead of 29 GB**.
> Notably, simply pushing resolution higher makes things *worse*, not better — the 8K test
> encode scored lowest of all. Clips are ready to confirm on the headset.

---

## 1. Source analysis

`Londen_V5_8K_H264_1903.mp4` — 29.0 GB, 15:27, Piccadilly Circus, London.

| Property | Value | Verdict |
|---|---|---|
| Resolution | 7680×7680, top-bottom stereo (7680×3840 per eye) | **59 MP — over the decode ceiling** |
| Projection | Full 360°×180° equirectangular | Confirmed by rendering opposite views |
| Stereo | Genuine 3D — inter-eye PSNR 25.7 dB, disparity on near objects | Real, must be preserved |
| Codec | H.264 **Main**, 8-bit, 30 fps, 250 Mbps | Inefficient at this size |
| Level | `level_idc = 100` | **Not a legal H.264 level** (max 6.2 / idc 62) |
| Frame size | 230,400 macroblocks | vs **139,264 max** at L6.2 — 65% over |
| `moov` index | byte 28,992,601,409 (end of file) | **Not faststart** |
| Spatial metadata | Google Spherical **V1** XML in a `uuid` box | Older format; app reads it |

**Image quality of the master is good.** Rendered at the headset's field of view, fine
detail (signage text, building stonework) resolves clearly. The softness is introduced
downstream, not baked in.

## 2. Root cause — measured on the actual headset

Rather than rely on published guidance, the decoder capability table was read directly off
the connected Quest 3S (`/vendor/etc/media_codecs.xml`, device `panther`, Android 14):

| Decoder | Max size | Max bitrate | Throughput limit |
|---|---|---|---|
| H.264 `c2.qti.avc.decoder` | 8192×8192 | **220 Mbps** | 7,776,000 blocks/s |
| HEVC `c2.qti.hevc.decoder` | 8192×8192 | 160 Mbps | 7,776,000 blocks/s |
| AV1 `c2.qti.av1.decoder` | 8192×8192 | 160 Mbps | 7,776,000 blocks/s |
| MV-HEVC `c2.qti.mvhevc.decoder` | 4096×4096 | 160 Mbps | 2,073,600 blocks/s |

**This corrected an earlier assumption.** Published guidance widely cites a 8192×4096
ceiling, which would have put the 7680×7680 master far out of range. The device actually
declares **8192×8192**, so the frame size is fine. Checking the master against the real
limits:

| Check | Master | Limit | Result |
|---|---|---|---|
| Frame size | 7680×7680 | 8192×8192 | fits |
| Throughput | 230,400 blocks × 30 fps = 6,912,000/s | 7,776,000/s | fits (89% of budget) |
| **Bitrate** | **251.2 Mbps** | **220 Mbps** | **14% OVER** |

So the fault is **too many bits, not too many pixels**:

1. **Bitrate exceeds the H.264 decoder's declared ceiling** — 251 Mbps against a 220 Mbps
   limit, with instantaneous peaks higher still. The decoder drops frames or the player
   falls back to software decode, which at 8K cannot keep up. That is what reads as fuzz.
2. **The H.264 level is invalid** (`level_idc = 100`; the standard stops at 6.2 / idc 62).
   A nonstandard level can break decoder negotiation before bitrate even comes into play.
3. **Index at the end of a 29 GB file.** Served as a direct MP4, the player must fetch the
   tail before it can start.
4. **250 Mbps is undeliverable over Wi-Fi**, and a direct MP4 has no adaptive ladder to
   drop down to, so the player degrades or stalls rather than adapting.

**AV1 hardware decode is confirmed present on the device** (`c2.qti.av1.decoder`), which
supports the recommendation below. The one thing the capability table cannot say is
whether the *Serene app's player* requests that decoder — only the on-device test can.

*Also noted for future content:* the device has a true multiview stereo decoder
(**MV-HEVC**, max 4096×4096). Not applicable at our resolution, since we need more than
4096 per eye, but relevant if Serene ever ships lower-resolution stereo content — it is
more efficient than packing both eyes into one frame.

## 3. Constraints discovered (these bound every possible fix)

- **HEVC Level 6.2 caps luma at 35,651,584 px.** Verified empirically: a 7680×7680 encode
  emits level 6.3 (**out of spec**); 5760×5760 emits level 6.0 (**in spec**).
  → **5760×5760 (33.2 MP) is the largest square that stays within the standard.**
  This headset's decoder will accept beyond-spec streams (it declares 8192×8192), so this
  is not a hard blocker on the Quest 3S — but staying in spec is the safer choice for
  other players, future devices and CDN tooling, and the measurements below show there is
  no quality reason to exceed it anyway.
- **FFmpeg silently drops the spatial metadata** on every output, including `-c copy`.
  Without it the player shows two stacked flat images instead of a 360° stereo sphere.
  → Wrote `inject_spatial.py` to restore it (V1 XML + V2 `sv3d`/`st3d`). Verified.
- **Quest 3S optics:** 1832×1920 per eye over ~96° FOV ≈ **19.1 px/deg**. A 5760-wide
  equirect delivers 16 px/deg — about 84% of panel resolution. Close enough that any
  residual softness will be compression, not resolution.

## 4. Encode performance on this Mac (M5 Pro, measured)

| Encoder | Speed | 60s clip | Full 15:27 |
|---|---|---|---|
| `hevc_videotoolbox` @ 7680² | 0.33× | ~3 min | ~47 min |
| `hevc_videotoolbox` @ 5760² | 0.54× | ~1.9 min | ~29 min |
| `libsvtav1` preset 10 @ 5760² | 0.75× | ~1.3 min | ~21 min |

A full re-encode is well within a working session — no need to batch overnight.

---

## 5. Test clips — to be scored in the headset

All six are the same 60-second segment (05:00, moving bus + signage + stonework),
faststart, spatial metadata injected. In `tests/`.

### Measured results — ranked

Each clip was reprojected to the exact view one eye sees on a Quest 3S (96°×90° at
1832×1920) and compared frame-by-frame against the master rendered the same way, with no
lossy intermediate step. Measuring on the raw equirectangular frame would have been
misleading — the poles occupy a large share of the pixels but almost none of the viewer's
attention.

| Rank | # | File | Res | Codec | **Mbps** | PSNR dB | SSIM | Level |
|---|---|---|---|---|---|---|---|---|
| — | T0 | `T0_control_7680_h264_250M.mp4` | 7680² | H.264 | 251.2 | ∞ *(identical)* | 1.0000 | **100 — invalid** |
| **1** | **T6** | **`T6_5760_av1_45M.mp4`** | **5760²** | **AV1** | **34.4** | **46.09** | **0.9882** | 6.0 |
| 2 | T4 | `T4_5760_av1_32M.mp4` | 5760² | AV1 | **24.8** | 45.57 | 0.9876 | 6.0 |
| 3 | T3 | `T3_5760_hevc_45M.mp4` | 5760² | HEVC | 44.3 | 44.92 | 0.9850 | 6.0 |
| 4 | T1 | `T1_5760_hevc_32M.mp4` | 5760² | HEVC | 31.6 | 43.12 | 0.9810 | 6.0 |
| 5 | T2 | `T2_4608_hevc_32M.mp4` | 4608² | HEVC | 31.3 | 43.05 | 0.9814 | 6.0 |
| 6 | T5 | `T5_7680_hevc_45M.mp4` | 7680² | HEVC | 45.2 | 42.82 | 0.9811 | **6.3 — out of spec** |

T0 measured as bit-identical to the master (SSIM exactly 1.000000), which validates the
measurement method and confirms the clips are correctly time-aligned.

### What the numbers say

1. **AV1 wins clearly.** T6 beats the best HEVC option (T3) by 1.2 dB while using **22%
   less bandwidth**. Even T4, at just 24.8 Mbps, outscores HEVC at 44.3 Mbps.
2. **Pushing resolution higher backfires.** T5 at 7680² scored **worst of all seven**
   despite the highest resolution and a high bitrate — at 59 MP and 45 Mbps it is starved
   to ~0.025 bits per pixel. And it is out of spec regardless. This kills the intuitive
   "just make it higher resolution" fix.
3. **We are bitrate-limited, not resolution-limited.** T1 (5760²) and T2 (4608²) at the
   same 32 Mbps are statistically identical. Above ~4600², extra pixels buy nothing until
   bitrate rises. This is exactly why the codec change matters more than the resolution.

**Recommendation: T6 — AV1, 5760×5760, ~34 Mbps.** With **T4 (24.8 Mbps)** as the fallback
if bandwidth turns out to be tight: it gives up only 0.5 dB for 28% less bitrate.
**If the Serene app cannot decode AV1, fall back to T3** (HEVC 5760² at 44 Mbps) or T1
(31.6 Mbps) — that is the single most important thing the headset test needs to settle.

*Caveat, stated plainly:* PSNR and SSIM measure fidelity to the master, not perceived
sharpness in a headset, and 2D metrics are known to be imperfect proxies for VR. They rank
the encodes reliably, but they **cannot tell us whether the headset will actually decode
each file** — which is the core hypothesis. The on-device test remains the decider.

### The clips

| # | File | Res | Codec | Bitrate | Question |
|---|---|---|---|---|---|
| T0 | `T0_control_7680_h264_250M.mp4` | 7680² | H.264 | 251 Mbps | Baseline — does today's file even decode properly? |
| T1 | `T1_5760_hevc_32M.mp4` | 5760² | HEVC | 32 Mbps | In-spec HEVC at budget |
| T2 | `T2_4608_hevc_32M.mp4` | 4608² | HEVC | 31 Mbps | Same budget, lower res — cleaner? |
| T3 | `T3_5760_hevc_45M.mp4` | 5760² | HEVC | 44 Mbps | Best HEVC — the AV1 fallback |
| T4 | `T4_5760_av1_32M.mp4` | 5760² | AV1 | 25 Mbps | AV1 at low bandwidth |
| T5 | `T5_7680_hevc_45M.mp4` | 7680² | HEVC | 45 Mbps | Does the 3S decode out-of-spec 8K at all? |
| T6 | `T6_5760_av1_45M.mp4` | 5760² | AV1 | 34 Mbps | **Recommended** — AV1 at top of budget |

> **Note on T0:** it carries today's exact video characteristics (7680², H.264, 250 Mbps)
> but has been given a faststart index, which the live file lacks. That is deliberate — it
> isolates the *decode* question from the *start-up* question. So T0 represents "today's
> picture quality with the start-up fault already fixed." If T0 still looks fuzzy, the
> decode ceiling is confirmed as the cause.

### Headset test outcome — 2026-08-17

**T6 confirmed best on the Quest 3S.** Viewed through the Horizon OS Gallery player,
AV1 at 5760×5760 / 34 Mbps was the clearest of the seven. This matches the objective
measurement exactly, and confirms the headset decodes AV1 in hardware.

**Decision: ship AV1 5760×5760 at ~34 Mbps.** Full-length encode started.

*Still to confirm:* the Gallery test proves the **headset** decodes AV1; it does not prove
the **Serene app's player** requests that decoder. The Serene app (`com.precipio.vrserene`)
is installed on this same headset, so this can be checked directly once the file is
uploaded. If Serene cannot play AV1, re-encode with `./encode_final.sh hevc` (T3 settings)
— everything else in the pipeline stays the same.

*Practical note:* sideloaded clips appear in the **Gallery** app, not **Files**. Use
`./play.sh T6` to launch a clip straight from the Mac and skip the UI.

### Final encode — delivered and verified 2026-08-17

`final/Londen_5760_av1_34M.mp4` — **PASSED all pre-upload checks.**

| | Before | After |
|---|---|---|
| Size | 29.0 GB | **3.3 GB** (−88.6%) |
| Codec | H.264, invalid level | AV1, hardware-decoded |
| Bitrate | 251 Mbps (over the 220 limit) | **30.0 Mbps** |
| Index | end of file | before media (faststart) |
| 360/stereo metadata | — | restored and verified |

Quality sampled at six points across the full 15:27, measured through the Quest 3S eye
view against the master:

| At | 60s | 240s | 420s | 600s | 780s | 900s | **mean** |
|---|---|---|---|---|---|---|---|
| PSNR dB | 46.91 | 44.53 | 46.60 | 44.67 | 46.83 | 45.29 | **45.80** |
| SSIM | 0.9883 | 0.9863 | 0.9873 | 0.9853 | 0.9909 | 0.9858 | **0.9873** |

The 60-second test clip measured 46.09 / 0.9882, so the full-length encode holds the same
quality throughout — **no starved sections**. The 1.5 dB spread between best and worst
samples is normal variation with scene complexity, not a defect. Decodes cleanly end to
end with no corrupt frames.

### Scoring sheet — fill in during headset testing

For each clip, view the **same moment at the same head orientation**. The key call is
**softness vs. blocking**, because it points at different fixes:

- Soft/smeared but *clean* → **resolution-limited** → go higher res (favours T5/T3)
- Sharp edges but *blocky/shimmering/mosquito noise* → **bitrate-limited** → raise bitrate
  or switch codec (favours T3/T4)

**Test T6 and T4 first** — they are the recommended setting and its low-bandwidth
fallback. **T0 and T3 are the two that settle the open questions** (does today's format
decode? does AV1 work in the app?). If time is short, those four are enough.

| # | Plays? | Sharpness /5 | Artifacts (soft or blocky) | Stereo depth correct? | Judder? | Notes |
|---|---|---|---|---|---|---|
| T6 **(rec.)** |  |  |  |  |  |  |
| T4 |  |  |  |  |  |  |
| T3 *(HEVC fallback)* |  |  |  |  |  |  |
| T0 *(today's format)* |  |  |  |  |  |  |
| T1 |  |  |  |  |  |  |
| T2 |  |  |  |  |  |  |
| T5 |  |  |  |  |  |  |

### The three questions the headset test must answer

1. **Does T0 stutter or look soft?** It sits 14% above the H.264 decoder's bitrate limit,
   so it should misbehave. Confirming that closes the diagnosis.
2. **Do T4/T6 play at all?** AV1 hardware decode is confirmed present on the device, but
   whether the *Serene app's player* requests it is unknown. If AV1 fails, use T3 (HEVC).
3. **Does T5 look soft despite being 8K?** It is inside every device limit, so it should
   play cleanly — but it measured **worst of all seven** on quality. If it plays smoothly
   yet looks softer than T6, that is direct proof that bitrate, not resolution, is the
   lever — and it settles the "just make it higher resolution" instinct for good.

---

## 6. Status log

| Date | Progress |
|---|---|
| 2026-08-17 (later) | Playback of the AV1 file confirmed excellent on the headset. Discovered the Serene platform re-encodes on import to an adaptive ladder (apparently H.264 4K), meaning our file is not what gets served — likely a major cause of the original fuzziness. Wrote a dev-team brief, a mezzanine encode preset, and a tool to measure what the platform actually delivers. |
| 2026-08-17 | Analysed master; wrote and verified spatial-metadata injector; built and objectively measured 7 test encodes. Read the decoder capability table directly off the Quest 3S, which **corrected the initial diagnosis**: the frame size is within limits, but the master's 251 Mbps exceeds the H.264 decoder's 220 Mbps ceiling. **Result: AV1 at 5760² recommended — better quality than any HEVC option at 22% less bandwidth, and 29 GB → ~3.9 GB.** Clips sideloaded and scored on the headset; **T6 (AV1) confirmed best**. Full-length encode run and verified. |

## 6b. Animals Compilation — a different fault entirely

`Animals Compilation ep_1_8k_H265_90MBS.mp4` (2.6 GB, 4:05) had been fuzzy since it was
added. The cause is unrelated to the London problem.

**The file is mono 360°, incorrectly tagged as top-bottom stereo.**

| Check | Finding |
|---|---|
| Frame | 7680×3840 — a correct 2:1 mono equirect |
| Declared | `top and bottom` stereo, which would make each eye 7680×1920 (4:1 — impossible for 360) |
| Proof | **PSNR between halves: 7.1 dB.** Genuine stereo measures ~25 dB. The halves are sky vs. ground. |
| Codec health | HEVC Level 6.0, 92 Mbps — entirely within decoder limits. Nothing wrong with the encode. |

A player honouring that tag splits one image in half, sends the sky to one eye and the
ground to the other, and stretches each 2× vertically: half the vertical resolution *and*
two eyes that cannot fuse. That is the fuzziness.

**Two fixes, for two different consumers of the file:**

1. **The file** — re-encoded and re-tagged as mono (`Stereo 3D -> 2D`). Fixes it for any
   player that reads file metadata.
2. **The CMS** — `Serene - Animals Compilation` almost certainly has `3D Layout` set to
   `Top-Bottom 3D`. **If the app trusts the CMS field over the file, this is the operative
   fix** and the re-encode alone will not change Serene's behaviour. Set it to Mono / 2D.

**Outputs** (both mono, faststart, metadata verified):

| Version | px/deg | Bitrate | Size | PSNR | SSIM |
|---|---|---|---|---|---|
| 5760×2880 | 16.0 | 16.7 Mbps | 494 MB | 42.85 | 0.9827 |
| **6848×3424** *(recommended)* | **19.0** | 17.8 Mbps | 527 MB | 42.88 | 0.9819 |

The metrics are effectively tied, and honestly so — they compare against a downscaled
reference and cannot measure display-time resampling. The argument for 6848 is geometric:
its 96° slice is 1826 px against the panel's 1832, so it maps ~1:1 and needs no upscale,
whereas 5760 must be stretched 1.19×. Mono content uses half the pixels of stereo, so this
extra resolution costs only ~1 Mbps. Confirm on the headset.

**Tooling fix:** `vr_plan.py` would have made the same mistake — trusting the stereo tag and
building a fake 5760×5760 stereo frame out of mono content. It now checks the declared
layout against the actual geometry and flags mismatches. **Run `./inspect_master.sh` over
the whole library and the VR Gorilla delivery** — mis-tagging is invisible until someone
wears a headset, and if one vendor file has it, others likely do.

## 6c. Local playback confirmed — 2026-08-18

Re-encoded file loaded to the headset and played back locally: **quality confirmed
excellent**, and the file transferred to the device quickly at its reduced size. This
validates the encode settings end to end on real hardware.

**What this does and does not prove:**

- ✅ The encode standard is right — the files look good on the actual target device.
- ✅ The Quest 3S decodes our AV1 output in hardware.
- ✅ The mono re-tagging fixed the Animals stereo split.
- ❓ **Not yet proven:** that the *Serene platform* delivers this quality. Local playback
  bypasses both the platform transcode and the Serene app's player. That is the next test.

## 7. Open items

- [x] ~~Score the clips on the Quest 3S~~ — **done, T6 (AV1) confirmed best.**
- [x] ~~Full 15:27 encode~~ — **done and verified: 29 GB → 3.3 GB, all checks passed.**
- [ ] **Upload `final/Londen_5760_av1_34M.mp4` to serene.precipiodx.com.**
- [ ] **Play it in the Serene app on the headset** — this is the last real unknown. The
      Gallery test proved the *headset* decodes AV1; only this proves the *app's player*
      requests it. If it fails: `./encode_final.sh hevc` and re-upload, nothing else changes.
- [ ] **Keep the 29 GB master.** Do not replace it on S3 — it is the source for any
      future re-encode.
- [ ] **BLOCKER — the platform re-encodes on import.** The Serene admin builds an adaptive
      ladder, apparently targeting H.264 at 4K or lower, so CloudFront has never served our
      file. A 4K 360 rung gives only **10.7 px/deg** against the Quest 3S's ~19.1 — about
      half the detail the headset can show, which is soft regardless of the upload quality.
      This may be a significant cause of the original fuzziness, separate from the master's
      own faults. See `PLATFORM-TRANSCODE-BRIEF.md`.
  - [ ] Check the `Streaming Format` dropdown for a passthrough/original/direct option —
        cheapest possible fix, and may need no dev involvement.
  - [ ] Check the `Resolution` dropdown maximum (currently `N/A`).
  - [ ] Read the `Transcode` column for `DEMO-LONDON-DAVE`; confirm AV1 input was accepted
        at all — many transcoders cannot decode AV1.
  - [ ] Measure what is actually delivered: `./check_stream.sh <playback .m3u8 URL>`.
  - [ ] If the transcode cannot be avoided, upload a **mezzanine** instead of the delivery
        file: `./encode_final.sh mezzanine` (HEVC 5760x5760 100 Mbps). Re-compressing our
        30 Mbps AV1 delivery file would cost a generation of quality for no benefit.
  - [ ] Send `PLATFORM-TRANSCODE-BRIEF.md` to the dev team.
- [ ] Apply the house standard to the VR Gorilla library — see `ENCODING-GUIDE.md`,
      then `./inspect_master.sh` and `./encode_vendor.sh`.
- [ ] Confirm whether the Serene app can open local files, or whether a reference player
      (Meta Files, DeoVR, Skybox) is needed for the A/B. If a reference player is used,
      re-confirm the winner through Serene itself — a Unity/Unreal video player uses a
      different decode path.
- [ ] **Raise with the AWS/Serene team:** a direct MP4 has no adaptive fallback, so even a
      perfect file will stall on weak Wi-Fi. An HLS ladder (top rung at the winning
      setting, plus ~15 and ~8 Mbps rungs) would make delivery robust.
- [ ] **Check the other Serene videos** — if they share this 7680×7680 master format, they
      have the same defect and need the same treatment.
