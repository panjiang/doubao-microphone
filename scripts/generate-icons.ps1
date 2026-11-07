param([string]$OutputDirectory)
# Original vector artwork. Windows PowerShell + System.Drawing only; no downloads.
$ErrorActionPreference = 'Stop'
if (!$OutputDirectory) { $OutputDirectory = Join-Path (Split-Path -Parent $PSScriptRoot) 'assets\icons' }
New-Item -ItemType Directory -Force $OutputDirectory | Out-Null
Add-Type -AssemblyName System.Drawing

$svg = @'
<svg xmlns="http://www.w3.org/2000/svg" width="256" height="256" viewBox="0 0 64 64">
  <circle cx="32" cy="32" r="32" fill="#234bc6"/>
  <g transform="translate(0 -2.5)">
  <rect x="24" y="13" width="16" height="27" rx="8" fill="#fff"/>
  <path d="M19 30v4c0 7.18 5.82 13 13 13s13-5.82 13-13v-4M32 47v7M25 54h14" fill="none" stroke="#fff" stroke-width="4" stroke-linecap="round" stroke-linejoin="round"/>
  </g>
</svg>
'@
[IO.File]::WriteAllText((Join-Path $OutputDirectory 'doubao-microphone.svg'), $svg+"`n", [Text.UTF8Encoding]::new($false))
$traySvg=$svg
[IO.File]::WriteAllText((Join-Path $OutputDirectory 'tray.svg'), $traySvg+"`n", [Text.UTF8Encoding]::new($false))

function Render-Icon([int]$Size) {
    # Render at 4x resolution for smooth curves at small shell/tray sizes.
    $large = [Drawing.Bitmap]::new($Size*4, $Size*4, [Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $graphics = [Drawing.Graphics]::FromImage($large)
    $mic = [Drawing.Drawing2D.GraphicsPath]::new()
    $support = [Drawing.Drawing2D.GraphicsPath]::new()
    $blue = [Drawing.SolidBrush]::new([Drawing.ColorTranslator]::FromHtml('#234bc6'))
    $pen = [Drawing.Pen]::new([Drawing.Color]::White, 4)
    try {
        $graphics.Clear([Drawing.Color]::Transparent)
        $graphics.SmoothingMode = [Drawing.Drawing2D.SmoothingMode]::AntiAlias
        $graphics.ScaleTransform($Size*4/64.0, $Size*4/64.0)
        # Fill the entire available diameter; padding belongs inside the blue circle.
        $graphics.FillEllipse($blue,[single]0,[single]0,[single]64,[single]64)
        $graphics.TranslateTransform(0,[single]-2.5)
        $mic.AddArc(24,13,16,16,180,180)
        $mic.AddArc(24,24,16,16,0,180)
        $mic.CloseFigure()
        $graphics.FillPath([Drawing.Brushes]::White,$mic)
        $pen.StartCap = [Drawing.Drawing2D.LineCap]::Round
        $pen.EndCap = [Drawing.Drawing2D.LineCap]::Round
        $pen.LineJoin = [Drawing.Drawing2D.LineJoin]::Round
        $support.AddLine(19,30,19,34)
        $support.AddBezier(19,34,19,41.18,24.82,47,32,47)
        $support.AddBezier(32,47,39.18,47,45,41.18,45,34)
        $support.AddLine(45,34,45,30)
        $graphics.DrawPath($pen,$support)
        $graphics.DrawLine($pen,32,47,32,54)
        $graphics.DrawLine($pen,25,54,39,54)
        $bitmap = [Drawing.Bitmap]::new($Size,$Size,[Drawing.Imaging.PixelFormat]::Format32bppArgb)
        $resize = [Drawing.Graphics]::FromImage($bitmap)
        try {
            $resize.CompositingMode = [Drawing.Drawing2D.CompositingMode]::SourceCopy
            $resize.InterpolationMode = [Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
            $resize.PixelOffsetMode = [Drawing.Drawing2D.PixelOffsetMode]::HighQuality
            $resize.DrawImage($large,[Drawing.Rectangle]::new(0,0,$Size,$Size))
        } finally { $resize.Dispose() }
        return $bitmap
    } finally {
        $pen.Dispose(); $blue.Dispose(); $mic.Dispose()
        $support.Dispose(); $graphics.Dispose(); $large.Dispose()
    }
}

$sizes = @(16,24,32,48,64,128,256)
$frames = @()
foreach ($size in $sizes) {
    $bitmap = Render-Icon $size
    $stream = [IO.MemoryStream]::new()
    try {
        $bitmap.Save($stream,[Drawing.Imaging.ImageFormat]::Png)
        $frames += ,($stream.ToArray())
        if ($size -eq 256) { [IO.File]::WriteAllBytes((Join-Path $OutputDirectory 'doubao-microphone.png'),$stream.ToArray()) }
    } finally { $stream.Dispose(); $bitmap.Dispose() }
}

foreach ($size in @(16,20,24,28,32,40,48,64,128)) {
    $bitmap = Render-Icon $size
    try {
        if ($size -eq 32) { $bitmap.Save((Join-Path $OutputDirectory 'tray-32.png'),[Drawing.Imaging.ImageFormat]::Png) }
        $rgba = [byte[]]::new($size*$size*4)
        for ($y=0; $y -lt $size; $y++) {
            for ($x=0; $x -lt $size; $x++) {
                $pixel=$bitmap.GetPixel($x,$y); $offset=($y*$size+$x)*4
                $rgba[$offset]=$pixel.R; $rgba[$offset+1]=$pixel.G
                $rgba[$offset+2]=$pixel.B; $rgba[$offset+3]=$pixel.A
            }
        }
        [IO.File]::WriteAllBytes((Join-Path $OutputDirectory ('tray-'+$size+'.rgba')),$rgba)
    } finally { $bitmap.Dispose() }
}

$file = [IO.File]::Create((Join-Path $OutputDirectory 'doubao-microphone.ico'))
$writer = [IO.BinaryWriter]::new($file)
try {
    $writer.Write([uint16]0); $writer.Write([uint16]1); $writer.Write([uint16]$sizes.Count)
    $offset=6+16*$sizes.Count
    for ($i=0; $i -lt $sizes.Count; $i++) {
        $dimension=if ($sizes[$i] -eq 256) {0} else {$sizes[$i]}
        $writer.Write([byte]$dimension); $writer.Write([byte]$dimension)
        $writer.Write([byte]0); $writer.Write([byte]0)
        $writer.Write([uint16]1); $writer.Write([uint16]32)
        $writer.Write([uint32]$frames[$i].Length); $writer.Write([uint32]$offset)
        $offset += $frames[$i].Length
    }
    foreach ($frame in $frames) { $writer.Write([byte[]]$frame) }
} finally { $writer.Dispose() }
Write-Host ('Generated original SVG, PNG, seven-size ICO and DPI-specific tray RGBA in '+$OutputDirectory)
