//! The app window: drop videos in, see what they are, convert them.
//!
//! Encoding runs on a worker thread and reports over a channel, so a multi-hour batch
//! never blocks the UI. Files the planner flags (mis-tagged stereo, ambiguous square
//! frames) are shown with their warning and must be approved explicitly — the same
//! refuse-to-guess stance as inspect_master.sh, because those are exactly the cases where
//! proceeding silently produces a file that looks fine and plays wrong.

use crate::encode::{self, Job, Update};
use crate::plan::{plan, Plan, Status, DEFAULT_MAX_MBPS, TARGET_PX_PER_DEG};
use crate::probe::{probe, VideoInfo};
use eframe::egui;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

#[derive(Clone, PartialEq)]
enum State {
    Ready,
    NeedsApproval,
    Approved,
    Encoding { fraction: f32, speed: f32 },
    Injecting,
    Done { output: PathBuf, bytes: u64 },
    Failed(String),
    Unfit(String),
}

struct Entry {
    source: PathBuf,
    info: Option<VideoInfo>,
    plan: Option<Plan>,
    state: State,
}

impl Entry {
    fn load(source: PathBuf) -> Self {
        match probe(&source) {
            Err(e) => Entry { source, info: None, plan: None, state: State::Failed(e.to_string()) },
            Ok(info) => {
                let p = plan(&info, DEFAULT_MAX_MBPS, TARGET_PX_PER_DEG);
                let state = match p.status {
                    Status::Unfit => State::Unfit(p.note()),
                    Status::Review => State::NeedsApproval,
                    Status::Ok => State::Ready,
                };
                Entry { source, info: Some(info), plan: Some(p), state }
            }
        }
    }

    fn will_run(&self) -> bool {
        matches!(self.state, State::Ready | State::Approved)
    }
}

pub struct App {
    entries: Vec<Entry>,
    out_dir: PathBuf,
    preset: u8,
    running: bool,
    cancel: Arc<AtomicBool>,
    tx: Sender<Update>,
    rx: Receiver<Update>,
    ffmpeg_error: Option<String>,
}

impl Default for App {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self {
            entries: Vec::new(),
            out_dir: dirs_final(),
            preset: 6,
            running: false,
            cancel: Arc::new(AtomicBool::new(false)),
            tx,
            rx,
            ffmpeg_error: encode::preflight().err(),
        }
    }
}

fn dirs_final() -> PathBuf {
    std::env::current_dir().unwrap_or_default().join("final")
}

fn human(bytes: u64) -> String {
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;
    let b = bytes as f64;
    if b >= GB { format!("{:.1} GB", b / GB) } else { format!("{:.0} MB", b / (1024.0 * 1024.0)) }
}

fn mmss(seconds: f64) -> String {
    let s = seconds.max(0.0) as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

impl App {
    fn add_files(&mut self, paths: Vec<PathBuf>) {
        for p in paths {
            let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            if !["mp4", "mov", "mkv"].contains(&ext.as_str()) {
                continue;
            }
            if self.entries.iter().any(|e| e.source == p) {
                continue;
            }
            self.entries.push(Entry::load(p));
        }
    }

    fn start(&mut self) {
        let jobs: Vec<Job> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.will_run())
            .map(|(i, e)| Job {
                index: i,
                source: e.source.clone(),
                info: e.info.clone().unwrap(),
                plan: e.plan.clone().unwrap(),
                out_dir: self.out_dir.clone(),
                preset: self.preset,
            })
            .collect();

        if jobs.is_empty() {
            return;
        }
        self.running = true;
        self.cancel.store(false, Ordering::Relaxed);

        let tx = self.tx.clone();
        let cancel = self.cancel.clone();
        // One worker, sequential: the encoder already saturates every core, so running
        // files in parallel would only make them all finish later.
        std::thread::spawn(move || {
            for job in jobs {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                if let Err(e) = encode::run(&job, &tx, &cancel) {
                    let _ = tx.send(Update::Failed { index: job.index, error: e.to_string() });
                }
            }
        });
    }

    fn drain(&mut self) {
        while let Ok(u) = self.rx.try_recv() {
            match u {
                Update::Progress { index, fraction, speed } => {
                    if let Some(e) = self.entries.get_mut(index) {
                        e.state = State::Encoding { fraction, speed };
                    }
                }
                Update::Injecting { index } => {
                    if let Some(e) = self.entries.get_mut(index) {
                        e.state = State::Injecting;
                    }
                }
                Update::Done { index, output, bytes } => {
                    if let Some(e) = self.entries.get_mut(index) {
                        e.state = State::Done { output, bytes };
                    }
                }
                Update::Failed { index, error } => {
                    if let Some(e) = self.entries.get_mut(index) {
                        e.state = State::Failed(error);
                    }
                }
            }
        }
        if self.running
            && !self.entries.iter().any(|e| {
                matches!(e.state, State::Encoding { .. } | State::Injecting)
                    || (e.will_run() && !matches!(e.state, State::Done { .. }))
            })
        {
            self.running = false;
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain();
        if self.running {
            // Keep the progress bars moving without spinning the CPU.
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }

        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect()
        });
        if !dropped.is_empty() {
            self.add_files(dropped);
        }

        egui::TopBottomPanel::top("head").show(ctx, |ui| {
            ui.add_space(8.0);
            ui.heading("Serene VR Encoder");
            ui.label(
                egui::RichText::new("360° video for Meta Quest — AV1, metadata restored, faststart")
                    .small()
                    .weak(),
            );
            ui.add_space(8.0);
        });

        if let Some(err) = self.ffmpeg_error.clone() {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.add_space(40.0);
                ui.vertical_centered(|ui| {
                    ui.heading("ffmpeg is not ready");
                    ui.add_space(12.0);
                    ui.label(err);
                    ui.add_space(16.0);
                    if ui.button("Check again").clicked() {
                        self.ffmpeg_error = encode::preflight().err();
                    }
                });
            });
            return;
        }

        egui::TopBottomPanel::bottom("foot").show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label("Output:");
                let shown = self.out_dir.to_string_lossy();
                ui.label(egui::RichText::new(shown.as_ref()).monospace().small());
                if ui.small_button("Choose…").clicked() {
                    if let Some(d) = rfd::FileDialog::new().pick_folder() {
                        self.out_dir = d;
                    }
                }
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let pending = self.entries.iter().filter(|e| e.will_run()).count();
                if self.running {
                    if ui.button("Cancel").clicked() {
                        self.cancel.store(true, Ordering::Relaxed);
                    }
                    ui.spinner();
                    ui.label("Encoding — this takes roughly 3× the video's length.");
                } else {
                    let label = if pending == 1 {
                        "Convert 1 file".to_string()
                    } else {
                        format!("Convert {pending} files")
                    };
                    if ui.add_enabled(pending > 0, egui::Button::new(label)).clicked() {
                        self.start();
                    }
                    if !self.entries.is_empty() && ui.button("Clear").clicked() {
                        self.entries.clear();
                    }
                }
            });
            ui.add_space(6.0);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            if self.entries.is_empty() {
                ui.add_space(50.0);
                ui.vertical_centered(|ui| {
                    ui.label(egui::RichText::new("Drop 360° videos here").size(18.0));
                    ui.add_space(10.0);
                    if ui.button("Add files…").clicked() {
                        if let Some(files) = rfd::FileDialog::new()
                            .add_filter("Video", &["mp4", "mov", "mkv"])
                            .pick_files()
                        {
                            self.add_files(files);
                        }
                    }
                });
                return;
            }

            egui::ScrollArea::vertical().show(ui, |ui| {
                let mut approve: Option<usize> = None;
                let mut reveal: Option<PathBuf> = None;

                for (i, e) in self.entries.iter().enumerate() {
                    ui.add_space(6.0);
                    let name = e.source.file_name().unwrap_or_default().to_string_lossy();
                    ui.label(egui::RichText::new(name.as_ref()).strong());

                    if let (Some(info), Some(p)) = (&e.info, &e.plan) {
                        ui.label(
                            egui::RichText::new(format!(
                                "{}×{} {} {}°  {} fps  ·  {}  →  {}×{} AV1 @ {} Mbps",
                                info.width, info.height, p.layout.as_str(), p.degrees,
                                info.fps, mmss(info.duration), p.out_w, p.out_h, p.mbps
                            ))
                            .small()
                            .weak(),
                        );
                    }

                    match &e.state {
                        State::NeedsApproval => {
                            if let Some(p) = &e.plan {
                                ui.colored_label(egui::Color32::from_rgb(190, 120, 40),
                                    format!("⚠ {}", p.note()));
                            }
                            if ui.small_button("Convert anyway").clicked() {
                                approve = Some(i);
                            }
                        }
                        State::Unfit(note) => {
                            ui.colored_label(egui::Color32::from_rgb(180, 70, 60),
                                format!("Cannot convert: {note}"));
                        }
                        State::Encoding { fraction, speed } => {
                            let remaining = e.info.as_ref().map(|inf| {
                                let left = inf.duration * (1.0 - *fraction as f64);
                                if *speed > 0.01 { left / *speed as f64 } else { 0.0 }
                            });
                            let text = match remaining {
                                Some(r) if *speed > 0.01 =>
                                    format!("{:.0}%  ·  {:.2}×  ·  ~{} left",
                                            fraction * 100.0, speed, mmss(r)),
                                _ => format!("{:.0}%", fraction * 100.0),
                            };
                            ui.add(egui::ProgressBar::new(*fraction).text(text));
                        }
                        State::Injecting => {
                            ui.add(egui::ProgressBar::new(1.0).text("restoring 360° metadata…"));
                        }
                        State::Done { output, bytes } => {
                            ui.horizontal(|ui| {
                                ui.colored_label(egui::Color32::from_rgb(60, 140, 90),
                                    format!("✓ done — {}", human(*bytes)));
                                if ui.small_button("Show in Finder").clicked() {
                                    reveal = Some(output.clone());
                                }
                            });
                        }
                        State::Failed(err) => {
                            ui.colored_label(egui::Color32::from_rgb(180, 70, 60),
                                format!("✗ {err}"));
                        }
                        State::Ready | State::Approved => {
                            ui.label(egui::RichText::new("ready").small().weak());
                        }
                    }
                    ui.separator();
                }

                if let Some(i) = approve {
                    self.entries[i].state = State::Approved;
                }
                if let Some(p) = reveal {
                    encode::reveal_in_finder(&p);
                }
            });
        });
    }
}
