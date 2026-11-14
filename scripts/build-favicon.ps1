# Rebuilds src/favicon.ico from assets/icons/128x128.png, without redrawing it.
#
#   powershell -File scripts/build-favicon.ps1
#
# The .ico is embedded in the Windows executable and the installer, and the shell asks for a
# different size in each place: 16px in the titlebar, 32px on the taskbar at 100% scale, 48px at
# 150%, and 64px and up in Alt-Tab and the start menu. A single frame gets stretched for all of
# them, so every size is written on its own. Each one is a plain downscale of the project's logo:
# this script only resamples it and never draws a replacement.
param(
    [string]$Source = "assets/icons/128x128.png",
    [string]$Destination = "assets/icons/favicon.ico"
)

Add-Type -AssemblyName System.Drawing

# Nothing larger than the 128px source is written: scaling it up would only make the blur bigger.
$SIZES = @(16, 20, 24, 32, 40, 48, 64, 96, 128)

$root = Split-Path -Parent $PSScriptRoot
$sourcePath = if ([System.IO.Path]::IsPathRooted($Source)) { $Source } else { Join-Path $root $Source }
$destinationPath = if ([System.IO.Path]::IsPathRooted($Destination)) { $Destination } else { Join-Path $root $Destination }

# Its own variable: assigning over the [string] parameter would leave this bound to a string.
$logo = [System.Drawing.Image]::FromFile($sourcePath)
$logoWidth = [int]$logo.Width
$logoHeight = [int]$logo.Height
$largest = ($SIZES | Measure-Object -Maximum).Maximum
if ($logoWidth -ne $logoHeight) {
    $logo.Dispose()
    throw "the logo must be square, but $sourcePath is ${logoWidth}x${logoHeight}"
}
if ($logoWidth -lt $largest) {
    $logo.Dispose()
    throw "the logo is ${logoWidth}px, smaller than the largest frame (${largest}px)"
}

$frames = @()
foreach ($size in $SIZES) {
    $bmp = New-Object System.Drawing.Bitmap([int]$size, [int]$size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
    $g.CompositingQuality = [System.Drawing.Drawing2D.CompositingQuality]::HighQuality
    $g.CompositingMode = [System.Drawing.Drawing2D.CompositingMode]::SourceCopy
    $g.Clear([System.Drawing.Color]::Transparent)
    $g.DrawImage($logo, 0, 0, $size, $size)
    $g.Dispose()

    $stream = New-Object System.IO.MemoryStream
    $bmp.Save($stream, [System.Drawing.Imaging.ImageFormat]::Png)
    $frames += ,@($size, $stream.ToArray())
    $stream.Dispose()
    $bmp.Dispose()
}
$logo.Dispose()

$out = New-Object System.IO.MemoryStream
$w = New-Object System.IO.BinaryWriter($out)
$w.Write([UInt16]0)               # reserved
$w.Write([UInt16]1)               # type: icon
$w.Write([UInt16]$frames.Count)   # image count

$offset = 6 + 16 * $frames.Count
foreach ($frame in $frames) {
    $size = $frame[0]
    $bytes = $frame[1]
    # The width and height fields are one byte each, and nothing above 128 is written here, so
    # no 256-is-written-as-0 trick is needed.
    $dim = [Byte]$size
    $w.Write($dim)                # width
    $w.Write($dim)                # height
    $w.Write([Byte]0)             # colour count: 0 = true colour, no palette
    $w.Write([Byte]0)             # reserved
    $w.Write([UInt16]1)           # colour planes
    $w.Write([UInt16]32)          # bits per pixel
    $w.Write([UInt32]$bytes.Length)
    $w.Write([UInt32]$offset)
    $offset += $bytes.Length
}
foreach ($frame in $frames) { $w.Write($frame[1]) }
$w.Flush()
[System.IO.File]::WriteAllBytes($destinationPath, $out.ToArray())
$w.Dispose()
$out.Dispose()

"wrote $Destination from $Source ($($SIZES -join ', ')px)"
