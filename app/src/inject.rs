//! Restore the 360/stereo metadata FFmpeg strips from every output.
//!
//! Faithful port of `inject_spatial.py`. FFmpeg discards spatial metadata on every MP4 it
//! writes — including `-c copy` — and without it a player shows a flat, doubled image
//! instead of a sphere. That failure is invisible to every check except wearing a headset,
//! so `tests/golden_inject.rs` asserts this produces output byte-identical to the Python.
//!
//! Both metadata generations are written:
//!   * V1 — RDF/XML in a `uuid` box on the video trak (what older players read)
//!   * V2 — `sv3d` + `st3d` inside the video sample entry (what modern players prefer)

use anyhow::{bail, Context, Result};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

const SPHERICAL_V1_UUID: [u8; 16] = [
    0xff, 0xcc, 0x82, 0x63, 0xf8, 0x55, 0x4a, 0x93,
    0x88, 0x14, 0x58, 0x7a, 0x02, 0x52, 0x1f, 0xdd,
];

/// Containers whose payload is just more boxes, so we can recurse straight in.
const CONTAINERS: [&[u8; 4]; 7] = [b"moov", b"trak", b"mdia", b"minf", b"stbl", b"edts", b"udta"];

fn stereo_mode(stereo: &str) -> Result<u8> {
    Ok(match stereo {
        "mono" => 0,
        "top-bottom" => 1,
        "left-right" => 2,
        other => bail!("unknown stereo mode '{other}'"),
    })
}

// --- box primitives -------------------------------------------------------------------

fn bx(box_type: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(payload.len() + 8);
    v.extend_from_slice(&((payload.len() + 8) as u32).to_be_bytes());
    v.extend_from_slice(box_type);
    v.extend_from_slice(payload);
    v
}

/// A FullBox: a box whose payload starts with a version byte and three flag bytes.
fn full_box(box_type: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut p = vec![0u8; 4]; // version 0, flags 0
    p.extend_from_slice(payload);
    bx(box_type, &p)
}

#[derive(Debug, Clone, Copy)]
struct BoxRef {
    btype: [u8; 4],
    header: usize,
    pos: usize,
    size: usize,
}

/// Walk the boxes in `buf[start..end]`, stopping on anything malformed.
fn iter_boxes(buf: &[u8], start: usize, end: usize) -> Vec<BoxRef> {
    let mut out = Vec::new();
    let mut pos = start;
    while pos + 8 <= end {
        let mut size = u32::from_be_bytes(buf[pos..pos + 4].try_into().unwrap()) as usize;
        let btype: [u8; 4] = buf[pos + 4..pos + 8].try_into().unwrap();
        let mut header = 8;
        if size == 1 {
            if pos + 16 > end {
                break;
            }
            size = u64::from_be_bytes(buf[pos + 8..pos + 16].try_into().unwrap()) as usize;
            header = 16;
        } else if size == 0 {
            size = end - pos;
        }
        if size < header || pos + size > end {
            break;
        }
        out.push(BoxRef { btype, header, pos, size });
        pos += size;
    }
    out
}

fn find_box(buf: &[u8], want: &[u8; 4], start: usize, end: usize) -> Option<BoxRef> {
    iter_boxes(buf, start, end).into_iter().find(|b| &b.btype == want)
}

// --- metadata builders ----------------------------------------------------------------

/// Spherical Video V1: RDF/XML inside a uuid box. Full-frame crop, no partial pano.
fn build_v1_uuid(width: u16, height: u16, stereo: &str) -> Vec<u8> {
    let stereo_tag = if stereo == "mono" {
        String::new()
    } else {
        format!("<GSpherical:StereoMode>{stereo}</GSpherical:StereoMode>")
    };

    let xml = format!(
        "<?xml version=\"1.0\"?>\
<rdf:SphericalVideo \
xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\" \
xmlns:GSpherical=\"http://ns.google.com/videos/1.0/spherical/\">\
<GSpherical:Spherical>true</GSpherical:Spherical>\
<GSpherical:Stitched>true</GSpherical:Stitched>\
<GSpherical:StitchingSoftware>Serene VR pipeline</GSpherical:StitchingSoftware>\
<GSpherical:ProjectionType>equirectangular</GSpherical:ProjectionType>\
{stereo_tag}\
<GSpherical:FullPanoWidthPixels>{width}</GSpherical:FullPanoWidthPixels>\
<GSpherical:FullPanoHeightPixels>{height}</GSpherical:FullPanoHeightPixels>\
<GSpherical:CroppedAreaImageWidthPixels>{width}</GSpherical:CroppedAreaImageWidthPixels>\
<GSpherical:CroppedAreaImageHeightPixels>{height}</GSpherical:CroppedAreaImageHeightPixels>\
<GSpherical:CroppedAreaLeftPixels>0</GSpherical:CroppedAreaLeftPixels>\
<GSpherical:CroppedAreaTopPixels>0</GSpherical:CroppedAreaTopPixels>\
</rdf:SphericalVideo>"
    );

    let mut payload = SPHERICAL_V1_UUID.to_vec();
    payload.extend_from_slice(xml.as_bytes());
    bx(b"uuid", &payload)
}

/// Spherical Video V2 projection box: full equirectangular sphere, no pose offset.
fn build_sv3d() -> Vec<u8> {
    let svhd = full_box(b"svhd", b"Serene VR pipeline\x00");
    let prhd = full_box(b"prhd", &[0u8; 12]); // yaw, pitch, roll
    let equi = full_box(b"equi", &[0u8; 16]); // full sphere bounds

    let mut proj_payload = prhd;
    proj_payload.extend_from_slice(&equi);
    let proj = bx(b"proj", &proj_payload);

    let mut sv3d_payload = svhd;
    sv3d_payload.extend_from_slice(&proj);
    bx(b"sv3d", &sv3d_payload)
}

fn build_st3d(stereo: &str) -> Result<Vec<u8>> {
    Ok(full_box(b"st3d", &[stereo_mode(stereo)?]))
}

// --- moov surgery ---------------------------------------------------------------------

/// Locate the trak whose handler is 'vide'.
fn video_trak_range(moov: &[u8]) -> Result<(usize, usize)> {
    let moov_header = if u32::from_be_bytes(moov[0..4].try_into().unwrap()) == 1 { 16 } else { 8 };
    for b in iter_boxes(moov, moov_header, moov.len()) {
        if &b.btype != b"trak" {
            continue;
        }
        if let Some(mdia) = find_box(moov, b"mdia", b.pos + b.header, b.pos + b.size) {
            if let Some(h) = find_box(moov, b"hdlr", mdia.pos + mdia.header, mdia.pos + mdia.size) {
                // hdlr payload: version/flags(4) + pre_defined(4) + handler_type(4)
                let at = h.pos + h.header + 8;
                if moov.get(at..at + 4) == Some(b"vide") {
                    return Ok((b.pos, b.size));
                }
            }
        }
    }
    bail!("no video track found")
}

/// Grow every ancestor box header by `delta`.
fn patch_sizes(moov: &mut [u8], ancestors: &[usize], delta: usize) {
    for &pos in ancestors {
        let size = u32::from_be_bytes(moov[pos..pos + 4].try_into().unwrap());
        if size == 1 {
            let big = u64::from_be_bytes(moov[pos + 8..pos + 16].try_into().unwrap());
            moov[pos + 8..pos + 16].copy_from_slice(&(big + delta as u64).to_be_bytes());
        } else {
            moov[pos..pos + 4].copy_from_slice(&(size + delta as u32).to_be_bytes());
        }
    }
}

/// moov grew and sits before mdat, so every chunk offset must move by the same delta.
fn shift_chunk_offsets(moov: &mut Vec<u8>, delta: usize) {
    let tops: Vec<BoxRef> = iter_boxes(moov, 0, moov.len());
    for b in tops {
        if &b.btype == b"moov" {
            shift_recursive(moov, b.pos + b.header, b.pos + b.size, delta);
        }
    }
}

fn shift_recursive(moov: &mut Vec<u8>, start: usize, end: usize, delta: usize) {
    let kids: Vec<BoxRef> = iter_boxes(moov, start, end);
    for b in kids {
        if CONTAINERS.iter().any(|c| *c == &b.btype) {
            shift_recursive(moov, b.pos + b.header, b.pos + b.size, delta);
        } else if &b.btype == b"stco" || &b.btype == b"co64" {
            let body = b.pos + b.header + 4; // skip version/flags
            let count = u32::from_be_bytes(moov[body..body + 4].try_into().unwrap()) as usize;
            let wide = &b.btype == b"co64";
            let width = if wide { 8 } else { 4 };
            let mut p = body + 4;
            for _ in 0..count {
                if p + width > moov.len() {
                    break;
                }
                if wide {
                    let v = u64::from_be_bytes(moov[p..p + 8].try_into().unwrap());
                    moov[p..p + 8].copy_from_slice(&(v + delta as u64).to_be_bytes());
                } else {
                    let v = u32::from_be_bytes(moov[p..p + 4].try_into().unwrap());
                    moov[p..p + 4].copy_from_slice(&(v + delta as u32).to_be_bytes());
                }
                p += width;
            }
        }
    }
}

pub struct Injected {
    pub width: u16,
    pub height: u16,
}

pub fn inject(src: &Path, dst: &Path, stereo: &str) -> Result<Injected> {
    let size_on_disk = std::fs::metadata(src)?.len() as usize;
    let mut f = File::open(src).with_context(|| format!("cannot open {}", src.display()))?;

    // Read the top-level layout without loading the whole (possibly multi-GB) file.
    let mut top: Vec<([u8; 4], usize, usize)> = Vec::new();
    let mut pos = 0usize;
    while pos < size_on_disk {
        f.seek(SeekFrom::Start(pos as u64))?;
        let mut hdr = [0u8; 16];
        let n = f.read(&mut hdr)?;
        if n < 8 {
            break;
        }
        let mut bsize = u32::from_be_bytes(hdr[0..4].try_into().unwrap()) as usize;
        let btype: [u8; 4] = hdr[4..8].try_into().unwrap();
        if bsize == 1 {
            bsize = u64::from_be_bytes(hdr[8..16].try_into().unwrap()) as usize;
        } else if bsize == 0 {
            bsize = size_on_disk - pos;
        }
        if bsize == 0 {
            break;
        }
        top.push((btype, pos, bsize));
        pos += bsize;
    }

    let moov_entry = top.iter().find(|t| &t.0 == b"moov").context("no moov box found")?;
    let mdat_entry = top.iter().find(|t| &t.0 == b"mdat").copied();

    f.seek(SeekFrom::Start(moov_entry.1 as u64))?;
    let mut moov = vec![0u8; moov_entry.2];
    f.read_exact(&mut moov)?;

    let (trak_start, mut trak_size) = video_trak_range(&moov)?;

    // --- walk down to the sample entry, recording every ancestor on the way ------------
    // All of these enclose the insertion point, so each one's size must grow with it.
    let mut ancestors = vec![trak_start];
    let (mut scan_start, mut scan_end) = (trak_start + 8, trak_start + trak_size);
    for name in [b"mdia", b"minf", b"stbl", b"stsd"] {
        let found = find_box(&moov, name, scan_start, scan_end)
            .with_context(|| format!("no {} box found", String::from_utf8_lossy(name)))?;
        ancestors.push(found.pos);
        // stsd is a FullBox carrying an entry count ahead of its children.
        let extra = if name == b"stsd" { 8 } else { 0 };
        scan_start = found.pos + found.header + extra;
        scan_end = found.pos + found.size;
    }

    let entry = *iter_boxes(&moov, scan_start, scan_end)
        .first()
        .context("no sample entry found")?;
    ancestors.push(entry.pos);

    // VisualSampleEntry: 6 reserved + 2 data_ref_index + 16 predefined/reserved,
    // then width and height as 16-bit values.
    let vpos = entry.pos + entry.header + 24;
    let width = u16::from_be_bytes(moov[vpos..vpos + 2].try_into().unwrap());
    let height = u16::from_be_bytes(moov[vpos + 2..vpos + 4].try_into().unwrap());

    // --- V2 boxes go inside the sample entry ------------------------------------------
    let mut v2 = build_sv3d();
    v2.extend_from_slice(&build_st3d(stereo)?);
    let at = entry.pos + entry.size;
    moov.splice(at..at, v2.iter().copied());
    patch_sizes(&mut moov, &ancestors, v2.len());
    trak_size += v2.len();

    // --- V1 uuid box is appended as a direct child of the trak ------------------------
    let v1 = build_v1_uuid(width, height, stereo);
    let at = trak_start + trak_size;
    moov.splice(at..at, v1.iter().copied());
    patch_sizes(&mut moov, &[trak_start], v1.len());

    // moov itself grew by everything we added.
    let delta = v2.len() + v1.len();
    patch_sizes(&mut moov, &[0], delta);

    // If moov precedes mdat (faststart layout), media data slid forward by delta.
    if let Some(m) = mdat_entry {
        if moov_entry.1 < m.1 {
            shift_chunk_offsets(&mut moov, delta);
        }
    }

    // --- write out, streaming mdat rather than buffering it ---------------------------
    let mut fin = File::open(src)?;
    let mut fout = File::create(dst)?;
    let mut buf = vec![0u8; 8 << 20];
    for (btype, bpos, bsize) in &top {
        if btype == b"moov" {
            fout.write_all(&moov)?;
            continue;
        }
        fin.seek(SeekFrom::Start(*bpos as u64))?;
        let mut remaining = *bsize;
        while remaining > 0 {
            let want = remaining.min(buf.len());
            let n = fin.read(&mut buf[..want])?;
            if n == 0 {
                break;
            }
            fout.write_all(&buf[..n])?;
            remaining -= n;
        }
    }

    Ok(Injected { width, height })
}
