# HOI4 Live Tools: one-line installer for Windows.
#
#   irm https://github.com/klimPaskov/HOI4-Live-Tools/releases/latest/download/install.ps1 | iex
#
# It finds Python 3.9 or newer, downloads install.py from the release, checks it against the release's
# SHA256SUMS.txt and runs it. install.py then downloads and checks the rest.
#
# HOI4_LIVE_INSTALL_ARGS holds arguments for install.py (for example "--register none").
# HOI4_LIVE_VERSION picks a release other than the latest. HOI4_LIVE_RELEASE_BASE names another release
# location: an https address shaped like the GitHub releases page, or a local folder holding the release files.

function Install-Hoi4LiveTools {
    $ErrorActionPreference = 'Stop'
    $base = if ($env:HOI4_LIVE_RELEASE_BASE) { $env:HOI4_LIVE_RELEASE_BASE } else { 'https://github.com/klimPaskov/HOI4-Live-Tools/releases' }
    $base = $base.TrimEnd('/', '\')
    $version = $env:HOI4_LIVE_VERSION

    $python = $null
    foreach ($candidate in 'py -3', 'python', 'python3') {
        $parts = @($candidate -split ' ')
        $rest = @($parts | Select-Object -Skip 1)
        if (-not (Get-Command $parts[0] -ErrorAction SilentlyContinue)) { continue }
        try {
            & $parts[0] @rest -c 'import sys; sys.exit(0 if sys.version_info >= (3, 9) else 1)' *> $null
            if ($LASTEXITCODE -eq 0) { $python = $parts; break }
        } catch { }
    }
    if (-not $python) {
        Write-Host 'error: Python 3.9 or newer is needed and was not found.'
        Write-Host 'Install it with:  winget install Python.Python.3.13   (or from https://www.python.org/downloads/),'
        Write-Host 'open a new PowerShell window and run this again.'
        $global:LASTEXITCODE = 2
        return
    }

    $tmp = Join-Path ([IO.Path]::GetTempPath()) ('hoi4live.' + [Guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $tmp | Out-Null
    try {
        foreach ($name in @('SHA256SUMS.txt', 'install.py')) {
            $target = Join-Path $tmp $name
            if ($base -like 'https://*') {
                $url = if ($version) { "$base/download/v$version/$name" } else { "$base/latest/download/$name" }
                [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
                Invoke-WebRequest -Uri $url -OutFile $target -UseBasicParsing
            } elseif ($base -like '*://*') {
                Write-Host 'error: HOI4_LIVE_RELEASE_BASE must be an https address or a local folder'
                $global:LASTEXITCODE = 1
                return
            } else {
                Copy-Item -LiteralPath (Join-Path $base $name) -Destination $target
            }
        }
        $expected = $null
        foreach ($line in Get-Content -LiteralPath (Join-Path $tmp 'SHA256SUMS.txt')) {
            $parts = $line -split '\s+'
            if ($parts.Count -eq 2 -and $parts[1].TrimStart('*') -eq 'install.py') { $expected = $parts[0].ToLower() }
        }
        $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $tmp 'install.py')).Hash.ToLower()
        if (-not $expected -or $expected -ne $actual) {
            Write-Host 'error: install.py does not match its checksum in SHA256SUMS.txt; nothing was installed.'
            $global:LASTEXITCODE = 1
            return
        }
        $rest = @($python | Select-Object -Skip 1)
        $extra = @()
        if ($version) { $extra += @('--version', $version) }
        if ($env:HOI4_LIVE_INSTALL_ARGS) { $extra += ($env:HOI4_LIVE_INSTALL_ARGS -split '\s+' | Where-Object { $_ }) }
        & $python[0] @rest (Join-Path $tmp 'install.py') @extra
    } finally {
        Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Install-Hoi4LiveTools
# Run as a file, the script ends with install.py's exit code. Piped into iex it must not close the window.
if ($PSCommandPath) { exit $LASTEXITCODE }
