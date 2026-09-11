# WinBend

Your desktop folds like a lid closing. WinBend captures the live desktop and renders it
on the GPU as a panel that tilts away from you around its bottom edge, bending softly,
darkening and blurring as it goes. It is a Windows take on the macOS app Bendy and the
foldable-phone "fold" animation.

Free and open source (MIT). One `winbend.exe`, no installer, no account, no network.
Lives in the tray. If it makes you smile, [donations](#donate) keep it going.

Website: **[winbend.me](https://winbend.me)** · Download: [latest release](https://github.com/MineGL/winbend/releases/latest)

## What it does

| Trigger | Behaviour |
| --- | --- |
| Hotkey (default `Ctrl+Alt+B`) or left-click the tray icon | Desktop folds down over ~1.1 s and stays folded. Press the hotkey again, any key, or click to unfold. Mouse wheel adjusts the angle while folded. |
| **Webcam motion** (opt-in, any laptop with a camera in the lid) | The camera pitches with the lid, so the scene slides up in the image as the lid closes. WinBend measures that shift on tiny 64x48 grayscale frames and integrates it into a lid angle. The fold follows the lid live through the **fold zone**, by default the last 40° before shut; above that the desktop is completely normal. Frames live only in memory for one comparison. The camera light stays on while enabled, which is why it is off by default. |
| Hinge angle sensor (2-in-1s, Surface Laptop Studio, some Yoga / Spectre models) | Fold follows the physical lid angle, live. |
| Accelerometer in the lid (most convertibles) | After a one-click calibration from the tray, the gravity vector gives the lid angle. |
| Lid closes or the PC goes to sleep | Whatever the fold is doing, it finishes closing in 0.35 s and stays parked through sleep. |
| Lid opens, PC wakes, or you unlock | The desktop is still folded when the screen comes back, then lifts, following the lid if the webcam is on. If Windows shows the sign-in screen first, the lift waits and plays right after you sign in. |
| *After the hotkey fold* → Lock / Sleep / Turn the screen off | The fold becomes your lock or sleep gesture. |
| *Fold when idle* → 1 / 5 / 10 / 15 minutes | The desktop folds down like a curtain when you walk away and lifts on the first key or mouse movement. |

## Settings window

Right-click the tray icon → **Settings…** (it also opens by itself on first launch with a
short welcome). Everything saves as you change it; there is no Save button.

- **Fold**: a live preview of your own desktop rendered with the current style, a fold
  slider and Play button, the Silk / Shade / Frost / Custom presets with six sliders, tilt,
  duration, background colour, and a press-a-key picker for the fold shortcut.
- **Lid**: webcam tracking switch with a live tilt gauge, the fold zone slider, a guided
  two-step calibration wizard, the **tracking shortcut** (default `Ctrl+Alt+C`, toggles
  webcam tracking on/off from anywhere), plus hinge / accelerometer rows when present.
- **Sleep & wake**, **Windows** (the two admin-prompt switches with inline confirmation),
  **About** (version, shortcuts, sensors, settings file, Donate, GitHub).

The window is a WebView2 page embedded in the exe (Microsoft Edge WebView2 Runtime, which
ships with Windows 11 and most Windows 10 PCs). If the runtime is missing, WinBend says so,
offers the download page, and opens the settings file instead; the tray menu keeps working.
`winbend --ui-test 5` opens the window headlessly for a few seconds and exits 0 once the page
and the first preview arrived (used by CI).

Three built-in styles, selectable from the tray or the settings window: **Silk** (clean perspective with a
glossy sweep), **Shade** (deep shadow toward the far edge), **Frost** (strong blur).
`Custom` reads six values from the settings file: perspective, blur, shadow, bend,
vignette, sheen.

### Webcam tracking, in detail

Enable it from the tray: *Follow the lid (webcam motion)*. The fold is driven by the
lid angle alone. The desktop stays completely normal until the lid is within the fold
zone; inside it the fold runs from flat to closed on an eased curve, so the hand-over is
invisible in both directions, and the overlay only appears once the lid is about a
degree inside the zone. When the camera goes dark near the end, the angle glides on to
the end of travel rather than jumping. While the fold is barely engaged the overlay is
click-through with a normal cursor, so the mouse keeps working; if the lid rests still
inside the zone for 10 s, WinBend adopts that as your new working angle and stands down.

Set the zone from the tray (*Lid fold zone (webcam)*: last 20/30/40/50/60° or the whole
motion) or calibrate both numbers to your lid with *Calibrate lid tracking (webcam)…*:
step 1 closes and reopens the lid once to measure its travel, step 2 holds the lid where
the fold should begin and presses the hotkey. Nothing folds while calibrating.

Safety: any input, the hotkey, Esc, or a click lifts a sensor-driven fold and mutes the
sensors for 8 s; a fold parked by a sensor with the camera seeing an open lid also lifts
by itself after 20 s. A "lid closed" report within 8 s of waking is ignored.

Check what the tracker sees with the debug build (the release build has no console):

```powershell
.\target\debug\winbend.exe --camera-test 10        # prints the estimated tilt for 10 s
$env:WINBEND_CAMERA_DEBUG=1; .\target\debug\winbend.exe --camera-test 5   # adds raw frame stats
```

### Windows integration (tray → *Windows integration*)

Two Windows power settings decide whether the fold can be seen around sleep. Both items
change them through one Windows admin (UAC) prompt, show a check mark while active, and
are reversed by clicking again.

- **WinBend handles the lid: fold, then sleep.** Windows normally sleeps the instant the
  lid switch fires, with the screen already dark. This sets the Windows lid-close action to
  *Do nothing*; WinBend then receives the lid-closed event, finishes the fold, and puts the
  PC to sleep itself. Opening the lid wakes the PC and the desktop lifts.
- **Skip the sign-in screen after sleep.** Sets Windows' "require sign-in after sleep"
  (`powercfg` alias `CONSOLELOCK`) to *Never*, so waking lands directly on the unfolding
  desktop. Windows 11 hides this option in Settings when "Only allow Windows Hello sign-in"
  is on. The confirmation dialog spells out the consequence: anyone who opens the lid gets
  the desktop. Click again to restore.
- **Open Windows sign-in settings…** opens `ms-settings:signinoptions`.

Sleep started from the Start menu or the power button still darkens the screen before
any animation can run; nothing in user space can delay that.

## Install

Download `WinBend-<version>-win64.zip` from the Releases page, unzip anywhere, run
`winbend.exe`. It appears in the tray (possibly behind the `^` arrow). Press `Ctrl+Alt+B`.

Windows may show "Windows protected your PC" for an unsigned download: click *More info*
→ *Run anyway*. WinBend never connects to the internet; you can also build it yourself.

Requirements: Windows 10 2004 or later (Windows 11 recommended), any Direct3D 11 GPU.

## Build

```powershell
cargo build --release
```

Rust stable (MSVC target), Visual Studio Build Tools with the C++ workload, and a Windows
10/11 SDK. Shaders compile at runtime with `d3dcompiler_47.dll`, which ships with Windows.
`dist\package.ps1` builds and zips a release. GitHub Actions does the same on every push
and attaches the zip to tagged releases (`v*`).

Useful during development:

```powershell
.\target\debug\winbend.exe --render-test out.png --style frost --t 0.7   # one folded frame to PNG
.\target\debug\winbend.exe --demo                                       # fold, hold, unfold, exit
.\target\debug\winbend.exe --diag                                       # sensors, camera, power settings
```

## Settings

`%APPDATA%\WinBend\config.toml`, created on first run. Tray → *Open settings file* /
*Reload settings*. Everything in the tray menu writes here too.

```toml
style = "silk"            # silk | shade | frost | custom
max_tilt_deg = 86.0       # how far the panel tilts when fully folded (90 = edge-on)
fold_ms = 1100            # hotkey animation duration
hotkey = "Ctrl+Alt+B"     # Ctrl / Alt / Shift / Win + a letter, digit, or F-key
camera_hotkey = "Ctrl+Alt+C"   # toggles webcam lid tracking
after_fold = "none"       # none | lock | sleep | display_off
fold_when_idle_min = 0    # 0 = off; otherwise fold after N idle minutes
unfold_on_wake = true
fold_on_sleep = true
handle_lid = false        # set by "WinBend handles the lid"
unfold_on_lid_open = true
follow_hinge = true
follow_accelerometer = false   # set by "Calibrate lid tracking" (accelerometer)
accel_travel_deg = 85.0
follow_camera = false          # webcam motion tracking; camera light stays on while true
camera_vfov_deg = 42.0         # your webcam's vertical field of view
camera_travel_deg = 75.0       # lid rotation seen by the camera to reach fully folded
camera_flat_deg = 40.0         # desktop stays normal until the lid is this close to shut
hinge_open_deg = 100.0
hinge_closed_deg = 10.0
background = "#000000"

[custom]
perspective = 2.6         # camera distance; smaller = more dramatic
blur = 0.0
shadow = 0.4
bend = 0.3                # 0 = rigid hinge, 1 = the whole panel curves
vignette = 0.25
sheen = 0.8
```

## Donate

WinBend is free. If you enjoy it, you can support development:

- **GitHub Sponsors**: [github.com/sponsors/MineGL](https://github.com/sponsors/MineGL), the
  *Sponsor* button on this repository, or *Donate* in the tray menu.
- **Crypto** (send only on the network named):
  - Ethereum (ERC-20): `0xf177d181a287f43e45b14235640693d5daac125a`
  - BNB Smart Chain (BEP-20): `0xf177d181a287f43e45b14235640693d5daac125a`
  - Tron (TRC-20): `TWMBxrG7ERvYYejd2PMuLJS3jnjQCrArAx`

  The same addresses are in the app under Settings → About → Support WinBend, with a copy button.

## Publishing checklist (maintainer)

1. Create the empty repository `MineGL/winbend` on GitHub (no README), then push:
   `git remote add origin https://github.com/MineGL/winbend.git && git push -u origin main`.
2. **Website** (`docs/` is a static landing page; `docs/CNAME` holds `winbend.me`):
   GitHub → repository Settings → Pages → Source: *Deploy from a branch*, branch `main`,
   folder `/docs`. Then at Namecheap → Domain List → winbend.me → Advanced DNS, add:
   `A @ 185.199.108.153`, `A @ 185.199.109.153`, `A @ 185.199.110.153`, `A @ 185.199.111.153`,
   `CNAME www MineGL.github.io`. Back in GitHub Pages set the custom domain `winbend.me`, wait
   for the DNS check, tick *Enforce HTTPS*. GitHub issues the certificate itself; the SSL
   certificate bought from Namecheap is not needed for Pages (keep it if you later move to a
   host where you control the server).
3. Tag a release: `git tag v0.1.0 && git push --tags`. The workflow builds, runs the tests and
   the settings-window smoke test, and attaches `WinBend-0.1.0-win64.zip` to the release, which
   is where the site's Download button points.
4. Enable GitHub Sponsors for the MineGL account (Stripe Connect payout). Until it is approved,
   the Sponsor links show GitHub's "not yet sponsorable" page; crypto works immediately.
5. Replace the CSS demo on the landing page with a real 10-second clip when you have one.

## Project layout

```
build.rs                procedural app icon + version resource + DPI manifest
src/main.rs             entry, command-line modes, message loop
src/ui/mod.rs           settings window: WebView2 host (callbacks only enqueue events)
src/ui/preview.rs       live desktop preview (second renderer → PNG data URL)
src/ui/index.html       the settings page (single file, inline CSS/JS)
src/app/settings.rs     settings presenter: page messages → App, state snapshots → page
src/app.rs              fold state machine, sessions, tray commands, calibration, safety
src/win.rs              Win32: windows, overlay + swapchain, tray, menu, hotkey, power events
src/camera.rs           webcam lid tracking (Media Foundation, vertical shift estimate)
src/sensor.rs           HingeAngleSensor + Accelerometer (WinRT)
src/power.rs            Windows power settings (lid action, sign-in on wake)
src/config.rs           TOML settings, style presets, hotkey parsing
src/links.rs            GitHub / donate URLs
src/gfx/capture.rs      Windows.Graphics.Capture → D3D11 texture
src/gfx/renderer.rs     blur chain + bent-panel pass, 4x MSAA
src/gfx/shaders.hlsl    the effect
```

## Contributing

Issues and pull requests are welcome. Please keep the sensor safety rules intact: a
sensor-driven fold must never be able to trap the user behind a black screen.

## License

MIT. See [LICENSE](LICENSE).
