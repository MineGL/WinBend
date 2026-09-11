# "Keynote" cut: a parody of the foldable-phone reveal ads. Black-and-white typography beats,
# one-word captions timed to the fold, slow motion on the Origami crease, a four-finishes grid
# and the price reveal ($0). Reuses the frames rendered by make-short.ps1 (run that first, with
# -Screen if you want your real desktop).
#
#   powershell -ExecutionPolicy Bypass -File promo\make-parody.ps1
param([string]$Out = "promo\out\winbend-keynote.mp4")
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $root
$ffmpeg = (Get-Command ffmpeg -ErrorAction Stop).Source
$work = "promo\out"
$tmp = "$work\keynote"
New-Item -ItemType Directory -Force $tmp | Out-Null
foreach ($s in "silk", "shade", "frost", "origami") {
    if (-not (Test-Path "$work\$s\0095.png")) { throw "frames missing: run promo\make-short.ps1 first" }
}

$fps = 30
$black = "0x000000"; $white = "0xffffff"
$light = "C\:/Windows/Fonts/segoeuil.ttf"     # thin display face, keynote feel
$semi  = "C\:/Windows/Fonts/seguisb.ttf"
$bold  = "C\:/Windows/Fonts/segoeuib.ttf"

function Esc($text) { $text -replace "\\", "\\\\" -replace "'", "\\'" -replace ":", "\\:" -replace ",", "\\," -replace "%", "\\%" }
function Draw($text, $font, $size, $color, $y, $extra = "") {
    "drawtext=fontfile='${font}':text='$(Esc $text)':fontsize=${size}:fontcolor=${color}:x=(w-text_w)/2:y=${y}${extra}"
}
function Run($outFile, $ffArgs) {
    & $ffmpeg -y -loglevel error @ffArgs $outFile
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path $outFile)) { throw "ffmpeg failed for $outFile (exit $LASTEXITCODE)" }
}
$parts = @()
$n = 0
function Next($name) { $script:n++; "$tmp\{0:d2}-$name.mp4" -f $script:n }

# A typography card: text lines rendered once, then a slow push-in (zoompan) with fades.
# $lines = @(@(text, font, size, color, y), ...)
function Card($name, $lines, $seconds, $bg, $zoom = 0.05) {
    $png = "$tmp\$name.png"
    # PowerShell flattens @(@(...)) into the inner array; re-wrap a single line.
    if ($lines.Count -gt 0 -and -not ($lines[0] -is [array])) { $lines = ,$lines }
    $draws = @()
    foreach ($l in $lines) { $draws += Draw $l[0] $l[1] $l[2] $l[3] $l[4] }
    $vf = $draws -join ","
    Run $png @("-f", "lavfi", "-i", "color=c=${bg}:s=1080x1920:r=1:d=1", "-vf", $vf, "-frames:v", "1", "-update", "1")
    $d = [int]($seconds * $fps)
    $seg = Next $name
    $vf = "zoompan=z='1+${zoom}*on/${d}':d=${d}:x='iw/2-(iw/zoom/2)':y='ih/2-(ih/zoom/2)':s=1080x1920:fps=${fps},fade=t=in:st=0:d=0.35,fade=t=out:st=$($seconds - 0.35):d=0.35"
    Run $seg @("-i", $png, "-vf", $vf, "-c:v", "libx264", "-pix_fmt", "yuv420p", "-r", "$fps", "-t", "$seconds")
    $script:parts += $seg
}

# A fold clip: frames zoomed to fill the width, slow push-in, optional captions with time windows.
# $caps = @(@(text, from, to), ...)
function Fold($name, $style, $seconds, $caps, $slow = 1.0, $trim = 0) {
    $seg = Next $name
    $vf = ""
    if ($trim -gt 0) { $vf += "trim=duration=${trim}," }
    if ($slow -ne 1.0) { $vf += "setpts=${slow}*PTS," }
    $d = [int]($seconds * $fps)
    $vf += "scale=1500:-2,crop=1080:ih:(iw-1080)/2:0,pad=1080:1920:(ow-iw)/2:(oh-ih)/2:color=${black}," +
           "zoompan=z='1+0.06*on/${d}':d=1:x='iw/2-(iw/zoom/2)':y='ih/2-(ih/zoom/2)':s=1080x1920:fps=${fps}"
    if ($caps.Count -gt 0 -and -not ($caps[0] -is [array])) { $caps = ,$caps }
    foreach ($c in $caps) {
        $vf += "," + (Draw $c[0] $light 110 $white 430 ":enable='between(t\,$($c[1])\,$($c[2]))':alpha='min(1\,(t-$($c[1]))/0.3)'")
    }
    $vf += ",fade=t=in:st=0:d=0.3,fade=t=out:st=$($seconds - 0.3):d=0.3"
    Run $seg @("-framerate", "$fps", "-i", "$work\$style\%04d.png", "-vf", $vf, "-c:v", "libx264", "-pix_fmt", "yuv420p", "-r", "$fps", "-t", "$seconds")
    $script:parts += $seg
}

# ---- the cut -------------------------------------------------------------------------------
Card "introducing" @(@("Introducing", $light, 78, $white, 900)) 2.0 $black
Card "winbend"     @(@("WinBend.", $bold, 150, $white, 860)) 2.2 $black 0.08
Fold "silk" "silk" 3.2 @(@("Fold.", 0.35, 1.75), @("Unfold.", 1.95, 3.2))
Card "bends"       @(@("It bends.", $light, 120, $white, 880)) 1.7 $black
Card "nohinge"     @(@("No hinge required.", $light, 84, $white, 900)) 1.9 $black
# Origami: the closing half at half speed so the creases read, then a caption.
Fold "origami-slow" "origami" 3.0 @(@("Origami.", 0.4, 3.0)) 2.0 1.45
Card "paper"       @(@("Creases like paper.", $light, 84, $white, 860), @("Unfolds like nothing happened.", $light, 54, "0x9a9a9a", 990)) 2.4 $black
# Four finishes: 2x2 grid of all styles playing at once (white card, then the grid).
Card "finishes"    @(@("Four finishes.", $light, 110, $black, 880)) 1.6 $white
$grid = Next "grid"
$cell = "scale=540:-2,setsar=1"
$fc = "[0:v]${cell}[a];[1:v]${cell}[b];[2:v]${cell}[c];[3:v]${cell}[d];[a][b]hstack[ab];[c][d]hstack[cd];[ab][cd]vstack[g];" +
      "[g]pad=1080:1920:0:(oh-ih)/2:color=${black}," +
      (Draw "Silk" $semi 40 $white 640) + "," +
      "drawtext=fontfile='${semi}':text='Shade':fontsize=40:fontcolor=${white}:x=540+(540-text_w)/2:y=640," +
      "drawtext=fontfile='${semi}':text='Frost':fontsize=40:fontcolor=${white}:x=(540-text_w)/2:y=1260," +
      "drawtext=fontfile='${semi}':text='Origami':fontsize=40:fontcolor=${white}:x=540+(540-text_w)/2:y=1260," +
      "fade=t=in:st=0:d=0.3,fade=t=out:st=2.9:d=0.3[v]"
Run $grid @("-framerate", "$fps", "-i", "$work\silk\%04d.png", "-framerate", "$fps", "-i", "$work\shade\%04d.png", "-framerate", "$fps", "-i", "$work\frost\%04d.png", "-framerate", "$fps", "-i", "$work\origami\%04d.png", "-filter_complex", $fc, "-map", "[v]", "-c:v", "libx264", "-pix_fmt", "yuv420p", "-r", "$fps", "-t", "3.2")
$parts += $grid
# Price reveal.
Card "starting"    @(@("Starting at", $light, 72, $white, 900)) 1.4 $black
Card "price"       @(@('$0.', $bold, 220, $white, 800), @("Free and open source.", $light, 54, "0x9a9a9a", 1080)) 2.4 $black 0.08
Card "available"   @(@("Available now.", $light, 100, $white, 840), @("On the laptop you already own.", $light, 54, "0x9a9a9a", 990)) 2.4 $black
Card "site"        @(@("winbend.me", $bold, 120, $white, 820), @("Ctrl+Alt+B to fold   |   follows your lid via webcam", $light, 40, "0x9a9a9a", 1000), @("Windows 10 / 11", $light, 40, "0x6a6a6a", 1070)) 3.2 $black 0.03

$list = "$tmp\concat.txt"
($parts | ForEach-Object { "file '" + ((Resolve-Path $_).Path -replace '\\', '/') + "'" }) | Set-Content $list -Encoding ascii
Run $Out @("-f", "concat", "-safe", "0", "-i", $list, "-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "18", "-movflags", "+faststart")
Write-Host "wrote $Out"
