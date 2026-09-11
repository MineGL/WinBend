//! Fold session state machine: what drives `t` (0 = flat, 1 = folded away), when the
//! overlay is shown, when capture runs, and what happens when a fold finishes.
use std::time::{Duration, Instant};
mod settings;

use windows::core::Result;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct3D11::ID3D11Texture2D;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::config::{Config, StyleParams};
use crate::gfx::capture::Capture;
use crate::gfx::device::Gpu;
use crate::gfx::renderer::{FoldParams, Renderer};
use crate::sensor;
use crate::win::{self, Event, MenuState, Overlay};

#[derive(Debug, Clone, Copy, PartialEq)]
enum Mode {
    Idle,
    /// Time-based ease between two fold amounts.
    Anim { from: f32, to: f32, start: Instant, dur: Duration },
    /// Folded (or partially folded) and waiting for the user.
    Hold,
    /// Tracking a sensor (hinge angle or lid accelerometer).
    Follow { target: f32, zero_since: Option<Instant> },
}

/// Why the current session exists. Decides focus handling, dismissal rules and trial counting.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Kind {
    /// Hotkey or tray click.
    Manual,
    /// Hinge sensor or accelerometer.
    Sensor,
    /// Lid opened / PC woke / session unlocked: unfold-only.
    Unfold,
    /// Folded because the user walked away; any input unfolds.
    IdleCurtain,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum CalibStage {
    /// Close the lid until the camera goes dark, then open it: measures the lid's travel.
    Travel,
    /// Hold the lid where the fold should begin and press the hotkey: sets the fold zone.
    Zone,
}

#[derive(Debug, Clone, Copy)]
struct Calib {
    stage: CalibStage,
    started: Instant,
    /// Largest tilt seen while the camera could still see.
    max_bright_tilt: f32,
    seen_dark: bool,
    travel: f32,
}

pub struct App {
    pub cfg: Config,
    ui: Option<crate::ui::SettingsWindow>,
    ui_page: String,
    pub ui_ready: bool,
    pub ui_smoke: bool,
    pub ui_preview_ok: bool,
    hotkey_capture: bool,
    tilt_stream: bool,
    last_ui_tilt: Instant,
    preview: Option<crate::ui::preview::Preview>,
    preview_visible: bool,
    preview_dirty: bool,
    preview_t: f32,
    last_preview: Instant,
    preview_wait: Option<Instant>,
    calib_message: serde_json::Value,
    gpu: Gpu,
    renderer: Renderer,
    overlay: Option<Overlay>,
    capture: Option<Capture>,
    mode: Mode,
    kind: Kind,
    t: f32,
    last_tick: Instant,
    session_start: Instant,
    msg_hwnd: HWND,
    pub hinge_available: bool,
    pub accel_available: bool,
    pub camera_available: bool,
    last_accel: Option<[f32; 3]>,
    /// Session is behind the Windows sign-in screen; unfold waits for the unlock.
    locked: bool,
    unfold_pending: bool,
    /// The active Follow session is driven by the webcam (used for its watchdog).
    follow_from_camera: bool,
    last_camera_msg: Option<Instant>,
    /// Lid closed while WinBend owns the lid action: sleep as soon as the fold completes.
    sleep_after_fold: bool,
    /// Webcam calibration in progress (folding is suppressed meanwhile).
    calib: Option<Calib>,
    last_tilt: f32,
    last_target: f32,
    low_still_since: Option<Instant>,
    /// After a user override, sensor input is ignored until this instant.
    sensor_muted_until: Option<Instant>,
    /// Latest camera frame was dark (lid shut or covered).
    camera_dark: bool,
    /// How long a sensor session has held the fold nearly closed.
    parked_since: Option<Instant>,
    resumed_at: Option<Instant>,
    lid_opened_since_resume: bool,
    /// Current Windows power values (AC, DC) for the two settings we can manage.
    lid_action: Option<(u32, u32)>,
    lock_on_wake: Option<(u32, u32)>,
    /// Window that had focus before we took it, restored when the session ends.
    prev_fg: Option<HWND>,
    lock_pending: Option<Instant>,
    /// --demo: fold, hold briefly, unfold, then quit.
    demo: bool,
    demo_hold_until: Option<Instant>,
    pub want_quit: bool,
}

fn ease_in_out(x: f32) -> f32 {
    // smooth, slightly weighted toward a soft landing
    let x = x.clamp(0.0, 1.0);
    if x < 0.5 {
        4.0 * x * x * x
    } else {
        1.0 - (-2.0 * x + 2.0).powi(3) / 2.0
    }
}

pub fn fold_params(style: &StyleParams, t: f32, max_tilt_deg: f32, bg: [f32; 4]) -> FoldParams {
    let t = t.clamp(0.0, 1.0);
    FoldParams {
        theta: t * max_tilt_deg.clamp(5.0, 89.5).to_radians(),
        bend: style.bend.clamp(0.0, 1.0),
        cam_dist: style.perspective.clamp(1.2, 8.0),
        shade: style.shadow.clamp(0.0, 1.0),
        blur_mix: style.blur.clamp(0.0, 1.0) * (t * 1.8).clamp(0.0, 1.0),
        blur_step: 0.6 + 2.4 * t,
        vignette: style.vignette.clamp(0.0, 1.0),
        sheen: style.sheen.clamp(0.0, 1.0),
        bg,
        panels: style.panels.round().clamp(1.0, 8.0),
    }
}

impl App {
    pub fn new(cfg: Config, msg_hwnd: HWND, hinge_available: bool, accel_available: bool, camera_available: bool, demo: bool) -> Result<Self> {
        let gpu = Gpu::new()?;
        let renderer = Renderer::new(&gpu)?;
        let mut app = Self {
            cfg,
            ui: None,
            ui_page: "fold".into(),
            ui_ready: false,
            ui_smoke: false,
            ui_preview_ok: false,
            hotkey_capture: false,
            tilt_stream: false,
            last_ui_tilt: Instant::now(),
            preview: None,
            preview_visible: false,
            preview_dirty: true,
            preview_t: 0.5,
            last_preview: Instant::now(),
            preview_wait: None,
            calib_message: serde_json::json!({"type":"calib","stage":"idle"}),
            gpu,
            renderer,
            overlay: None,
            capture: None,
            mode: Mode::Idle,
            kind: Kind::Manual,
            t: 0.0,
            last_tick: Instant::now(),
            session_start: Instant::now(),
            msg_hwnd,
            hinge_available,
            accel_available,
            camera_available,
            last_accel: None,
            locked: false,
            unfold_pending: false,
            follow_from_camera: false,
            last_camera_msg: None,
            sleep_after_fold: false,
            calib: None,
            last_tilt: 0.0,
            last_target: 0.0,
            low_still_since: None,
            sensor_muted_until: None,
            camera_dark: false,
            parked_since: None,
            resumed_at: None,
            lid_opened_since_resume: true,
            lid_action: None,
            lock_on_wake: None,
            prev_fg: None,
            lock_pending: None,
            demo,
            demo_hold_until: None,
            want_quit: false,
        };
        app.refresh_power();
        app.publish_menu();
        app.sync_camera();
        Ok(app)
    }

    /// Re-read the Windows power settings we display and act on.
    fn refresh_power(&mut self) {
        self.lid_action = crate::power::read(crate::power::Setting::LidAction);
        self.lock_on_wake = crate::power::read(crate::power::Setting::LockOnWake);
    }

    fn lid_do_nothing(&self) -> bool {
        matches!(self.lid_action, Some((0, 0)))
    }

    fn skip_signin(&self) -> bool {
        matches!(self.lock_on_wake, Some((0, 0)))
    }

    pub fn active(&self) -> bool {
        self.mode != Mode::Idle
    }

    fn fold_duration(&self) -> Duration {
        Duration::from_millis(self.cfg.fold_ms.clamp(150, 5000) as u64)
    }

    pub fn publish_menu(&self) {
        let st = MenuState {
            style: self.cfg.style.clone(),
            hotkey: self.cfg.hotkey.clone(),
            follow_hinge: self.cfg.follow_hinge,
            hinge_available: self.hinge_available,
            unfold_on_lid_open: self.cfg.unfold_on_lid_open,
            unfold_on_wake: self.cfg.unfold_on_wake,
            fold_on_sleep: self.cfg.fold_on_sleep,
            handle_lid: self.cfg.handle_lid,
            lid_do_nothing: self.lid_do_nothing(),
            skip_signin: self.skip_signin(),
            after_fold: self.cfg.after_fold.clone(),
            idle_min: self.cfg.fold_when_idle_min,
            accel_available: self.accel_available,
            follow_accel: self.cfg.follow_accelerometer,
            accel_calibrated: self.cfg.accel_calibration.is_some(),
            camera_available: self.camera_available,
            follow_camera: self.cfg.follow_camera,
            camera_hotkey: self.cfg.camera_hotkey.clone(),
            flat_deg: self.cfg.camera_flat_deg.round() as u32,
            run_at_startup: self.cfg.run_at_startup,
        };
        *win::MENU.lock().unwrap() = Some(st);
        win::tray_tip(self.msg_hwnd, &format!("WinBend \u{2014} {} to fold", self.cfg.hotkey));
    }

    fn save(&mut self) {
        if let Err(e) = self.cfg.save() {
            eprintln!("could not save settings: {e}");
        }
        self.publish_menu();
        self.preview_dirty = true;
        self.post_state();
    }

    // ------------------------------------------------------------------ sessions

    fn begin_session(&mut self, kind: Kind) -> bool {
        if self.overlay.is_none() {
            let geom = win::primary_monitor();
            match Overlay::new(&self.gpu, geom) {
                Ok(o) => self.overlay = Some(o),
                Err(e) => {
                    eprintln!("overlay creation failed: {e}");
                    return false;
                }
            }
        }
        if self.capture.is_none() {
            let hmon = self.overlay.as_ref().unwrap().geom.hmon;
            match Capture::for_monitor(&self.gpu, hmon) {
                Ok(c) => self.capture = Some(c),
                Err(e) => {
                    eprintln!("screen capture failed: {e}");
                    win::tray_notify(self.msg_hwnd, "WinBend", "Screen capture is not available on this Windows build.");
                    return false;
                }
            }
        }
        self.kind = kind;
        self.prev_fg = None;
        self.session_start = Instant::now();
        self.last_tick = self.session_start;
        true
    }

    fn end_session(&mut self) {
        if let Some(o) = &mut self.overlay {
            o.hide();
        }
        if !self.preview_visible { self.capture = None; }
        self.mode = Mode::Idle;
        self.t = 0.0;
        self.lock_pending = None;
        if let Some(fg) = self.prev_fg.take() {
            win::focus_window(fg);
        }
        if self.demo {
            self.want_quit = true;
        }
    }

    fn animate_to(&mut self, to: f32) {
        self.mode = Mode::Anim { from: self.t, to, start: Instant::now(), dur: self.fold_duration() };
    }

    fn animate_to_in(&mut self, to: f32, ms: u64) {
        self.mode = Mode::Anim { from: self.t, to, start: Instant::now(), dur: Duration::from_millis(ms) };
    }

    /// Deliberate fold (hotkey / tray click).
    pub fn toggle_fold(&mut self) {
        if let Some(c) = self.calib {
            if c.stage == CalibStage::Zone {
                self.finish_calibration_zone();
            }
            return;
        }
        match self.mode {
            Mode::Idle => {
                if !self.begin_session(Kind::Manual) {
                    return;
                }
                self.t = 0.0;
                self.animate_to(1.0);
            }
            Mode::Anim { to, .. } if to > 0.5 => self.user_override(),
            Mode::Anim { .. } => self.animate_to(1.0),
            Mode::Hold | Mode::Follow { .. } => self.user_override(),
        }
    }

    pub fn dismiss(&mut self) {
        if self.active() {
            self.user_override();
        }
    }

    /// The user wants the desktop back, whatever the sensors say: lift, ignore sensor input
    /// for a while, and make the webcam forget its integrated angle so it cannot re-fold us.
    fn user_override(&mut self) {
        self.sensor_muted_until = Some(Instant::now() + Duration::from_secs(8));
        self.follow_from_camera = false;
        self.sleep_after_fold = false;
        crate::camera::reset_angle();
        if self.kind == Kind::Sensor {
            self.kind = Kind::Unfold;
        }
        self.animate_to(0.0);
    }

    fn sensors_muted(&self) -> bool {
        self.sensor_muted_until.map_or(false, |t| Instant::now() < t)
    }

    pub fn wheel(&mut self, delta: i32) {
        if !self.active() {
            return;
        }
        let step = 0.08 * (delta as f32 / 120.0);
        let nt = (self.t - step).clamp(0.0, 1.0);
        if nt <= 0.01 {
            self.animate_to(0.0);
        } else {
            self.t = nt;
            self.mode = Mode::Hold;
        }
    }

    /// Shared by the hinge sensor and the accelerometer: `target` is the wanted fold amount.
    fn follow_target(&mut self, target: f32) {
        if self.sensors_muted() {
            return;
        }
        match self.mode {
            Mode::Idle => {
                // Trigger only once the lid is clearly inside the fold zone (about a degree
                // past `camera_flat_deg`), so the boundary never flickers the overlay on and off.
                if target > 0.02 && self.begin_session(Kind::Sensor) {
                    self.t = 0.0;
                    self.mode = Mode::Follow { target, zero_since: None };
                }
            }
            Mode::Follow { .. } => {
                self.mode = Mode::Follow { target, zero_since: None };
            }
            // Another session is running; the sensor takes over only when it reports real folding.
            _ => {
                if target > 0.05 {
                    self.kind = Kind::Sensor;
                    self.mode = Mode::Follow { target, zero_since: None };
                }
            }
        }
    }

    pub fn hinge(&mut self, angle_deg: f32) {
        if !self.cfg.follow_hinge || self.calib.is_some() {
            return;
        }
        let open = self.cfg.hinge_open_deg.max(self.cfg.hinge_closed_deg + 1.0);
        let closed = self.cfg.hinge_closed_deg;
        let target = ((open - angle_deg) / (open - closed)).clamp(0.0, 1.0);
        self.follow_from_camera = false;
        self.follow_target(target);
    }

    pub fn accel(&mut self, g: [f32; 3]) {
        self.last_accel = Some(g);
        if !self.cfg.follow_accelerometer || self.calib.is_some() {
            return;
        }
        let Some(cal) = self.cfg.accel_calibration else { return };
        let delta = sensor::lid_delta_deg(cal, g);
        // Dead band so desk vibration and typing do not trigger a fold.
        const DEAD: f32 = 4.0;
        let travel = self.cfg.accel_travel_deg.clamp(20.0, 170.0);
        let target = ((delta - DEAD) / (travel - DEAD)).clamp(0.0, 1.0);
        self.follow_from_camera = false;
        self.follow_target(target);
    }

    /// Webcam motion estimate: `tilt_deg` is how far the lid has rotated from the open pose.
    pub fn camera(&mut self, tilt_deg: f32, dark: bool) {
        self.last_tilt = tilt_deg;
        self.camera_dark = dark;
        if self.tilt_stream && self.last_ui_tilt.elapsed() >= Duration::from_millis(100) {
            self.last_ui_tilt = Instant::now();
            self.ui_send(serde_json::json!({"type":"tilt", "deg":tilt_deg, "dark":dark}));
        }
        if self.calib.is_some() { self.calibration_sample(tilt_deg, dark); return; }
        if !self.cfg.follow_camera { return; }
        // The fold follows the angle alone; a dark frame only affects the safety heuristics.
        // The desktop stays completely normal until the lid is within `camera_flat_deg` of
        // closed; the whole fold happens inside that last stretch, eased at both ends so the
        // hand-over to and from the plain desktop is invisible.
        let travel = self.cfg.camera_travel_deg.clamp(20.0, 120.0);
        let flat = self.cfg.camera_flat_deg.clamp(5.0, travel - 5.0);
        let start = travel - flat;
        let x = ((tilt_deg - start) / flat).clamp(0.0, 1.0);
        let target = x * x * (3.0 - 2.0 * x);
        self.last_camera_msg = Some(Instant::now());
        self.camera_dark = dark;
        self.last_tilt = tilt_deg;
        if self.calib.is_some() {
            self.calibration_sample(tilt_deg, dark);
            return;
        }
        // Resting inside the fold zone: if the lid sits still, barely folded, for 10 s with the
        // camera seeing a bright scene, the user is simply working at a new lid angle. Adopt it
        // as the open pose (angle 0) and stand down instead of keeping a faint fold on screen.
        let now = Instant::now();
        let still = (target - self.last_target).abs() < 0.01;
        self.last_target = target;
        if !dark && target > 0.0 && target < 0.35 && matches!(self.mode, Mode::Follow { .. }) && self.follow_from_camera {
            let since = *self.low_still_since.get_or_insert(now);
            if !still {
                self.low_still_since = Some(now);
            } else if now - since > Duration::from_secs(10) {
                self.low_still_since = None;
                crate::camera::reset_angle();
                self.sensor_muted_until = Some(now + Duration::from_secs(2));
                self.follow_from_camera = false;
                self.kind = Kind::Unfold;
                self.animate_to(0.0);
                return;
            }
        } else {
            self.low_still_since = None;
        }
        if self.sensors_muted() {
            return;
        }
        self.follow_from_camera = true;
        self.follow_target(target);
    }

    pub fn camera_status(&mut self, running: bool) {
        if !running && self.calib.is_some() {
            self.cancel_calibration();
            self.calib_notify("failed", "The camera could not be opened or stopped reporting. Close other camera apps and try again.");
        }
        self.post_state();
        if !running && self.cfg.follow_camera && !crate::camera::is_running() {
            win::tray_notify(self.msg_hwnd, "WinBend webcam tracking stopped", "The camera could not be opened or is in use by another app. Webcam lid tracking is off.");
            self.cfg.follow_camera = false;
            self.save();
        }
    }

    /// Start or stop the camera thread so it matches the setting.
    pub fn sync_camera(&self) {
        let want = (self.cfg.follow_camera || self.calib.is_some()) && self.camera_available;
        if want && !crate::camera::is_running() {
            crate::camera::start(self.msg_hwnd, self.cfg.camera_vfov_deg, self.cfg.camera_travel_deg);
        } else if !want && crate::camera::is_running() {
            crate::camera::stop();
        }
    }

    fn calibrate_accel(&mut self) {
        match self.last_accel {
            Some(g) => {
                self.cfg.accel_calibration = Some(g);
                self.cfg.follow_accelerometer = true;
                self.save();
                win::tray_notify(
                    self.msg_hwnd,
                    "Lid tracking calibrated",
                    "Close the lid slowly to test. If nothing happens, this laptop's accelerometer sits in the base, not the lid.",
                );
            }
            None => {
                self.notify("No accelerometer reading yet. Tilt the laptop slightly and try again.", "info");
            }
        }
    }

    fn camera_live(&self) -> bool {
        self.cfg.follow_camera && crate::camera::is_running()
    }

    // ------------------------------------------------------------------ webcam calibration

    fn start_calibration(&mut self) {
        if self.calib.is_some() { return; }
        if !self.camera_available {
            self.calib_notify("failed", "No webcam found on this PC.");
            return;
        }
        if self.active() { self.end_session(); }
        self.calib = Some(Calib { stage: CalibStage::Travel, started: Instant::now(), max_bright_tilt: 0.0, seen_dark: false, travel: self.cfg.camera_travel_deg });
        self.last_tilt = 0.0;
        crate::camera::reset_angle();
        self.sync_camera();
        self.calib_notify("travel", "Close the lid slowly until the camera is covered, then reopen to your normal angle. The camera light stays on during calibration; nothing folds.");
        self.post_state();
    }

    fn calibration_sample(&mut self, tilt_deg: f32, dark: bool) {
        let Some(mut c) = self.calib else { return };
        if c.stage == CalibStage::Travel {
            if !dark { c.max_bright_tilt = c.max_bright_tilt.max(tilt_deg); }
            if dark { c.seen_dark = true; }
            if c.seen_dark && !dark && c.max_bright_tilt >= 15.0 && tilt_deg < c.max_bright_tilt * 0.5 {
                c.travel = (c.max_bright_tilt + 6.0).clamp(20.0, 120.0);
                c.stage = CalibStage::Zone;
                c.started = Instant::now();
                self.last_tilt = 0.0;
                crate::camera::reset_angle();
                self.calib = Some(c);
                self.calib_notify("zone", "Tilt the lid to where the fold should start, hold it there, and choose Set fold start here or press your hotkey.");
                return;
            }
            self.calib = Some(c);
        }
    }

    fn calibration_timeout(&mut self) {
        if let Some(c) = self.calib {
            let limit = if c.stage == CalibStage::Travel { 45 } else { 90 };
            if c.started.elapsed() > Duration::from_secs(limit) {
                self.cancel_calibration();
                self.calib_notify("failed", "Calibration timed out. Your previous settings are unchanged. Check that the camera is uncovered and try again.");
            }
        }
    }

    fn cancel_calibration(&mut self) {
        if self.calib.take().is_some() {
            crate::camera::reset_angle();
            self.sync_camera();
            self.calib_notify("idle", "Calibration canceled. Your previous settings are unchanged.");
            self.post_state();
        }
    }

    fn finish_calibration_zone(&mut self) {
        let Some(c) = self.calib else { return };
        if c.stage != CalibStage::Zone { return; }
        let mut next = self.cfg.clone();
        next.camera_travel_deg = c.travel;
        next.camera_flat_deg = (c.travel - self.last_tilt).clamp(5.0, c.travel - 5.0);
        next.follow_camera = true;
        if let Err(e) = next.save() { self.notify(&format!("Could not save calibration: {e}"), "error"); return; }
        self.cfg = next;
        self.calib = None;
        self.restart_camera();
        self.publish_menu();
        self.calib_notify("done", "Your lid is calibrated. Webcam tracking is enabled; close the lid to try it.");
        self.post_state();
    }

    /// Restart the camera thread so it picks up new travel settings.
    fn restart_camera(&mut self) {
        if crate::camera::is_running() {
            crate::camera::stop();
        }
        crate::camera::reset_angle();
        self.sync_camera();
    }

    /// Lid opened, PC resumed, or session unlocked: the desktop lifts back up.
    /// With webcam tracking on, the lift follows the physical lid; otherwise it is timed.
    fn start_unfold(&mut self) {
        if self.calib.is_some() { return; }
        self.unfold_pending = false;
        match self.mode {
            Mode::Idle => {
                if !self.begin_session(Kind::Unfold) {
                    return;
                }
                self.t = 1.0;
            }
            Mode::Hold => {}
            // Already moving (or following a sensor): leave it alone.
            _ => return,
        }
        if self.camera_live() {
            self.kind = Kind::Sensor;
            self.follow_from_camera = true;
            self.last_camera_msg = Some(Instant::now());
            self.mode = Mode::Follow { target: 1.0, zero_since: None };
        } else {
            self.kind = Kind::Unfold;
            self.animate_to(0.0);
        }
    }

    /// Lid closed or the PC is about to sleep: finish the fold fast and park there, so the
    /// wake-up shows the desktop still folded and then lifting.
    pub fn sleeping(&mut self) {
        if self.calib.is_some() { return; }
        match self.mode {
            Mode::Idle => {
                if self.cfg.fold_on_sleep && self.begin_session(Kind::Unfold) {
                    self.t = 0.0;
                    self.animate_to_in(1.0, 350);
                }
            }
            Mode::Hold => {}
            Mode::Anim { .. } | Mode::Follow { .. } => {
                if self.kind == Kind::Sensor {
                    self.kind = Kind::Unfold;
                }
                self.follow_from_camera = false;
                self.animate_to_in(1.0, 350);
            }
        }
    }

    pub fn lid(&mut self, open: bool) {
        if self.calib.is_some() { return; }
        if open {
            self.sleep_after_fold = false;
            self.lid_opened_since_resume = true;
            if self.cfg.unfold_on_lid_open {
                self.start_unfold();
            }
        } else if self.resumed_at.map_or(false, |t| Instant::now() - t < Duration::from_secs(8)) && !self.lid_opened_since_resume {
            // Lid "closed" reported within seconds of waking, without an open in between: stale.
        } else if self.cfg.handle_lid && self.lid_do_nothing() {
            // Windows leaves the lid to us: finish the fold, then put the PC to sleep ourselves.
            match self.mode {
                Mode::Idle => {
                    if self.begin_session(Kind::Unfold) {
                        self.t = 0.0;
                        self.sleep_after_fold = true;
                        self.animate_to_in(1.0, (self.cfg.fold_ms as u64).clamp(300, 900));
                    } else {
                        win::sleep_pc();
                    }
                }
                Mode::Hold => win::sleep_pc(),
                Mode::Anim { .. } | Mode::Follow { .. } => {
                    if self.kind == Kind::Sensor {
                        self.kind = Kind::Unfold;
                    }
                    self.follow_from_camera = false;
                    self.sleep_after_fold = true;
                    self.animate_to_in(1.0, 450);
                }
            }
        } else {
            self.sleeping();
        }
    }

    pub fn wake(&mut self) {
        // Some firmware re-reports the lid state on resume; do not let a stale "closed"
        // put the PC straight back to sleep.
        self.resumed_at = Some(Instant::now());
        self.lid_opened_since_resume = false;
        self.sleep_after_fold = false;
        if !self.cfg.unfold_on_wake {
            return;
        }
        if self.locked {
            // Sign-in screen first; the lift plays right after the unlock.
            self.unfold_pending = true;
        } else {
            self.start_unfold();
        }
    }

    pub fn unlocked(&mut self) {
        self.locked = false;
        if self.unfold_pending || self.cfg.unfold_on_wake {
            self.start_unfold();
        }
    }

    // ------------------------------------------------------------------ Windows integration

    fn toggle_handle_lid(&mut self) {
        use crate::power::{write_elevated, Setting};
        if self.cfg.handle_lid && self.lid_do_nothing() {
            // Give the lid back to Windows: closing it sleeps directly again.
            if write_elevated(Setting::LidAction, 1) {
                self.cfg.handle_lid = false;
                self.refresh_power();
                self.save();
                win::tray_notify(self.msg_hwnd, "Lid handed back to Windows", "Closing the lid puts the PC to sleep the normal way again.");
            } else {
                self.refresh_power();
                self.publish_menu();
            }
            return;
        }
        let text = "WinBend will set Windows' lid-close action to \"Do nothing\" (Windows asks for admin approval once).\n\nFrom then on, closing the lid folds the desktop first and WinBend puts the PC to sleep right after. Opening the lid wakes it and the desktop lifts back up.\n\nTurn this off again from the same menu to restore the normal lid behaviour.\n\nContinue?";
        if win::message_box(self.msg_hwnd, "WinBend handles the lid", text, MB_YESNO | MB_ICONQUESTION) != IDYES {
            return;
        }
        if write_elevated(Setting::LidAction, 0) {
            self.cfg.handle_lid = true;
            self.refresh_power();
            self.save();
            win::tray_notify(self.msg_hwnd, "WinBend handles the lid", "Close the lid to test: fold, then sleep. Open it and the desktop lifts.");
        } else {
            self.refresh_power();
            self.publish_menu();
            win::message_box(self.msg_hwnd, "WinBend", "The change was not applied (admin approval was declined or failed). The lid behaves as before.", MB_OK | MB_ICONWARNING);
        }
    }

    fn toggle_skip_signin(&mut self) {
        use crate::power::{write_elevated, Setting};
        if self.skip_signin() {
            if write_elevated(Setting::LockOnWake, 1) {
                self.refresh_power();
                self.publish_menu();
                win::tray_notify(self.msg_hwnd, "Sign-in after sleep restored", "Windows asks for your password after sleep again.");
            } else {
                self.refresh_power();
                self.publish_menu();
            }
            return;
        }
        let text = "WinBend will set Windows' \"require sign-in after sleep\" to Never (Windows asks for admin approval once).\n\nWaking the PC then goes straight to the unfolding desktop instead of the sign-in screen. Anyone who opens the lid gets your desktop without a password until you turn this back on.\n\nThis is the same setting as Settings > Accounts > Sign-in options > \"when should Windows require you to sign in again\", which Windows 11 hides when Windows Hello-only sign-in is on.\n\nContinue?";
        if win::message_box(self.msg_hwnd, "Skip the sign-in screen after sleep", text, MB_YESNO | MB_ICONQUESTION) != IDYES {
            return;
        }
        if write_elevated(Setting::LockOnWake, 0) {
            self.refresh_power();
            self.publish_menu();
            win::tray_notify(self.msg_hwnd, "Sign-in after sleep: never", "Sleep and wake to test. Turn it back on from the same menu any time.");
        } else {
            self.refresh_power();
            self.publish_menu();
            win::message_box(self.msg_hwnd, "WinBend", "The change was not applied (admin approval was declined or failed).", MB_OK | MB_ICONWARNING);
        }
    }

    /// Periodic timer: idle detection.
    pub fn tick_timer(&mut self) {
        self.calibration_timeout();
        if self.calib.is_some() { return; }
        let minutes = self.cfg.fold_when_idle_min;
        if minutes == 0 || self.mode != Mode::Idle {
            return;
        }
        if win::idle_seconds() >= minutes as f64 * 60.0 && self.begin_session(Kind::IdleCurtain) {
            self.t = 0.0;
            self.animate_to(1.0);
        }
    }

    pub fn mouse_move(&mut self) {
        if matches!(self.kind, Kind::IdleCurtain | Kind::Unfold) && matches!(self.mode, Mode::Hold) && !self.sleep_after_fold {
            self.animate_to(0.0);
        }
    }

    pub fn display_changed(&mut self) {
        // Geometry changes invalidate the overlay and the capture item.
        self.capture = None;
        self.preview = None;
        self.preview_dirty = true;
        self.overlay = None;
        self.mode = Mode::Idle;
        self.t = 0.0;
        self.prev_fg = None;
    }

    // ------------------------------------------------------------------ menu

    pub fn menu(&mut self, cmd: u32) {
        use win::*;
        match cmd {
            CMD_SETTINGS => self.open_settings("fold"),
            CMD_FOLD => self.toggle_fold(),
            CMD_STYLE_SILK => { self.cfg.style = "silk".into(); self.save(); }
            CMD_STYLE_SHADE => { self.cfg.style = "shade".into(); self.save(); }
            CMD_STYLE_FROST => { self.cfg.style = "frost".into(); self.save(); }
            CMD_STYLE_ORIGAMI => { self.cfg.style = "origami".into(); self.save(); }
            CMD_STYLE_CUSTOM => { self.cfg.style = "custom".into(); self.save(); }
            CMD_FOLLOW_HINGE => { self.cfg.follow_hinge = !self.cfg.follow_hinge; self.save(); }
            CMD_FOLLOW_ACCEL => { self.cfg.follow_accelerometer = !self.cfg.follow_accelerometer; self.save(); }
            CMD_ACCEL_CALIBRATE => self.calibrate_accel(),
            CMD_FOLLOW_CAMERA => self.toggle_camera_tracking(),
            CMD_UNFOLD_ON_LID => { self.cfg.unfold_on_lid_open = !self.cfg.unfold_on_lid_open; self.save(); }
            CMD_UNFOLD_ON_WAKE => { self.cfg.unfold_on_wake = !self.cfg.unfold_on_wake; self.save(); }
            CMD_FOLD_ON_SLEEP => { self.cfg.fold_on_sleep = !self.cfg.fold_on_sleep; self.save(); }
            CMD_SIGNIN_SETTINGS => win::open_url("ms-settings:signinoptions"),
            CMD_CAMERA_CALIBRATE => { self.open_settings("calibrate"); if self.ui.is_none() { self.start_calibration(); } },
            c if (CMD_FLAT_BASE..CMD_FLAT_BASE + FLAT_CHOICES.len() as u32).contains(&c) => {
                let deg = FLAT_CHOICES[(c - CMD_FLAT_BASE) as usize] as f32;
                self.cfg.camera_flat_deg = deg;
                if self.cfg.camera_travel_deg < deg + 5.0 {
                    self.cfg.camera_travel_deg = deg + 5.0;
                }
                self.save();
            }
            CMD_HANDLE_LID => self.toggle_handle_lid(),
            CMD_SKIP_SIGNIN => self.toggle_skip_signin(),
            CMD_AFTER_NONE => { self.cfg.after_fold = "none".into(); self.save(); }
            CMD_AFTER_LOCK => { self.cfg.after_fold = "lock".into(); self.save(); }
            CMD_AFTER_SLEEP => { self.cfg.after_fold = "sleep".into(); self.save(); }
            CMD_AFTER_DISPLAY_OFF => { self.cfg.after_fold = "display_off".into(); self.save(); }
            CMD_IDLE_OFF => { self.cfg.fold_when_idle_min = 0; self.save(); }
            CMD_IDLE_1 => { self.cfg.fold_when_idle_min = 1; self.save(); }
            CMD_IDLE_5 => { self.cfg.fold_when_idle_min = 5; self.save(); }
            CMD_IDLE_10 => { self.cfg.fold_when_idle_min = 10; self.save(); }
            CMD_IDLE_15 => { self.cfg.fold_when_idle_min = 15; self.save(); }
            CMD_STARTUP => {
                let want = !self.cfg.run_at_startup;
                if win::set_run_at_startup(want) {
                    self.cfg.run_at_startup = want;
                    self.save();
                }
            }
            CMD_DONATE => win::open_url(crate::links::DONATE_URL),
            CMD_GITHUB => win::open_url(crate::links::GITHUB_URL),
            CMD_OPEN_SETTINGS => {
                let _ = self.cfg.save();
                win::open_url(&Config::path().display().to_string());
            }
            CMD_RELOAD_SETTINGS => self.reload_settings(),
            CMD_ABOUT => self.open_settings("about"),
            CMD_QUIT => self.want_quit = true,
            _ => {}
        }
    }

    pub fn reload_settings(&mut self) {
        match std::fs::read_to_string(Config::path()).map_err(|e| e.to_string()).and_then(|s| toml::from_str::<Config>(&s).map_err(|e| e.to_string())) {
            Ok(cfg) => if let Err(e) = self.apply_config(cfg) { self.notify(&e, "error"); },
            Err(e) => self.notify(&format!("Could not reload settings: {e}"), "error"),
        }
    }

    /// Webcam lid tracking on/off, from the tray, the settings page, or its own hotkey.
    pub fn toggle_camera_tracking(&mut self) {
        if self.calib.is_some() {
            self.notify("Finish or cancel calibration first.", "info");
            return;
        }
        if !self.camera_available {
            self.notify("No webcam found on this PC, so lid tracking cannot be turned on.", "error");
            return;
        }
        self.cfg.follow_camera = !self.cfg.follow_camera;
        self.save();
        self.sync_camera();
        if self.cfg.follow_camera {
            self.notify("Webcam lid tracking is on. Close the lid slowly to test. Frames stay in memory; your camera light stays on while this is enabled.", "info");
        } else {
            self.notify("Webcam lid tracking is off.", "info");
        }
        self.post_state();
    }

    /// Register both global hotkeys from the current settings.
    pub fn apply_hotkey(&mut self) {
        self.apply_fold_hotkey();
        match crate::config::parse_hotkey(&self.cfg.camera_hotkey) {
            Some((mods, vk)) if win::register_hotkey_id(self.msg_hwnd, win::HOTKEY_ID_CAMERA, mods, vk) => {}
            _ => {
                win::tray_notify(
                    self.msg_hwnd,
                    "WinBend tracking shortcut unavailable",
                    &format!("Could not register {}. Change the tracking shortcut in Settings.", self.cfg.camera_hotkey),
                );
            }
        }
    }

    fn apply_fold_hotkey(&mut self) {
        match crate::config::parse_hotkey(&self.cfg.hotkey) {
            Some((mods, vk)) if win::register_hotkey(self.msg_hwnd, mods, vk) => {}
            _ => {
                win::tray_notify(
                    self.msg_hwnd,
                    "WinBend hotkey unavailable",
                    &format!("Could not register {}. Change `hotkey` in the settings file.", self.cfg.hotkey),
                );
            }
        }
    }

    pub fn handle(&mut self, ev: Event) {
        match ev {
            Event::Ui(ev) => self.handle_ui(ev),
            Event::Hotkey => if !self.hotkey_capture { self.toggle_fold(); },
            Event::CameraHotkey => if !self.hotkey_capture { self.toggle_camera_tracking(); },
            Event::Menu(cmd) => { self.menu(cmd); self.post_state(); },
            Event::Hinge(a) => self.hinge(a),
            Event::Accel(g) => self.accel(g),
            Event::Camera { tilt_deg, dark } => self.camera(tilt_deg, dark),
            Event::CameraStatus(ok) => self.camera_status(ok),
            Event::LidOpen => self.lid(true),
            Event::LidClosed => self.lid(false),
            Event::Wake => self.wake(),
            Event::Suspend => self.sleeping(),
            Event::Lock => self.locked = true,
            Event::Unlock => self.unlocked(),
            Event::Tick => self.tick_timer(),
            Event::MouseMove => self.mouse_move(),
            Event::Dismiss => self.dismiss(),
            Event::Wheel(d) => self.wheel(d),
            Event::DisplayChange => self.display_changed(),
            Event::TaskbarCreated => {
                win::tray_add(self.msg_hwnd, "WinBend");
                self.publish_menu();
            }
            Event::Quit => self.want_quit = true,
        }
    }

    fn after_fold_action(&mut self, now: Instant) {
        match self.cfg.after_fold.as_str() {
            "lock" => self.lock_pending = Some(now + Duration::from_millis(120)),
            "sleep" => {
                self.end_session();
                win::sleep_pc();
            }
            "display_off" => {
                if let Some(o) = &self.overlay {
                    win::display_off(o.hwnd);
                }
                // From here on behave like the idle curtain: the first touch unfolds.
                self.kind = Kind::IdleCurtain;
            }
            _ => {}
        }
    }

    // ------------------------------------------------------------------ per-frame

    /// Advance the animation and draw one frame. Call only while `active()`.
    pub fn tick(&mut self) {
        let now = Instant::now();
        let dt = (now - self.last_tick).as_secs_f32().min(0.1);
        self.last_tick = now;

        // The idle curtain lifts on any input anywhere, even if it never reaches our window.
        if matches!(self.kind, Kind::IdleCurtain | Kind::Unfold) && matches!(self.mode, Mode::Hold | Mode::Anim { to: 1.0, .. }) {
            let since_start = (now - self.session_start).as_secs_f64();
            if win::idle_seconds() + 0.5 < since_start {
                self.animate_to(0.0);
            }
        }

        // Webcam watchdog: if the camera stops reporting (unplugged, reopening after sleep),
        // do not leave the desktop parked behind a black fold.
        if let Mode::Follow { .. } = self.mode {
            if self.follow_from_camera && self.last_camera_msg.map_or(false, |t| now - t > Duration::from_secs(3)) {
                self.user_override();
            }
        }

        // Sensor sessions must never trap the user. While a sensor holds the fold nearly
        // closed with the camera seeing a bright (open-lid) scene, lift on any user input
        // after a short grace period, and lift unconditionally after 20 s.
        if self.kind == Kind::Sensor && matches!(self.mode, Mode::Follow { .. }) {
            if self.t >= 0.9 {
                let since = *self.parked_since.get_or_insert(now);
                let parked_for = now - since;
                let lid_looks_open = !self.follow_from_camera || !self.camera_dark;
                if lid_looks_open && parked_for > Duration::from_millis(1500) && win::idle_seconds() < 0.25 {
                    self.user_override();
                } else if lid_looks_open && parked_for > Duration::from_secs(20) {
                    self.user_override();
                }
            } else {
                self.parked_since = None;
            }
        } else {
            self.parked_since = None;
        }

        // 1. advance t
        let mut finished_unfold = false;
        let mut finished_fold = false;
        match self.mode {
            Mode::Idle => return,
            Mode::Anim { from, to, start, dur } => {
                let x = (now - start).as_secs_f32() / dur.as_secs_f32().max(0.01);
                self.t = from + (to - from) * ease_in_out(x);
                if x >= 1.0 {
                    self.t = to;
                    if to <= 0.0 {
                        finished_unfold = true;
                    } else {
                        finished_fold = true;
                        self.mode = Mode::Hold;
                    }
                }
            }
            Mode::Hold => {}
            Mode::Follow { target, zero_since } => {
                let k = 1.0 - (-dt / 0.07).exp();
                self.t += (target - self.t) * k;
                if target <= 0.0 && self.t < 0.004 {
                    let since = zero_since.unwrap_or(now);
                    self.mode = Mode::Follow { target, zero_since: Some(since) };
                    if now - since > Duration::from_millis(150) {
                        finished_unfold = true;
                    }
                } else if zero_since.is_some() {
                    self.mode = Mode::Follow { target, zero_since: None };
                }
            }
        }

        if finished_fold {
            if self.sleep_after_fold {
                self.sleep_after_fold = false;
                win::sleep_pc();
            } else if self.demo {
                self.demo_hold_until = Some(now + Duration::from_millis(900));
            } else if self.kind == Kind::Manual {
                self.after_fold_action(now);
                if self.mode == Mode::Idle {
                    return;
                }
            }
        }
        if let Some(when) = self.lock_pending {
            if now >= when {
                self.lock_pending = None;
                win::lock_workstation();
                self.end_session();
                return;
            }
        }
        if let Some(until) = self.demo_hold_until {
            if now >= until {
                self.demo_hold_until = None;
                self.animate_to(0.0);
            }
        }
        if finished_unfold {
            self.end_session();
            return;
        }

        // 2. capture + draw
        let Some(capture) = self.capture.as_mut() else { return };
        let had_frames = capture.frames > 0;
        capture.poll(&self.gpu);
        if capture.frames == 0 {
            // No frame yet: keep the overlay hidden so the user sees the real desktop.
            return;
        }
        if !had_frames {
            // First frame: restart the animation clock so nothing is skipped while we waited.
            if let Mode::Anim { from, to, dur, .. } = self.mode {
                self.mode = Mode::Anim { from, to, start: now, dur };
                self.t = from;
            }
        }
        let Some(overlay) = self.overlay.as_mut() else { return };
        let (w, h) = (overlay.geom.width(), overlay.geom.height());
        let params = fold_params(&self.cfg.style_params(), self.t, self.cfg.max_tilt_deg, self.cfg.background_rgba());
        let r: Result<()> = (|| {
            let bb: ID3D11Texture2D = unsafe { overlay.swapchain.GetBuffer(0)? };
            self.renderer.render(&self.gpu, &capture.srv, &bb, w, h, &params)?;
            unsafe { overlay.swapchain.Present(1, windows::Win32::Graphics::Dxgi::DXGI_PRESENT(0)).ok()? };
            Ok(())
        })();
        if let Err(e) = r {
            eprintln!("render failed: {e}");
            self.end_session();
            return;
        }
        // Below a hair of fold the picture is the desktop itself: keep the overlay hidden so the
        // mouse and keyboard keep working. Once shown, hand clicks through and leave the cursor
        // alone until the fold is clearly engaged.
        const SHOW_AT: f32 = 0.012;
        const ENGAGE_AT: f32 = 0.2;
        if self.t < SHOW_AT {
            if overlay.visible {
                overlay.hide();
            }
            return;
        }
        let manual = matches!(self.kind, Kind::Manual | Kind::IdleCurtain);
        overlay.set_engaged(manual || self.t >= ENGAGE_AT);
        if !overlay.visible {
            if manual {
                self.prev_fg = Some(win::foreground_window());
            }
            overlay.show(manual);
        }
    }

    /// Load a PNG into a BGRA8 shader resource (used by `--render-clip --source`).
    fn load_png_texture(gpu: &Gpu, path: &std::path::Path) -> Result<(windows::Win32::Graphics::Direct3D11::ID3D11ShaderResourceView, u32, u32)> {
        use windows::Win32::Graphics::Direct3D11::*;
        use windows::Win32::Graphics::Dxgi::Common::*;
        fn fail<E>(_: E) -> windows::core::Error { windows::core::Error::from_hresult(windows::core::HRESULT(-2147467259)) }
        let file = std::fs::File::open(path).map_err(fail)?;
        let mut decoder = png::Decoder::new(std::io::BufReader::new(file));
        decoder.set_transformations(png::Transformations::normalize_to_color8() | png::Transformations::ALPHA);
        let mut reader = decoder.read_info().map_err(fail)?;
        let mut buf = vec![0u8; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).map_err(fail)?;
        let (w, h) = (info.width, info.height);
        let mut bgra = vec![0u8; (w * h * 4) as usize];
        match info.color_type {
            png::ColorType::Rgba => {
                for (s, d) in buf.chunks_exact(4).zip(bgra.chunks_exact_mut(4)) {
                    d.copy_from_slice(&[s[2], s[1], s[0], 255]);
                }
            }
            png::ColorType::GrayscaleAlpha => {
                for (s, d) in buf.chunks_exact(2).zip(bgra.chunks_exact_mut(4)) {
                    d.copy_from_slice(&[s[0], s[0], s[0], 255]);
                }
            }
            _ => return Err(fail(())),
        }
        let desc = D3D11_TEXTURE2D_DESC {
            Width: w,
            Height: h,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_IMMUTABLE,
            BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
            CPUAccessFlags: 0,
            MiscFlags: 0,
        };
        let init = D3D11_SUBRESOURCE_DATA { pSysMem: bgra.as_ptr() as *const _, SysMemPitch: w * 4, SysMemSlicePitch: 0 };
        let mut tex = None;
        unsafe { gpu.device.CreateTexture2D(&desc, Some(&init), Some(&mut tex))? };
        let tex = tex.unwrap();
        let mut srv = None;
        unsafe { gpu.device.CreateShaderResourceView(&tex, None, Some(&mut srv))? };
        Ok((srv.unwrap(), w, h))
    }

    /// Offline render for `--render-clip`: a full close-hold-open cycle as numbered PNG frames,
    /// from a PNG (`source`) or the live primary monitor. Used to produce promo videos.
    pub fn render_clip(gpu: &Gpu, renderer: &mut Renderer, style: &StyleParams, max_tilt: f32, bg: [f32; 4], source: Option<&std::path::Path>, frames: u32, out_dir: &std::path::Path) -> Result<()> {
        use windows::Win32::Graphics::Direct3D11::*;
        use windows::Win32::Graphics::Dxgi::Common::*;
        fn fail<E>(_: E) -> windows::core::Error { windows::core::Error::from_hresult(windows::core::HRESULT(-2147467259)) }
        std::fs::create_dir_all(out_dir).map_err(fail)?;
        let mut _cap = None;
        let (srv, w, h) = match source {
            Some(p) => Self::load_png_texture(gpu, p)?,
            None => {
                let mut cap = Capture::for_monitor(gpu, win::primary_monitor().hmon)?;
                let deadline = Instant::now() + Duration::from_secs(3);
                while cap.frames == 0 && Instant::now() < deadline {
                    cap.poll(gpu);
                    std::thread::sleep(Duration::from_millis(10));
                }
                if cap.frames == 0 {
                    return Err(fail(()));
                }
                let r = (cap.srv.clone(), cap.width, cap.height);
                _cap = Some(cap);
                r
            }
        };
        let mk = |usage: D3D11_USAGE, bind: u32, cpu: u32| D3D11_TEXTURE2D_DESC {
            Width: w,
            Height: h,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: usage,
            BindFlags: bind,
            CPUAccessFlags: cpu,
            MiscFlags: 0,
        };
        let mut dst = None;
        let mut staging = None;
        unsafe {
            gpu.device.CreateTexture2D(&mk(D3D11_USAGE_DEFAULT, D3D11_BIND_RENDER_TARGET.0 as u32, 0), None, Some(&mut dst))?;
            gpu.device.CreateTexture2D(&mk(D3D11_USAGE_STAGING, 0, D3D11_CPU_ACCESS_READ.0 as u32), None, Some(&mut staging))?;
        }
        let dst = dst.unwrap();
        let staging = staging.unwrap();
        let frames = frames.max(2);
        for i in 0..frames {
            // Close over the first 45 %, rest folded, then open: the same feel as a hotkey fold.
            let u = i as f32 / (frames - 1) as f32;
            let t = if u < 0.45 {
                ease_in_out(u / 0.45)
            } else if u < 0.58 {
                1.0
            } else {
                1.0 - ease_in_out((u - 0.58) / 0.42)
            };
            let params = fold_params(style, t, max_tilt, bg);
            renderer.render(gpu, &srv, &dst, w, h, &params)?;
            unsafe { gpu.ctx.CopyResource(&staging, &dst); }
            let rgba = crate::gfx::readback_rgba(gpu, &staging, w, h)?;
            let file = std::fs::File::create(out_dir.join(format!("{i:04}.png"))).map_err(fail)?;
            let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            enc.set_compression(png::Compression::Fast);
            enc.set_filter(png::FilterType::Sub);
            let mut writer = enc.write_header().map_err(fail)?;
            writer.write_image_data(&rgba).map_err(fail)?;
        }
        Ok(())
    }

    /// Offline render for `--render-test`: capture one frame and write a PNG.
    pub fn render_test(gpu: &Gpu, renderer: &mut Renderer, t: f32, style: &StyleParams, max_tilt: f32, bg: [f32; 4], out: &std::path::Path) -> Result<()> {
        use windows::Win32::Graphics::Direct3D11::*;
        use windows::Win32::Graphics::Dxgi::Common::*;
        let geom = win::primary_monitor();
        let mut cap = Capture::for_monitor(gpu, geom.hmon)?;
        let deadline = Instant::now() + Duration::from_secs(3);
        while cap.frames == 0 && Instant::now() < deadline {
            cap.poll(gpu);
            std::thread::sleep(Duration::from_millis(10));
        }
        if cap.frames == 0 {
            eprintln!("no capture frame arrived within 3 s");
            return Err(windows::core::Error::from_hresult(windows::core::HRESULT(-2147467259)));
        }
        let (w, h) = (cap.width, cap.height);
        let mk = |usage: D3D11_USAGE, bind: u32, cpu: u32| D3D11_TEXTURE2D_DESC {
            Width: w,
            Height: h,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: usage,
            BindFlags: bind,
            CPUAccessFlags: cpu,
            MiscFlags: 0,
        };
        let mut dst = None;
        let mut staging = None;
        unsafe {
            gpu.device.CreateTexture2D(&mk(D3D11_USAGE_DEFAULT, D3D11_BIND_RENDER_TARGET.0 as u32, 0), None, Some(&mut dst))?;
            gpu.device.CreateTexture2D(&mk(D3D11_USAGE_STAGING, 0, D3D11_CPU_ACCESS_READ.0 as u32), None, Some(&mut staging))?;
        }
        let dst = dst.unwrap();
        let staging = staging.unwrap();
        let params = fold_params(style, t, max_tilt, bg);
        renderer.render(gpu, &cap.srv, &dst, w, h, &params)?;
        unsafe { gpu.ctx.CopyResource(&staging, &dst); }
        let rgba = crate::gfx::readback_rgba(gpu, &staging, w, h)?;
        let file = std::fs::File::create(out).map_err(|_| windows::core::Error::from_hresult(windows::core::HRESULT(-2147024894)))?;
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().map_err(|_| windows::core::Error::from_hresult(windows::core::HRESULT(-2147467259)))?;
        writer.write_image_data(&rgba).map_err(|_| windows::core::Error::from_hresult(windows::core::HRESULT(-2147467259)))?;
        Ok(())
    }
}
