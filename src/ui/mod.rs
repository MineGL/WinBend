//! Main-STA WebView2 host. Callbacks only enqueue app events.
pub mod preview;
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};
use webview2_com::{
    take_pwstr, CreateCoreWebView2ControllerCompletedHandler,
    CreateCoreWebView2EnvironmentCompletedHandler, Microsoft::Web::WebView2::Win32::*,
    NavigationStartingEventHandler, NewWindowRequestedEventHandler, ProcessFailedEventHandler,
    WebMessageReceivedEventHandler,
};
use windows::{
    core::*,
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::LibraryLoader::GetModuleHandleW,
        UI::{HiDpi::*, WindowsAndMessaging::*},
    },
};

#[derive(Debug, Clone, PartialEq)]
pub enum UiEvent {
    Message(String),
    Closed,
    Failed(String),
    Tick,
}
fn emit(event: UiEvent) {
    crate::win::push_event(crate::win::Event::Ui(event));
}

const TIMER: usize = 8;
thread_local! { static ACTIVE: RefCell<Weak<State>> = RefCell::new(Weak::new()); }
struct State {
    hwnd: HWND,
    closed: Cell<bool>,
    ready: Cell<bool>,
    ticking: Cell<bool>,
    controller: RefCell<Option<ICoreWebView2Controller>>,
    webview: RefCell<Option<ICoreWebView2>>,
    tokens: RefCell<Vec<(u8, i64)>>,
    pending: RefCell<Vec<String>>,
}
pub struct SettingsWindow {
    state: Rc<State>,
}

pub fn runtime_version() -> Option<String> {
    unsafe {
        let mut value = PWSTR::null();
        GetAvailableCoreWebView2BrowserVersionString(PCWSTR::null(), &mut value).ok()?;
        Some(take_pwstr(value))
    }
}

impl SettingsWindow {
    pub fn new() -> Result<Self> {
        unsafe {
            let instance = HINSTANCE(GetModuleHandleW(None)?.0);
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(wndproc),
                hInstance: instance,
                lpszClassName: w!("WinBendSettings"),
                hCursor: LoadCursorW(None, IDC_ARROW)?,
                hIcon: crate::win::app_icon(32),
                ..Default::default()
            });
            let mut point = POINT::default();
            let _ = GetCursorPos(&mut point);
            let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
            let mut info = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            GetMonitorInfoW(monitor, &mut info).ok()?;
            let mut dpi_x = 96;
            let mut dpi_y = 96;
            let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y);
            let work = info.rcWork;
            let width = (900 * dpi_x as i32 / 96).min(work.right - work.left);
            let height = (680 * dpi_y as i32 / 96).min(work.bottom - work.top);
            let hwnd = CreateWindowExW(
                WS_EX_APPWINDOW,
                w!("WinBendSettings"),
                w!("WinBend Settings"),
                WS_OVERLAPPEDWINDOW,
                work.left + (work.right - work.left - width) / 2,
                work.top + (work.bottom - work.top - height) / 2,
                width,
                height,
                None,
                None,
                Some(instance),
                None,
            )?;
            let state = Rc::new(State {
                hwnd,
                closed: Cell::new(false),
                ready: Cell::new(false),
                ticking: Cell::new(false),
                controller: RefCell::new(None),
                webview: RefCell::new(None),
                tokens: RefCell::new(Vec::new()),
                pending: RefCell::new(Vec::new()),
            });
            ACTIVE.with(|slot| *slot.borrow_mut() = Rc::downgrade(&state));
            let window = Self { state };
            let folder = std::env::var_os("LOCALAPPDATA")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(std::env::temp_dir)
                .join("WinBend")
                .join("WebView2");
            let folder = HSTRING::from(folder.to_string_lossy().as_ref());
            let weak = Rc::downgrade(&window.state);
            let handler = CreateCoreWebView2EnvironmentCompletedHandler::create(Box::new(
                move |result, environment| {
                    let Some(state) = live(&weak) else {
                        return Ok(());
                    };
                    let result = result
                        .and_then(|_| environment.ok_or_else(|| Error::from(E_POINTER)))
                        .and_then(|environment| {
                            let weak = Rc::downgrade(&state);
                            let handler = CreateCoreWebView2ControllerCompletedHandler::create(
                                Box::new(move |result, controller| {
                                    let Some(state) = live(&weak) else {
                                        if let Some(controller) = controller {
                                            let _ = controller.Close();
                                        }
                                        return Ok(());
                                    };
                                    let result = result
                                        .and_then(|_| {
                                            controller.ok_or_else(|| Error::from(E_POINTER))
                                        })
                                        .and_then(|controller| initialize(&state, controller));
                                    if let Err(error) = result {
                                        emit(UiEvent::Failed(error.to_string()));
                                    }
                                    Ok(())
                                }),
                            );
                            environment.CreateCoreWebView2Controller(state.hwnd, &handler)
                        });
                    if let Err(error) = result {
                        emit(UiEvent::Failed(error.to_string()));
                    }
                    Ok(())
                },
            ));
            CreateCoreWebView2EnvironmentWithOptions(PCWSTR::null(), &folder, None, &handler)?;
            Ok(window)
        }
    }

    pub fn show(&self) {
        if self.state.closed.get() || self.state.controller.borrow().is_none() {
            return;
        }
        unsafe {
            let _ = ShowWindow(
                self.state.hwnd,
                if IsIconic(self.state.hwnd).as_bool() {
                    SW_RESTORE
                } else {
                    SW_SHOW
                },
            );
            let _ = SetForegroundWindow(self.state.hwnd);
        }
    }

    /// Called by the app after receiving the page's ready message.
    pub fn ready(&self) {
        self.state.ready.set(true);
        let pending = self.state.pending.take();
        for json in pending {
            post(&self.state, &json);
        }
    }

    pub fn send(&self, value: &Value) {
        if self.state.closed.get() {
            return;
        }
        let json = value.to_string();
        if self.state.ready.get() {
            post(&self.state, &json);
        } else {
            self.state.pending.borrow_mut().push(json);
        }
    }

}

fn live(weak: &Weak<State>) -> Option<Rc<State>> {
    weak.upgrade().filter(|state| !state.closed.get())
}
fn post(state: &State, json: &str) {
    if state.closed.get() {
        return;
    }
    let webview = state.webview.borrow().clone();
    if let Some(webview) = webview {
        unsafe {
            if let Err(error) = webview.PostWebMessageAsJson(&HSTRING::from(json)) {
                emit(UiEvent::Failed(error.to_string()));
            }
        }
    }
}

unsafe fn initialize(state: &Rc<State>, controller: ICoreWebView2Controller) -> Result<()> {
    // Retain immediately, so partial initialization is also cleaned up by Drop.
    *state.controller.borrow_mut() = Some(controller.clone());
    let webview = controller.CoreWebView2()?;
    *state.webview.borrow_mut() = Some(webview.clone());
    let settings = webview.Settings()?;
    settings.SetAreDefaultContextMenusEnabled(cfg!(debug_assertions))?;
    settings.SetAreDevToolsEnabled(cfg!(debug_assertions))?;
    settings.SetIsZoomControlEnabled(false)?;
    settings.SetIsStatusBarEnabled(false)?;
    settings.SetAreDefaultScriptDialogsEnabled(false)?;
    settings.SetIsBuiltInErrorPageEnabled(false)?;
    if let Ok(settings) = settings.cast::<ICoreWebView2Settings3>() {
        settings.SetAreBrowserAcceleratorKeysEnabled(false)?;
    }
    if let Ok(settings) = settings.cast::<ICoreWebView2Settings4>() {
        settings.SetIsGeneralAutofillEnabled(false)?;
        settings.SetIsPasswordAutosaveEnabled(false)?;
    }
    if let Ok(controller) = controller.cast::<ICoreWebView2Controller2>() {
        controller.SetDefaultBackgroundColor(COREWEBVIEW2_COLOR {
            A: 255,
            R: 246,
            G: 247,
            B: 249,
        })?;
    }
    let weak = Rc::downgrade(state);
    let mut token = 0;
    webview.add_WebMessageReceived(
        &WebMessageReceivedEventHandler::create(Box::new(move |_, args| {
            if live(&weak).is_none() {
                return Ok(());
            }
            if let Some(args) = args {
                let mut value = PWSTR::null();
                if args.TryGetWebMessageAsString(&mut value).is_ok() {
                    emit(UiEvent::Message(take_pwstr(value)));
                }
            }
            Ok(())
        })),
        &mut token,
    )?;
    state.tokens.borrow_mut().push((0, token));
    let mut initial_navigation = true;
    webview.add_NavigationStarting(
        &NavigationStartingEventHandler::create(Box::new(move |_, args| {
            if let Some(args) = args {
                let mut uri = PWSTR::null();
                args.Uri(&mut uri)?;
                // Current runtimes expose NavigateToString as a data URI during
                // NavigationStarting, although the document origin is about:blank.
                // Only the first packaged-page navigation may use that URI.
                let uri = take_pwstr(uri);
                let allowed = initial_navigation
                    && (uri == "about:blank" || uri.starts_with("data:text/html"));
                initial_navigation = false;
                args.SetCancel(!allowed)?;
            }
            Ok(())
        })),
        &mut token,
    )?;
    state.tokens.borrow_mut().push((1, token));
    webview.add_NewWindowRequested(
        &NewWindowRequestedEventHandler::create(Box::new(move |_, args| {
            if let Some(args) = args {
                args.SetHandled(true)?;
            }
            Ok(())
        })),
        &mut token,
    )?;
    state.tokens.borrow_mut().push((2, token));
    let weak = Rc::downgrade(state);
    webview.add_ProcessFailed(
        &ProcessFailedEventHandler::create(Box::new(move |_, _| {
            if live(&weak).is_some() {
                emit(UiEvent::Failed(
                    "The settings browser process stopped. Please reopen Settings.".into(),
                ));
            }
            Ok(())
        })),
        &mut token,
    )?;
    state.tokens.borrow_mut().push((3, token));
    let mut bounds = RECT::default();
    GetClientRect(state.hwnd, &mut bounds)?;
    controller.SetBounds(bounds)?;
    controller.SetIsVisible(true)?;
    webview.NavigateToString(&HSTRING::from(include_str!("index.html")))?;
    let _ = ShowWindow(state.hwnd, SW_SHOW);
    let _ = SetForegroundWindow(state.hwnd);
    let _ = controller.MoveFocus(COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC);
    SetTimer(Some(state.hwnd), TIMER, 100, None);
    state.ticking.set(true);
    Ok(())
}

impl Drop for SettingsWindow {
    fn drop(&mut self) {
        self.state.closed.set(true);
        let webview = self.state.webview.take();
        let tokens = self.state.tokens.take();
        let controller = self.state.controller.take();
        unsafe {
            let _ = KillTimer(Some(self.state.hwnd), TIMER);
            if let Some(webview) = webview {
                for (kind, token) in tokens {
                    let _ = match kind {
                        0 => webview.remove_WebMessageReceived(token),
                        1 => webview.remove_NavigationStarting(token),
                        2 => webview.remove_NewWindowRequested(token),
                        _ => webview.remove_ProcessFailed(token),
                    };
                }
            }
            if let Some(controller) = controller {
                let _ = controller.Close();
            }
            if IsWindow(Some(self.state.hwnd)).as_bool() {
                let _ = DestroyWindow(self.state.hwnd);
            }
        }
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    // Clone the controller before invoking COM: it can re-enter the window procedure.
    let state = ACTIVE
        .with(|slot| slot.borrow().upgrade())
        .filter(|state| state.hwnd == hwnd);
    let controller = state
        .as_ref()
        .and_then(|state| state.controller.borrow().clone());
    match msg {
        WM_CLOSE => {
            if let Some(state) = state {
                if !state.closed.replace(true) {
                    emit(UiEvent::Closed);
                }
            }
            let _ = ShowWindow(hwnd, SW_HIDE);
            return LRESULT(0);
        }
        WM_TIMER if wp.0 == TIMER => {
            if state
                .as_ref()
                .is_some_and(|state| !state.closed.get() && state.ticking.get())
            {
                emit(UiEvent::Tick);
            }
            return LRESULT(0);
        }
        WM_SIZE => {
            if let Some(controller) = controller {
                let _ = controller.SetIsVisible(!IsIconic(hwnd).as_bool());
                let mut rect = RECT::default();
                if GetClientRect(hwnd, &mut rect).is_ok() {
                    let _ = controller.SetBounds(rect);
                }
            }
        }
        WM_MOVE => {
            if let Some(controller) = controller {
                let _ = controller.NotifyParentWindowPositionChanged();
            }
        }
        WM_SETFOCUS => {
            if let Some(controller) = controller {
                let _ = controller.MoveFocus(COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC);
            }
        }
        WM_GETMINMAXINFO => {
            let dpi = GetDpiForWindow(hwnd).max(96) as i32;
            let info = &mut *(lp.0 as *mut MINMAXINFO);
            info.ptMinTrackSize = POINT {
                x: 720 * dpi / 96,
                y: 520 * dpi / 96,
            };
            return LRESULT(0);
        }
        WM_DPICHANGED => {
            let rect = &*(lp.0 as *const RECT);
            let _ = SetWindowPos(
                hwnd,
                None,
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            return LRESULT(0);
        }
        _ => {}
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}
