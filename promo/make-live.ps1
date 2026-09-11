# "How it actually works" cut: real-world lid shots (filmed, or generated with Gemini/Veo, see
# veo-prompts.md) intercut with the real engine render and a call to action. 1080x1920.
#
#   powershell -ExecutionPolicy Bypass -File promo\make-live.ps1 -Clips promo\clips\close.mp4,promo\clips\open.mp4
#   ... -Music <track.mp3>     add a soundtrack (handled by add-music.ps1)
#   ... -Fill                  crop landscape clips to fill the frame instead of letterboxing
#   ... -ClipSeconds 5         seconds to keep from each clip (default 6)
param(
    [Parameter(Mandatory = $true)][string[]]$Clips,
    [string[]]$Captions = @("The lid closes.|The lid opens.|From any angle."),   # one per clip, separated by |
    [string]$Music,
    [switch]$Fill,
    [double]$ClipSeconds = 6,
    [string]$Out = "promo\out\winbend-live.mp4"
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $root
$ffmpeg = (Get-Command ffmpeg -ErrorAction Stop).Source
$ffprobe = (Get-Command ffprobe -ErrorAction Stop).Source
# `powershell -File x.ps1 -Clips a,b` hands us one string; split it so both spellings work.
$Clips = @($Clips | ForEach-Object { $_ -split ',' } | ForEach-Object { $_.Trim() } | Where-Object { $_ })
$Captions = @($Captions | ForEach-Object { $_ -split '\|' } | ForEach-Object { $_.Trim() })
$work = "promo\out"; $tmp = "$work\live"
New-Item -ItemType Directory -Force $tmp | Out-Null
if (-not (Test-Path "$work\silk\0095.png")) { throw "fold frames missing: run promo\make-short.ps1 first" }

$fps = 30
$black = "0x000000"; $white = "0xffffff"
$light = "C\:/Windows/Fonts/segoeuil.ttf"; $bold = "C\:/Windows/Fonts/segoeuib.ttf"
function Esc($text) { $text -replace "\\", "\\\\" -replace "'", "\\'" -replace ":", "\\:" -replace ",", "\\," -replace "%", "\\%" }
function Draw($text, $font, $size, $color, $y, $extra = "") { "drawtext=fontfile='${font}':text='$(Esc $text)':fontsize=${size}:fontcolor=${color}:x=(w-text_w)/2:y=${y}${extra}" }
function Run($outFile, $ffArgs) {
    & $ffmpeg -y -loglevel error @ffArgs $outFile
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path $outFile)) { throw "ffmpeg failed for $outFile (exit $LASTEXITCODE)" }
}
$parts = @(); $n = 0
function Next($name) { $script:n++; "$tmp\{0:d2}-$name.mp4" -f $script:n }
function Card($name, $lines, $seconds, $bg, $zoom = 0.05) {
    if ($lines.Count -gt 0 -and -not ($lines[0] -is [array])) { $lines = ,$lines }
    $draws = @(); foreach ($l in $lines) { $draws += Draw $l[0] $l[1] $l[2] $l[3] $l[4] }
    $png = "$tmp\$name.png"
    Run $png @("-f", "lavfi", "-i", "color=c=${bg}:s=1080x1920:r=1:d=1", "-vf", ($draws -join ","), "-frames:v", "1", "-update", "1")
    $d = [int]($seconds * $fps); $seg = Next $name
    $vf = "zoompan=z='1+${zoom}*on/${d}':d=${d}:x='iw/2-(iw/zoom/2)':y='ih/2-(ih/zoom/2)':s=1080x1920:fps=${fps},fade=t=in:st=0:d=0.35,fade=t=out:st=$($seconds - 0.35):d=0.35"
    Run $seg @("-i", $png, "-vf", $vf, "-c:v", "libx264", "-pix_fmt", "yuv420p", "-r", "$fps", "-t", "$seconds")
    $script:parts += $seg
}

# ---- the cut -------------------------------------------------------------------------------
Card "title" @(@("This is what happens", $light, 84, $white, 840), @("when you close the lid.", $light, 84, $white, 940)) 2.4 $black

# Real-world clips, normalized to 1080x1920 with a caption for the first second and a half.
$i = 0
foreach ($clip in $Clips) {
    if (-not (Test-Path $clip)) { throw "clip not found: $clip" }
    $len = [double](& $ffprobe -v error -show_entries format=duration -of default=nw=1:nk=1 $clip)
    $keep = [Math]::Min($ClipSeconds, $len)
    $seg = Next "clip$i"
    if ($Fill) { $fit = "scale=1080:1920:force_original_aspect_ratio=increase,crop=1080:1920" }
    else       { $fit = "scale=1080:1920:force_original_aspect_ratio=decrease,pad=1080:1920:(ow-iw)/2:(oh-ih)/2:color=${black}" }
    $cap = if ($i -lt $Captions.Count) { $Captions[$i] } else { "" }
    $vf = "${fit},fps=${fps},setsar=1"
    if ($cap) { $vf += "," + (Draw $cap $light 96 $white 260 ":enable='between(t\,0.3\,2.2)':alpha='min(1\,(t-0.3)/0.3)*min(1\,(2.2-t)/0.3)'") }
    $vf += ",fade=t=in:st=0:d=0.3,fade=t=out:st=$($keep - 0.3):d=0.3"
    Run $seg @("-i", $clip, "-t", "$keep", "-vf", $vf, "-an", "-c:v", "libx264", "-pix_fmt", "yuv420p", "-r", "$fps")
    $parts += $seg
    $i++
}

# The real thing: the engine render, so viewers see the exact effect the app produces.
Card "real" @(@("Rendered live", $light, 96, $white, 840), @("from your own desktop.", $light, 60, "0x9a9a9a", 960)) 2.0 $black
$seg = Next "engine"
$vf = "scale=1500:-2,crop=1080:ih:(iw-1080)/2:0,pad=1080:1920:(ow-iw)/2:(oh-ih)/2:color=${black}," +
      "zoompan=z='1+0.06*on/96':d=1:x='iw/2-(iw/zoom/2)':y='ih/2-(ih/zoom/2)':s=1080x1920:fps=${fps}," +
      (Draw "GPU rendered  |  60 fps  |  nothing uploaded" $light 40 "0x9a9a9a" 1460) + ",fade=t=in:st=0:d=0.3,fade=t=out:st=2.9:d=0.3"
Run $seg @("-framerate", "$fps", "-i", "$work\silk\%04d.png", "-vf", $vf, "-c:v", "libx264", "-pix_fmt", "yuv420p", "-r", "$fps", "-t", "3.2")
$parts += $seg

Card "how" @(@("Follows your lid", $light, 84, $white, 800), @("through the webcam.", $light, 84, $white, 900), @("Or press Ctrl+Alt+B.", $light, 54, "0x9a9a9a", 1040)) 2.6 $black
Card "cta" @(@("winbend.me", $bold, 120, $white, 820), @("Free and open source  |  Windows 10 / 11", $light, 40, "0x9a9a9a", 1000)) 3.0 $black 0.03

$list = "$tmp\concat.txt"
($parts | ForEach-Object { "file '" + ((Resolve-Path $_).Path -replace '\\', '/') + "'" }) | Set-Content $list -Encoding ascii
$silent = if ($Music) { "$tmp\silent.mp4" } else { $Out }
Run $silent @("-f", "concat", "-safe", "0", "-i", $list, "-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "18", "-movflags", "+faststart")
if ($Music) {
    powershell -NoProfile -ExecutionPolicy Bypass -File promo\add-music.ps1 -In $silent -Out $Out -Music $Music
    if ($LASTEXITCODE -ne 0) { throw "add-music failed" }
}
Write-Host "wrote $Out"
