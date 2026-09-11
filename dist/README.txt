WinBend - your desktop folds like a lid closing
================================================
Free and open source (MIT). Source, issues and updates: see "WinBend on GitHub" in the tray menu.

Install
  Installer: run WinBend-Setup-<version>.exe. No admin prompt; it installs for your user only,
  adds a Start menu entry and can start WinBend when you sign in.
  Portable (this zip): unzip anywhere (for example your Desktop or C:\Tools\WinBend) and
  double-click winbend.exe.

  WinBend appears in the tray (bottom-right, near the clock; maybe behind the ^ arrow).
  Press Ctrl+Alt+B. Your desktop folds down. Press it again, any key, or click to unfold.

  If Windows shows "Windows protected your PC", click "More info" and then "Run anyway".
  WinBend is a single file that never connects to the internet.

Use
  - Right-click the tray icon and choose "Settings..." for the settings window: live preview,
    styles, shortcuts, lid tracking with guided calibration, sleep and Windows options.
    It opens by itself the first time you run WinBend.
  - Ctrl+Alt+C turns webcam lid tracking on or off from anywhere (changeable in Settings > Lid).
  - The tray menu has the same options if you prefer it.
  - Mouse wheel while folded adjusts the angle.
  - "Follow the lid (webcam motion)": the fold tracks your lid with the camera through the
    last part of the closing motion. Frames are analysed in memory only and never saved or
    sent anywhere; the camera light stays on while it is enabled. Off by default.
    Use "Lid fold zone" or "Calibrate lid tracking" to fit it to your laptop.
  - "Unfold when the lid opens" / "Unfold on wake" play the snap-back when you come back.
  - "Lock Windows after folding" turns the hotkey into a cinematic lock.
  - "Fold when idle" folds the desktop like a curtain when you walk away.
  - Windows integration > "WinBend handles the lid": closing the lid folds the desktop first,
    then WinBend puts the PC to sleep. Windows asks for admin approval once; click again to
    give the lid back to Windows.
  - Windows integration > "Skip the sign-in screen after sleep": wake straight into the
    unfolding desktop. Anyone who opens the lid then gets your desktop without a password,
    so only use it where that is fine. Click again to restore the sign-in screen.
  - "Run at startup" keeps WinBend in the tray after you sign in.

Uninstall
  Installed version: Windows Settings > Apps > WinBend > Uninstall.
  Portable version: quit from the tray menu, delete winbend.exe, and delete the folder %APPDATA%\WinBend.
  If "Run at startup" was on, turn it off first. If you enabled either Windows integration
  item, click it again first so Windows' lid and sign-in behaviour go back to normal.

Requirements
  Windows 10 version 2004 or newer / Windows 11. Any Direct3D 11 capable GPU.

Donate
  WinBend is free. If it makes you smile, "Donate" in the tray menu keeps it going.
