//! winbend-composite: put WinBend's fold onto a filmed or generated laptop whose screen is
//! switched off (black).
//!
//!   winbend-composite --video frames/%05d.png-dir --fold fold-dir --out out-dir
//!                     [--threshold 46] [--close-at 0.9] [--gain 0.9] [--reverse]
//!
//! For every video frame: find the largest dark blob near the middle (the screen), take its four
//! corners, smooth them over time, estimate how far the lid has closed from the screen's
//! apparent height, pick the fold frame with that fold amount (fold frames must be rendered
//! with `winbend --render-clip ... --linear`), and warp it into the screen quad with a
//! perspective map. Pixels outside the screen are left untouched.
use image::{Rgb, RgbImage};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug)]
struct P {
    x: f64,
    y: f64,
}

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned())
}

fn list_pngs(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map_or(false, |x| x.eq_ignore_ascii_case("png")))
        .collect();
    v.sort();
    v
}

/// Largest dark connected component whose centre lies in the middle of the frame, as its pixel
/// set on a downsampled grid. Returns (pixels, scale).
/// Hysteresis segmentation: components are seeded by truly black pixels (< `threshold`) and
/// grown into connected darker-than-room pixels (< `grow`), so glossy reflections of the
/// keyboard or the room inside the panel stay part of the screen while the light deck around
/// it does not. The component with the most seed pixels wins.
fn dark_component(img: &RgbImage, threshold: u8, grow: u8, hinge_y: Option<f64>) -> Option<(Vec<(u32, u32)>, u32)> {
    let scale = ((img.width().max(img.height()) / 1400).max(1)) as u32;
    let (w, h) = (img.width() / scale, img.height() / scale);
    let mut level = vec![0u8; (w * h) as usize];
    for y in 0..h {
        for x in 0..w {
            let mut sum = 0u32;
            let mut n = 0u32;
            for yy in 0..scale {
                for xx in 0..scale {
                    let p = img.get_pixel(x * scale + xx, y * scale + yy).0;
                    sum += p[0].max(p[1]).max(p[2]) as u32;
                    n += 1;
                }
            }
            // Everything below the hinge line is base/keyboard/desk, never screen.
            let below_hinge = hinge_y.map_or(false, |hy| (y * scale) as f64 >= hy);
            level[(y * w + x) as usize] = if below_hinge { 255 } else { (sum / n) as u8 };
        }
    }
    let mut label = vec![0u32; (w * h) as usize];
    let mut best: Option<(Vec<(u32, u32)>, usize)> = None;
    let mut next = 1u32;
    let mut stack = Vec::new();
    for start in 0..(w * h) {
        if level[start as usize] >= threshold || label[start as usize] != 0 {
            continue;
        }
        next += 1;
        stack.clear();
        stack.push(start);
        label[start as usize] = next;
        let mut pixels = Vec::new();
        let mut seeds = 0usize;
        let (mut sx, mut sy) = (0f64, 0f64);
        while let Some(i) = stack.pop() {
            let (x, y) = (i % w, i / w);
            pixels.push((x, y));
            if level[i as usize] < threshold {
                seeds += 1;
            }
            sx += x as f64;
            sy += y as f64;
            let neighbours = [
                (x.wrapping_sub(1), y),
                (x + 1, y),
                (x, y.wrapping_sub(1)),
                (x, y + 1),
            ];
            for (nx, ny) in neighbours {
                if nx < w && ny < h {
                    let j = (ny * w + nx) as usize;
                    if level[j] < grow && label[j] == 0 {
                        label[j] = next;
                        stack.push(j as u32);
                    }
                }
            }
        }
        let area = pixels.len() as f64 / (w * h) as f64;
        let seed_area = seeds as f64 / (w * h) as f64;
        let (cx, cy) = (sx / pixels.len() as f64 / w as f64, sy / pixels.len() as f64 / h as f64);
        // a screen: enough truly black pixels, not the whole room, roughly central
        if seed_area < 0.003 || area > 0.6 || !(0.12..=0.88).contains(&cx) || !(0.08..=0.92).contains(&cy) {
            continue;
        }
        if best.as_ref().map_or(true, |(_, s)| seeds > *s) {
            best = Some((pixels, seeds));
        }
    }
    best.map(|(p, _)| (p, scale))
}

/// Corners of a convex-ish blob: TL, TR, BR, BL from the extreme points of x+y and x-y.
fn corners(pixels: &[(u32, u32)], scale: u32) -> [P; 4] {
    let s = scale as f64;
    let mut tl = (f64::MAX, P { x: 0.0, y: 0.0 });
    let mut br = (f64::MIN, P { x: 0.0, y: 0.0 });
    let mut tr = (f64::MIN, P { x: 0.0, y: 0.0 });
    let mut bl = (f64::MAX, P { x: 0.0, y: 0.0 });
    for &(x, y) in pixels {
        let (fx, fy) = (x as f64 * s + s / 2.0, y as f64 * s + s / 2.0);
        let sum = fx + fy;
        let diff = fx - fy;
        if sum < tl.0 { tl = (sum, P { x: fx, y: fy }); }
        if sum > br.0 { br = (sum, P { x: fx, y: fy }); }
        if diff > tr.0 { tr = (diff, P { x: fx, y: fy }); }
        if diff < bl.0 { bl = (diff, P { x: fx, y: fy }); }
    }
    // grow by half a cell so the warp covers the blob's outer edge
    let g = s * 0.5;
    [
        P { x: tl.1.x - g, y: tl.1.y - g },
        P { x: tr.1.x + g, y: tr.1.y - g },
        P { x: br.1.x + g, y: br.1.y + g },
        P { x: bl.1.x - g, y: bl.1.y + g },
    ]
}

/// Homography mapping the unit square (TL,TR,BR,BL) onto `q`, as a row-major 3x3.
fn square_to_quad(q: &[P; 4]) -> [f64; 9] {
    let (x0, y0, x1, y1, x2, y2, x3, y3) = (q[0].x, q[0].y, q[1].x, q[1].y, q[2].x, q[2].y, q[3].x, q[3].y);
    let (dx1, dy1) = (x1 - x2, y1 - y2);
    let (dx2, dy2) = (x3 - x2, y3 - y2);
    let (dx3, dy3) = (x0 - x1 + x2 - x3, y0 - y1 + y2 - y3);
    if dx3.abs() < 1e-9 && dy3.abs() < 1e-9 {
        return [x1 - x0, x2 - x1, x0, y1 - y0, y2 - y1, y0, 0.0, 0.0, 1.0];
    }
    let den = dx1 * dy2 - dx2 * dy1;
    let g = (dx3 * dy2 - dx2 * dy3) / den;
    let h = (dx1 * dy3 - dx3 * dy1) / den;
    [x1 - x0 + g * x1, x3 - x0 + h * x3, x0, y1 - y0 + g * y1, y3 - y0 + h * y3, y0, g, h, 1.0]
}

fn invert3(m: &[f64; 9]) -> [f64; 9] {
    let [a, b, c, d, e, f, g, h, i] = *m;
    let det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
    let inv = 1.0 / det;
    [
        (e * i - f * h) * inv, (c * h - b * i) * inv, (b * f - c * e) * inv,
        (f * g - d * i) * inv, (a * i - c * g) * inv, (c * d - a * f) * inv,
        (d * h - e * g) * inv, (b * g - a * h) * inv, (a * e - b * d) * inv,
    ]
}

fn sample(img: &RgbImage, u: f64, v: f64) -> [f64; 3] {
    let (w, h) = (img.width() as f64, img.height() as f64);
    let x = (u * w - 0.5).clamp(0.0, w - 1.0);
    let y = (v * h - 0.5).clamp(0.0, h - 1.0);
    let (x0, y0) = (x.floor() as u32, y.floor() as u32);
    let (x1, y1) = ((x0 + 1).min(img.width() - 1), (y0 + 1).min(img.height() - 1));
    let (fx, fy) = (x - x0 as f64, y - y0 as f64);
    let p = |x: u32, y: u32| img.get_pixel(x, y).0.map(|c| c as f64);
    let (a, b, c, d) = (p(x0, y0), p(x1, y0), p(x0, y1), p(x1, y1));
    let mut out = [0.0; 3];
    for k in 0..3 {
        out[k] = (a[k] * (1.0 - fx) + b[k] * fx) * (1.0 - fy) + (c[k] * (1.0 - fx) + d[k] * fx) * fy;
    }
    out
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let video = PathBuf::from(arg(&args, "--video").expect("--video DIR (png frames of the clip)"));
    let fold = PathBuf::from(arg(&args, "--fold").expect("--fold DIR (png frames from winbend --render-clip --linear)"));
    let out = PathBuf::from(arg(&args, "--out").expect("--out DIR"));
    let threshold: u8 = arg(&args, "--threshold").and_then(|s| s.parse().ok()).unwrap_or(46);
    // reflections on a glossy off screen are grey, the deck and desk around it are light
    let grow: u8 = arg(&args, "--grow").and_then(|s| s.parse().ok()).unwrap_or(150);
    // lid closure fraction at which the fold is complete (the screen never reaches 0 px tall)
    let close_at: f64 = arg(&args, "--close-at").and_then(|s| s.parse().ok()).unwrap_or(0.75);
    // lid closure fraction at which the fold starts (the desktop stays flat before that)
    let open_at: f64 = arg(&args, "--open-at").and_then(|s| s.parse().ok()).unwrap_or(0.12);
    let gain: f64 = arg(&args, "--gain").and_then(|s| s.parse().ok()).unwrap_or(0.9);
    let reverse = args.iter().any(|a| a == "--reverse"); // clip opens the lid instead of closing it
    std::fs::create_dir_all(&out).expect("create out dir");

    let frames = list_pngs(&video);
    let fold_paths = list_pngs(&fold);
    assert!(!frames.is_empty(), "no video frames in {}", video.display());
    assert!(!fold_paths.is_empty(), "no fold frames in {}", fold.display());
    let folds: Vec<RgbImage> = fold_paths.iter().map(|p| image::open(p).expect("fold png").to_rgb8()).collect();

    // Pass 0: the hinge line. The camera and base are static, so the bottom edge of the screen
    // in the frame where the screen is largest (truly black pixels only, no growing) is where
    // the lid meets the base for the whole clip. Nothing below it may be counted as screen.
    let hinge_y = {
        let mut best: Option<(usize, f64)> = None;
        let probes: Vec<usize> = (0..frames.len()).step_by((frames.len() / 12).max(1)).collect();
        for &i in &probes {
            let img = image::open(&frames[i]).expect("video png").to_rgb8();
            if let Some((px, _)) = dark_component(&img, threshold, threshold, None) {
                let n = px.len() as f64;
                if best.map_or(true, |(_, b)| n > b) {
                    best = Some((i, n));
                }
            }
        }
        best.and_then(|(i, _)| {
            let img = image::open(&frames[i]).expect("video png").to_rgb8();
            dark_component(&img, threshold, threshold, None).map(|(px, s)| {
                let q = corners(&px, s);
                let hy = q[2].y.max(q[3].y) + 3.0;
                eprintln!("hinge line at y={hy:.0} (from frame {i})");
                hy
            })
        })
    };

    // Pass 1: track the screen in every frame.
    let mut quads: Vec<Option<[P; 4]>> = Vec::with_capacity(frames.len());
    let mut smooth: Option<[P; 4]> = None;
    let mut missing = 0u32;
    for (i, f) in frames.iter().enumerate() {
        let img = image::open(f).expect("video png").to_rgb8();
        let found = dark_component(&img, threshold, grow, hinge_y).map(|(px, s)| corners(&px, s));
        let q = match (found, smooth) {
            (Some(q), Some(prev)) => {
                missing = 0;
                let mut s = prev;
                for k in 0..4 {
                    s[k].x = prev[k].x * 0.55 + q[k].x * 0.45;
                    s[k].y = prev[k].y * 0.55 + q[k].y * 0.45;
                }
                Some(s)
            }
            (Some(q), None) => {
                missing = 0;
                Some(q)
            }
            // Bridge a dropout of a few frames; after that the screen is really gone (lid shut).
            (None, prev) => {
                missing += 1;
                if missing <= 3 { prev } else { None }
            }
        };
        smooth = q;
        quads.push(q);
        if i % 30 == 0 {
            eprintln!("track {}/{}", i + 1, frames.len());
        }
    }
    // Screen height per frame (mean of the two vertical edges); its maximum is "lid fully open".
    let heights: Vec<f64> = quads
        .iter()
        .map(|q| q.map_or(0.0, |q| ((q[3].y - q[0].y).abs() + (q[2].y - q[1].y).abs()) / 2.0))
        .collect();
    let h_open = heights.iter().cloned().fold(0.0, f64::max).max(1.0);

    // Pass 2: composite.
    for (i, f) in frames.iter().enumerate() {
        let mut img = image::open(f).expect("video png").to_rgb8();
        // Below ~8 % of the open height the "screen" is a sliver of bezel: leave the frame alone.
        if let Some(q) = quads[i].filter(|_| heights[i] > 0.08 * h_open) {
            let closure = (1.0 - heights[i] / h_open).clamp(0.0, 1.0);
            let mut t = ((closure - open_at) / (close_at - open_at).max(0.05)).clamp(0.0, 1.0);
            t = t * t * (3.0 - 2.0 * t);
            let _ = reverse; // the mapping is symmetric; the flag only documents intent
            let fi = ((t * (folds.len() - 1) as f64).round() as usize).min(folds.len() - 1);
            let src = &folds[fi];
            let inv = invert3(&square_to_quad(&q));
            let (minx, maxx) = (q.iter().map(|p| p.x).fold(f64::MAX, f64::min).max(0.0), q.iter().map(|p| p.x).fold(f64::MIN, f64::max).min(img.width() as f64 - 1.0));
            let (miny, maxy) = (q.iter().map(|p| p.y).fold(f64::MAX, f64::min).max(0.0), q.iter().map(|p| p.y).fold(f64::MIN, f64::max).min(img.height() as f64 - 1.0));
            let feather = 0.006;
            for y in miny as u32..=maxy as u32 {
                for x in minx as u32..=maxx as u32 {
                    let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
                    let w = inv[6] * px + inv[7] * py + inv[8];
                    if w.abs() < 1e-9 { continue; }
                    let u = (inv[0] * px + inv[1] * py + inv[2]) / w;
                    let v = (inv[3] * px + inv[4] * py + inv[5]) / w;
                    if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) { continue; }
                    let edge = u.min(1.0 - u).min(v).min(1.0 - v);
                    let a = (edge / feather).clamp(0.0, 1.0);
                    let c = sample(src, u, v);
                    let dst = img.get_pixel_mut(x, y);
                    for k in 0..3 {
                        // the screen is a light source: add onto the dark panel rather than replace it
                        let lit = (dst.0[k] as f64 * 0.35 + c[k] * gain).min(255.0);
                        dst.0[k] = (dst.0[k] as f64 * (1.0 - a) + lit * a).round() as u8;
                    }
                }
            }
        }
        let name = f.file_name().unwrap();
        img.save(out.join(name)).expect("write png");
        if i % 30 == 0 {
            eprintln!("composite {}/{}", i + 1, frames.len());
        }
    }
    println!("composited {} frames into {}", frames.len(), out.display());
    let _ = Rgb([0u8; 3]);
}
