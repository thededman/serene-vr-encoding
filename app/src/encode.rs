//! Run ffmpeg and report progress.
//!
//! The shell scripts scrape ffmpeg's stderr scroll; this uses `-progress pipe:1`, which
//! emits machine-readable `key=value` lines. That is what makes a real progress bar and a
//! per-file ETA possible rather than a wall of text.
//!
//! Encoding runs on its own thread and reports back over a channel, so the UI stays
//! responsive through a multi-hour batch.

use crate::{inject, plan::Plan, probe::VideoInfo};
use anyhow::{bail, Context, Result};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum Update {
    /// Fraction complete (0.0–1.0) and the encoder's speed multiplier.
    Progress { index: usize, fraction: f32, speed: f32 },
    Injecting { index: usize },
    Done { index: usize, output: PathBuf, bytes: u64 },
    Failed { index: usize, error: String },
}

pub struct Job {
    pub index: usize,
    pub source: PathBuf,
    pub info: VideoInfo,
    pub plan: Plan,
    pub out_dir: PathBuf,
    pub preset: u8,
}

/// `final/<stem>_<WxH>_av1_<N>M.mp4` — same convention as encode_vendor.sh, so outputs
/// from the app and the scripts are indistinguishable.
pub fn output_path(job: &Job) -> PathBuf {
    let stem = job.source.file_stem().unwrap_or_default().to_string_lossy();
    job.out_dir.join(format!(
        "{stem}_{}x{}_av1_{}M.mp4",
        job.plan.out_w, job.plan.out_h, job.plan.mbps
    ))
}

fn parse_progress_line(line: &str, duration: f64, speed: &mut f32) -> Option<f32> {
    let (key, value) = line.split_once('=')?;
    match key.trim() {
        "out_time_ms" => {
            let us: f64 = value.trim().parse().ok()?;
            if duration <= 0.0 {
                return None;
            }
            // Despite the name, ffmpeg reports microseconds here.
            Some(((us / 1_000_000.0) / duration).clamp(0.0, 1.0) as f32)
        }
        "speed" => {
            if let Ok(s) = value.trim().trim_end_matches('x').parse::<f32>() {
                *speed = s;
            }
            None
        }
        _ => None,
    }
}

fn kill(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

pub fn run(job: &Job, tx: &Sender<Update>, cancel: &Arc<AtomicBool>) -> Result<()> {
    let dest = output_path(job);
    std::fs::create_dir_all(&job.out_dir)?;

    // Encode to a temporary name so a cancelled or crashed run never leaves something
    // that looks like a finished file.
    let tmp = job.out_dir.join(format!(
        "_{}",
        dest.file_name().unwrap_or_default().to_string_lossy()
    ));

    let mut child = Command::new("ffmpeg")
        .args(["-nostdin", "-hide_banner", "-loglevel", "error", "-progress", "pipe:1", "-i"])
        .arg(&job.source)
        .args([
            "-vf", &format!("scale={}:{}:flags=lanczos", job.plan.out_w, job.plan.out_h),
            "-pix_fmt", "yuv420p",
            // Colour tagging is carried across explicitly; an untagged output can shift
            // in-headset.
            "-color_primaries", "bt709", "-color_trc", "bt709", "-colorspace", "bt709",
            "-c:v", "libsvtav1",
            "-preset", &job.preset.to_string(),
            "-b:v", &format!("{}M", job.plan.mbps),
            "-g", &(job.info.fps * 2).to_string(),
            "-c:a", "aac", "-b:a", "192k", "-ac", "2",
            "-movflags", "+faststart",
        ])
        .arg(&tmp)
        .arg("-y")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("could not start ffmpeg — is it installed?")?;

    let stdout = child.stdout.take().context("no ffmpeg stdout")?;
    let mut speed = 0.0f32;
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        if cancel.load(Ordering::Relaxed) {
            kill(&mut child);
            let _ = std::fs::remove_file(&tmp);
            bail!("cancelled");
        }
        if let Some(fraction) = parse_progress_line(&line, job.info.duration, &mut speed) {
            let _ = tx.send(Update::Progress { index: job.index, fraction, speed });
        }
    }

    let status = child.wait()?;
    if !status.success() {
        let _ = std::fs::remove_file(&tmp);
        bail!("ffmpeg exited with {status}");
    }

    // FFmpeg has just stripped the spatial metadata, as it does on every output. Put it
    // back before anyone can mistake the file for finished.
    let _ = tx.send(Update::Injecting { index: job.index });
    inject::inject(&tmp, &dest, job.plan.layout.stereo_flag())
        .context("metadata injection failed")?;
    std::fs::remove_file(&tmp).ok();

    let bytes = std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
    let _ = tx.send(Update::Done { index: job.index, output: dest, bytes });
    Ok(())
}

/// Confirm ffmpeg is present and can actually encode AV1 — the single most common reason
/// the toolkit fails on a new machine.
pub fn preflight() -> std::result::Result<(), String> {
    let out = Command::new("ffmpeg").args(["-hide_banner", "-encoders"]).output();
    match out {
        Err(_) => Err("ffmpeg not found.\n\nInstall it with:    brew install ffmpeg".into()),
        Ok(o) => {
            if String::from_utf8_lossy(&o.stdout).contains("libsvtav1") {
                Ok(())
            } else {
                Err("This ffmpeg cannot encode AV1 (libsvtav1 is missing).\n\n\
                     Fix with:    brew uninstall ffmpeg && brew install ffmpeg".into())
            }
        }
    }
}

pub fn reveal_in_finder(path: &Path) {
    let _ = Command::new("open").arg("-R").arg(path).status();
}

#[cfg(test)]
mod tests {
    use super::parse_progress_line;

    #[test]
    fn progress_lines_convert_microseconds_to_a_fraction() {
        let mut speed = 0.0;
        // Halfway through a 100-second video.
        assert_eq!(parse_progress_line("out_time_ms=50000000", 100.0, &mut speed), Some(0.5));
        // Past the end clamps rather than overshooting the bar.
        assert_eq!(parse_progress_line("out_time_ms=200000000", 100.0, &mut speed), Some(1.0));
        // Unknown duration yields nothing rather than a bogus fraction.
        assert_eq!(parse_progress_line("out_time_ms=5000000", 0.0, &mut speed), None);
    }

    #[test]
    fn speed_is_captured_without_affecting_progress() {
        let mut speed = 0.0;
        assert_eq!(parse_progress_line("speed=0.42x", 100.0, &mut speed), None);
        assert!((speed - 0.42).abs() < 1e-6);
    }
}
