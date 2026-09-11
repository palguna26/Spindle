param(
    [string]$Binary = (Join-Path $PSScriptRoot "..\target\release\spindle.exe")
)

$Binary = [IO.Path]::GetFullPath($Binary)
if (-not (Test-Path -LiteralPath $Binary)) {
    throw "Binary not found: $Binary"
}

$stateRoot = Join-Path ([IO.Path]::GetTempPath()) ("spindle-smoke-" + [guid]::NewGuid())
$previousLocalAppData = $env:LOCALAPPDATA
New-Item -ItemType Directory -Path $stateRoot | Out-Null

function Invoke-Spindle {
    param([string[]]$Arguments)
    & $Binary @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "spindle $($Arguments -join ' ') failed with exit code $LASTEXITCODE"
    }
}

try {
    $env:LOCALAPPDATA = $stateRoot
    Invoke-Spindle @("help")
    Invoke-Spindle @("start")
    Invoke-Spindle @("start")
    Invoke-Spindle @("list")
    Invoke-Spindle @("doctor")
    $projectState = Get-ChildItem -LiteralPath (Join-Path $stateRoot "Spindle\projects") -Directory |
        Select-Object -First 1
    $logPath = Join-Path $projectState.FullName "server.log"
    if (-not (Test-Path -LiteralPath $logPath -PathType Leaf)) {
        throw "server log was not created"
    }
    $startedLog = Get-Content -LiteralPath $logPath |
        ForEach-Object { $_ | ConvertFrom-Json } |
        Where-Object { $_.event -eq "server_started" }
    if ($null -eq $startedLog) {
        throw "server_started log record was not found"
    }
    Invoke-Spindle @("stop")
    $markers = Get-ChildItem -LiteralPath (Join-Path $stateRoot "Spindle\projects") -Recurse -File -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -in @("server.endpoint", "server.interactive.endpoint", "server.pid", "server.json") }
    if ($markers.Count -ne 0) {
        throw "server markers were not cleaned up"
    }
} finally {
    if ($null -eq $previousLocalAppData) {
        Remove-Item Env:LOCALAPPDATA -ErrorAction SilentlyContinue
    } else {
        $env:LOCALAPPDATA = $previousLocalAppData
    }
    if (Test-Path -LiteralPath $stateRoot) {
        Remove-Item -LiteralPath $stateRoot -Recurse -Force
    }
}
