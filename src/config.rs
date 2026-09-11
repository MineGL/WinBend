//! User settings, stored as TOML in %APPDATA%\WinBend\config.toml.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct StyleParams {
    /// Camera distance in half-heights. Smaller = stronger perspective. 1.5 .. 6.
    pub perspective: f32,
    /// 0..1 blur strength.
    pub blur: f32,
    /// 0..1 shading strength (panel darkens as it turns away).
    pub shadow: f32,
    /// 0..1 how much of the panel bends (0 = rigid hinge).
    pub bend: f32,
    /// 0..1 extra darkening toward the far edge.
    pub vignette: f32,
    /// 0..1 glossy highlight.
    pub sheen: f32,
}

impl StyleParams {
    pub const SILK: StyleParams = StyleParams { perspective: 2.6, blur: 0.0, shadow: 0.40, bend: 0.30, vignette: 0.25, sheen: 0.8 };
    pub const SHADE: StyleParams = StyleParams { perspective: 2.4, blur: 0.15, shadow: 0.90, bend: 0.20, vignette: 0.55, sheen: 0.2 };
    pub const FROST: StyleParams = StyleParams { perspective: 2.8, blur: 1.0, shadow: 0.45, bend: 0.35, vignette: 0.30, sheen: 0.4 };

    pub fn preset(name: &str) -> Option<StyleParams> {
        match name {
            "silk" => Some(Self::SILK),
            "shade" => Some(Self::SHADE),
            "frost" => Some(Self::FROST),
            _ => None,
        }
    }
}

fn d_style() -> String { "silk".into() }
fn d_custom() -> StyleParams { StyleParams::SILK }
fn d_max_tilt() -> f32 { 86.0 }
fn d_fold_ms() -> u32 { 1100 }
fn d_hotkey() -> String { "Ctrl+Alt+B".into() }
fn d_camera_hotkey() -> String { "Ctrl+Alt+C".into() }
fn d_true() -> bool { true }
fn d_hinge_open() -> f32 { 100.0 }
fn d_hinge_closed() -> f32 { 10.0 }
fn d_bg() -> String { "#000000".into() }
fn d_after_fold() -> String { "none".into() }
fn d_accel_travel() -> f32 { 85.0 }
fn d_camera_vfov() -> f32 { 42.0 }
fn d_camera_travel() -> f32 { 75.0 }
fn d_camera_flat() -> f32 { 40.0 }

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Config {
    /// "silk" | "shade" | "frost" | "custom"
    #[serde(default = "d_style")]
    pub style: String,
    #[serde(default = "d_custom")]
    pub custom: StyleParams,
    /// Tilt in degrees when fully folded (90 = edge-on).
    #[serde(default = "d_max_tilt")]
    pub max_tilt_deg: f32,
    /// Duration of the hotkey fold animation.
    #[serde(default = "d_fold_ms")]
    pub fold_ms: u32,
    /// Global hotkey, e.g. "Ctrl+Alt+B", "Ctrl+Shift+F12", "Win+Alt+L".
    #[serde(default = "d_hotkey")]
    pub hotkey: String,
    /// Global hotkey that turns webcam lid tracking on or off.
    #[serde(default = "d_camera_hotkey")]
    pub camera_hotkey: String,
    /// What happens once a hotkey fold completes: "none" | "lock" | "sleep" | "display_off".
    #[serde(default = "d_after_fold")]
    pub after_fold: String,
    /// Legacy flag from 0.1.0; migrated into `after_fold` on load.
    #[serde(default, skip_serializing)]
    pub lock_after_fold: bool,
    /// Fold automatically after this many minutes without input (0 = off).
    #[serde(default)]
    pub fold_when_idle_min: u32,
    /// Play the unfold animation when the PC resumes from sleep or is unlocked.
    #[serde(default = "d_true")]
    pub unfold_on_wake: bool,
    /// Finish the fold when the lid closes or the PC goes to sleep, and stay folded until wake.
    #[serde(default = "d_true")]
    pub fold_on_sleep: bool,
    /// WinBend owns the lid: the Windows lid action is "do nothing", WinBend folds, then sleeps.
    #[serde(default)]
    pub handle_lid: bool,
    /// Drive the fold from the lid accelerometer (needs calibration from the tray menu).
    #[serde(default)]
    pub follow_accelerometer: bool,
    /// Gravity vector (g) measured with the lid at your normal open angle.
    #[serde(default)]
    pub accel_calibration: Option<[f32; 3]>,
    /// Degrees of lid rotation from the calibrated pose to reach fully folded.
    #[serde(default = "d_accel_travel")]
    pub accel_travel_deg: f32,
    /// Drive the fold from webcam motion (camera light stays on while enabled).
    #[serde(default)]
    pub follow_camera: bool,
    /// Vertical field of view of the webcam in degrees (typical laptops: 38-50).
    #[serde(default = "d_camera_vfov")]
    pub camera_vfov_deg: f32,
    /// Degrees of lid rotation seen by the camera to reach fully folded.
    #[serde(default = "d_camera_travel")]
    pub camera_travel_deg: f32,
    /// The desktop is completely flat (no overlay) while the lid is more than this many
    /// degrees from closed; the fold happens within this last stretch.
    #[serde(default = "d_camera_flat")]
    pub camera_flat_deg: f32,
    /// Play the unfold animation when the lid opens.
    #[serde(default = "d_true")]
    pub unfold_on_lid_open: bool,
    /// Follow the hinge angle sensor if the device has one.
    #[serde(default = "d_true")]
    pub follow_hinge: bool,
    /// Hinge angle (degrees) at which the fold starts.
    #[serde(default = "d_hinge_open")]
    pub hinge_open_deg: f32,
    /// Hinge angle (degrees) at which the desktop is fully folded away.
    #[serde(default = "d_hinge_closed")]
    pub hinge_closed_deg: f32,
    /// Background color behind the panel, "#rrggbb".
    #[serde(default = "d_bg")]
    pub background: String,
    #[serde(default)]
    pub run_at_startup: bool,
    #[serde(default = "d_true")]
    pub first_run_done: bool,
}

impl Default for Config {
    fn default() -> Self {
        toml::from_str("").expect("defaults")
    }
}

impl Config {
    pub fn dir() -> PathBuf {
        let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
        base.join("WinBend")
    }

    pub fn path() -> PathBuf {
        Self::dir().join("config.toml")
    }

    pub fn load() -> Config {
        Self::load_from(&Self::path())
    }

    fn load_from(path: &std::path::Path) -> Config {
        match std::fs::read_to_string(path) {
            Ok(s) => {
                let mut c: Config = toml::from_str(&s).unwrap_or_else(|e| {
                    eprintln!("config parse error, using defaults: {e}");
                    Config::default()
                });
                if c.lock_after_fold && c.after_fold == "none" {
                    c.after_fold = "lock".into();
                }
                c
            }
            Err(e) => {
                let mut c = Config::default();
                if e.kind() == std::io::ErrorKind::NotFound {
                    c.first_run_done = false;
                    let _ = c.save_to(path);
                } else { eprintln!("Could not read settings: {e}"); }
                c
            }
        }
    }

    pub fn save(&self) -> std::io::Result<()> {
        self.save_to(&Self::path())
    }

    fn save_to(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
        let s = toml::to_string_pretty(self).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        let header = "# WinBend settings. Edit and save; WinBend reloads this file when you pick\n# \"Reload settings\" from the tray menu.\n\n";
        std::fs::write(path, format!("{header}{s}"))
    }

    pub fn style_params(&self) -> StyleParams {
        StyleParams::preset(&self.style).unwrap_or(self.custom)
    }

    pub fn background_rgba(&self) -> [f32; 4] {
        parse_hex_color(&self.background).unwrap_or([0.0, 0.0, 0.0, 1.0])
    }
}

pub fn parse_hex_color(s: &str) -> Option<[f32; 4]> {
    let h = s.trim().trim_start_matches('#');
    if h.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok()?;
    let c = |x: u32| (x & 0xff) as f32 / 255.0;
    Some([c(v >> 16), c(v >> 8), c(v), 1.0])
}

/// Parsed hotkey: (modifier flags for RegisterHotKey, virtual key code).
pub fn parse_hotkey(s: &str) -> Option<(u32, u32)> {
    // MOD_ALT=1 MOD_CONTROL=2 MOD_SHIFT=4 MOD_WIN=8 MOD_NOREPEAT=0x4000
    let mut mods = 0x4000u32;
    let mut vk = None;
    for part in s.split('+') {
        let p = part.trim();
        match p.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => mods |= 2,
            "alt" => mods |= 1,
            "shift" => mods |= 4,
            "win" | "super" | "meta" => mods |= 8,
            k => {
                if vk.is_some() { return None; }
                vk = Some(match k {
                    "space" => 0x20,
                    "esc" | "escape" => 0x1b,
                    "tab" => 0x09,
                    "enter" | "return" => 0x0d,
                    "backspace" => 0x08,
                    "pause" => 0x13,
                    "home" => 0x24,
                    "end" => 0x23,
                    "insert" => 0x2d,
                    "delete" => 0x2e,
                    "pageup" => 0x21,
                    "pagedown" => 0x22,
                    "up" => 0x26,
                    "down" => 0x28,
                    "left" => 0x25,
                    "right" => 0x27,
                    "`" | "grave" => 0xc0,
                    "-" | "minus" => 0xbd,
                    "=" | "equals" => 0xbb,
                    _ => {
                        if let Some(n) = k.strip_prefix('f') {
                            let n: u32 = n.parse().ok()?;
                            if !(1..=24).contains(&n) {
                                return None;
                            }
                            0x70 + n - 1
                        } else if k.len() == 1 {
                            let c = k.chars().next().unwrap().to_ascii_uppercase();
                            if c.is_ascii_alphanumeric() { c as u32 } else { return None; }
                        } else {
                            return None;
                        }
                    }
                });
            }
        }
    }
    vk.map(|v| (mods, v))
}

/// Validate settings at the UI boundary before any system side effects occur.
pub fn with_field(cfg: &Config, key: &str, value: serde_json::Value) -> Result<Config, String> {
    if matches!(key, "lock_after_fold" | "accel_calibration" | "first_run_done" | "handle_lid") {
        return Err("This setting requires its dedicated action.".into());
    }
    let mut data = serde_json::to_value(cfg).map_err(|e| e.to_string())?;
    let mut slot = &mut data;
    for part in key.split('.') {
        slot = slot.get_mut(part).ok_or_else(|| format!("Unknown setting: {key}"))?;
    }
    *slot = value;
    let mut next: Config = serde_json::from_value(data).map_err(|e| e.to_string())?;
    next.validate()?;
    Ok(next)
}

impl Config {
    pub fn validate(&mut self) -> Result<(), String> {
        if !matches!(self.style.as_str(), "silk" | "shade" | "frost" | "custom") { return Err("Unknown style.".into()); }
        if !matches!(self.after_fold.as_str(), "none" | "lock" | "sleep" | "display_off") { return Err("Unknown after-fold action.".into()); }
        if parse_hex_color(&self.background).is_none() { return Err("Use a six-digit color such as #102030.".into()); }
        let Some((mods, _)) = parse_hotkey(&self.hotkey) else { return Err("Invalid hotkey.".into()) };
        if mods & 11 == 0 { return Err("Include Ctrl, Alt, or Win in the hotkey.".into()); }
        let Some((cam_mods, _)) = parse_hotkey(&self.camera_hotkey) else { return Err("Invalid tracking shortcut.".into()) };
        if cam_mods & 11 == 0 { return Err("Include Ctrl, Alt, or Win in the tracking shortcut.".into()); }
        if parse_hotkey(&self.hotkey) == parse_hotkey(&self.camera_hotkey) { return Err("The fold and tracking shortcuts must be different.".into()); }
        fn limit(v: &mut f32, lo: f32, hi: f32) -> Result<(), String> {
            if !v.is_finite() { return Err("Settings must be finite numbers.".into()); }
            *v = v.clamp(lo, hi); Ok(())
        }
        limit(&mut self.max_tilt_deg, 5.0, 89.5)?;
        self.fold_ms = self.fold_ms.clamp(150, 5000);
        self.fold_when_idle_min = self.fold_when_idle_min.min(1440);
        limit(&mut self.custom.perspective, 1.2, 8.0)?;
        for v in [&mut self.custom.blur, &mut self.custom.shadow, &mut self.custom.bend, &mut self.custom.vignette, &mut self.custom.sheen] { limit(v, 0.0, 1.0)?; }
        limit(&mut self.camera_vfov_deg, 20.0, 120.0)?;
        limit(&mut self.camera_travel_deg, 20.0, 120.0)?;
        limit(&mut self.camera_flat_deg, 5.0, self.camera_travel_deg - 5.0)?;
        limit(&mut self.accel_travel_deg, 20.0, 170.0)?;
        limit(&mut self.hinge_closed_deg, 0.0, 359.0)?;
        limit(&mut self.hinge_open_deg, self.hinge_closed_deg + 1.0, 360.0)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn first_run_and_upgrade() {
        let dir = std::env::temp_dir().join(format!("winbend-config-test-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let path = dir.join("config.toml");
        let cfg = Config::load_from(&path);
        assert!(!cfg.first_run_done);
        assert!(path.exists());
        std::fs::write(&path, "style = \"shade\"\n").unwrap();
        assert!(Config::load_from(&path).first_run_done);
        std::fs::write(&path, "this is invalid toml").unwrap();
        assert!(Config::load_from(&path).first_run_done);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "this is invalid toml");
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }
    #[test]
    fn field_validation() {
        let cfg = Config::default();
        assert_eq!(with_field(&cfg, "custom.blur", json!(0.7)).unwrap().custom.blur, 0.7);
        assert_eq!(with_field(&cfg, "fold_ms", json!(1)).unwrap().fold_ms, 150);
        for (key, value) in [("missing", json!(true)), ("custom.nope", json!(1)), ("follow_camera", json!("true")), ("first_run_done", json!(true)), ("handle_lid", json!(true)), ("background", json!("invalid")), ("style", json!("invalid")), ("hotkey", json!("B"))] {
            assert!(with_field(&cfg, key, value).is_err(), "{key}");
        }
        let c = with_field(&cfg, "camera_travel_deg", json!(20)).unwrap();
        assert_eq!(c.camera_flat_deg, 15.0);
        assert!(cfg.first_run_done); // Existing config files do not get a welcome again.
    }
    #[test]
    fn picker_tokens_and_invalid_chords() {
        for key in ["A", "5", "F1", "F24", "Space", "Tab", "Enter", "Backspace", "Pause", "Home", "End", "Insert", "Delete", "PageUp", "PageDown", "Up", "Down", "Left", "Right", "Grave", "Minus", "Equals"] {
            assert!(parse_hotkey(&format!("Ctrl+Alt+{key}")).is_some(), "{key}");
        }
        for key in ["Ctrl+A+B", "Ctrl+", "F25", "Ctrl+Numpad1"] { assert!(parse_hotkey(key).is_none()); }
    }
}
