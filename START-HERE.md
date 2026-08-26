# Encode a video for Serene

## 1. Encode

```
cd ~/Desktop/Serene-VR-Encoding
./encode_vendor.sh /path/to/your/videos/
```

Point it at a folder or a single file. Roughly 50 minutes per 15-minute video —
leave it running.

## 2. Upload

The finished files appear in the `final/` folder.
Upload those to serene.precipiodx.com.

**Done.**

---

### If something looks wrong in the headset

```
./inspect_master.sh /path/to/your/videos/
```

Prints what each file is and flags anything odd. Send me the output.

### On a new Mac, first time only

```
brew install ffmpeg
./check_setup.sh
```

---

Everything else in this folder is reference material for the dev team.
You don't need any of it to encode a video.
