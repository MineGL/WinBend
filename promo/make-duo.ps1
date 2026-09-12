# "They made a phone that folds" cut: sells WinBend with a dry foldable-phone bit, then shows the
# 3D laptop lid closing with the desktop folding in sync. Works for both formats.
#
#   powershell -ExecutionPolicy Bypass -File promo\make-duo.ps1 -Portrait  -Music <mp3>   # 1080x1920 Short
#   powershell -ExecutionPolicy Bypass -File promo\make-duo.ps1 -Landscape -Music <mp3>   # 1920x1080
#
# Needs the lid clips from promo\laptop\render.js: lid-silk.mp4 / lid-origami.mp4 (portrait) and
# lid-silk-wide.mp4 / lid-origami-wide.mp4 (landscape) in promo\out.
param(
    [switch]$Portrait,
    [switch]$Landscape,
    [string]$Music,
    [string]$Out = ""
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $root
if (-not $Portrait -and -not $Landscape) { $Portrait = $true }
$W = if ($Portrait) { 1080 } else { 1920 }
$H = if ($Portrait) { 1920 } else { 1080 }
$tag = if ($Portrait) { "short" } else { "wide" }
if (-not $Out) { $Out = "promo\out\winbend-duo-$tag.mp4" }
$silkClip = if ($Portrait) { "promo\out\lid-silk.mp4" } else { "promo\out\lid-silk-wide.mp4" }
$origamiClip = if ($Portrait) { "promo\out\lid-origami.mp4" } else { "promo\out\lid-origami-wide.mp4" }
foreach ($c in $silkClip, $origamiClip) { if (-not (Test-Path $c)) { throw "missing $c (render it with promo\laptop\render.js)" } }

$ffmpeg = (Get-Command ffmpeg -ErrorAction Stop).Source
$fps = 30
$tmp = "promo\out\duo-$tag"
New-Item -ItemType Directory -Force $tmp | Out-Null
$bg = "0x0d0f16"; $white = "0xffffff"; $accent = "0x8f8bff"; $muted = "0x9ea6bd"; $dim = "0x5b6380"
$light = "C\:/Windows/Fonts/segoeuil.ttf"; $reg = "C\:/Windows/Fonts/segoeui.ttf"; $bold = "C\:/Windows/Fonts/segoeuib.ttf"
$S = [Math]::Min($W, $H) / 1080.0   # type scale; y positions are fractions of H

function Esc($text) { $text -replace "\\", "\\\\" -replace "'", "\\'" -replace ":", "\\:" -replace ",", "\\," -replace "%", "\\%" }
function Draw($text, $font, $size, $color, $yFrac, $extra = "") {
    $px = [int]($size * $S); $y = [int]($yFrac * $H)
    "drawtext=fontfile='${font}':text='$(Esc $text)':fontsize=${px}:fontcolor=${color}:x=(w-text_w)/2:y=${y}${extra}"
}
function Run($outFile, $ffArgs) {
    & $ffmpeg -y -loglevel error @ffArgs $outFile
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path $outFile)) { throw "ffmpeg failed for $outFile (exit $LASTEXITCODE)" }
}
$parts = @(); $n = 0
function Next($name) { $script:n++; "$tmp\{0:d2}-$name.mp4" -f $script:n }

# Text card with a slow push-in. $lines = @(@(text, font, size, color, yFrac), ...)
function Card($name, $lines, $seconds, $zoom = 0.05) {
    if ($lines.Count -gt 0 -and -not ($lines[0] -is [array])) { $lines = ,$lines }
    $draws = @(); foreach ($l in $lines) { $draws += Draw $l[0] $l[1] $l[2] $l[3] $l[4] }
    $png = "$tmp\$name.png"
    Run $png @("-f", "lavfi", "-i", "color=c=${bg}:s=${W}x${H}:r=1:d=1", "-vf", ($draws -join ","), "-frames:v", "1", "-update", "1")
    $d = [int]($seconds * $fps); $seg = Next $name
    $vf = "zoompan=z='1+${zoom}*on/${d}':d=${d}:x='iw/2-(iw/zoom/2)':y='ih/2-(ih/zoom/2)':s=${W}x${H}:fps=${fps},fade=t=in:st=0:d=0.3,fade=t=out:st=$($seconds - 0.3):d=0.3"
    Run $seg @("-i", $png, "-vf", $vf, "-c:v", "libx264", "-pix_fmt", "yuv420p", "-r", "$fps", "-t", "$seconds")
    $script:parts += $seg
}

# Lid clip, trimmed, with a caption that fades in and out (shadow + box for legibility on the light desk).
function Clip($name, $file, $seconds, $caption, $from = 0.4, $to = 3.0, $yFrac = 0.16) {
    $seg = Next $name
    $vf = "scale=${W}:${H}:force_original_aspect_ratio=increase,crop=${W}:${H},fps=${fps},setsar=1"
    if ($caption) { $vf += "," + (Draw $caption $light 86 $white $yFrac ":shadowcolor=black@0.5:shadowx=3:shadowy=3:box=1:boxcolor=black@0.30:boxborderw=22:enable='between(t\,${from}\,${to})':alpha='min(1\,(t-${from})/0.3)*min(1\,(${to}-t)/0.3)'") }
    $vf += ",fade=t=in:st=0:d=0.3,fade=t=out:st=$($seconds - 0.3):d=0.3"
    Run $seg @("-i", $file, "-t", "$seconds", "-vf", $vf, "-an", "-c:v", "libx264", "-pix_fmt", "yuv420p", "-r", "$fps")
    $script:parts += $seg
}

# ---- the bit --------------------------------------------------------------------------------
Card "phone"   @(@("They made a phone that folds.", $light, 78, $white, 0.44), @("for about two thousand dollars", $reg, 36, $muted, 0.52)) 2.6
Card "cute"    @(@("Cute.", $bold, 190, $white, 0.40)) 1.7 0.10
Card "laptop"  @(@("Your laptop has folded", $light, 78, $white, 0.40), @("since 1985.", $light, 78, $white, 0.47), @("You just never watched the screen.", $reg, 36, $muted, 0.56)) 2.8
Clip "silk" $silkClip 7.0 "Now the desktop folds with it." 0.5 3.4
Card "nohinge" @(@("No hinge upgrade.", $light, 84, $white, 0.44)) 1.7
Card "nocrease" @(@("No crease down the middle.", $light, 84, $white, 0.44)) 1.7
Card "okay"    @(@("Okay. One crease.", $light, 84, $white, 0.41), @("If you want.", $light, 84, $accent, 0.48)) 1.8
Clip "origami" $origamiClip 5.4 "Origami." 0.4 2.6
Card "price"   @(@('$0.', $bold, 210, $white, 0.36), @("vs. the phone.", $reg, 40, $muted, 0.53)) 2.2 0.08
Card "cta"     @(@("winbend.me", $bold, 108, $white, 0.40), @("Free and open source  |  Windows 10 / 11", $reg, 36, $muted, 0.49), @("Ctrl+Alt+B to fold   |   follows your lid via webcam", $reg, 32, $dim, 0.535)) 3.0 0.03

$list = "$tmp\concat.txt"
($parts | ForEach-Object { "file '" + ((Resolve-Path $_).Path -replace '\\', '/') + "'" }) | Set-Content $list -Encoding ascii
$silent = if ($Music) { "$tmp\silent.mp4" } else { $Out }
Run $silent @("-f", "concat", "-safe", "0", "-i", $list, "-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "17", "-movflags", "+faststart")
if ($Music) {
    powershell -NoProfile -ExecutionPolicy Bypass -File promo\add-music.ps1 -In $silent -Out $Out -Music $Music
    if ($LASTEXITCODE -ne 0) { throw "add-music failed" }
}
Write-Host "wrote $Out (${W}x${H})"
