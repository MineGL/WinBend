//! Webcam lid tracking (opt-in). The camera sits in the lid, so when the lid closes the
//! camera pitches down and the scene slides *up* in the image. We measure that vertical
//! shift between consecutive low-resolution grayscale frames and integrate it into a lid
//! angle using the camera's vertical field of view. A dark frame means the lid is shut.
//!
//! Frames stay in this thread's memory and are dropped immediately after the shift is
//! measured. Nothing is written or transmitted. The camera indicator light will be on
//! while tracking is enabled, which is why this is off by default.
//!
//! Results are posted to the message window as WM_CAMERA (wparam = centi-degrees + OFFSET,
//! lparam bit 0 = dark) and WM_CAMERA_STATUS (wparam 1 = running, 0 = could not open).
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, Once};
use std::time::{Duration, Instant};

use windows::core::Result;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, CoTaskMemFree, COINIT_MULTITHREADED};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

use crate::win::{WM_CAMERA, WM_CAMERA_STATUS};

pub const OFFSET: usize = 100_000;
const SW: usize = 64; // analysis frame size
const SH: usize = 48;
const MAX_SHIFT: i32 = 6; // rows searched per frame at analysis resolution
const DARK_LEVEL: f32 = 22.0; // mean Y below this = lid shut / camera covered

static RUNNING: AtomicBool = AtomicBool::new(false);
/// Set by the app when the user overrides the fold: the integrated angle is discarded.
static RESET: AtomicBool = AtomicBool::new(false);

pub fn reset_angle() {
    RESET.store(true, Ordering::Relaxed);
}
static THREAD: Mutex<Option<std::thread::JoinHandle<()>>> = Mutex::new(None);
static MF_INIT: Once = Once::new();

fn mf_init() {
    MF_INIT.call_once(|| unsafe {
        let _ = MFStartup(MF_VERSION, MFSTARTUP_NOSOCKET);
    });
}

fn video_devices() -> Result<Vec<IMFActivate>> {
    unsafe {
        let mut attrs: Option<IMFAttributes> = None;
        MFCreateAttributes(&mut attrs, 1)?;
        let attrs = attrs.unwrap();
        attrs.SetGUID(&MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE, &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID)?;
        let mut list: *mut Option<IMFActivate> = std::ptr::null_mut();
        let mut count = 0u32;
        MFEnumDeviceSources(&attrs, &mut list, &mut count)?;
        let mut out = Vec::new();
        if !list.is_null() {
            for i in 0..count as usize {
                let slot = list.add(i);
                if let Some(a) = (*slot).take() {
                    out.push(a);
                }
            }
            CoTaskMemFree(Some(list as *const _));
        }
        Ok(out)
    }
}

/// True when at least one video capture device exists. Does not open it.
pub fn available() -> bool {
    mf_init();
    video_devices().map(|v| !v.is_empty()).unwrap_or(false)
}

pub fn is_running() -> bool {
    RUNNING.load(Ordering::Relaxed)
}

#[derive(Clone, Copy, PartialEq)]
enum Fmt {
    Nv12,
    Yuy2,
}

struct Reader {
    reader: IMFSourceReader,
    width: usize,
    height: usize,
    fmt: Fmt,
}

fn open_reader() -> Result<Reader> {
    unsafe {
        let devices = video_devices()?;
        let activate = devices.into_iter().next().ok_or_else(|| windows::core::Error::from_hresult(windows::core::HRESULT(-2147023728)))?; // ERROR_NOT_FOUND
        let source: IMFMediaSource = activate.ActivateObject()?;
        let mut rattrs: Option<IMFAttributes> = None;
        MFCreateAttributes(&mut rattrs, 2)?;
        let rattrs = rattrs.unwrap();
        rattrs.SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1)?;
        let reader = MFCreateSourceReaderFromMediaSource(&source, &rattrs)?;
        let stream = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;

        // Smallest native frame size that is still useful (>= 160 px wide).
        let mut best: Option<(u32, u32)> = None;
        let mut i = 0;
        while let Ok(mt) = reader.GetNativeMediaType(stream, i) {
            i += 1;
            if let Ok(fs) = mt.GetUINT64(&MF_MT_FRAME_SIZE) {
                let (w, h) = ((fs >> 32) as u32, fs as u32);
                if w >= 160 && h >= 120 && best.map_or(true, |(bw, bh)| w * h < bw * bh) {
                    best = Some((w, h));
                }
            }
        }
        let want = best.unwrap_or((320, 240));

        let mut chosen: Option<Fmt> = None;
        for (fmt, guid) in [(Fmt::Nv12, MFVideoFormat_NV12), (Fmt::Yuy2, MFVideoFormat_YUY2)] {
            let mt = MFCreateMediaType()?;
            mt.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            mt.SetGUID(&MF_MT_SUBTYPE, &guid)?;
            mt.SetUINT64(&MF_MT_FRAME_SIZE, ((want.0 as u64) << 32) | want.1 as u64)?;
            if reader.SetCurrentMediaType(stream, None, &mt).is_ok() {
                chosen = Some(fmt);
                break;
            }
            // Retry letting the reader pick the size.
            let mt = MFCreateMediaType()?;
            mt.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            mt.SetGUID(&MF_MT_SUBTYPE, &guid)?;
            if reader.SetCurrentMediaType(stream, None, &mt).is_ok() {
                chosen = Some(fmt);
                break;
            }
        }
        let fmt = chosen.ok_or_else(|| windows::core::Error::from_hresult(windows::core::HRESULT(-1072875852)))?; // MF_E_INVALIDMEDIATYPE
        let cur = reader.GetCurrentMediaType(stream)?;
        let fs = cur.GetUINT64(&MF_MT_FRAME_SIZE)?;
        let (width, height) = ((fs >> 32) as usize, (fs & 0xffff_ffff) as usize);
        reader.SetStreamSelection(stream, true)?;
        Ok(Reader { reader, width, height, fmt })
    }
}

/// Box-downsample the luma plane into a SW x SH grayscale frame.
fn downsample(y: &[u8], stride: usize, step: usize, w: usize, h: usize, out: &mut [f32; SW * SH]) {
    let bw = w / SW;
    let bh = h / SH;
    if bw == 0 || bh == 0 {
        return;
    }
    let norm = 1.0 / (bw * bh) as f32;
    for oy in 0..SH {
        for ox in 0..SW {
            let mut acc = 0u32;
            for yy in 0..bh {
                let row = (oy * bh + yy) * stride;
                let mut idx = row + ox * bw * step;
                for _ in 0..bw {
                    acc += y[idx] as u32;
                    idx += step;
                }
            }
            out[oy * SW + ox] = acc as f32 * norm;
        }
    }
}

/// Vertical shift (rows, at analysis resolution) that best maps `prev` onto `cur`,
/// using only the upper part of the frame where the user's head rarely is.
/// Positive = scene moved up = camera pitched down = lid closing.
fn vertical_shift(prev: &[f32; SW * SH], cur: &[f32; SW * SH]) -> Option<f32> {
    let rows_used = (SH as f32 * 0.62) as i32; // top 62 %
    let mut sad = [0f32; (2 * MAX_SHIFT + 1) as usize];
    for (k, dy) in (-MAX_SHIFT..=MAX_SHIFT).enumerate() {
        let mut s = 0f32;
        let mut n = 0;
        for y in MAX_SHIFT..rows_used {
            let py = y + dy;
            if py < 0 || py >= SH as i32 {
                continue;
            }
            let c = &cur[(y as usize) * SW..(y as usize + 1) * SW];
            let p = &prev[(py as usize) * SW..(py as usize + 1) * SW];
            for x in 0..SW {
                s += (c[x] - p[x]).abs();
            }
            n += SW;
        }
        sad[k] = if n > 0 { s / n as f32 } else { f32::MAX };
    }
    let (kmin, &vmin) = sad.iter().enumerate().min_by(|a, b| a.1.partial_cmp(b.1).unwrap())?;
    let vmax = sad.iter().cloned().fold(0f32, f32::max);
    // A flat or uniformly changing image gives no usable minimum.
    if vmax - vmin < 1.5 {
        return None;
    }
    let mut dy = kmin as f32 - MAX_SHIFT as f32;
    if kmin > 0 && kmin + 1 < sad.len() {
        let (a, b, c) = (sad[kmin - 1], sad[kmin], sad[kmin + 1]);
        let denom = a - 2.0 * b + c;
        if denom.abs() > 1e-6 {
            dy += 0.5 * (a - c) / denom;
        }
    }
    Some(dy)
}

fn post(hwnd: isize, tilt_deg: f32, dark: bool) {
    let w = (tilt_deg * 100.0).round() as isize + OFFSET as isize;
    unsafe {
        let _ = PostMessageW(Some(HWND(hwnd as *mut _)), WM_CAMERA, WPARAM(w.max(0) as usize), LPARAM(dark as isize));
    }
}

fn post_status(hwnd: isize, ok: bool) {
    unsafe {
        let _ = PostMessageW(Some(HWND(hwnd as *mut _)), WM_CAMERA_STATUS, WPARAM(ok as usize), LPARAM(0));
    }
}

fn run(hwnd: isize, vfov_deg: f32, travel_deg: f32) {
    // COM apartments belong to threads, unlike process-wide Media Foundation startup.
    let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
    struct Apartment(bool);
    impl Drop for Apartment { fn drop(&mut self) { if self.0 { unsafe { CoUninitialize() }; } } }
    let _apartment = Apartment(initialized);
    mf_init();
    let mut rd = match open_reader() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("camera open failed: {e}");
            post_status(hwnd, false);
            RUNNING.store(false, Ordering::Relaxed);
            return;
        }
    };
    post_status(hwnd, true);
    let travel_deg = travel_deg.clamp(20.0, 120.0);
    let stream = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
    let deg_per_row = vfov_deg.clamp(20.0, 90.0) / SH as f32;

    let mut prev: Option<Box<[f32; SW * SH]>> = None;
    let mut cur: Box<[f32; SW * SH]> = Box::new([0.0; SW * SH]);
    let mut angle = 0f32;
    let mut last_motion = Instant::now();
    let mut last_post = Instant::now() - Duration::from_secs(1);
    // The integrated angle cannot be trusted right after the stream (re)opens or after a
    // dark spell: the lid may have moved while we could not see. While `uncertain`, a
    // still and bright scene means "the lid is open, at rest" and the angle fades to 0.
    let mut uncertain = true;
    let mut was_dark = false;
    let mut last_frame = Instant::now();

    while RUNNING.load(Ordering::Relaxed) {
        let mut flags = 0u32;
        let mut sample: Option<IMFSample> = None;
        let r = unsafe { rd.reader.ReadSample(stream, 0, None, Some(&mut flags), None, Some(&mut sample)) };
        if r.is_err() || flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 || flags & MF_SOURCE_READERF_ERROR.0 as u32 != 0 {
            // Typical after sleep/resume or when another app grabbed the camera: reopen and carry on.
            eprintln!("camera stream interrupted; reopening");
            drop(rd);
            prev = None;
            uncertain = true;
            loop {
                if !RUNNING.load(Ordering::Relaxed) {
                    RUNNING.store(false, Ordering::Relaxed);
                    post_status(hwnd, false);
                    return;
                }
                std::thread::sleep(Duration::from_millis(1500));
                if let Ok(r) = open_reader() {
                    rd = r;
                    break;
                }
            }
            continue;
        }
        let Some(sample) = sample else { continue };
        let Ok(buf) = (unsafe { sample.ConvertToContiguousBuffer() }) else { continue };
        let mut p: *mut u8 = std::ptr::null_mut();
        let mut len = 0u32;
        if unsafe { buf.Lock(&mut p, None, Some(&mut len)) }.is_err() || p.is_null() {
            continue;
        }
        let data = unsafe { std::slice::from_raw_parts(p, len as usize) };
        let (stride, step) = match rd.fmt {
            Fmt::Nv12 => (rd.width, 1),
            Fmt::Yuy2 => (rd.width * 2, 2),
        };
        if data.len() >= stride * rd.height {
            downsample(data, stride, step, rd.width, rd.height, &mut cur);
        }
        let _ = unsafe { buf.Unlock() };
        drop(buf);
        drop(sample);

        let mean = cur.iter().sum::<f32>() / (SW * SH) as f32;
        if std::env::var_os("WINBEND_CAMERA_DEBUG").is_some() {
            let mx = cur.iter().cloned().fold(0f32, f32::max);
            eprintln!("frame {}x{} fmt={} len={} mean={mean:.1} max={mx:.1}", rd.width, rd.height, if rd.fmt == Fmt::Nv12 { "NV12" } else { "YUY2" }, len);
        }
        let dark = mean < DARK_LEVEL;
        let now = Instant::now();
        if RESET.swap(false, Ordering::Relaxed) {
            angle = 0.0;
            uncertain = false;
            last_motion = now;
        }
        if was_dark && !dark {
            uncertain = true;
            last_motion = now;
        }
        was_dark = dark;
        if let Some(pv) = prev.as_ref() {
            if !dark {
                if let Some(dy) = vertical_shift(pv, &cur) {
                    if dy.abs() >= 0.25 {
                        angle = (angle + dy * deg_per_row).clamp(0.0, 130.0);
                        last_motion = now;
                    }
                }
            }
        }
        // Drift control: a still, bright scene with a small residual angle means the lid is
        // simply open. Fade the residual away instead of leaving a half-fold on screen.
        let still_for = now - last_motion;
        if !dark && ((uncertain && still_for > Duration::from_millis(1200)) || (still_for > Duration::from_millis(2000) && angle < 18.0)) {
            angle *= if uncertain { 0.7 } else { 0.85 };
            if angle < 0.3 {
                angle = 0.0;
                uncertain = false;
            }
        }
        if angle == 0.0 {
            uncertain = false;
        }
        if dark {
            // The camera can no longer see (lid nearly shut or covered). Instead of jumping to
            // fully folded, let the angle glide on toward the end of travel, so the fold
            // completes smoothly from wherever the last visible frame left it.
            let dt = (now - last_frame).as_secs_f32().clamp(0.0, 0.2);
            let k = 1.0 - (-dt / 0.25).exp();
            angle += (travel_deg - angle) * k;
            if travel_deg - angle < 0.5 {
                angle = travel_deg;
            }
            last_motion = now;
        }
        last_frame = now;

        if now - last_post >= Duration::from_millis(50) {
            post(hwnd, angle, dark);
            last_post = now;
        }
        match prev.as_mut() {
            Some(pv) => pv.copy_from_slice(&cur[..]),
            None => prev = Some(cur.clone()),
        }
    }
    RUNNING.store(false, Ordering::Relaxed);
    post(hwnd, 0.0, false);
    post_status(hwnd, false);
}

/// Start tracking; the camera opens on a worker thread and reports through WM_CAMERA_STATUS.
pub fn start(hwnd: HWND, vfov_deg: f32, travel_deg: f32) {
    if RUNNING.swap(true, Ordering::Relaxed) {
        return;
    }
    let raw = hwnd.0 as isize;
    let handle = std::thread::Builder::new()
        .name("winbend-camera".into())
        .spawn(move || run(raw, vfov_deg, travel_deg))
        .ok();
    *THREAD.lock().unwrap() = handle;
}

pub fn stop() {
    RUNNING.store(false, Ordering::Relaxed);
    if let Some(h) = THREAD.lock().unwrap().take() {
        // ReadSample returns at the next frame (tens of ms); do not block the UI longer than that.
        let deadline = Instant::now() + Duration::from_millis(1500);
        while !h.is_finished() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        if h.is_finished() {
            let _ = h.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn textured(offset_rows: i32) -> Box<[f32; SW * SH]> {
        // A scene with structure in both axes, shifted vertically by `offset_rows`.
        let mut f = Box::new([0f32; SW * SH]);
        for y in 0..SH {
            for x in 0..SW {
                let sy = y as i32 + offset_rows;
                let v = 90.0 + 60.0 * ((sy as f32) * 0.55).sin() * ((x as f32) * 0.31).cos() + 20.0 * (((sy * 7 + x as i32 * 3) % 11) as f32 / 11.0);
                f[y * SW + x] = v;
            }
        }
        f
    }

    #[test]
    fn detects_upward_scene_motion_as_positive() {
        // Camera pitches down (lid closing): content that was at row y+2 is now at row y.
        let prev = textured(0);
        let cur = textured(2);
        let dy = vertical_shift(&prev, &cur).expect("shift");
        assert!((dy - 2.0).abs() < 0.35, "expected +2 rows, got {dy}");
    }

    #[test]
    fn detects_downward_scene_motion_as_negative() {
        let prev = textured(0);
        let cur = textured(-3);
        let dy = vertical_shift(&prev, &cur).expect("shift");
        assert!((dy + 3.0).abs() < 0.35, "expected -3 rows, got {dy}");
    }

    #[test]
    fn still_scene_is_zero_and_flat_scene_is_none() {
        let a = textured(0);
        let dy = vertical_shift(&a, &a).expect("shift");
        assert!(dy.abs() < 0.05);
        let flat = Box::new([100f32; SW * SH]);
        assert!(vertical_shift(&flat, &flat).is_none());
    }
}
