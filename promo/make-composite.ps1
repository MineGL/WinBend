# Puts the real WinBend fold onto a clip of a laptop with a switched-off (black) screen, filmed
# or generated with Gemini/Veo (see veo-prompts.md, "black screen" prompts).
#
#   powershell -ExecutionPolicy Bypass -File promo\make-composite.ps1 -Clip promo\clips\close-black.mp4
#   ... -Style origami        fold style (default silk)
#   ... -Source x.png         desktop picture for the fold (default: promo\out\desktop.png, or your wallpaper)
#   ... -Threshold 46         how dark counts as "screen" (raise if the screen is not picked up, lower if the desk is)
#   ... -OpenAt 0.12 -CloseAt 0.75   lid closure fractions where the fold starts / is complete
#   ... -Grow 150             how light a connected reflection may be and still count as screen
#   ... -Out promo\clips\close.mp4
#
# The result is a normal clip to hand to make-live.ps1 with -Clips.
param(
    [Parameter(Mandatory = $true)][string]$Clip,
    [string]$Style = "silk",
    [string]$Source = "",
    [int]$Threshold = 46,
    [int]$Grow = 150,
    [double]$CloseAt = 0.75,
    [double]$OpenAt = 0.12,
    [string]$Out = ""
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $root
$ffmpeg = (Get-Command ffmpeg -ErrorAction Stop).Source
$ffprobe = (Get-Command ffprobe -ErrorAction Stop).Source
if (-not (Test-Path $Clip)) { throw "clip not found: $Clip" }
if (-not $Out) { $Out = [IO.Path]::ChangeExtension($Clip, $null).TrimEnd('.') + "-winbend.mp4" }
$exe = "target\release\winbend.exe"
if (-not (Test-Path $exe)) { throw "build first: cargo build --release" }
$tool = "promo\composite\target\release\winbend-composite.exe"
if (-not (Test-Path $tool)) {
    Write-Host "building the compositor..."
    Push-Location promo\composite; cargo build --release -q; Pop-Location
    if ($LASTEXITCODE -ne 0) { throw "compositor build failed" }
}
$work = "promo\out\composite"
if (Test-Path $work) { Remove-Item -Recurse -Force $work }
New-Item -ItemType Directory -Force "$work\video", "$work\fold", "$work\out" | Out-Null

# 1. clip -> frames (keep the clip's own frame rate)
$fpsStr = (& $ffprobe -v error -select_streams v:0 -show_entries stream=r_frame_rate -of default=nw=1:nk=1 $Clip).Trim()
$fps = if ($fpsStr -match '^(\d+)/(\d+)$') { [double]$matches[1] / [double]$matches[2] } else { [double]$fpsStr }
& $ffmpeg -y -loglevel error -i $Clip -vsync 0 "$work\video\%05d.png"
$n = (Get-ChildItem "$work\video\*.png").Count
Write-Host "clip: $n frames at $([math]::Round($fps,2)) fps"

# 2. fold frames with a linear fold amount (frame i = i/(N-1)), so the compositor can pick by lid angle
if (-not $Source) {
    if (Test-Path "promo\out\desktop.png") { $Source = "promo\out\desktop.png" }
    else { powershell -NoProfile -ExecutionPolicy Bypass -File promo\make-desktop.ps1 "promo\out\desktop.png" | Out-Null; $Source = "promo\out\desktop.png" }
}
& $exe --render-clip "$work\fold" --style $Style --frames 120 --linear --source $Source | Out-Host
if ($LASTEXITCODE -ne 0) { throw "fold render failed" }

# 3. track + composite
& $tool --video "$work\video" --fold "$work\fold" --out "$work\out" --threshold $Threshold --grow $Grow --close-at $CloseAt --open-at $OpenAt
if ($LASTEXITCODE -ne 0) { throw "compositor failed" }

# 4. frames -> clip (video only; make-live.ps1 adds the soundtrack)
& $ffmpeg -y -loglevel error -framerate $fps -i "$work\out\%05d.png" -c:v libx264 -pix_fmt yuv420p -crf 16 $Out
if ($LASTEXITCODE -ne 0) { throw "encode failed" }
Write-Host "wrote $Out"
