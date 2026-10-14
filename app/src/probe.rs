//! Read a video's shape with ffprobe.
//!
//! Faithful port of `vr_plan.py` probe(). The fields here are exactly the ones the planner
//! needs — resolution, frame rate, and the two pieces of spatial metadata (projection and
//! stereo layout) that decide how the frame should be interpreted.

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, PartialEq)]
pub struct VideoInfo {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub codec: String,
    pub pix_fmt: String,
    pub bitrate: u64,
    /// As ffprobe reports it: "top and bottom", "side by side", or "mono" when absent.
    pub stereo: String,
    /// None when the file carries no spherical metadata at all.
    pub projection: Option<String>,
    pub duration: f64,
}

// --- the subset of ffprobe's JSON we care about ---------------------------------------

#[derive(Deserialize)]
struct ProbeOutput {
    streams: Vec<Stream>,
    format: Option<Format>,
}

#[derive(Deserialize)]
struct Format {
    duration: Option<String>,
}

#[derive(Deserialize)]
struct Stream {
    width: Option<u32>,
    height: Option<u32>,
    codec_name: Option<String>,
    pix_fmt: Option<String>,
    bit_rate: Option<String>,
    r_frame_rate: Option<String>,
    side_data_list: Option<Vec<SideData>>,
}

#[derive(Deserialize)]
struct SideData {
    side_data_type: Option<String>,
    #[serde(rename = "type")]
    stereo_type: Option<String>,
    projection: Option<String>,
}

/// "30000/1001" -> 30. Rounded, matching the Python's round().
fn parse_fps(raw: &str) -> u32 {
    let (num, den) = raw.split_once('/').unwrap_or((raw, "1"));
    let num: f64 = num.parse().unwrap_or(30.0);
    let den: f64 = den.parse().unwrap_or(1.0);
    if den == 0.0 {
        return 30;
    }
    (num / den).round() as u32
}

pub fn probe(path: &Path) -> Result<VideoInfo> {
    let out = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "v:0", "-show_streams", "-show_format",
               "-print_format", "json"])
        .arg(path)
        .output()
        .context("could not run ffprobe — is ffmpeg installed?")?;

    if !out.status.success() {
        bail!("ffprobe failed on {}", path.display());
    }

    let parsed: ProbeOutput =
        serde_json::from_slice(&out.stdout).context("ffprobe returned unreadable JSON")?;
    let s = parsed.streams.first().context("no video stream found")?;

    // Spatial metadata arrives as side data; absent stereo means mono, as in the Python.
    let mut stereo = "mono".to_string();
    let mut projection = None;
    for sd in s.side_data_list.iter().flatten() {
        match sd.side_data_type.as_deref() {
            Some("Stereo 3D") => {
                if let Some(t) = &sd.stereo_type {
                    stereo = t.clone();
                }
            }
            Some("Spherical Mapping") => projection = sd.projection.clone(),
            _ => {}
        }
    }

    Ok(VideoInfo {
        width: s.width.context("stream has no width")?,
        height: s.height.context("stream has no height")?,
        fps: parse_fps(s.r_frame_rate.as_deref().unwrap_or("30/1")),
        codec: s.codec_name.clone().unwrap_or_else(|| "?".into()),
        pix_fmt: s.pix_fmt.clone().unwrap_or_else(|| "?".into()),
        bitrate: s.bit_rate.as_deref().and_then(|b| b.parse().ok()).unwrap_or(0),
        stereo,
        projection,
        duration: parsed
            .format
            .and_then(|f| f.duration)
            .and_then(|d| d.parse().ok())
            .unwrap_or(0.0),
    })
}

#[cfg(test)]
mod tests {
    use super::parse_fps;

    #[test]
    fn frame_rates_round_like_the_python() {
        assert_eq!(parse_fps("30/1"), 30);
        assert_eq!(parse_fps("60/1"), 60);
        assert_eq!(parse_fps("30000/1001"), 30); // 29.97
        assert_eq!(parse_fps("24"), 24);
        assert_eq!(parse_fps("0/0"), 30); // degenerate -> default
    }
}
