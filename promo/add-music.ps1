# Adds a soundtrack to a promo video.
#   -Music <file>   use your own track (e.g. downloaded from the YouTube Audio Library), trimmed and
#                   faded to the video length
#   (no -Music)     synthesize an original ambient pad with ffmpeg: a slow I-V-vi-IV progression of
#                   soft detuned sines with a sub bass, plus a gentle noise swell. Royalty-free by
#                   construction, so it can ship with the repo and the video.
#
#   powershell -ExecutionPolicy Bypass -File promo\add-music.ps1 -In promo\out\winbend-keynote.mp4
#   powershell -ExecutionPolicy Bypass -File promo\add-music.ps1 -In promo\out\winbend-short.mp4 -Music C:\music\track.mp3
param(
    [Parameter(Mandatory = $true)][string]$In,
    [string]$Out,
    [string]$Music,
    [double]$Volume = 2.0   # bed lands around -22 dB mean; YouTube normalizes loudness anyway
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $root
$ffmpeg = (Get-Command ffmpeg -ErrorAction Stop).Source
$ffprobe = (Get-Command ffprobe -ErrorAction Stop).Source
if (-not $Out) { $Out = [IO.Path]::ChangeExtension($In, $null).TrimEnd('.') + "-music.mp4" }
$dur = [double](& $ffprobe -v error -show_entries format=duration -of default=nw=1:nk=1 $In)
$fadeOut = [Math]::Max(0.5, $dur - 1.8)

if ($Music) {
    # A real track is already mastered: keep it at unity unless -Volume was given explicitly.
    if (-not $PSBoundParameters.ContainsKey('Volume')) { $Volume = 1.0 }
    $af = "atrim=0:${dur},afade=t=in:st=0:d=1.2,afade=t=out:st=${fadeOut}:d=1.8,volume=${Volume},alimiter=limit=0.95"
    & $ffmpeg -y -loglevel error -i $In -stream_loop -1 -i $Music -filter_complex "[1:a]${af}[a]" -map 0:v -map "[a]" -c:v copy -c:a aac -b:a 192k -shortest $Out
    if ($LASTEXITCODE -ne 0) { throw "ffmpeg failed" }
    Write-Host "wrote $Out (your track)"
    exit 0
}

# ---- synthesized bed -------------------------------------------------------------------------
# Chords as (bass, root, third, fifth) in Hz. C major I-V-vi-IV, repeated to cover the video.
$chords = @(
    @(130.81, 261.63, 329.63, 392.00),  # C
    @( 98.00, 196.00, 246.94, 293.66),  # G
    @(110.00, 220.00, 261.63, 329.63),  # Am
    @( 87.31, 174.61, 220.00, 261.63)   # F
)
$chordLen = 3.6
$count = [int][Math]::Ceiling($dur / $chordLen)
$terms = @()
for ($i = 0; $i -lt $count; $i++) {
    $c = $chords[$i % 4]
    $a = [Math]::Round($i * $chordLen, 3); $b = [Math]::Round(($i + 1) * $chordLen + 0.4, 3)   # slight overlap
    # soft envelope: 0.9 s attack, 1.2 s release
    $env = "between(t,$a,$b)*min(1,(t-$a)/0.9)*min(1,($b-t)/1.2)"
    $voices = @()
    $voices += "0.16*sin(2*PI*$($c[0])*t)"                                    # sub bass
    foreach ($f in $c[1..3]) {
        $voices += "0.07*(sin(2*PI*$f*t)+0.6*sin(2*PI*($f+1.7)*t)+0.25*sin(2*PI*2*$f*t))"   # detuned pad
    }
    $terms += "($env)*(" + ($voices -join "+") + ")"
}
$expr = ($terms -join "+")
# A very quiet breath of filtered noise that swells with each chord gives the pad some air.
$fc = "aevalsrc='${expr}':s=48000:d=${dur}[pad];" +
      "anoisesrc=color=pink:r=48000:d=${dur}:a=0.03,lowpass=f=900,volume='0.5+0.5*sin(2*PI*t/${chordLen}-PI/2)':eval=frame[air];" +
      "[pad][air]amix=inputs=2:weights='1 0.6':normalize=0,aecho=0.6:0.5:180|340:0.25|0.15,lowpass=f=5200," +
      "afade=t=in:st=0:d=1.5,afade=t=out:st=${fadeOut}:d=1.8,volume=${Volume},alimiter=limit=0.9[a]"
& $ffmpeg -y -loglevel error -i $In -filter_complex $fc -map 0:v -map "[a]" -c:v copy -c:a aac -b:a 192k -shortest $Out
if ($LASTEXITCODE -ne 0) { throw "ffmpeg failed" }
Write-Host "wrote $Out (synthesized ambient bed, ${dur}s)"
