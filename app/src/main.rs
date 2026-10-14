//! Serene VR Encoder.
//!
//! Launches the app window by default. The `plan` and `inject` subcommands expose the
//! same logic on the command line — they exist so the Rust implementation can be diffed
//! directly against the Python reference during the port, and stay useful for scripting.

use anyhow::Result;
use serene_vr_encoder::{inject, plan, probe, ui};
use std::path::PathBuf;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(String::as_str) {
        Some("plan") => cli_plan(&args[1..]),
        Some("inject") => cli_inject(&args[1..]),
        Some("convert") => cli_convert(&args[1..]),
        Some("--help") | Some("-h") => {
            println!("serene-vr-encoder                     launch the app");
            println!("serene-vr-encoder plan <file>         print the encode plan");
            println!("serene-vr-encoder convert <file> [out-dir]   encode one file");
            println!("serene-vr-encoder inject <src> <dst> <mono|top-bottom|left-right>");
            Ok(())
        }
        _ => launch_gui(),
    }
}

fn launch_gui() -> Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([720.0, 560.0])
            .with_min_inner_size([520.0, 400.0])
            .with_title("Serene VR Encoder")
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native(
        "Serene VR Encoder",
        options,
        Box::new(|_cc| Ok(Box::<ui::App>::default())),
    )
    .map_err(|e| anyhow::anyhow!("could not open the window: {e}"))
}

/// Same KEY=VALUE output as vr_plan.py, so the two can be diffed line for line.
fn cli_plan(args: &[String]) -> Result<()> {
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
    let Some(path) = files.first() else {
        anyhow::bail!("usage: serene-vr-encoder plan <file>");
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

fn cli_inject(args: &[String]) -> Result<()> {
    if args.len() < 3 {
        anyhow::bail!("usage: serene-vr-encoder inject <src> <dst> <mono|top-bottom|left-right>");
    }
    let r = inject::inject(&PathBuf::from(&args[0]), &PathBuf::from(&args[1]), &args[2])?;
    println!(
        "  injected {}x{} equirectangular / {} -> {}",
        r.width, r.height, args[2],
        PathBuf::from(&args[1]).file_name().unwrap_or_default().to_string_lossy()
    );
    Ok(())
}

/// Encode one file through exactly the path the GUI uses — same planner, same ffmpeg
/// invocation, same metadata injection. Scriptable, and the way the pipeline is tested.
fn cli_convert(args: &[String]) -> Result<()> {
    use serene_vr_encoder::encode::{self, Job, Update};
    use std::sync::atomic::AtomicBool;
    use std::sync::mpsc::channel;
    use std::sync::Arc;

    let Some(src) = args.first() else {
        anyhow::bail!("usage: serene-vr-encoder convert <file> [out-dir]");
    };
    if let Err(e) = encode::preflight() {
        anyhow::bail!(e);
    }

    let source = PathBuf::from(src);
    let info = probe::probe(&source)?;
    let p = plan::plan(&info, plan::DEFAULT_MAX_MBPS, plan::TARGET_PX_PER_DEG);
    if p.status == plan::Status::Unfit {
        anyhow::bail!("cannot convert: {}", p.note());
    }
    if p.status == plan::Status::Review {
        eprintln!("note: {}", p.note());
    }

    let out_dir = args.get(1).map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default().join("final"));

    let job = Job { index: 0, source, info, plan: p, out_dir, preset: 6 };
    println!("-> {}", encode::output_path(&job).display());

    let (tx, rx) = channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let worker = std::thread::spawn(move || encode::run(&job, &tx, &cancel));

    let mut last = -1i32;
    while let Ok(u) = rx.recv() {
        match u {
            Update::Progress { fraction, speed, .. } => {
                let pct = (fraction * 100.0) as i32;
                if pct / 5 != last / 5 {
                    print!("\r   {pct:3}%  {speed:.2}x");
                    use std::io::Write;
                    let _ = std::io::stdout().flush();
                    last = pct;
                }
            }
            Update::Injecting { .. } => println!("\r   restoring 360 metadata…      "),
            Update::Done { bytes, .. } => println!("   done — {:.0} MB", bytes as f64 / 1048576.0),
            Update::Failed { error, .. } => eprintln!("   failed: {error}"),
        }
    }
    worker.join().map_err(|_| anyhow::anyhow!("worker panicked"))??;
    Ok(())
}
