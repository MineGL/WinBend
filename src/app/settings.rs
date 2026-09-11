//! Settings presenter. All App mutations happen after dispatch, never in COM callbacks.
use super::*;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum PageMsg {
    Ready,
    Set { key: String, value: Value, #[serde(default)] confirmed: bool },
    Action { name: String, #[serde(default)] arg: Value },
    Preview { #[serde(default)] t: f32, #[serde(default = "yes")] visible: bool },
    TiltStream { on: bool },
    CaptureHotkey { on: bool },
}
fn yes() -> bool { true }

impl App {
    pub fn open_settings(&mut self, page: &str) {
        self.ui_page = page.into();
        if let Some(ui) = &self.ui {
            ui.show();
            self.ui_send(json!({"type":"navigate", "page":page}));
            return;
        }
        match crate::ui::SettingsWindow::new() {
            Ok(ui) => { self.ui = Some(ui); self.ui_ready = false; println!("ui: created"); }
            Err(e) => self.ui_fallback(&e.to_string()),
        }
    }
    fn ui_fallback(&mut self, message: &str) {
        self.close_settings();
        if !self.ui_smoke {
            self.cfg.first_run_done = true;
            self.save();
            win::message_box(self.msg_hwnd, "WinBend settings", &format!("The settings window could not start. Microsoft Edge WebView2 Runtime is required.\n\n{message}\n\nThe settings file will open instead. Tray controls still work."), MB_OK | MB_ICONINFORMATION);
            win::open_url(&Config::path().display().to_string());
        }
        eprintln!("ui: unavailable: {message}");
    }
    fn close_settings(&mut self) {
        self.cancel_calibration();
        if self.hotkey_capture { self.hotkey_capture = false; if !self.ui_smoke { self.apply_hotkey(); } }
        self.tilt_stream = false;
        self.preview_visible = false;
        self.preview = None;
        self.preview_wait = None;
        if !self.active() { self.capture = None; }
        self.ui = None;
        if !self.cfg.first_run_done && !self.ui_smoke { self.cfg.first_run_done = true; self.save(); }
    }
    pub(super) fn ui_send(&self, msg: Value) { if let Some(ui) = &self.ui { ui.send(&msg); } }
    pub(super) fn notify(&self, text: &str, kind: &str) {
        if self.ui.is_some() { self.ui_send(json!({"type":"toast", "text":text, "kind":kind})); }
        else { win::tray_notify(self.msg_hwnd, "WinBend", text); }
    }
    fn calib_snapshot(&self) -> Value {
        if let Some(c) = self.calib {
            json!({"type":"calib", "stage":if c.stage == CalibStage::Travel {"travel"} else {"zone"},
                "travel":c.travel, "flat":self.cfg.camera_flat_deg,"max_tilt":c.max_bright_tilt,
                "seen_dark":c.seen_dark,"elapsed_s":c.started.elapsed().as_secs_f32(),
                "message":self.calib_message.get("message").and_then(Value::as_str).unwrap_or("")})
        } else { self.calib_message.clone() }
    }
    pub(super) fn calib_notify(&mut self, stage: &str, message: &str) {
        self.calib_message = json!({"type":"calib","stage":stage,"message":message,"travel":self.cfg.camera_travel_deg,"flat":self.cfg.camera_flat_deg});
        self.ui_send(self.calib_snapshot());
        if self.ui.is_none() { win::tray_notify(self.msg_hwnd, "Lid calibration", message); }
    }
    pub(super) fn post_state(&self) {
        if self.ui.is_none() { return; }
        self.ui_send(json!({"type":"state", "cfg":self.cfg,
            "caps":{"hinge":self.hinge_available,"accel":self.accel_available,"camera":self.camera_available,"webview_version":crate::ui::runtime_version()},
            "power":{"lid_do_nothing":self.lid_do_nothing(), "skip_signin":self.skip_signin()},
            "camera_running":crate::camera::is_running(), "calib":self.calib_snapshot(),
            "links":{"github":crate::links::GITHUB_URL, "donate":crate::links::DONATE_URL,
                "wallets":crate::links::WALLETS.iter().map(|w| json!({"name":w.name,"address":w.address})).collect::<Vec<_>>()},
            "version":env!("CARGO_PKG_VERSION"), "settings_path":Config::path().display().to_string(),
            "first_run":!self.cfg.first_run_done, "page":self.ui_page}));
    }
    /// Register `hotkey` under `id`; on failure re-register `previous` so nothing is lost.
    fn try_hotkey(&mut self, id: i32, hotkey: &str, previous: &str) -> std::result::Result<(), String> {
        let (mods, vk) = crate::config::parse_hotkey(hotkey).ok_or("Invalid hotkey")?;
        if self.ui_smoke { return Ok(()); }
        if win::register_hotkey_id(self.msg_hwnd, id, mods, vk) { Ok(()) }
        else {
            if let Some((mods, vk)) = crate::config::parse_hotkey(previous) { win::register_hotkey_id(self.msg_hwnd, id, mods, vk); }
            Err(format!("{hotkey} is already used by another app. Your previous shortcut is unchanged."))
        }
    }
    pub(super) fn apply_config(&mut self, mut next: Config) -> std::result::Result<(), String> {
        next.validate()?;
        if self.ui_smoke && (next.run_at_startup != self.cfg.run_at_startup || next.follow_camera || next.follow_accelerometer) {
            return Err("System changes are disabled during the UI check.".into());
        }
        if self.calib.is_some() { return Err("Finish or cancel calibration before changing settings.".into()); }
        if next.follow_camera && !self.camera_available { return Err("No webcam is available.".into()); }
        if next.follow_accelerometer && next.accel_calibration.is_none() { return Err("Calibrate the accelerometer first.".into()); }
        let hotkey_changed = next.hotkey != self.cfg.hotkey;
        let cam_hotkey_changed = next.camera_hotkey != self.cfg.camera_hotkey;
        let startup_changed = next.run_at_startup != self.cfg.run_at_startup;
        let camera_changed = next.camera_vfov_deg != self.cfg.camera_vfov_deg || next.camera_travel_deg != self.cfg.camera_travel_deg;
        if hotkey_changed {
            let previous = self.cfg.hotkey.clone();
            self.try_hotkey(win::HOTKEY_ID, &next.hotkey, &previous)?;
        }
        if cam_hotkey_changed {
            let previous = self.cfg.camera_hotkey.clone();
            if let Err(e) = self.try_hotkey(win::HOTKEY_ID_CAMERA, &next.camera_hotkey, &previous) {
                if hotkey_changed { self.apply_hotkey(); }
                return Err(e);
            }
        }
        if startup_changed && !win::set_run_at_startup(next.run_at_startup) {
            if hotkey_changed { self.apply_hotkey(); }
            return Err("Windows could not update startup registration.".into());
        }
        if !self.ui_smoke {
            if let Err(e) = next.save() {
                if hotkey_changed { self.apply_hotkey(); }
                if startup_changed { win::set_run_at_startup(self.cfg.run_at_startup); }
                return Err(format!("Could not save settings: {e}"));
            }
        }
        let camera_enabled = next.follow_camera && !self.cfg.follow_camera;
        self.cfg = next;
        if camera_changed { self.restart_camera(); } else { self.sync_camera(); }
        if camera_enabled { self.notify("Webcam tracking is on. Your camera light stays on; frames are analyzed only in memory.", "info"); }
        self.preview_dirty = true;
        self.publish_menu();
        self.post_state();
        Ok(())
    }
    fn set_power(&mut self, key: &str, value: Value, confirmed: bool) -> std::result::Result<(), String> {
        let enabled = value.as_bool().ok_or("Expected true or false")?;
        if self.ui_smoke { return Err("Power changes are disabled during the UI check.".into()); }
        self.refresh_power();
        let current = if key == "handle_lid" { self.cfg.handle_lid && self.lid_do_nothing() } else { self.skip_signin() };
        if current == enabled { return Ok(()); }
        if enabled && !confirmed { return Err("Confirm this Windows integration change first.".into()); }
        self.notify("Waiting for Windows admin approval...", "info");
        let setting = if key == "handle_lid" { crate::power::Setting::LidAction } else { crate::power::Setting::LockOnWake };
        let ok = crate::power::write_elevated(setting, if enabled {0} else {1});
        self.refresh_power();
        if !ok { return Err("The Windows change was declined or could not be applied.".into()); }
        if key == "handle_lid" {
            self.cfg.handle_lid = enabled && self.lid_do_nothing();
            self.cfg.save().map_err(|e| format!("Windows changed, but settings could not be saved: {e}"))?;
        }
        self.publish_menu();
        Ok(())
    }
    pub(super) fn handle_ui(&mut self, event: crate::ui::UiEvent) {
        if self.ui.is_none() { return; }
        use crate::ui::UiEvent;
        match event {
            UiEvent::Closed => self.close_settings(),
            UiEvent::Failed(message) => self.ui_fallback(&message),
            UiEvent::Tick => {
                self.calibration_timeout();
                if self.calib.is_some() { self.ui_send(self.calib_snapshot()); }
                self.preview_tick();
            }
            UiEvent::Message(raw) => {
                if raw.len() > 65536 { return; }
                let msg = match serde_json::from_str::<PageMsg>(&raw) { Ok(m) => m, Err(e) => { self.notify(&format!("Invalid settings message: {e}"), "error"); return; } };
                match msg {
                    PageMsg::Ready => {
                        self.ui_ready = true;
                        if let Some(ui) = &self.ui { ui.ready(); }
                        println!("ui: ready");
                        self.post_state();
                    }
                    PageMsg::Set { key, value, confirmed } => {
                        let result = if matches!(key.as_str(), "handle_lid" | "skip_signin") { self.set_power(&key, value, confirmed) }
                            else { crate::config::with_field(&self.cfg, &key, value).and_then(|cfg| self.apply_config(cfg)) };
                        self.ui_send(json!({"type":"set_result","key":key,"ok":result.is_ok(),"error":result.err()}));
                        self.post_state();
                    }
                    PageMsg::Action { name, arg } => self.ui_action(&name, arg),
                    PageMsg::Preview { t, visible } => {
                        self.preview_visible = visible;
                        self.preview_t = t.clamp(0.0, 1.0);
                        self.preview_dirty = true;
                        if !visible { self.preview = None; self.preview_wait = None; if !self.active() { self.capture = None; } }
                    }
                    PageMsg::TiltStream { on } => { self.tilt_stream = on; }
                    PageMsg::CaptureHotkey { on } => {
                        self.hotkey_capture = on;
                        if !self.ui_smoke { if on { win::unregister_hotkey(self.msg_hwnd); } else { self.apply_hotkey(); } }
                    }
                }
            }
        }
    }
    fn ui_action(&mut self, name: &str, arg: Value) {
        if self.ui_smoke && matches!(name, "fold_now" | "calibrate_start" | "calibrate_accel" | "calibrate_set_zone" | "reload_settings" | "open_url" | "open_settings_file") {
            self.notify("This action is disabled during the UI check.", "info"); return;
        }
        match name {
            "fold_now" => self.toggle_fold(),
            "calibrate_start" => self.start_calibration(),
            "calibrate_cancel" => self.cancel_calibration(),
            "calibrate_set_zone" => self.finish_calibration_zone(),
            "calibrate_accel" => self.calibrate_accel(),
            "open_settings_file" => win::open_url(&Config::path().display().to_string()),
            "reload_settings" => self.reload_settings(),
            "open_url" => match arg.as_str().or_else(|| arg.get("which").and_then(Value::as_str)) {
                Some("github") => win::open_url(crate::links::GITHUB_URL),
                Some("donate") => win::open_url(crate::links::DONATE_URL),
                Some("signin") => win::open_url("ms-settings:signinoptions"),
                Some("webview2") => win::open_url("https://developer.microsoft.com/microsoft-edge/webview2/"),
                _ => self.notify("Unknown destination.", "error"),
            },
            "copy" => {
                // Only addresses we ship may be copied, so the page cannot plant arbitrary text.
                let text = arg.as_str().unwrap_or("");
                if crate::links::WALLETS.iter().any(|w| w.address == text) && win::set_clipboard_text(self.msg_hwnd, text) {
                    self.notify("Address copied to the clipboard. Thank you!", "info");
                } else {
                    self.notify("Could not copy the address.", "error");
                }
            }
            "first_run_done" => { self.cfg.first_run_done = true; if !self.ui_smoke { self.save(); } self.ui_page = "fold".into(); self.ui_send(json!({"type":"navigate","page":"fold"})); },
            "navigate" => { if let Some(p) = arg.as_str() { self.ui_page = p.into(); } },
            "close" => self.close_settings(),
            _ => self.notify("Unknown settings action.", "error"),
        }
    }
    fn preview_tick(&mut self) {
        if !self.preview_visible || !self.ui_ready || self.last_preview.elapsed() < Duration::from_millis(100) { return; }
        if !self.preview_dirty && self.last_preview.elapsed() < Duration::from_millis(500) { return; }
        self.last_preview = Instant::now();
        let result = (|| -> Result<Option<String>> {
            if self.capture.is_none() { self.capture = Some(Capture::for_monitor(&self.gpu, win::primary_monitor().hmon)?); }
            let cap = self.capture.as_mut().ok_or_else(|| windows::core::Error::from_hresult(windows::core::HRESULT(0x80004005u32 as i32)))?;
            cap.poll(&self.gpu);
            if cap.frames == 0 { return Ok(None); }
            if self.preview.is_none() { self.preview = Some(crate::ui::preview::Preview::new(&self.gpu, cap)?); }
            self.preview.as_mut().map(|p| p.render(&self.gpu, cap, &self.cfg, self.preview_t)).transpose()
        })();
        match result {
            Ok(Some(png)) => {
                if !self.ui_preview_ok { println!("ui: preview {} bytes", png.len()); self.ui_preview_ok = true; }
                self.ui_send(json!({"type":"preview", "png":png,"t":self.preview_t}));
                self.preview_dirty = false;
                self.preview_wait = None;
            }
            Ok(None) => {
                let since = *self.preview_wait.get_or_insert_with(Instant::now);
                if since.elapsed() > Duration::from_secs(3) { self.preview_visible = false; self.preview = None; self.preview_wait = None; if !self.active() { self.capture = None; } self.ui_send(json!({"type":"preview_error","message":"No desktop capture frame arrived. Move the slider to retry."})); }
            }
            Err(e) => { self.preview_visible = false; self.preview = None; if !self.active() { self.capture = None; } self.ui_send(json!({"type":"preview_error","message":format!("Desktop preview unavailable: {e}")})); }
        }
    }
}
impl Drop for App { fn drop(&mut self) { self.close_settings(); } }

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn protocol_messages() {
        for msg in [json!({"type":"ready"}),json!({"type":"set","key":"custom.blur","value":0.4}),json!({"type":"action","name":"calibrate_cancel"}),json!({"type":"preview","t":0.5,"visible":true}),json!({"type":"tilt_stream","on":true}),json!({"type":"capture_hotkey","on":false})] { assert!(serde_json::from_value::<PageMsg>(msg).is_ok()); }
        assert!(serde_json::from_value::<PageMsg>(json!({"type":"unknown"})).is_err());
        assert!(serde_json::from_value::<PageMsg>(json!({"type":"set","key":"x"})).is_err());
    }
}
