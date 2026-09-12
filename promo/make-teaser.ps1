# Build the website teaser with WinBend's actual Silk renderer.
# Requires target/release/winbend.exe and ffmpeg on PATH.
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path -Parent $PSScriptRoot)
$work = 'promo/out/website-teaser'
New-Item -ItemType Directory -Force "$work/frames", 'docs/media' | Out-Null
# Use a synthetic desktop so the video never contains personal screen content.
$desktopScript = Get-Content 'promo/make-desktop.ps1' -Raw -Encoding UTF8
$desktopScript = $desktopScript.Replace('Notes — Friday', 'A little more delightful.').Replace('Ship WinBend 0.1.1', 'Your desktop. With a twist.').Replace('Record the promo short', 'Close the lid. Feel the fold.').Replace('Answer GitHub issues', 'Open it. Pick up where you left off.')
$desktopScript = $desktopScript.Replace('main.rs — winbend', 'WinBend / Silk').Replace('fn fold(t: f32) -> Panel {', 'A softer way to sign off.').Replace('    let theta = t * MAX_TILT;', 'Smooth perspective.').Replace('    Panel::hinged(theta)', 'A gentle bend.').Replace('        .bend(0.3)', 'A sweep of light.').Replace('        .sheen(0.8)', 'Everything moves together.').Replace("'}', '', '// close the lid and watch'", "'', '', 'Made for Windows.'")
$desktopScript | Set-Content "$work/desktop.ps1" -Encoding UTF8
& powershell -NoProfile -ExecutionPolicy Bypass -File "$work/desktop.ps1" "$work/desktop.png"
if ($LASTEXITCODE -ne 0) { throw 'Desktop generation failed' }
$render = Start-Process -FilePath './target/release/winbend.exe' -ArgumentList @('--render-clip', "$work/frames", '--style', 'silk', '--frames', '180', '--source', "$work/desktop.png") -Wait -PassThru -WindowStyle Hidden
if ($render.ExitCode -ne 0) { throw 'WinBend rendering failed' }
# A quiet pause at each end makes the six-second motion an eight-second loop.
$filter = "scale=1120:630:flags=lanczos,pad=1280:800:80:100:color=0x0d0f16,drawtext=fontfile='C\:/Windows/Fonts/segoeuib.ttf':text='WinBend':fontsize=30:fontcolor=0xeef0f7:x=80:y=36,drawtext=fontfile='C\:/Windows/Fonts/segoeui.ttf':text='SILK  /  THE DESKTOP IN MOTION':fontsize=17:fontcolor=0x9ea6bd:x=820:y=45,drawbox=x=80:y=738:w=1120:h=2:color=0x8f8bff:t=fill,tpad=start_duration=1:stop_duration=1:start_mode=clone:stop_mode=clone"
& ffmpeg -nostdin -y -loglevel error -framerate 30 -i "$work/frames/%04d.png" -vf $filter -an -c:v libx264 -crf 20 -preset slow -pix_fmt yuv420p -movflags +faststart 'docs/media/winbend-teaser.mp4'
if ($LASTEXITCODE -ne 0) { throw 'Video encoding failed' }
& ffmpeg -nostdin -y -loglevel error -ss 2.1 -i 'docs/media/winbend-teaser.mp4' -frames:v 1 -update 1 'docs/media/winbend-teaser-poster.jpg'
if ($LASTEXITCODE -ne 0) { throw 'Poster generation failed' }


