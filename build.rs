// Generates the app icon procedurally (no binary assets checked in) and embeds it,
// plus a manifest for per-monitor DPI awareness and common controls.
use std::io::Write;
use std::path::Path;

fn draw_icon(size: u32) -> Vec<u8> {
    // BGRA top-down buffer. Dark rounded tile with a light "lid" panel folding down.
    let s = size as f32;
    let mut px = vec![0u8; (size * size * 4) as usize];
    let radius = s * 0.22;
    let panel_x0 = s * 0.20;
    let panel_x1 = s * 0.80;
    for y in 0..size {
        for x in 0..size {
            let fx = x as f32 + 0.5;
            let fy = y as f32 + 0.5;
            // rounded-rect coverage
            let dx = (fx - s / 2.0).abs() - (s / 2.0 - radius);
            let dy = (fy - s / 2.0).abs() - (s / 2.0 - radius);
            let d = (dx.max(0.0).powi(2) + dy.max(0.0).powi(2)).sqrt() + dx.max(dy).min(0.0) - radius;
            let cov = (0.5 - d).clamp(0.0, 1.0);
            if cov <= 0.0 { continue; }
            // background gradient (near-black to deep slate)
            let t = fy / s;
            let (mut r, mut g, mut b) = (18.0 + 14.0 * t, 18.0 + 16.0 * t, 24.0 + 24.0 * t);
            // lid panel: trapezoid, top edge narrower & higher = tilted away
            let hinge_y = s * 0.78;
            let top_y = s * 0.30;
            if fy >= top_y && fy <= hinge_y && fx >= panel_x0 && fx <= panel_x1 {
                let v = (fy - top_y) / (hinge_y - top_y); // 0 top .. 1 hinge
                let inset = (1.0 - v) * s * 0.10;
                if fx >= panel_x0 + inset && fx <= panel_x1 - inset {
                    let shade = 0.55 + 0.45 * v;
                    r = 235.0 * shade; g = 240.0 * shade; b = 255.0 * shade;
                }
            }
            // hinge line
            if (fy - hinge_y).abs() < s * 0.02 && fx >= panel_x0 && fx <= panel_x1 {
                r = 120.0; g = 190.0; b = 255.0;
            }
            let i = ((y * size + x) * 4) as usize;
            px[i] = (b * cov) as u8;
            px[i + 1] = (g * cov) as u8;
            px[i + 2] = (r * cov) as u8;
            px[i + 3] = (255.0 * cov) as u8;
        }
    }
    px
}

fn write_ico(path: &Path, sizes: &[u32]) {
    let mut images = Vec::new();
    for &sz in sizes {
        let mut bgra = draw_icon(sz);
        // ICO DIB rows are bottom-up
        let row = (sz * 4) as usize;
        let mut flipped = Vec::with_capacity(bgra.len());
        for y in (0..sz as usize).rev() { flipped.extend_from_slice(&bgra[y * row..(y + 1) * row]); }
        bgra = flipped;
        let mask_row = (((sz + 31) / 32) * 4) as usize;
        let mask = vec![0u8; mask_row * sz as usize];
        let mut dib = Vec::new();
        dib.extend_from_slice(&40u32.to_le_bytes());
        dib.extend_from_slice(&(sz as i32).to_le_bytes());
        dib.extend_from_slice(&((sz * 2) as i32).to_le_bytes());
        dib.extend_from_slice(&1u16.to_le_bytes());
        dib.extend_from_slice(&32u16.to_le_bytes());
        dib.extend_from_slice(&0u32.to_le_bytes());
        dib.extend_from_slice(&((bgra.len() + mask.len()) as u32).to_le_bytes());
        dib.extend_from_slice(&[0u8; 16]);
        dib.extend_from_slice(&bgra);
        dib.extend_from_slice(&mask);
        images.push((sz, dib));
    }
    let mut out = Vec::new();
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(images.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * images.len() as u32;
    for (sz, dib) in &images {
        let b = if *sz >= 256 { 0u8 } else { *sz as u8 };
        out.push(b); out.push(b);
        out.push(0); out.push(0);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&(dib.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += dib.len() as u32;
    }
    for (_, dib) in &images { out.extend_from_slice(dib); }
    std::fs::File::create(path).unwrap().write_all(&out).unwrap();
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/winbend.manifest");
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let ico = Path::new(&out_dir).join("winbend.ico");
    write_ico(&ico, &[16, 24, 32, 48, 64, 128, 256]);
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap() == "windows" {
        let mut res = winresource::WindowsResource::new();
        res.set_icon(ico.to_str().unwrap());
        res.set_manifest_file("assets/winbend.manifest");
        res.set("ProductName", "WinBend");
        res.set("FileDescription", "WinBend - your desktop folds like a lid closing");
        res.set("LegalCopyright", "Copyright (c) 2026");
        res.compile().expect("resource compile failed");
    }
}
