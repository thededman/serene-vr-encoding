# Converting videos for Serene — the complete walkthrough

This is the whole process, start to finish: raw video in, streaming on the
headset out. Same steps for everyone.

---

## What you need

- A **Mac** (the toolkit doesn't run on Windows — if the videos live on a
  Windows machine, copy them to the Mac first; an external drive is fine)
- **32 GB+ RAM for stereo titles; 16 GB is enough for mono only.** The encoder itself
  peaks at ~9 GB (mono) / ~16 GB (stereo, measured) — on a 16 GB Mac a stereo encode
  swaps and slows to a crawl. `./check_setup.sh` tells you which your machine can handle.
- The video files, all in one folder
- A login for serene.precipiodx.com

---

## Step 1 — One-time setup (~5 minutes, once per Mac)

Open Terminal and paste, one line at a time:

```
git clone https://github.com/thededman/serene-vr-encoding.git
cd serene-vr-encoding
brew install ffmpeg
./check_setup.sh
```

✅ **Checkpoint:** the last command ends with **READY**.
If it says NOT READY, it prints the exact command that fixes it. Run that, retry.

Already set up? Just `cd serene-vr-encoding` and go to Step 2.

## Step 2 — Look before you convert (~10 seconds)

```
./inspect_master.sh /path/to/your/videos/
```

This reads each file and prints a table: what it is, and what it will become.

✅ **Checkpoint:** every row says **ok**.

⚠️ If a row says **review** or **SKIP**, that file has something odd (usually a
wrong stereo label — the thing that once made a video play as unfocusable blur).
Don't convert it yet: copy the output and send it to David.

**Note each file's LAYOUT column — mono or tb — you'll need it in Step 5.**

## Step 3 — Convert

```
./encode_vendor.sh /path/to/your/videos/
```

One command does every file in the folder: picks the right size and bitrate for
its format, converts to AV1, and restores the 360° metadata (without which the
video plays flat instead of wrapping around you).

⏱ **This is slow — about 3× the video's length per file.** If it is running far
slower than that, it is memory pressure — see the Hardware note in "What you need". A 30-minute video
takes ~90 minutes; a folder of four is an overnight job. Leave the Mac plugged
in and let it run.

✅ **Checkpoint:** it ends with an "Encoded:" list naming every file, and no
"Skipped" section.

## Step 4 — Find your files

The finished videos are in the **`final/`** folder, named like:

```
London_8K_HEVC_Mono_5760x2880_av1_17M.mp4
```

Each is roughly **1/5 the size of the original**, at the same visible quality
on the headset.

## Step 5 — Upload and label

Upload each file at serene.precipiodx.com, then set its fields in the portal.
**Getting 3D Layout wrong is the #1 cause of blurry playback**, so use this
rule — you can read it straight off the output filename:

| Filename shape | Example | Set 3D Layout to |
|---|---|---|
| Wide (width = 2× height) | `..._5760x2880_...` | **Mono / 2D** |
| Square (width = height) | `..._5760x5760_...` | **Top-Bottom 3D** |

For every video also set: **Video Angle 360°** · **Content Shape Spherical**.

## Step 6 — Check it on the headset

Put on the Quest, open the video through the Serene app, and look for:

- **Sharp** — signage/texture detail is crisp, not smeared
- **One world** — the scene wraps around you as a single sphere; no doubling,
  no seam at the horizon, easy to focus
- **Smooth** — no stutter when you turn your head

✅ All three → done. Move to the next video.
❌ Any problem → note which video and which symptom, and tell David.

---

## Quick reference

| I want to… | Command |
|---|---|
| See what my files are | `./inspect_master.sh <folder>` |
| Convert everything | `./encode_vendor.sh <folder>` |
| Deep-check one output | `./verify_final.sh final/<file>` |

Everything else in this repo is reference material (`docs/`) — the measurements
and reasoning behind these settings. You never need it to convert a video.
