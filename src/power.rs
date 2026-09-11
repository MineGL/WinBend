//! Windows power-scheme integration: read and (with one admin prompt) write the two
//! settings WinBend needs to own the sleep choreography.
//!
//! * Lid close action (`GUID_LIDCLOSE_ACTION`): 0 = do nothing, 1 = sleep, 2 = hibernate,
//!   3 = shut down. With "do nothing", WinBend sees the lid close, plays the fold and puts
//!   the PC to sleep itself, so the animation runs before the screen goes dark.
//! * Require sign-in on wake (`GUID_LOCK_CONSOLE_ON_WAKE`, powercfg alias CONSOLELOCK):
//!   1 = ask for the password after sleep, 0 = go straight to the desktop. This is the
//!   setting behind "when should Windows require you to sign in again", which Windows 11
//!   hides from Settings when Windows Hello-only sign-in is enabled.
//!
//! Reads work as a normal user. Writes need administrator rights, so `write_elevated`
//! relaunches winbend.exe with `--set-power` through the UAC prompt and waits for it.
use windows::core::GUID;
use windows::Win32::Foundation::*;
use windows::Win32::System::Power::*;
use windows::Win32::System::SystemServices::{GUID_LIDCLOSE_ACTION, GUID_LOCK_CONSOLE_ON_WAKE, GUID_SYSTEM_BUTTON_SUBGROUP, NO_SUBGROUP_GUID};
use windows::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject};
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    LidAction,
    LockOnWake,
}

impl Setting {
    fn guids(self) -> (GUID, GUID) {
        match self {
            Setting::LidAction => (GUID_SYSTEM_BUTTON_SUBGROUP, GUID_LIDCLOSE_ACTION),
            Setting::LockOnWake => (NO_SUBGROUP_GUID, GUID_LOCK_CONSOLE_ON_WAKE),
        }
    }

    pub fn from_arg(s: &str) -> Option<Setting> {
        match s {
            "lid" => Some(Setting::LidAction),
            "lock" => Some(Setting::LockOnWake),
            _ => None,
        }
    }

    pub fn arg(self) -> &'static str {
        match self {
            Setting::LidAction => "lid",
            Setting::LockOnWake => "lock",
        }
    }
}

fn active_scheme() -> Option<GUID> {
    unsafe {
        let mut p: *mut GUID = std::ptr::null_mut();
        if PowerGetActiveScheme(None, &mut p) != ERROR_SUCCESS || p.is_null() {
            return None;
        }
        let g = *p;
        let _ = LocalFree(Some(HLOCAL(p as *mut _)));
        Some(g)
    }
}

/// Current (AC, DC) value indices for the active scheme.
pub fn read(s: Setting) -> Option<(u32, u32)> {
    let scheme = active_scheme()?;
    let (sub, set) = s.guids();
    unsafe {
        let mut ac = 0u32;
        let mut dc = 0u32;
        if PowerReadACValueIndex(None, Some(&scheme), Some(&sub), Some(&set), &mut ac) != ERROR_SUCCESS {
            return None;
        }
        if PowerReadDCValueIndex(None, Some(&scheme), Some(&sub), Some(&set), &mut dc) != ERROR_SUCCESS.0 {
            return None;
        }
        Some((ac, dc))
    }
}

/// Write both AC and DC values and re-apply the scheme. Needs an elevated process.
pub fn write_here(s: Setting, value: u32) -> bool {
    let Some(scheme) = active_scheme() else { return false };
    let (sub, set) = s.guids();
    unsafe {
        PowerWriteACValueIndex(None, &scheme, Some(&sub), Some(&set), value) == ERROR_SUCCESS
            && PowerWriteDCValueIndex(None, &scheme, Some(&sub), Some(&set), value) == ERROR_SUCCESS.0
            && PowerSetActiveScheme(None, Some(&scheme)) == ERROR_SUCCESS
    }
}

/// Relaunch ourselves elevated to apply the change; returns true when the value now reads back.
pub fn write_elevated(s: Setting, value: u32) -> bool {
    if read(s) == Some((value, value)) {
        return true;
    }
    let Ok(exe) = std::env::current_exe() else { return false };
    let file = crate::win::wide(&exe.display().to_string());
    let params = crate::win::wide(&format!("--set-power {} {}", s.arg(), value));
    let verb = crate::win::wide("runas");
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC,
        lpVerb: windows::core::PCWSTR(verb.as_ptr()),
        lpFile: windows::core::PCWSTR(file.as_ptr()),
        lpParameters: windows::core::PCWSTR(params.as_ptr()),
        nShow: SW_HIDE.0,
        ..Default::default()
    };
    unsafe {
        if ShellExecuteExW(&mut info).is_err() || info.hProcess.is_invalid() {
            return false; // UAC declined or launch failed
        }
        let _ = WaitForSingleObject(info.hProcess, 60_000);
        let mut code = 1u32;
        let _ = GetExitCodeProcess(info.hProcess, &mut code);
        let _ = CloseHandle(info.hProcess);
        code == 0 && read(s) == Some((value, value))
    }
}
