# Straight product demo, footage only: the 3D laptop closing with the desktop folding in sync,
# the fold on a real desktop with the hotkey, the four styles, the settings window, the URL.
# Small lower-third captions, no title cards, no jokes.
#
#   powershell -ExecutionPolicy Bypass -File promo\make-demo.ps1 -Portrait  -Music <mp3>   # 1080x1920
#   powershell -ExecutionPolicy Bypass -File promo\make-demo.ps1 -Landscape -Music <mp3>   # 1920x1080
#
# Inputs (all in promo\out): lid-silk[-wide].mp4, lid-origami[-wide].mp4 (promo\laptop\render.js),
# silk/ shade/ frost/ origami/ frame folders (make-short.ps1), settings.png (screenshot).
param([switch]$Portrait, [switch]$Landscape, [string]$Music, [string]$Out = "")
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $root
if (-not $Portrait -and -not $Landscape) { $Portrait = $true }
$W = if ($Portrait) { 1080 } else { 1920 }; $H = if ($Portrait) { 1920 } else { 1080 }
$tag = if ($Portrait) { "short" } else { "wide" }
if (-not $Out) { $Out = "promo\out\winbend-demo-$tag.mp4" }
$silkClip = if ($Portrait) { "promo\out\lid-silk.mp4" } else { "promo\out\lid-silk-wide.mp4" }
$origamiClip = if ($Portrait) { "promo\out\lid-origami.mp4" } else { "promo\out\lid-origami-wide.mp4" }
foreach ($f in $silkClip, $origamiClip, "promo\out\settings.png", "promo\out\silk\0000.png") { if (-not (Test-Path $f)) { throw "missing $f" } }

$ffmpeg = (Get-Command ffmpeg -ErrorAction Stop).Source
$fps = 30
$tmp = "promo\out\demo-$tag"; New-Item -ItemType Directory -Force $tmp | Out-Null
$bg = "0x0d0f16"; $white = "0xffffff"; $muted = "0x9ea6bd"
$light = "C\:/Windows/Fonts/segoeuil.ttf"; $reg = "C\:/Windows/Fonts/segoeui.ttf"; $bold = "C\:/Windows/Fonts/segoeuib.ttf"
$S = [Math]::Min($W, $H) / 1080.0
$capY = if ($Portrait) { 0.80 } else { 0.86 }   # lower third
# drawtext text escaping. PowerShell replacement strings treat backslash literally, so each
# replacement below emits exactly the one backslash ffmpeg expects.
function Esc($t) { (((($t -replace '\\', '\\') -replace "'", "\'") -replace ':', '\:') -replace ',', '\,') -replace '%', '\%' }
function Draw($text, $font, $size, $color, $yFrac, $extra = "") { "drawtext=fontfile='${font}':text='$(Esc $text)':fontsize=$([int]($size * $S)):fontcolor=${color}:x=(w-text_w)/2:y=$([int]($yFrac * $H))${extra}" }
function Cap($text, $from, $to, $yFrac = $capY, $size = 0) {
    # Auto-size so a caption never exceeds ~90 % of the frame width (Segoe UI averages ~0.5 em per glyph).
    if ($size -le 0) { $size = [Math]::Min(54, [Math]::Floor(0.9 * $W / ($text.Length * 0.5) / $S)) }
    Draw $text $reg $size $white $yFrac ":shadowcolor=black@0.5:shadowx=2:shadowy=2:box=1:boxcolor=black@0.42:boxborderw=18:enable='between(t\,${from}\,${to})':alpha='min(1\,(t-${from})/0.25)*min(1\,(${to}-t)/0.25)'" }
function Run($outFile, $ffArgs) { & $ffmpeg -y -loglevel error @ffArgs $outFile; if ($LASTEXITCODE -ne 0 -or -not (Test-Path $outFile)) { throw "ffmpeg failed for $outFile" } }
$parts = @(); $n = 0
function Next($name) { $script:n++; "$tmp\{0:d2}-$name.mp4" -f $script:n }
$fill = "scale=${W}:${H}:force_original_aspect_ratio=increase,crop=${W}:${H},fps=${fps},setsar=1"

# 3D lid clip with captions; $caps = @(@(text, from, to), ...)
function Lid($name, $file, $seconds, $caps) {
    $seg = Next $name; $vf = $fill
    if ($caps.Count -gt 0 -and -not ($caps[0] -is [array])) { $caps = ,$caps }   # PowerShell flattens @(@(...))
    foreach ($c in $caps) { $vf += "," + (Cap $c[0] $c[1] $c[2]) }
    $vf += ",fade=t=in:st=0:d=0.25,fade=t=out:st=$($seconds - 0.25):d=0.25"
    Run $seg @("-i", $file, "-t", "$seconds", "-vf", $vf, "-an", "-c:v", "libx264", "-pix_fmt", "yuv420p", "-r", "$fps")
    $script:parts += $seg
}
# Engine frames of a real desktop folding (close-hold-open, 96 frames = 3.2 s) with a style label.
function Desk($name, $style, $seconds, $label, $caption = $null) {
    $seg = Next $name
    $vf = if ($Portrait) { "scale=1500:-2,crop=1080:ih:(iw-1080)/2:0,pad=${W}:${H}:(ow-iw)/2:(oh-ih)/2:color=${bg}" } else { "scale=${W}:-2,pad=${W}:${H}:(ow-iw)/2:(oh-ih)/2:color=${bg}" }
    $vf += ",fps=${fps}"
    if ($caption) { $vf += "," + (Cap $caption 0.3 ($seconds - 0.3)) }
    $labelY = if ($Portrait) { 0.27 } else { 0.06 }
    $vf += "," + (Draw $label $light 60 $muted $labelY)
    $vf += ",fade=t=in:st=0:d=0.2,fade=t=out:st=$($seconds - 0.2):d=0.2"
    Run $seg @("-framerate", "$fps", "-i", "promo\out\$style\%04d.png", "-t", "$seconds", "-vf", $vf, "-c:v", "libx264", "-pix_fmt", "yuv420p", "-r", "$fps")
    $script:parts += $seg
}
# Still image with a slow push-in and a caption.
function Still($name, $file, $seconds, $caption, $zoom = 0.06) {
    $seg = Next $name; $d = [int]($seconds * $fps)
    $fit = if ($Portrait) { "scale=${W}:-2" } else { "scale=-2:$([int]($H * 0.86))" }
    $vf = "${fit},pad=${W}:${H}:(ow-iw)/2:(oh-ih)/2:color=${bg},zoompan=z='1+${zoom}*on/${d}':d=${d}:x='iw/2-(iw/zoom/2)':y='ih/2-(ih/zoom/2)':s=${W}x${H}:fps=${fps}," + (Cap $caption 0.3 ($seconds - 0.3)) + ",fade=t=in:st=0:d=0.25,fade=t=out:st=$($seconds - 0.25):d=0.25"
    Run $seg @("-loop", "1", "-i", $file, "-t", "$seconds", "-vf", $vf, "-c:v", "libx264", "-pix_fmt", "yuv420p", "-r", "$fps")
    $script:parts += $seg
}

# ---- timeline (~30 s) -------------------------------------------------------------------------
# lid-silk cycle: closing ~0.6-3.7 s, shut until ~4.6 s, opening ~4.6-7.5 s
Lid "lid-silk" $silkClip 8.0 @(@("Close the lid. Your desktop folds with it.", 0.5, 3.6), @("Open it. It unfolds.", 4.7, 7.5))
Lid "lid-origami" $origamiClip 4.4 @(@("Origami style: it creases like paper.", 0.4, 4.1))
Desk "desk-silk" "silk" 3.2 "Silk" "Or press Ctrl+Alt+B on any desktop."
Desk "desk-shade" "shade" 2.4 "Shade"
Desk "desk-frost" "frost" 2.4 "Frost"
Desk "desk-origami" "origami" 3.2 "Origami" "Four styles, plus your own."
Still "settings" "promo\out\settings.png" 3.6 "Settings: styles, fold zone, lid calibration."
# end: URL only
$png = "$tmp\end.png"
Run $png @("-f", "lavfi", "-i", "color=c=${bg}:s=${W}x${H}:r=1:d=1", "-vf", ((Draw "winbend.me" $bold 104 $white 0.42) + "," + (Draw "Free and open source  |  Windows 10 / 11" $reg 36 $muted 0.51)), "-frames:v", "1", "-update", "1")
$seg = Next "end"; Run $seg @("-loop", "1", "-i", $png, "-t", "2.8", "-vf", "fps=${fps},fade=t=in:st=0:d=0.3,fade=t=out:st=2.4:d=0.4", "-c:v", "libx264", "-pix_fmt", "yuv420p", "-r", "$fps"); $parts += $seg

$list = "$tmp\concat.txt"
($parts | ForEach-Object { "file '" + ((Resolve-Path $_).Path -replace '\\', '/') + "'" }) | Set-Content $list -Encoding ascii
$silent = if ($Music) { "$tmp\silent.mp4" } else { $Out }
Run $silent @("-f", "concat", "-safe", "0", "-i", $list, "-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "17", "-movflags", "+faststart")
if ($Music) { powershell -NoProfile -ExecutionPolicy Bypass -File promo\add-music.ps1 -In $silent -Out $Out -Music $Music; if ($LASTEXITCODE -ne 0) { throw "add-music failed" } }
Write-Host "wrote $Out (${W}x${H})"
