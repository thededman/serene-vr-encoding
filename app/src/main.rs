//! Phase-1 CLI. Mirrors `vr_plan.py`'s output format exactly so the two can be diffed
//! directly while the port is being validated.

use anyhow::Result;
use serene_vr_encoder::{plan, probe};
use std::path::PathBuf;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let files: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();

    let mut max_mbps = plan::DEFAULT_MAX_MBPS;
    let mut px_per_deg = plan::TARGET_PX_PER_DEG;
    for (i, a) in args.iter().enumerate() {
        match a.as_str() {
            "--max-mbps" => max_mbps = args.get(i + 1).and_then(|v| v.parse().ok()).unwrap_or(max_mbps),
            "--px-per-deg" => px_per_deg = args.get(i + 1).and_then(|v| v.parse().ok()).unwrap_or(px_per_deg),
            _ => {}
        }
    }

    // `inject SRC DST STEREO` exists so the Rust injector can be diffed byte-for-byte
    // against inject_spatial.py during the port.
    if files.first().map(|s| s.as_str()) == Some("inject") {
        let (src, dst, stereo) = (files[1], files[2], files[3].as_str());
        let r = serene_vr_encoder::inject::inject(
            &PathBuf::from(src), &PathBuf::from(dst), stereo)?;
        println!("  injected {}x{} equirectangular / {} -> {}",
                 r.width, r.height, stereo,
                 PathBuf::from(dst).file_name().unwrap().to_string_lossy());
        return Ok(());
    }

    let Some(path) = files.first() else {
        eprintln!("usage: serene-vr-encoder <master.mp4> [--max-mbps N] [--px-per-deg N]");
        eprintln!("       serene-vr-encoder inject <src> <dst> <mono|top-bottom|left-right>");
        std::process::exit(1);
    };

    let info = probe::probe(&PathBuf::from(path))?;
    let p = plan::plan(&info, max_mbps, px_per_deg);

    let status = match p.status {
        plan::Status::Ok => "ok",
        plan::Status::Review => "review",
        plan::Status::Unfit => "unfit",
    };

    println!("STATUS={status}");
    println!("WIDTH={}", info.width);
    println!("HEIGHT={}", info.height);
    println!("FPS={}", info.fps);
    println!("CODEC={}", info.codec);
    println!("PIX_FMT={}", info.pix_fmt);
    println!("STEREO={}", info.stereo);
    println!("PROJECTION={}", info.projection.as_deref().unwrap_or(""));
    println!("LAYOUT={}", p.layout.as_str());
    println!("DEGREES={}", p.degrees);
    println!("OUT_W={}", p.out_w);
    println!("OUT_H={}", p.out_h);
    println!("MBPS={}", p.mbps);
    println!("STEREO_FLAG={}", p.layout.stereo_flag());
    println!("NOTE={}", p.note());
    Ok(())
}
