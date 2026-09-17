param(
    [string]$Binary = (Join-Path $PSScriptRoot "..\target\release\spindle.exe")
)

$Binary = [IO.Path]::GetFullPath($Binary)
if (-not (Test-Path -LiteralPath $Binary -PathType Leaf)) {
    throw "Binary not found: $Binary"
}

$root = Join-Path ([IO.Path]::GetTempPath()) ("spindle-installer-smoke-" + [guid]::NewGuid())
$bin = Join-Path $root "bin"
$state = Join-Path $root "state"
$previousLocalAppData = $env:LOCALAPPDATA
New-Item -ItemType Directory -Path $root | Out-Null

try {
    powershell.exe -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "..\scripts\install.ps1") -Binary $Binary -Destination $bin
    $installed = Join-Path $bin "spindle.exe"
    if (-not (Test-Path -LiteralPath $installed -PathType Leaf)) {
        throw "Installer did not create $installed"
    }

    $env:LOCALAPPDATA = $state
    & $installed help
    if ($LASTEXITCODE -ne 0) { throw "installed help failed" }
    & $installed start
    if ($LASTEXITCODE -ne 0) { throw "installed start failed" }
    & $installed doctor
    if ($LASTEXITCODE -ne 0) { throw "installed doctor failed" }
    & $installed stop
    if ($LASTEXITCODE -ne 0) { throw "installed stop failed" }
} finally {
    if (Test-Path -LiteralPath $installed) {
        try { & $installed stop *> $null } catch { }
    }
    if ($null -eq $previousLocalAppData) {
        Remove-Item Env:LOCALAPPDATA -ErrorAction SilentlyContinue
    } else {
        $env:LOCALAPPDATA = $previousLocalAppData
    }
    if (Test-Path -LiteralPath $root) {
        Remove-Item -LiteralPath $root -Recurse -Force
    }
}
