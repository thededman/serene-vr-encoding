//! Decide how a master should be encoded.
//!
//! Faithful port of `vr_plan.py` identify()/plan(). The Python remains the reference
//! implementation; `tests/plan_cases.rs` pins this against its known-correct outputs.
//!
//! Every constant here was measured, not assumed — see docs/ENCODING-GUIDE.md.

use crate::probe::VideoInfo;

// --- Limits read off the Quest 3S (/vendor/etc/media_codecs.xml, device "panther") ----
const BLOCKS_PER_SEC: f64 = 7_776_000.0; // decoder throughput, 16x16 blocks
const SAFETY: f64 = 0.80; // a saturated decoder drops frames rather than erroring
const MAX_PIXEL_RATE: f64 = BLOCKS_PER_SEC * 256.0 * SAFETY;

const SPEC_MAX_PIXELS: f64 = 35_651_584.0; // AV1/HEVC Level 6.2 max luma per frame
const DEVICE_MAX_DIM: u32 = 8192;

// --- House standard, validated by the 7-way A/B test ----------------------------------
pub const TARGET_PX_PER_DEG: f64 = 16.0; // Quest 3S resolves ~19; 16 is the sweet spot
const BITS_PER_PIXEL: f64 = 0.0346; // from the winning encode: 34.4 Mbps at 5760x5760@30
pub const DEFAULT_MAX_MBPS: f64 = 35.0;
const MIN_MBPS: u32 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    Tb,
    Sbs,
    Mono,
}

impl Layout {
    /// The value `inject_spatial` expects for `--stereo`.
    pub fn stereo_flag(self) -> &'static str {
        match self {
            Layout::Tb => "top-bottom",
            Layout::Sbs => "left-right",
            Layout::Mono => "mono",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Layout::Tb => "tb",
            Layout::Sbs => "sbs",
            Layout::Mono => "mono",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Encode with the values given.
    Ok,
    /// Encodable, but a human should look at the note first.
    Review,
    /// Cannot be handled automatically.
    Unfit,
}

#[derive(Debug, Clone)]
pub struct Plan {
    pub status: Status,
    pub layout: Layout,
    pub degrees: u32,
    pub out_w: u32,
    pub out_h: u32,
    pub mbps: u32,
    pub notes: Vec<String>,
}

impl Plan {
    pub fn note(&self) -> String {
        self.notes.join("; ")
    }
}

/// Round to a multiple of the macroblock size, minimum one block.
fn align(n: f64) -> u32 {
    let v = ((n / 16.0).round() as i64) * 16;
    v.max(16) as u32
}

struct Identified {
    layout: Layout,
    eye_w: u32,
    eye_h: u32,
    degrees: u32,
    note: String,
}

/// Work out the stereo layout and how much of the sphere one eye covers.
///
/// Coverage is inferred from the per-eye aspect ratio: a 2:1 eye is a full 360x180
/// equirect, a 1:1 eye is 180x180.
fn identify(info: &VideoInfo) -> Identified {
    let (w, h) = (info.width, info.height);
    let stereo = info.stereo.as_str();

    let (layout, eye_w, eye_h) = if stereo.contains("top") {
        (Layout::Tb, w, h / 2)
    } else if stereo.contains("side") {
        (Layout::Sbs, w / 2, h)
    } else {
        (Layout::Mono, w, h)
    };

    if eye_h == 0 {
        return Identified { layout, eye_w, eye_h, degrees: 0,
            note: "could not determine eye dimensions".into() };
    }

    let ratio = eye_w as f64 / eye_h as f64;

    // A square frame carrying NO stereo tag is genuinely ambiguous: it is either 360
    // stereo packed top-bottom (the standard 8K VR layout) or a VR180 mono frame. Any
    // FFmpeg pass strips the stereo tag, so intermediate files routinely arrive like
    // this. Top-bottom 360 is overwhelmingly the common case at these sizes — guessing
    // VR180 mono instead silently halves the resolution and mangles the projection.
    if layout == Layout::Mono && (ratio - 1.0).abs() < 0.05 {
        return Identified {
            layout: Layout::Tb, eye_w, eye_h: eye_h / 2, degrees: 360,
            note: format!(
                "{w}x{h} is square with no stereo tag (ambiguous). ASSUMING 360 STEREO \
                 TOP-BOTTOM, the usual layout at this size. If it is really VR180 mono, \
                 the output will be wrong -- check one frame before trusting a batch"),
        };
    }

    // Sanity-check the declared stereo layout against the geometry. A correct per-eye
    // frame is 2:1 (360x180) or 1:1 (180x180). Anything wildly outside that means the
    // stereo tag is wrong — most often a mono equirect exported with a spurious stereo
    // flag. Trusting such a tag makes a player split one image in half and send unrelated
    // halves to each eye, which looks like permanent blur.
    if layout != Layout::Mono && (ratio > 3.0 || ratio < 0.6) {
        let whole = if h > 0 { w as f64 / h as f64 } else { 0.0 };
        if whole > 1.7 && whole < 2.3 {
            return Identified {
                layout: Layout::Mono, eye_w: w, eye_h: h, degrees: 360,
                note: format!(
                    "file declares '{stereo}' stereo, but that would make each eye \
                     {eye_w}x{eye_h} ({ratio:.1}:1). The full frame is {w}x{h} \
                     ({whole:.1}:1), a normal mono 360 equirect. TREATING AS MONO -- the \
                     stereo tag is wrong and will cause blur if a player honours it"),
            };
        }
        return Identified { layout, eye_w, eye_h, degrees: 360,
            note: format!("per-eye aspect {ratio:.1}:1 is not valid for '{stereo}' \
                           stereo; check manually") };
    }

    if ratio > 1.6 {
        return Identified { layout, eye_w, eye_h, degrees: 360, note: String::new() };
    }
    if ratio < 1.3 {
        return Identified { layout, eye_w, eye_h, degrees: 180, note: String::new() };
    }
    Identified { layout, eye_w, eye_h, degrees: 360,
        note: format!("unusual per-eye aspect {ratio:.2}, assuming 360") }
}

pub fn plan(info: &VideoInfo, max_mbps: f64, px_per_deg: f64) -> Plan {
    let id = identify(info);
    let mut notes: Vec<String> = Vec::new();
    if !id.note.is_empty() {
        notes.push(id.note.clone());
    }

    let mut status = if ["TREATING AS MONO", "check manually", "ASSUMING 360 STEREO"]
        .iter()
        .any(|k| id.note.contains(k))
    {
        Status::Review
    } else {
        Status::Ok
    };

    let unfit = |notes: Vec<String>| Plan {
        status: Status::Unfit, layout: id.layout, degrees: id.degrees,
        out_w: 0, out_h: 0, mbps: 0, notes,
    };

    match info.projection.as_deref() {
        None => {
            notes.push("no 360 metadata in source; assuming equirectangular".into());
            status = Status::Review;
        }
        Some("equirectangular") => {}
        Some(other) => {
            return unfit(vec![format!(
                "projection is {other}, not equirectangular — handle manually")]);
        }
    }

    if info.fps > 60 {
        return unfit(vec![format!(
            "{} fps exceeds what the budget supports — request a 30 fps master", info.fps)]);
    }

    if info.pix_fmt.contains("10") || info.pix_fmt.contains("12") {
        notes.push(format!(
            "source is {} (10-bit/HDR); output forced to 8-bit", info.pix_fmt));
        status = Status::Review;
    }

    // --- ideal per-eye size at the target pixel density ---
    let mut eye_w = align(px_per_deg * id.degrees as f64);
    let mut eye_h = align(if id.degrees == 360 { eye_w as f64 / 2.0 } else { eye_w as f64 });

    // Never upscale — a vendor master smaller than target stays at its own size.
    if id.eye_w < eye_w {
        eye_w = align(id.eye_w as f64);
        eye_h = align(id.eye_h as f64);
        notes.push(format!(
            "source is only {}px per eye, below the {} px/deg target; keeping source \
             resolution", id.eye_w, px_per_deg));
    }

    let (mut out_w, mut out_h) = match id.layout {
        Layout::Tb => (eye_w, eye_h * 2),
        Layout::Sbs => (eye_w * 2, eye_h),
        Layout::Mono => (eye_w, eye_h),
    };

    // --- clamp to the three hard limits, preserving aspect ratio ---
    for (limit, label) in [
        (SPEC_MAX_PIXELS, "codec level"),
        (MAX_PIXEL_RATE / info.fps as f64, "decoder throughput"),
    ] {
        let px = out_w as f64 * out_h as f64;
        if px > limit {
            let f = (limit / px).sqrt();
            out_w = align(out_w as f64 * f);
            out_h = align(out_h as f64 * f);
            notes.push(format!(
                "reduced to {out_w}x{out_h} to stay within {label} at {} fps", info.fps));
        }
    }

    if out_w.max(out_h) > DEVICE_MAX_DIM {
        let f = DEVICE_MAX_DIM as f64 / out_w.max(out_h) as f64;
        out_w = align(out_w as f64 * f);
        out_h = align(out_h as f64 * f);
        notes.push(format!(
            "reduced to {out_w}x{out_h} to stay within the 8192px device limit"));
    }

    // --- bitrate: hold bits-per-pixel constant, then apply the streaming budget ---
    let mut mbps = BITS_PER_PIXEL * out_w as f64 * out_h as f64 * info.fps as f64 / 1e6;
    if mbps > max_mbps {
        notes.push(format!(
            "ideal bitrate {mbps:.0} Mbps exceeds the {max_mbps:.0} Mbps budget; capped, \
             so this will be slightly softer than the 30 fps standard"));
        mbps = max_mbps;
        status = Status::Review;
    }
    let mbps = (mbps.round() as u32).max(MIN_MBPS);

    Plan { status, layout: id.layout, degrees: id.degrees, out_w, out_h, mbps, notes }
}
