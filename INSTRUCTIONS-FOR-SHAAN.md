# Converting the headset videos — instructions for Shaan

You have these four files (in `VR_HEADSETS\headset-videos`):

- Florence_8K_HEVC_Mono.mp4
- London_8K_HEVC_Mono.mp4
- Sequoia - Forest Meditation_8K_HEVC_Mono.mp4
- Siquijor_8K_HEVC_Mono.mp4

They are too heavy to stream as-is. The steps below convert each one into a small
file that streams cleanly to the Quest 3S. **Run this on a Mac** — copy the four
videos onto it first (external drive is fine).

## One-time setup (5 minutes)

```
git clone https://github.com/thededman/serene-vr-encoding.git
cd serene-vr-encoding
brew install ffmpeg
./check_setup.sh
```

`check_setup.sh` must end with **READY**. If it doesn't, it tells you the exact
command to fix it.

## Convert (one command)

```
./encode_vendor.sh /path/to/headset-videos/
```

That's all. It handles all four files in one run, automatically:

- detects each file's format (these are mono 360° — it will say `mono` / `360`)
- converts to AV1 at 5760×2880, ~17 Mbps
- restores the 360° metadata that ffmpeg normally strips (without this the
  video plays flat instead of wrapping around the viewer)

**Time:** roughly 3× the video's length per file — a 30-minute video takes about
90 minutes. Total for all four is likely several hours; leave it running.

## Result

Finished files appear in the `final/` folder, named like:

```
Florence_8K_HEVC_Mono_5760x2880_av1_17M.mp4
```

Each will be roughly **1/5 the size of the original**. Upload these to the
Serene portal.

## If anything says "review" or "SKIP"

The tool refuses to guess when a file looks mislabelled (we've had a mono video
tagged as stereo — it plays as unfocusable blur). If you see a note like that,
send the full output of this to David before encoding:

```
./inspect_master.sh /path/to/headset-videos/
```

## In the Serene portal, after upload

For these four videos set: **Video Angle 360° · Content Shape Spherical ·
3D Layout Mono/2D** (they are mono — *not* Top-Bottom 3D; that setting is what
made a previous video blurry).
