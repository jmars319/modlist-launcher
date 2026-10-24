# Draws the app icon (stacked cards with a play glyph) and writes
# assets/icon-256.png plus a multi-size assets/app.ico. Windows only.
# assets/icon.svg is the same drawing as a vector source, for sizes above
# 256 px and for brand use; change both together.
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

$assets = Join-Path $PSScriptRoot '..\assets'
New-Item -ItemType Directory -Force $assets | Out-Null

function New-RoundedRect([float]$x, [float]$y, [float]$w, [float]$h, [float]$r) {
    $p = New-Object System.Drawing.Drawing2D.GraphicsPath
    $d = $r * 2
    $p.AddArc($x, $y, $d, $d, 180, 90)
    $p.AddArc($x + $w - $d, $y, $d, $d, 270, 90)
    $p.AddArc($x + $w - $d, $y + $h - $d, $d, $d, 0, 90)
    $p.AddArc($x, $y + $h - $d, $d, $d, 90, 90)
    $p.CloseFigure()
    $p
}

function Draw-Icon([int]$size) {
    $bmp = New-Object System.Drawing.Bitmap $size, $size, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = 'AntiAlias'
    $g.PixelOffsetMode = 'HighQuality'
    $g.Clear([System.Drawing.Color]::Transparent)
    $s = $size / 256.0

    # Vaexil: the #05070d canvas with the mark's blue-to-magenta.
    $blue = [System.Drawing.Color]::FromArgb(255, 47, 123, 255)
    $magenta = [System.Drawing.Color]::FromArgb(255, 255, 63, 216)
    $diag = { param($a, $b) New-Object System.Drawing.Drawing2D.LinearGradientBrush (
        (New-Object System.Drawing.PointF 0, 0), (New-Object System.Drawing.PointF $size, $size), $a, $b) }

    $bg = New-RoundedRect (8 * $s) (8 * $s) (240 * $s) (240 * $s) (54 * $s)
    $g.FillPath((& $diag ([System.Drawing.Color]::FromArgb(255, 11, 22, 40)) ([System.Drawing.Color]::FromArgb(255, 5, 7, 13))), $bg)
    $edge = New-Object System.Drawing.Pen ((& $diag $blue $magenta), [Math]::Max(1, 5 * $s))
    $g.DrawPath($edge, $bg)

    $cards = @(
        @{ X = 78; Y = 52; A = 90 },
        @{ X = 64; Y = 70; A = 160 },
        @{ X = 50; Y = 88; A = 255 }
    )
    foreach ($c in $cards) {
        $rect = New-RoundedRect ($c.X * $s) ($c.Y * $s) (128 * $s) (116 * $s) (18 * $s)
        $tint = if ($c.A -eq 255) { [System.Drawing.Color]::FromArgb(255, 248, 250, 252) }
                else { [System.Drawing.Color]::FromArgb($c.A, 103, 232, 249) }
        $g.FillPath((New-Object System.Drawing.SolidBrush $tint), $rect)
    }

    $tri = New-Object System.Drawing.Drawing2D.GraphicsPath
    $tri.AddPolygon([System.Drawing.PointF[]]@(
        (New-Object System.Drawing.PointF (98 * $s), (118 * $s)),
        (New-Object System.Drawing.PointF (98 * $s), (174 * $s)),
        (New-Object System.Drawing.PointF (146 * $s), (146 * $s))
    ))
    $g.FillPath((New-Object System.Drawing.Drawing2D.LinearGradientBrush (
        (New-Object System.Drawing.PointF (94 * $s), (118 * $s)),
        (New-Object System.Drawing.PointF (150 * $s), (174 * $s)), $blue, $magenta)), $tri)
    $g.Dispose()
    $bmp
}

$sizes = 16, 24, 32, 48, 64, 128, 256
$pngs = @{}
foreach ($size in $sizes) {
    $bmp = Draw-Icon $size
    $ms = New-Object System.IO.MemoryStream
    $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
    $pngs[$size] = $ms.ToArray()
    if ($size -eq 256) { $bmp.Save((Join-Path $assets 'icon-256.png'), [System.Drawing.Imaging.ImageFormat]::Png) }
    $bmp.Dispose()
}

# ICO container with PNG-compressed entries.
$out = New-Object System.IO.MemoryStream
$w = New-Object System.IO.BinaryWriter $out
$w.Write([uint16]0); $w.Write([uint16]1); $w.Write([uint16]$sizes.Count)
$offset = 6 + 16 * $sizes.Count
foreach ($size in $sizes) {
    $dim = if ($size -ge 256) { 0 } else { $size }
    $w.Write([byte]$dim); $w.Write([byte]$dim); $w.Write([byte]0); $w.Write([byte]0)
    $w.Write([uint16]1); $w.Write([uint16]32)
    $w.Write([uint32]$pngs[$size].Length); $w.Write([uint32]$offset)
    $offset += $pngs[$size].Length
}
foreach ($size in $sizes) { $w.Write($pngs[$size]) }
$w.Flush()
[System.IO.File]::WriteAllBytes((Join-Path $assets 'app.ico'), $out.ToArray())
Write-Output "Wrote $assets\icon-256.png and app.ico"
