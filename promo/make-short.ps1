# Builds a vertical 1080x1920 promo clip (YouTube Short / Reel / TikTok) showing all four
# fold styles on a clean showcase desktop, with title and call-to-action cards.
#
#   powershell -ExecutionPolicy Bypass -File promo\make-short.ps1             # painted showcase desktop
#   powershell -ExecutionPolicy Bypass -File promo\make-short.ps1 -Wallpaper  # your current wallpaper, no windows
#   powershell -ExecutionPolicy Bypass -File promo\make-short.ps1 -Image x.jpg # any picture (jpg/png/bmp)
#   powershell -ExecutionPolicy Bypass -File promo\make-short.ps1 -Screen     # your real screen as it is now
#
# Needs: a release build (cargo build --release) and ffmpeg on PATH (winget install Gyan.FFmpeg).
param([switch]$Screen, [switch]$Wallpaper, [string]$Image, [string]$Out = "promo\out\winbend-short.mp4")
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $root
$exe = "target\release\winbend.exe"
if (-not (Test-Path $exe)) { throw "build first: cargo build --release" }
$ffmpeg = (Get-Command ffmpeg -ErrorAction Stop).Source
$work = "promo\out"
New-Item -ItemType Directory -Force $work | Out-Null

# Fit any picture to the primary screen (cover: scale up, crop the overflow) and save it as PNG,
# which is the format --render-clip reads.
function Fit-Image($path, $outPng) {
    Add-Type -AssemblyName System.Drawing
    Add-Type -AssemblyName System.Windows.Forms
    # Without this, a 125 % display scale reports 1536x864 instead of the real 1920x1080.
    if (-not ("PromoDpi" -as [type])) { Add-Type -TypeDefinition 'using System.Runtime.InteropServices; public class PromoDpi { [DllImport("user32.dll")] public static extern bool SetProcessDPIAware(); }' }
    [PromoDpi]::SetProcessDPIAware() | Out-Null
    $b = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
    $W = [Math]::Max(1280, $b.Width); $H = [Math]::Max(720, $b.Height)
    $src = [System.Drawing.Image]::FromFile((Resolve-Path $path).Path)
    $scale = [Math]::Max($W / $src.Width, $H / $src.Height)
    $sw = [int]([Math]::Ceiling($src.Width * $scale)); $sh = [int]([Math]::Ceiling($src.Height * $scale))
    $bmp = New-Object System.Drawing.Bitmap $W, $H
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.InterpolationMode = 'HighQualityBicubic'
    $g.DrawImage($src, [int](($W - $sw) / 2), [int](($H - $sh) / 2), $sw, $sh)
    $g.Dispose(); $src.Dispose()
    $bmp.Save($outPng, [System.Drawing.Imaging.ImageFormat]::Png); $bmp.Dispose()
    Write-Host "wrote $outPng (${W}x${H} from $path)"
}

$source = @()
if ($Screen) {
    # live capture of the primary monitor, nothing to prepare
} elseif ($Image) {
    Fit-Image $Image "$work\desktop.png"
    $source = @("--source", "$work\desktop.png")
} elseif ($Wallpaper) {
    $wp = (Get-ItemProperty 'HKCU:\Control Panel\Desktop' -ErrorAction SilentlyContinue).WallPaper
    if (-not $wp -or -not (Test-Path $wp)) { $wp = "$env:APPDATA\Microsoft\Windows\Themes\TranscodedWallpaper" }
    if (-not (Test-Path $wp)) { throw "could not find the current wallpaper; use -Image <file> instead" }
    Fit-Image $wp "$work\desktop.png"
    $source = @("--source", "$work\desktop.png")
} else {
    powershell -NoProfile -ExecutionPolicy Bypass -File promo\make-desktop.ps1 "$work\desktop.png"
    $source = @("--source", "$work\desktop.png")
}

$fps = 30
$styles = @(
    @{ id = "silk";    name = "Silk";    blurb = "Clean tilt, soft bend, glossy sweep" },
    @{ id = "shade";   name = "Shade";   blurb = "Deep shadow toward the far edge" },
    @{ id = "frost";   name = "Frost";   blurb = "Blurs like glass coming down" },
    @{ id = "origami"; name = "Origami"; blurb = "Creases into an accordion" }
)
foreach ($s in $styles) {
    if (Test-Path "$work\$($s.id)") { Remove-Item -Recurse -Force "$work\$($s.id)" }
    & $exe --render-clip "$work\$($s.id)" --style $s.id --frames 96 @source | Out-Host
    if ($LASTEXITCODE -ne 0) { throw "render failed for $($s.id)" }
}

$bg = "0x0d0f16"
$fontB = "C\:/Windows/Fonts/segoeuib.ttf"
$fontR = "C\:/Windows/Fonts/segoeui.ttf"
$fontL = "C\:/Windows/Fonts/segoeuil.ttf"
function Text($text, $font, $size, $color, $y, $extra = "") {
    # PowerShell reads `$size:font` as a scoped variable, so every variable here is braced.
    $t = $text -replace "'", "\\'" -replace ":", "\\:"
    "drawtext=fontfile='${font}':text='${t}':fontsize=${size}:fontcolor=${color}:x=(w-text_w)/2:y=${y}${extra}"
}
function Run($outFile, $ffArgs) {
    & $ffmpeg -y -loglevel error @ffArgs $outFile
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path $outFile)) { throw "ffmpeg failed for $outFile (exit $LASTEXITCODE)" }
}
$fadeIn = ":alpha='if(lt(t,0.5),t/0.5,1)'"

# Title card (2.6 s)
$title = "$work\00-title.mp4"
$vf = (Text "WinBend" $fontB 118 "white" 700 $fadeIn) + "," +
      (Text "Your desktop bends" $fontL 74 "0xdfe3ff" 880 $fadeIn) + "," +
      (Text "as you close the lid." $fontL 74 "0xdfe3ff" 970 $fadeIn) + "," +
      (Text "for Windows 10 / 11" $fontR 40 "0x9ea6bd" 1120 $fadeIn) + ",fade=t=out:st=2.2:d=0.4"
Run $title @("-f", "lavfi", "-i", "color=c=${bg}:s=1080x1920:r=${fps}:d=2.6", "-vf", $vf, "-c:v", "libx264", "-pix_fmt", "yuv420p", "-r", "$fps")

# One segment per style: frames scaled to 1000 px wide, centred, label above, blurb below.
$parts = @($title)
$n = 1
foreach ($s in $styles) {
    $seg = "$work\0$n-$($s.id).mp4"
    # Zoom so the desktop fills the width: scale to 1500 px, crop the middle 1080, then pad to 9:16.
    $vf = "scale=1500:-2,crop=1080:ih:(iw-1080)/2:0,pad=1080:1920:(ow-iw)/2:(oh-ih)/2:color=${bg}," +
          (Text $s.name $fontB 96 "white" 400) + "," +
          (Text $s.blurb $fontR 42 "0x9ea6bd" 1440) + "," +
          (Text "$n / 4" $fontR 34 "0x5b6380" 1780) + ",fade=t=in:st=0:d=0.25,fade=t=out:st=2.95:d=0.25"
    Run $seg @("-framerate", "$fps", "-i", "$work\$($s.id)\%04d.png", "-vf", $vf, "-c:v", "libx264", "-pix_fmt", "yuv420p", "-r", "$fps", "-t", "3.2")
    $parts += $seg
    $n++
}

# Call to action (3.4 s)
$cta = "$work\09-cta.mp4"
$vf = (Text "Free & open source" $fontB 82 "white" 760 $fadeIn) + "," +
      (Text "winbend.me" $fontB 96 "0x8f8bff" 880 $fadeIn) + "," +
      (Text "Ctrl+Alt+B to fold   |   follows your lid via webcam" $fontR 38 "0x9ea6bd" 1040 $fadeIn) + "," +
      (Text "MIT licensed   |   no account   |   no network" $fontR 34 "0x5b6380" 1120 $fadeIn) + ",fade=t=out:st=3.0:d=0.4"
Run $cta @("-f", "lavfi", "-i", "color=c=${bg}:s=1080x1920:r=${fps}:d=3.4", "-vf", $vf, "-c:v", "libx264", "-pix_fmt", "yuv420p", "-r", "$fps")
$parts += $cta

$list = "$work\concat.txt"
($parts | ForEach-Object { "file '" + ((Resolve-Path $_).Path -replace '\\', '/') + "'" }) | Set-Content $list -Encoding ascii
Run $Out @("-f", "concat", "-safe", "0", "-i", $list, "-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "18", "-movflags", "+faststart")
Write-Host "wrote $Out"
