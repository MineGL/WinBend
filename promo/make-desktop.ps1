# Paints a clean 1920x1080 "showcase desktop" PNG so promo renders never expose a real screen.
# Usage: powershell -ExecutionPolicy Bypass -File promo\make-desktop.ps1 [out.png]
param([string]$Out = "promo\out\desktop.png")
Add-Type -AssemblyName System.Drawing
$W = 1920; $H = 1080
$bmp = New-Object System.Drawing.Bitmap $W, $H
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.SmoothingMode = 'AntiAlias'; $g.TextRenderingHint = 'ClearTypeGridFit'

function Color($hex) { [System.Drawing.ColorTranslator]::FromHtml($hex) }
function RoundRect([float]$x, [float]$y, [float]$w, [float]$h, [float]$r) {
    $p = New-Object System.Drawing.Drawing2D.GraphicsPath
    $p.AddArc($x, $y, $r*2, $r*2, 180, 90); $p.AddArc($x+$w-$r*2, $y, $r*2, $r*2, 270, 90)
    $p.AddArc($x+$w-$r*2, $y+$h-$r*2, $r*2, $r*2, 0, 90); $p.AddArc($x, $y+$h-$r*2, $r*2, $r*2, 90, 90)
    $p.CloseFigure(); $p
}

# Wallpaper: deep indigo gradient with two soft light blobs (Windows 11 "Bloom" mood).
$wall = New-Object System.Drawing.Drawing2D.LinearGradientBrush ([System.Drawing.Point]::new(0,0)), ([System.Drawing.Point]::new($W,$H)), (Color '#5a63ff'), (Color '#0e1122')
$g.FillRectangle($wall, 0, 0, $W, $H)
foreach ($blob in @(@(300, 250, 900, '#8f8bff', 110), @(1500, 750, 1100, '#78beff', 70))) {
    $path = New-Object System.Drawing.Drawing2D.GraphicsPath
    $path.AddEllipse($blob[0]-$blob[2]/2, $blob[1]-$blob[2]/2, $blob[2], $blob[2])
    $pb = New-Object System.Drawing.Drawing2D.PathGradientBrush $path
    $c = Color $blob[3]; $pb.CenterColor = [System.Drawing.Color]::FromArgb($blob[4], $c); $pb.SurroundColors = @([System.Drawing.Color]::FromArgb(0, $c))
    $g.FillPath($pb, $path)
}

# Windows: a light "notes" window and a dark "code" window with a title bar and text lines.
function Window([float]$x, [float]$y, [float]$w, [float]$h, $bg, $bar, $line, $title, $lines, $font) {
    $shadow = New-Object System.Drawing.Drawing2D.GraphicsPath; $shadow.AddPath((RoundRect ($x+6) ($y+14) $w $h 14), $false)
    $g.FillPath((New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(70, 0, 0, 0))), $shadow)
    $g.FillPath((New-Object System.Drawing.SolidBrush (Color $bg)), (RoundRect $x $y $w $h 14))
    $g.FillRectangle((New-Object System.Drawing.SolidBrush (Color $bar)), $x, $y+14, $w, 40)
    $g.FillRectangle((New-Object System.Drawing.SolidBrush (Color $bar)), $x, $y, $w, 24)
    $g.DrawString($title, $font, (New-Object System.Drawing.SolidBrush (Color $line)), $x+18, $y+15)
    $i = 0
    foreach ($ln in $lines) {
        $ly = $y + 78 + $i*34
        if ($ln -is [double] -or $ln -is [int]) { $g.FillRectangle((New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(90, (Color $line)))), $x+24, $ly+8, [float]($w*$ln), 12) }
        else { $g.DrawString($ln, $font, (New-Object System.Drawing.SolidBrush (Color $line)), $x+24, $ly) }
        $i++
    }
}
$ui = New-Object System.Drawing.Font 'Segoe UI', 15
$mono = New-Object System.Drawing.Font 'Consolas', 15
Window 150 120 920 620 '#f6f7fb' '#e9ebf2' '#3a4060' 'Notes — Friday' @('Ship WinBend 0.1.1', 0.62, 0.48, '', 'Record the promo short', 0.55, 0.3, '', 'Answer GitHub issues', 0.4) $ui
Window 900 300 870 640 '#1b1f2e' '#242a3d' '#c9d1e8' 'main.rs — winbend' @('fn fold(t: f32) -> Panel {', '    let theta = t * MAX_TILT;', '    Panel::hinged(theta)', '        .bend(0.3)', '        .sheen(0.8)', '}', '', '// close the lid and watch', 0.45, 0.6) $mono

# Taskbar with a centred set of "icons" and a clock.
$g.FillRectangle((New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(215, 16, 18, 30))), 0, $H-56, $W, 56)
$icons = @('#4c8bf5', '#f7c948', '#34c38f', '#e46b6b', '#8f8bff', '#ffffff')
for ($k = 0; $k -lt $icons.Count; $k++) {
    $ix = $W/2 - ($icons.Count*54)/2 + $k*54
    $g.FillPath((New-Object System.Drawing.SolidBrush (Color $icons[$k])), (RoundRect $ix ($H-44) 32 32 8))
}
$g.DrawString('9:41', (New-Object System.Drawing.Font 'Segoe UI', 13), [System.Drawing.Brushes]::White, $W-92, $H-40)

New-Item -ItemType Directory -Force (Split-Path $Out) | Out-Null
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $bmp.Dispose()
Write-Host "wrote $Out"
