# HOI4 Live Tools OCR host: Windows.Media.Ocr behind a line protocol on stdin/stdout.
# Request  (one JSON line):  {"id": 1, "path": "C:\\...\\crop.png", "lang": "en-US"}
# Response (one JSON line):  {"id": 1, "ok": true, "angle": 0, "lines": [{"text": "...", "words": [{"t": "..", "x": 0, "y": 0, "w": 0, "h": 0}]}]}
# A line "quit" or end of input stops the host. Errors come back as {"id":..,"ok":false,"error":".."}.
$ErrorActionPreference = 'Stop'
[Console]::InputEncoding = New-Object System.Text.UTF8Encoding($false)
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false)
Add-Type -AssemblyName System.Runtime.WindowsRuntime
$null = [Windows.Storage.StorageFile, Windows.Storage, ContentType = WindowsRuntime]
$null = [Windows.Storage.Streams.IRandomAccessStream, Windows.Storage.Streams, ContentType = WindowsRuntime]
$null = [Windows.Media.Ocr.OcrEngine, Windows.Foundation, ContentType = WindowsRuntime]
$null = [Windows.Graphics.Imaging.BitmapDecoder, Windows.Graphics, ContentType = WindowsRuntime]
$null = [Windows.Graphics.Imaging.SoftwareBitmap, Windows.Graphics, ContentType = WindowsRuntime]
$null = [Windows.Globalization.Language, Windows.Globalization, ContentType = WindowsRuntime]

$asTask = [System.WindowsRuntimeSystemExtensions].GetMethods() | Where-Object {
    $_.Name -eq 'AsTask' -and $_.GetParameters().Count -eq 1 -and
    $_.GetParameters()[0].ParameterType.Name -eq 'IAsyncOperation`1' } | Select-Object -First 1

function Await($op, [Type]$type) {
    $task = $asTask.MakeGenericMethod($type).Invoke($null, @($op))
    $null = $task.Wait(-1)
    return $task.Result
}

$engines = @{}
function Get-Engine([string]$lang) {
    if (-not $engines.ContainsKey($lang)) {
        $e = [Windows.Media.Ocr.OcrEngine]::TryCreateFromLanguage([Windows.Globalization.Language]::new($lang))
        if ($e -eq $null) { $e = [Windows.Media.Ocr.OcrEngine]::TryCreateFromUserProfileLanguages() }
        $engines[$lang] = $e
    }
    return $engines[$lang]
}

$out = [Console]::Out
$out.WriteLine((@{ ready = $true; max_dim = [Windows.Media.Ocr.OcrEngine]::MaxImageDimension } | ConvertTo-Json -Compress))
$out.Flush()

while ($true) {
    $line = [Console]::In.ReadLine()
    if ($line -eq $null -or $line -eq 'quit') { break }
    if ($line.Trim().Length -eq 0) { continue }
    $id = $null
    try {
        $req = $line | ConvertFrom-Json
        $id = $req.id
        $lang = if ($req.lang) { [string]$req.lang } else { 'en-US' }
        $engine = Get-Engine $lang
        if ($engine -eq $null) { throw "no OCR engine for language $lang" }
        $file = Await ([Windows.Storage.StorageFile]::GetFileFromPathAsync([string]$req.path)) ([Windows.Storage.StorageFile])
        $stream = Await ($file.OpenAsync([Windows.Storage.FileAccessMode]::Read)) ([Windows.Storage.Streams.IRandomAccessStream])
        try {
            $decoder = Await ([Windows.Graphics.Imaging.BitmapDecoder]::CreateAsync($stream)) ([Windows.Graphics.Imaging.BitmapDecoder])
            $bitmap = Await ($decoder.GetSoftwareBitmapAsync()) ([Windows.Graphics.Imaging.SoftwareBitmap])
            $result = Await ($engine.RecognizeAsync($bitmap)) ([Windows.Media.Ocr.OcrResult])
            $lines = New-Object System.Collections.ArrayList
            foreach ($l in $result.Lines) {
                $words = New-Object System.Collections.ArrayList
                foreach ($w in $l.Words) {
                    $r = $w.BoundingRect
                    $null = $words.Add(@{ t = $w.Text; x = [int]$r.X; y = [int]$r.Y; w = [int]$r.Width; h = [int]$r.Height })
                }
                $null = $lines.Add(@{ text = $l.Text; words = $words })
            }
            $angle = if ($result.TextAngle -ne $null) { [double]$result.TextAngle } else { 0 }
            $resp = @{ id = $id; ok = $true; angle = $angle; lines = $lines }
            $bitmap.Dispose()
        } finally {
            $stream.Dispose()
        }
        $out.WriteLine(($resp | ConvertTo-Json -Compress -Depth 6))
    } catch {
        $out.WriteLine((@{ id = $id; ok = $false; error = $_.Exception.Message } | ConvertTo-Json -Compress))
    }
    $out.Flush()
}
