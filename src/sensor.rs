//! Hinge angle sensor (Windows.Devices.Sensors.HingeAngleSensor). Present on some
//! 2-in-1s and Surface devices; most clamshell laptops do not have one.
//! Readings are posted to the message window as WM_HINGE (angle * 100 in wparam).
use std::sync::Mutex;

use windows::core::Result;
use windows::Devices::Sensors::{HingeAngleSensor, HingeAngleSensorReadingChangedEventArgs};
use windows::Foundation::TypedEventHandler;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

use crate::win::WM_HINGE;

static SENSOR: Mutex<Option<HingeAngleSensor>> = Mutex::new(None);

fn post(hwnd: isize, degrees: f64) {
    let centi = (degrees.clamp(0.0, 360.0) * 100.0) as usize;
    unsafe {
        let _ = PostMessageW(Some(HWND(hwnd as *mut _)), WM_HINGE, WPARAM(centi), LPARAM(0));
    }
}

/// Returns true when a hinge sensor exists and reporting has started.
pub fn start(hwnd: HWND) -> bool {
    let hwnd_raw = hwnd.0 as isize;
    let r: Result<()> = (|| {
        let op = HingeAngleSensor::GetDefaultAsync()?;
        let sensor = op.join()?; // Err when the device has no sensor (null result)
        let _ = sensor.SetReportThresholdInDegrees(0.5);
        let handler = TypedEventHandler::<HingeAngleSensor, HingeAngleSensorReadingChangedEventArgs>::new(
            move |_sender, args| {
                if let Some(a) = args.as_ref() {
                    if let Ok(reading) = a.Reading() {
                        if let Ok(deg) = reading.AngleInDegrees() {
                            post(hwnd_raw, deg);
                        }
                    }
                }
                Ok(())
            },
        );
        sensor.ReadingChanged(&handler)?;
        if let Ok(reading) = sensor.GetCurrentReadingAsync().and_then(|op| op.join()) {
            if let Ok(deg) = reading.AngleInDegrees() {
                post(hwnd_raw, deg);
            }
        }
        *SENSOR.lock().unwrap() = Some(sensor);
        Ok(())
    })();
    r.is_ok()
}

// ------------------------------------------------------------------------------------
// Accelerometer. On convertibles and some clamshells it sits in the lid, so the gravity
// vector tracks the lid angle. Readings are posted as WM_ACCEL with x/y/z packed as
// three signed 16-bit milli-g values in wparam.
// ------------------------------------------------------------------------------------
use windows::Devices::Sensors::{Accelerometer, AccelerometerReadingChangedEventArgs};

use crate::win::WM_ACCEL;

static ACCEL: Mutex<Option<Accelerometer>> = Mutex::new(None);

pub fn pack_accel(x: f64, y: f64, z: f64) -> usize {
    let q = |v: f64| ((v * 1000.0).clamp(-32000.0, 32000.0) as i16) as u16 as usize;
    q(x) | (q(y) << 16) | (q(z) << 32)
}

pub fn unpack_accel(w: usize) -> [f32; 3] {
    let u = |shift: usize| ((w >> shift) & 0xffff) as u16 as i16 as f32 / 1000.0;
    [u(0), u(16), u(32)]
}

/// Returns true when an accelerometer exists and reporting has started.
pub fn start_accelerometer(hwnd: HWND) -> bool {
    let hwnd_raw = hwnd.0 as isize;
    let r: Result<()> = (|| {
        let acc = Accelerometer::GetDefault()?;
        let min = acc.MinimumReportInterval().unwrap_or(16);
        let _ = acc.SetReportInterval(min.max(50));
        let handler = TypedEventHandler::<Accelerometer, AccelerometerReadingChangedEventArgs>::new(move |_s, args| {
            if let Some(a) = args.as_ref() {
                if let Ok(r) = a.Reading() {
                    if let (Ok(x), Ok(y), Ok(z)) = (r.AccelerationX(), r.AccelerationY(), r.AccelerationZ()) {
                        unsafe {
                            let _ = PostMessageW(Some(HWND(hwnd_raw as *mut _)), WM_ACCEL, WPARAM(pack_accel(x, y, z)), LPARAM(0));
                        }
                    }
                }
            }
            Ok(())
        });
        acc.ReadingChanged(&handler)?;
        *ACCEL.lock().unwrap() = Some(acc);
        Ok(())
    })();
    r.is_ok()
}

/// Angle in degrees between two gravity vectors = how far the lid rotated.
pub fn lid_delta_deg(cal: [f32; 3], now: [f32; 3]) -> f32 {
    let dot = cal[0] * now[0] + cal[1] * now[1] + cal[2] * now[2];
    let na = (cal[0] * cal[0] + cal[1] * cal[1] + cal[2] * cal[2]).sqrt();
    let nb = (now[0] * now[0] + now[1] * now[1] + now[2] * now[2]).sqrt();
    if na < 1e-3 || nb < 1e-3 {
        return 0.0;
    }
    (dot / (na * nb)).clamp(-1.0, 1.0).acos().to_degrees()
}
