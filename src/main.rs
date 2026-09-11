//! WinBend: your desktop folds like a lid closing.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod camera;
mod config;
mod gfx;
mod links;
mod power;
mod sensor;
mod win;
mod ui;

use windows::Win32::Foundation::HWND;
use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_SINGLETHREADED};
use windows::Win32::UI::WindowsAndMessaging::*;

use app::App;
use config::{Config, StyleParams};

fn arg_value(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // The release build has no console of its own; for the command-line modes borrow the parent's.
    if args.iter().any(|a| a.starts_with("--")) {
        unsafe {
            let _ = windows::Win32::System::Console::AttachConsole(windows::Win32::System::Console::ATTACH_PARENT_PROCESS);
        }
    }
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("winbend {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    // Elevated helper: `winbend --set-power lid|lock VALUE`, launched by power::write_elevated
    // through the UAC prompt after the user picks the matching tray item.
    if let Some(i) = args.iter().position(|a| a == "--set-power") {
        let s = args.get(i + 1).and_then(|s| power::Setting::from_arg(s));
        let v = args.get(i + 2).and_then(|s| s.parse::<u32>().ok());
        let ok = matches!((s, v), (Some(s), Some(v)) if v <= 3 && power::write_here(s, v));
        std::process::exit(if ok { 0 } else { 1 });
    }
    unsafe {
        if let Err(e) = RoInitialize(RO_INIT_SINGLETHREADED) {
            eprintln!("Could not initialize the Windows runtime: {e}");
            return;
        }
    }

    // Offline render check: capture one frame, fold it, write a PNG, exit.
    if let Some(out) = arg_value(&args, "--render-test") {
        let t: f32 = arg_value(&args, "--t").and_then(|s| s.parse().ok()).unwrap_or(0.5);
        let cfg = Config::load();
        let style = arg_value(&args, "--style").and_then(|s| StyleParams::preset(&s)).unwrap_or_else(|| cfg.style_params());
        let gpu = gfx::device::Gpu::new().expect("D3D11 device");
        let mut renderer = gfx::renderer::Renderer::new(&gpu).expect("shaders");
        match App::render_test(&gpu, &mut renderer, t, &style, cfg.max_tilt_deg, cfg.background_rgba(), std::path::Path::new(&out)) {
            Ok(()) => println!("wrote {out}"),
            Err(e) => {
                eprintln!("render test failed: {e}");
                std::process::exit(1);
            }
        }
        return;
    }

    // Support diagnostics: which triggers this machine can use.
    if args.iter().any(|a| a == "--diag") {
        win::register_class().expect("window class");
        let hwnd = win::create_message_window().expect("message window");
        let geom = win::primary_monitor();
        println!("winbend {}", env!("CARGO_PKG_VERSION"));
        println!("primary monitor: {}x{} at ({}, {})", geom.width(), geom.height(), geom.rect.left, geom.rect.top);
        println!("hinge angle sensor: {}", if sensor::start(hwnd) { "yes" } else { "no" });
        println!("accelerometer: {}", if sensor::start_accelerometer(hwnd) { "yes" } else { "no" });
        println!("WebView2: {}", ui::runtime_version().unwrap_or_else(|| "not installed".into()));
        println!("webcam: {}", if camera::available() { "yes" } else { "no" });
        let show = |v: Option<(u32, u32)>| v.map(|(ac, dc)| format!("AC={ac} DC={dc}")).unwrap_or_else(|| "unreadable".into());
        println!("lid close action (0=do nothing, 1=sleep): {}", show(power::read(power::Setting::LidAction)));
        println!("require sign-in on wake (1=yes, 0=never): {}", show(power::read(power::Setting::LockOnWake)));
        println!("idle for: {:.1} s", win::idle_seconds());
        println!("settings: {}", Config::path().display());
        let cfg = Config::load();
        println!("hotkey: {}   after_fold: {}   fold_when_idle_min: {}   unfold_on_wake: {}", cfg.hotkey, cfg.after_fold, cfg.fold_when_idle_min, cfg.unfold_on_wake);
        return;
    }

    // Webcam tracker check: print the estimated lid tilt for N seconds, then exit.
    if let Some(secs) = arg_value(&args, "--camera-test") {
        let secs: u64 = secs.parse().unwrap_or(8);
        win::register_class().expect("window class");
        let hwnd = win::create_message_window().expect("message window");
        let cfg = Config::load();
        camera::start(hwnd, cfg.camera_vfov_deg, cfg.camera_travel_deg);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(secs);
        let mut msg = MSG::default();
        let mut last = String::new();
        while std::time::Instant::now() < deadline {
            unsafe {
                while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                    DispatchMessageW(&msg);
                }
            }
            while let Some(ev) = win::EVENTS.lock().ok().and_then(|mut q| q.pop_front()) {
                let line = match ev {
                    win::Event::CameraStatus(ok) => format!("camera: {}", if ok { "running" } else { "not available" }),
                    win::Event::Camera { tilt_deg, dark } => format!("tilt {tilt_deg:6.2} deg  dark={dark}"),
                    _ => continue,
                };
                if line != last {
                    println!("{line}");
                    last = line;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        camera::stop();
        return;
    }

    let ui_test = arg_value(&args, "--ui-test").map(|s| s.parse::<u64>().unwrap_or(5).clamp(1, 120));
    let demo = args.iter().any(|a| a == "--demo");

    if !demo && ui_test.is_none() && !win::single_instance() {
        win::message_box(HWND::default(), "WinBend", "WinBend is already running. Look for it in the tray.", MB_OK | MB_ICONINFORMATION);
        return;
    }

    let mut cfg = Config::load();
    if ui_test.is_some() {
        cfg = Config::default();
        cfg.follow_hinge = false;
        cfg.first_run_done = true;
    }

    win::register_class().expect("window class");
    let msg_hwnd = win::create_message_window().expect("message window");
    win::tray_add(msg_hwnd, "WinBend");
    win::register_lid_notifications(msg_hwnd);
    win::register_session_notifications(msg_hwnd);
    let hinge_available = sensor::start(msg_hwnd);
    let accel_available = sensor::start_accelerometer(msg_hwnd);
    let camera_available = camera::available();

    let mut app = match App::new(cfg, msg_hwnd, hinge_available, accel_available, camera_available, demo) {
        Ok(a) => a,
        Err(e) => {
            win::message_box(HWND::default(), "WinBend", &format!("Could not start Direct3D 11: {e}"), MB_OK | MB_ICONERROR);
            win::tray_remove(msg_hwnd);
            return;
        }
    };
    if ui_test.is_none() { app.apply_hotkey(); }
    app.ui_smoke = ui_test.is_some();
    let ui_deadline = ui_test.map(|n| std::time::Instant::now() + std::time::Duration::from_secs(n));
    if ui_test.is_some() {
        println!("ui: runtime {}", ui::runtime_version().unwrap_or_else(|| "missing".into()));
        app.open_settings("fold");
    } else if !app.cfg.first_run_done && !demo {
        app.open_settings("welcome");
    }

    if demo {
        app.toggle_fold();
    }

    let mut msg = MSG::default();
    loop {
        unsafe {
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_QUIT {
                    app.want_quit = true;
                    break;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        loop {
            let ev = win::EVENTS.lock().ok().and_then(|mut q| q.pop_front());
            match ev {
                Some(ev) => app.handle(ev),
                None => break,
            }
        }
        if ui_deadline.is_some_and(|d| std::time::Instant::now() >= d) { break; }
        if app.want_quit {
            break;
        }
        if app.active() {
            app.tick(); // paced by vsync in Present
        } else if ui_test.is_some() {
            std::thread::sleep(std::time::Duration::from_millis(10));
        } else {
            unsafe {
                let _ = WaitMessage();
            }
        }
    }
    let smoke_result = if app.ui_ready && app.ui_preview_ok { 0 } else if ui::runtime_version().is_none() { 3 } else { 2 };
    drop(app);
    camera::stop();
    win::tray_remove(msg_hwnd);
    if ui_test.is_some() { std::process::exit(smoke_result); }
}

