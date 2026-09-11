//! Win32 plumbing: window class, hidden message window, fullscreen overlay window,
//! tray icon + menu, hotkey, lid-switch notifications, clipboard, registry.
//!
//! The window procedure never touches `App`; it only pushes `Event`s into a queue
//! that the main loop drains. That keeps re-entrancy out of the picture.
use std::collections::VecDeque;
use std::sync::Mutex;

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Dxgi::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Power::*;
use windows::Win32::System::SystemServices::GUID_LIDSWITCH_STATE_CHANGE;
use windows::Win32::System::Registry::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::gfx::device::Gpu;

pub const WM_TRAY: u32 = WM_APP + 1;
pub const WM_HINGE: u32 = WM_APP + 2; // wparam = angle in centi-degrees
pub const WM_ACCEL: u32 = WM_APP + 3; // wparam = packed x/y/z milli-g
pub const WM_CAMERA: u32 = WM_APP + 4; // wparam = centi-degrees + camera::OFFSET, lparam bit0 = dark
pub const WM_CAMERA_STATUS: u32 = WM_APP + 5; // wparam 1 = running, 0 = stopped or unavailable
pub const IDLE_TIMER_ID: usize = 7;
pub const HOTKEY_ID: i32 = 1;
pub const HOTKEY_ID_CAMERA: i32 = 2;
const USERDATA_OVERLAY: isize = 1;

// Tray menu command ids
pub const CMD_FOLD: u32 = 100;
pub const CMD_STYLE_SILK: u32 = 101;
pub const CMD_STYLE_SHADE: u32 = 102;
pub const CMD_STYLE_FROST: u32 = 103;
pub const CMD_STYLE_CUSTOM: u32 = 104;
pub const CMD_FOLLOW_HINGE: u32 = 110;
pub const CMD_UNFOLD_ON_LID: u32 = 111;
pub const CMD_SETTINGS: u32 = 112;
pub const CMD_STARTUP: u32 = 113;
pub const CMD_UNFOLD_ON_WAKE: u32 = 114;
pub const CMD_ACCEL_CALIBRATE: u32 = 115;
pub const CMD_FOLLOW_ACCEL: u32 = 116;
pub const CMD_FOLLOW_CAMERA: u32 = 117;
pub const CMD_SIGNIN_SETTINGS: u32 = 118;
pub const CMD_FOLD_ON_SLEEP: u32 = 119;
pub const CMD_FLAT_BASE: u32 = 180; // + index into FLAT_CHOICES
pub const FLAT_CHOICES: [u32; 6] = [20, 30, 40, 50, 60, 75];
pub const CMD_CAMERA_CALIBRATE: u32 = 172;
pub const CMD_HANDLE_LID: u32 = 170;
pub const CMD_SKIP_SIGNIN: u32 = 171;
pub const CMD_AFTER_NONE: u32 = 150;
pub const CMD_AFTER_LOCK: u32 = 151;
pub const CMD_AFTER_SLEEP: u32 = 152;
pub const CMD_AFTER_DISPLAY_OFF: u32 = 153;
pub const CMD_IDLE_OFF: u32 = 160;
pub const CMD_IDLE_1: u32 = 161;
pub const CMD_IDLE_5: u32 = 162;
pub const CMD_IDLE_10: u32 = 163;
pub const CMD_IDLE_15: u32 = 164;
pub const CMD_DONATE: u32 = 120;
pub const CMD_GITHUB: u32 = 121;
pub const CMD_OPEN_SETTINGS: u32 = 130;
pub const CMD_RELOAD_SETTINGS: u32 = 131;
pub const CMD_ABOUT: u32 = 140;
pub const CMD_QUIT: u32 = 199;

#[derive(Debug)]
pub enum Event {
    Ui(crate::ui::UiEvent),
    Hotkey,
    /// Second global hotkey: toggle webcam lid tracking.
    CameraHotkey,
    Menu(u32),
    Hinge(f32),
    Accel([f32; 3]),
    /// Webcam-estimated lid tilt in degrees from the open pose; `dark` = lid shut.
    Camera { tilt_deg: f32, dark: bool },
    CameraStatus(bool),
    LidOpen,
    LidClosed,
    /// Resume from sleep.
    Wake,
    /// The PC is about to sleep (PBT_APMSUSPEND).
    Suspend,
    /// Session locked / unlocked (sign-in screen shown / dismissed).
    Lock,
    Unlock,
    /// Periodic timer (idle detection).
    Tick,
    MouseMove,
    Dismiss,
    Wheel(i32),
    DisplayChange,
    TaskbarCreated,
    Quit,
}

pub static EVENTS: Mutex<VecDeque<Event>> = Mutex::new(VecDeque::new());

pub fn push_event(e: Event) { push(e); }

fn push(e: Event) {
    if let Ok(mut q) = EVENTS.lock() {
        q.push_back(e);
    }
}

/// Snapshot of what the tray menu should show. Written by App, read in the wndproc.
#[derive(Debug, Clone, Default)]
pub struct MenuState {
    pub style: String,
    pub hotkey: String,
    pub follow_hinge: bool,
    pub hinge_available: bool,
    pub unfold_on_lid_open: bool,
    pub unfold_on_wake: bool,
    pub fold_on_sleep: bool,
    pub handle_lid: bool,
    pub lid_do_nothing: bool,
    pub skip_signin: bool,
    pub after_fold: String,
    pub idle_min: u32,
    pub accel_available: bool,
    pub follow_accel: bool,
    pub accel_calibrated: bool,
    pub camera_available: bool,
    pub follow_camera: bool,
    pub camera_hotkey: String,
    pub flat_deg: u32,
    pub run_at_startup: bool,
}

pub static MENU: Mutex<Option<MenuState>> = Mutex::new(None);
static TASKBAR_CREATED_MSG: Mutex<u32> = Mutex::new(0);

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn hinstance() -> HINSTANCE {
    let m = unsafe { GetModuleHandleW(None) }.unwrap_or_default();
    HINSTANCE(m.0)
}

const CLASS_NAME: PCWSTR = w!("WinBendWindow");

pub fn register_class() -> Result<()> {
    let wc = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(wndproc),
        hInstance: hinstance(),
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW)? },
        lpszClassName: CLASS_NAME,
        ..Default::default()
    };
    let atom = unsafe { RegisterClassW(&wc) };
    if atom == 0 {
        return Err(Error::from_thread());
    }
    *TASKBAR_CREATED_MSG.lock().unwrap() = unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) };
    Ok(())
}

pub fn create_message_window() -> Result<HWND> {
    unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            CLASS_NAME,
            w!("WinBend"),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(hinstance()),
            None,
        )
    }
}

// --------------------------------------------------------------------------------------
// Monitor
// --------------------------------------------------------------------------------------
#[derive(Debug, Clone, Copy)]
pub struct MonitorGeom {
    pub hmon: HMONITOR,
    pub rect: RECT,
}

impl MonitorGeom {
    pub fn width(&self) -> u32 {
        (self.rect.right - self.rect.left).max(1) as u32
    }
    pub fn height(&self) -> u32 {
        (self.rect.bottom - self.rect.top).max(1) as u32
    }
}

pub fn primary_monitor() -> MonitorGeom {
    let hmon = unsafe { MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY) };
    let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
    unsafe { GetMonitorInfoW(hmon, &mut mi).ok().ok() };
    MonitorGeom { hmon, rect: mi.rcMonitor }
}

// --------------------------------------------------------------------------------------
// Overlay window + swapchain
// --------------------------------------------------------------------------------------
pub struct Overlay {
    pub hwnd: HWND,
    pub swapchain: IDXGISwapChain1,
    pub geom: MonitorGeom,
    pub visible: bool,
    engaged: Option<bool>,
}

impl Overlay {
    pub fn new(gpu: &Gpu, geom: MonitorGeom) -> Result<Self> {
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                CLASS_NAME,
                w!("WinBend Overlay"),
                WS_POPUP,
                geom.rect.left,
                geom.rect.top,
                geom.width() as i32,
                geom.height() as i32,
                None,
                None,
                Some(hinstance()),
                None,
            )?
        };
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, USERDATA_OVERLAY);
            // Keep ourselves out of the capture so the effect does not feed back on itself.
            // WDA_EXCLUDEFROMCAPTURE needs Windows 10 2004+; fall back silently.
            if SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE).is_err() {
                let _ = SetWindowDisplayAffinity(hwnd, WDA_MONITOR);
            }
        }
        let factory: IDXGIFactory2 = unsafe { CreateDXGIFactory1()? };
        let desc = DXGI_SWAP_CHAIN_DESC1 {
            Width: geom.width(),
            Height: geom.height(),
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            Stereo: false.into(),
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: 2,
            Scaling: DXGI_SCALING_STRETCH,
            SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
            AlphaMode: DXGI_ALPHA_MODE_UNSPECIFIED,
            Flags: 0,
        };
        let swapchain = unsafe { factory.CreateSwapChainForHwnd(&gpu.device, hwnd, &desc, None, None)? };
        unsafe {
            let _ = factory.MakeWindowAssociation(hwnd, DXGI_MWA_NO_WINDOW_CHANGES | DXGI_MWA_NO_ALT_ENTER);
        }
        Ok(Self { hwnd, swapchain, geom, visible: false, engaged: None })
    }

    pub fn show(&mut self, take_focus: bool) {
        if !self.visible {
            unsafe {
                let _ = SetWindowPos(
                    self.hwnd,
                    Some(HWND_TOPMOST),
                    self.geom.rect.left,
                    self.geom.rect.top,
                    self.geom.width() as i32,
                    self.geom.height() as i32,
                    SWP_SHOWWINDOW | SWP_NOACTIVATE,
                );
            }
            self.visible = true;
        }
        if take_focus {
            unsafe {
                let _ = SetForegroundWindow(self.hwnd);
                let _ = SetFocus(Some(self.hwnd));
            }
        }
    }

    pub fn hide(&mut self) {
        if self.visible {
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
            }
            self.visible = false;
        }
    }

    /// While the fold is barely engaged the overlay looks like the desktop, so the mouse must
    /// keep working: pass clicks through and leave the cursor alone. Once the fold is clearly
    /// under way, take the clicks (they dismiss) and hide the cursor.
    pub fn set_engaged(&mut self, engaged: bool) {
        if self.engaged == Some(engaged) {
            return;
        }
        self.engaged = Some(engaged);
        HIDE_CURSOR.store(engaged, std::sync::atomic::Ordering::Relaxed);
        unsafe {
            let ex = GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE);
            let transparent = WS_EX_TRANSPARENT.0 as isize;
            let new_ex = if engaged { ex & !transparent } else { ex | transparent };
            if new_ex != ex {
                SetWindowLongPtrW(self.hwnd, GWL_EXSTYLE, new_ex);
            }
            // Re-evaluate the cursor under the pointer right away instead of waiting for a move.
            let mut pt = POINT::default();
            if GetCursorPos(&mut pt).is_ok() && WindowFromPoint(pt) == self.hwnd {
                if engaged {
                    SetCursor(None);
                } else if let Ok(c) = LoadCursorW(None, IDC_ARROW) {
                    SetCursor(Some(c));
                }
            }
        }
    }
}

/// Whether the overlay should hide the mouse cursor (read in the window procedure).
pub static HIDE_CURSOR: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

impl Drop for Overlay {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

// --------------------------------------------------------------------------------------
// Tray icon
// --------------------------------------------------------------------------------------
pub(crate) fn app_icon(size: i32) -> HICON {
    unsafe {
        match LoadImageW(Some(hinstance()), PCWSTR(1 as *const u16), IMAGE_ICON, size, size, LR_DEFAULTCOLOR) {
            Ok(h) => HICON(h.0),
            Err(_) => LoadIconW(None, IDI_APPLICATION).unwrap_or_default(),
        }
    }
}

fn nid(hwnd: HWND) -> NOTIFYICONDATAW {
    let mut n = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        ..Default::default()
    };
    n.Anonymous.uVersion = NOTIFYICON_VERSION_4;
    n
}

fn copy_wide(dst: &mut [u16], s: &str) {
    let w: Vec<u16> = s.encode_utf16().take(dst.len() - 1).collect();
    dst[..w.len()].copy_from_slice(&w);
    dst[w.len()] = 0;
}

pub fn tray_add(hwnd: HWND, tip: &str) {
    let mut n = nid(hwnd);
    n.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP;
    n.uCallbackMessage = WM_TRAY;
    n.hIcon = app_icon(16);
    copy_wide(&mut n.szTip, tip);
    unsafe {
        let _ = Shell_NotifyIconW(NIM_ADD, &n);
        let _ = Shell_NotifyIconW(NIM_SETVERSION, &n);
    }
}

pub fn tray_tip(hwnd: HWND, tip: &str) {
    let mut n = nid(hwnd);
    n.uFlags = NIF_TIP | NIF_SHOWTIP;
    copy_wide(&mut n.szTip, tip);
    unsafe {
        let _ = Shell_NotifyIconW(NIM_MODIFY, &n);
    }
}

pub fn tray_notify(hwnd: HWND, title: &str, text: &str) {
    let mut n = nid(hwnd);
    n.uFlags = NIF_INFO;
    n.dwInfoFlags = NIIF_INFO | NIIF_RESPECT_QUIET_TIME;
    copy_wide(&mut n.szInfoTitle, title);
    copy_wide(&mut n.szInfo, text);
    unsafe {
        let _ = Shell_NotifyIconW(NIM_MODIFY, &n);
    }
}

pub fn tray_remove(hwnd: HWND) {
    let n = nid(hwnd);
    unsafe {
        let _ = Shell_NotifyIconW(NIM_DELETE, &n);
    }
}

fn menu_item(menu: HMENU, id: u32, text: &str, checked: bool, enabled: bool) {
    let mut flags = MF_STRING;
    if checked {
        flags |= MF_CHECKED;
    }
    if !enabled {
        flags |= MF_GRAYED;
    }
    let w = wide(text);
    unsafe {
        let _ = AppendMenuW(menu, flags, id as usize, PCWSTR(w.as_ptr()));
    }
}

fn menu_sep(menu: HMENU) {
    unsafe {
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
    }
}

fn show_tray_menu(hwnd: HWND) {
    let st = MENU.lock().ok().and_then(|m| m.clone()).unwrap_or_default();
    unsafe {
        let Ok(menu) = CreatePopupMenu() else { return };
        menu_item(menu, CMD_SETTINGS, "Settings...", false, true);
        let _ = SetMenuDefaultItem(menu, CMD_SETTINGS, 0);
        menu_item(menu, CMD_FOLD, &format!("Fold now\t{}", st.hotkey), false, true);
        menu_sep(menu);
        let Ok(style) = CreatePopupMenu() else { return };
        menu_item(style, CMD_STYLE_SILK, "Silk", st.style == "silk", true);
        menu_item(style, CMD_STYLE_SHADE, "Shade", st.style == "shade", true);
        menu_item(style, CMD_STYLE_FROST, "Frost", st.style == "frost", true);
        menu_item(style, CMD_STYLE_CUSTOM, "Custom (from settings file)", st.style == "custom", true);
        let w = wide("Style");
        let _ = AppendMenuW(menu, MF_POPUP, style.0 as usize, PCWSTR(w.as_ptr()));
        menu_sep(menu);
        let hinge_label = if st.hinge_available { "Follow the lid (hinge sensor)" } else { "Follow the lid (no hinge sensor found)" };
        menu_item(menu, CMD_FOLLOW_HINGE, hinge_label, st.follow_hinge && st.hinge_available, st.hinge_available);
        let accel_label = if st.accel_available { "Follow the lid (accelerometer)" } else { "Follow the lid (no accelerometer found)" };
        menu_item(menu, CMD_FOLLOW_ACCEL, accel_label, st.follow_accel && st.accel_available && st.accel_calibrated, st.accel_available && st.accel_calibrated);
        menu_item(menu, CMD_ACCEL_CALIBRATE, "Calibrate lid tracking (lid at your normal angle)", false, st.accel_available);
        let cam_label = if st.camera_available { format!("Follow the lid (webcam motion, camera light stays on)\t{}", st.camera_hotkey) } else { "Follow the lid (no webcam found)".to_string() };
        menu_item(menu, CMD_FOLLOW_CAMERA, &cam_label, st.follow_camera && st.camera_available, st.camera_available);
        let Ok(zone) = CreatePopupMenu() else { return };
        for (i, deg) in FLAT_CHOICES.iter().enumerate() {
            let label = if *deg >= 75 { "Whole closing motion".to_string() } else { format!("Last {deg}\u{b0} before shut") };
            menu_item(zone, CMD_FLAT_BASE + i as u32, &label, st.flat_deg == *deg, st.camera_available);
        }
        let w = wide("Lid fold zone (webcam)");
        let _ = AppendMenuW(menu, MF_POPUP, zone.0 as usize, PCWSTR(w.as_ptr()));
        menu_item(menu, CMD_CAMERA_CALIBRATE, "Calibrate lid tracking (webcam)...", false, st.camera_available);
        menu_sep(menu);
        menu_item(menu, CMD_UNFOLD_ON_LID, "Unfold when the lid opens", st.unfold_on_lid_open, true);
        menu_item(menu, CMD_FOLD_ON_SLEEP, "Fold when the lid closes or the PC sleeps", st.fold_on_sleep, true);
        menu_item(menu, CMD_UNFOLD_ON_WAKE, "Unfold on wake / unlock", st.unfold_on_wake, true);
        let Ok(after) = CreatePopupMenu() else { return };
        menu_item(after, CMD_AFTER_NONE, "Stay folded until I press a key", st.after_fold == "none", true);
        menu_item(after, CMD_AFTER_LOCK, "Lock Windows", st.after_fold == "lock", true);
        menu_item(after, CMD_AFTER_SLEEP, "Sleep", st.after_fold == "sleep", true);
        menu_item(after, CMD_AFTER_DISPLAY_OFF, "Turn the screen off", st.after_fold == "display_off", true);
        let w = wide("After the hotkey fold");
        let _ = AppendMenuW(menu, MF_POPUP, after.0 as usize, PCWSTR(w.as_ptr()));
        let Ok(idle) = CreatePopupMenu() else { return };
        menu_item(idle, CMD_IDLE_OFF, "Off", st.idle_min == 0, true);
        menu_item(idle, CMD_IDLE_1, "After 1 minute", st.idle_min == 1, true);
        menu_item(idle, CMD_IDLE_5, "After 5 minutes", st.idle_min == 5, true);
        menu_item(idle, CMD_IDLE_10, "After 10 minutes", st.idle_min == 10, true);
        menu_item(idle, CMD_IDLE_15, "After 15 minutes", st.idle_min == 15, true);
        let w = wide("Fold when idle");
        let _ = AppendMenuW(menu, MF_POPUP, idle.0 as usize, PCWSTR(w.as_ptr()));
        let Ok(winint) = CreatePopupMenu() else { return };
        menu_item(winint, CMD_HANDLE_LID, "WinBend handles the lid: fold, then sleep  (admin prompt)", st.handle_lid && st.lid_do_nothing, true);
        menu_item(winint, CMD_SKIP_SIGNIN, "Skip the sign-in screen after sleep  (admin prompt)", st.skip_signin, true);
        menu_item(winint, CMD_SIGNIN_SETTINGS, "Open Windows sign-in settings...", false, true);
        let w = wide("Windows integration");
        let _ = AppendMenuW(menu, MF_POPUP, winint.0 as usize, PCWSTR(w.as_ptr()));
        menu_item(menu, CMD_STARTUP, "Run at startup", st.run_at_startup, true);
        menu_sep(menu);
        menu_item(menu, CMD_DONATE, "Donate  \u{2665}", false, true);
        menu_item(menu, CMD_GITHUB, "WinBend on GitHub", false, true);
        menu_sep(menu);
        menu_item(menu, CMD_OPEN_SETTINGS, "Open settings file", false, true);
        menu_item(menu, CMD_RELOAD_SETTINGS, "Reload settings", false, true);
        menu_item(menu, CMD_ABOUT, "About WinBend", false, true);
        menu_sep(menu);
        menu_item(menu, CMD_QUIT, "Quit", false, true);

        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let _ = SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenuEx(menu, (TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY).0, pt.x, pt.y, hwnd, None);
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
        if cmd.0 > 0 {
            push(Event::Menu(cmd.0 as u32));
        }
    }
}

// --------------------------------------------------------------------------------------
// Hotkey / power / misc helpers
// --------------------------------------------------------------------------------------
/// Release both global hotkeys (used while the settings page captures a new combination).
pub fn unregister_hotkey(hwnd: HWND) {
    unsafe {
        let _ = UnregisterHotKey(Some(hwnd), HOTKEY_ID);
        let _ = UnregisterHotKey(Some(hwnd), HOTKEY_ID_CAMERA);
    }
}

/// Put UTF-16 text on the clipboard (used for copying donation addresses from the settings page).
pub fn set_clipboard_text(hwnd: HWND, text: &str) -> bool {
    use windows::Win32::System::DataExchange::*;
    use windows::Win32::System::Memory::*;
    let w = wide(text);
    unsafe {
        if OpenClipboard(Some(hwnd)).is_err() {
            return false;
        }
        let ok = (|| {
            EmptyClipboard().ok()?;
            let bytes = w.len() * 2;
            let h = GlobalAlloc(GMEM_MOVEABLE, bytes).ok()?;
            let p = GlobalLock(h) as *mut u16;
            if p.is_null() {
                return None;
            }
            std::ptr::copy_nonoverlapping(w.as_ptr(), p, w.len());
            let _ = GlobalUnlock(h);
            SetClipboardData(13 /* CF_UNICODETEXT */, Some(HANDLE(h.0))).ok()
        })()
        .is_some();
        let _ = CloseClipboard();
        ok
    }
}

pub fn register_hotkey_id(hwnd: HWND, id: i32, mods: u32, vk: u32) -> bool {
    unsafe {
        let _ = UnregisterHotKey(Some(hwnd), id);
        RegisterHotKey(Some(hwnd), id, HOT_KEY_MODIFIERS(mods), vk).is_ok()
    }
}

pub fn register_hotkey(hwnd: HWND, mods: u32, vk: u32) -> bool {
    register_hotkey_id(hwnd, HOTKEY_ID, mods, vk)
}

pub fn register_lid_notifications(hwnd: HWND) {
    unsafe {
        let _ = RegisterPowerSettingNotification(HANDLE(hwnd.0), &GUID_LIDSWITCH_STATE_CHANGE, DEVICE_NOTIFY_WINDOW_HANDLE);
    }
}

pub fn register_session_notifications(hwnd: HWND) {
    unsafe {
        let _ = windows::Win32::System::RemoteDesktop::WTSRegisterSessionNotification(hwnd, windows::Win32::System::RemoteDesktop::NOTIFY_FOR_THIS_SESSION);
        SetTimer(Some(hwnd), IDLE_TIMER_ID, 5000, None);
    }
}

/// Seconds since the last keyboard or mouse input, session-wide.
pub fn idle_seconds() -> f64 {
    unsafe {
        let mut lii = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
        if GetLastInputInfo(&mut lii).as_bool() {
            let now = windows::Win32::System::SystemInformation::GetTickCount();
            return now.wrapping_sub(lii.dwTime) as f64 / 1000.0;
        }
    }
    0.0
}

pub fn sleep_pc() {
    // SetSuspendState can block until resume on some machines; keep it off the UI thread.
    std::thread::spawn(|| unsafe {
        let _ = SetSuspendState(false, false, false);
    });
}

pub fn display_off(hwnd: HWND) {
    unsafe {
        let _ = SendMessageW(hwnd, WM_SYSCOMMAND, Some(WPARAM(SC_MONITORPOWER as usize)), Some(LPARAM(2)));
    }
}

pub fn foreground_window() -> HWND {
    unsafe { GetForegroundWindow() }
}

pub fn focus_window(hwnd: HWND) {
    if !hwnd.is_invalid() {
        unsafe {
            let _ = SetForegroundWindow(hwnd);
        }
    }
}

pub fn lock_workstation() {
    unsafe {
        let _ = windows::Win32::System::Shutdown::LockWorkStation();
    }
}

pub fn message_box(hwnd: HWND, title: &str, text: &str, flags: MESSAGEBOX_STYLE) -> MESSAGEBOX_RESULT {
    let t = wide(title);
    let x = wide(text);
    unsafe { MessageBoxW(Some(hwnd), PCWSTR(x.as_ptr()), PCWSTR(t.as_ptr()), flags | MB_SETFOREGROUND | MB_TOPMOST) }
}

pub fn open_url(url: &str) {
    let u = wide(url);
    unsafe {
        let _ = ShellExecuteW(None, w!("open"), PCWSTR(u.as_ptr()), None, None, SW_SHOWNORMAL);
    }
}

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const RUN_VALUE: PCWSTR = w!("WinBend");

pub fn set_run_at_startup(enable: bool) -> bool {
    unsafe {
        let mut hkey = HKEY::default();
        if RegCreateKeyExW(HKEY_CURRENT_USER, RUN_KEY, None, None, REG_OPTION_NON_VOLATILE, KEY_SET_VALUE, None, &mut hkey, None) != ERROR_SUCCESS {
            return false;
        }
        let ok = if enable {
            let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default();
            let val = wide(&format!("\"{exe}\""));
            let bytes = std::slice::from_raw_parts(val.as_ptr() as *const u8, val.len() * 2);
            RegSetValueExW(hkey, RUN_VALUE, None, REG_SZ, Some(bytes)) == ERROR_SUCCESS
        } else {
            let r = RegDeleteValueW(hkey, RUN_VALUE);
            r == ERROR_SUCCESS || r == ERROR_FILE_NOT_FOUND
        };
        let _ = RegCloseKey(hkey);
        ok
    }
}

pub fn single_instance() -> bool {
    unsafe {
        match windows::Win32::System::Threading::CreateMutexW(None, false, w!("Local\\WinBend.SingleInstance")) {
            Ok(_) => GetLastError() != ERROR_ALREADY_EXISTS,
            Err(_) => true,
        }
    }
}

// --------------------------------------------------------------------------------------
// Window procedure
// --------------------------------------------------------------------------------------
unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let is_overlay = GetWindowLongPtrW(hwnd, GWLP_USERDATA) == USERDATA_OVERLAY;
    match msg {
        WM_HOTKEY => {
            match wparam.0 as i32 {
                HOTKEY_ID => push(Event::Hotkey),
                HOTKEY_ID_CAMERA => push(Event::CameraHotkey),
                _ => {}
            }
            LRESULT(0)
        }
        WM_TRAY => {
            match (lparam.0 & 0xffff) as u32 {
                WM_RBUTTONUP | WM_CONTEXTMENU => show_tray_menu(hwnd),
                WM_LBUTTONUP | NIN_SELECT | 0x401 => push(Event::Hotkey),
                _ => {}
            }
            LRESULT(0)
        }
        WM_HINGE => {
            push(Event::Hinge(wparam.0 as u32 as f32 / 100.0));
            LRESULT(0)
        }
        WM_ACCEL => {
            push(Event::Accel(crate::sensor::unpack_accel(wparam.0)));
            LRESULT(0)
        }
        WM_CAMERA => {
            let tilt_deg = (wparam.0 as isize - crate::camera::OFFSET as isize) as f32 / 100.0;
            push(Event::Camera { tilt_deg, dark: lparam.0 & 1 == 1 });
            LRESULT(0)
        }
        WM_CAMERA_STATUS => {
            push(Event::CameraStatus(wparam.0 == 1));
            LRESULT(0)
        }
        WM_WTSSESSION_CHANGE => {
            match wparam.0 as u32 {
                WTS_SESSION_LOCK => push(Event::Lock),
                WTS_SESSION_UNLOCK => push(Event::Unlock),
                _ => {}
            }
            LRESULT(0)
        }
        WM_TIMER if wparam.0 == IDLE_TIMER_ID => {
            push(Event::Tick);
            LRESULT(0)
        }
        WM_MOUSEMOVE if is_overlay => {
            push(Event::MouseMove);
            LRESULT(0)
        }
        WM_POWERBROADCAST => {
            if wparam.0 as u32 == PBT_APMRESUMEAUTOMATIC || wparam.0 as u32 == PBT_APMRESUMESUSPEND {
                push(Event::Wake);
            }
            if wparam.0 as u32 == PBT_APMSUSPEND {
                push(Event::Suspend);
            }
            if wparam.0 as u32 == PBT_POWERSETTINGCHANGE && lparam.0 != 0 {
                let s = &*(lparam.0 as *const POWERBROADCAST_SETTING);
                if s.PowerSetting == GUID_LIDSWITCH_STATE_CHANGE && s.DataLength >= 1 {
                    push(if s.Data[0] == 0 { Event::LidClosed } else { Event::LidOpen });
                }
            }
            LRESULT(1)
        }
        WM_KEYDOWN | WM_SYSKEYDOWN | WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN => {
            if is_overlay {
                push(Event::Dismiss);
                return LRESULT(0);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_MOUSEWHEEL => {
            if is_overlay {
                let delta = ((wparam.0 >> 16) & 0xffff) as u16 as i16 as i32;
                push(Event::Wheel(delta));
                return LRESULT(0);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_SETCURSOR if is_overlay => {
            if HIDE_CURSOR.load(std::sync::atomic::Ordering::Relaxed) {
                SetCursor(None);
                LRESULT(1)
            } else {
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
        }
        WM_MOUSEACTIVATE if is_overlay => LRESULT(MA_NOACTIVATE as isize),
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            let _ = ValidateRect(Some(hwnd), None);
            LRESULT(0)
        }
        WM_DISPLAYCHANGE | WM_DPICHANGED => {
            push(Event::DisplayChange);
            LRESULT(0)
        }
        WM_CLOSE => {
            if is_overlay {
                push(Event::Dismiss);
            } else {
                push(Event::Quit);
            }
            LRESULT(0)
        }
        WM_QUERYENDSESSION => LRESULT(1),
        WM_ENDSESSION => {
            push(Event::Quit);
            LRESULT(0)
        }
        WM_DESTROY => LRESULT(0),
        _ => {
            if msg == *TASKBAR_CREATED_MSG.lock().unwrap() && msg != 0 {
                push(Event::TaskbarCreated);
                return LRESULT(0);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
    }
}
