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
    $workspaces = (& $Binary workspace list) -join "`n"
    if ($LASTEXITCODE -ne 0) { throw "spindle workspace list failed" }
    if ($workspaces -notmatch '(?m)^\*\s+\S+\s+Current project\s+\[Default\]$') {
        throw "workspace list did not mark the active workspace: $workspaces"
    }
    $workspace = (& $Binary workspace get workspace-1) -join "`n"
    if ($LASTEXITCODE -ne 0 -or $workspace -notmatch '(?m)^name: Current project$') {
        throw "workspace get did not return the requested workspace: $workspace"
    }
    Invoke-Spindle @("workspace", "rename", "workspace-1", "Smoke test")
    $renamedWorkspaces = (& $Binary workspace list) -join "`n"
    if ($LASTEXITCODE -ne 0 -or $renamedWorkspaces -notmatch '(?m)^\*\s+workspace-1\s+Smoke test\s+\[Default\]$') {
        throw "workspace rename did not update the workspace label: $renamedWorkspaces"
    }
    Invoke-Spindle @("workspace", "focus", "workspace-1")
    Invoke-Spindle @("list")
    $doctor = (& $Binary doctor) -join "`n"
    if ($LASTEXITCODE -ne 0) { throw "spindle doctor failed" }
    if ($doctor -notmatch "client version: spindle " -or
        $doctor -notmatch ("client binary: " + [regex]::Escape($Binary)) -or
        $doctor -notmatch "client protocol: ") {
        throw "doctor did not identify the running client build: $doctor"
    }
    Write-Output $doctor
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
    $staleEndpoint = Join-Path $projectState.FullName "server.endpoint"
    Set-Content -LiteralPath $staleEndpoint -Value "127.0.0.1:1" -NoNewline
    $staleDoctor = (& $Binary doctor) -join "`n"
    if ($LASTEXITCODE -ne 0) {
        throw "spindle doctor failed for stale endpoint metadata"
    }
    if ($staleDoctor -notmatch "endpoint status: stale" -or
        $staleDoctor -notmatch "run spindle start or spindle attach") {
        throw "doctor did not explain stale endpoint recovery: $staleDoctor"
    }
    Invoke-Spindle @("start")
    Invoke-Spindle @("doctor")
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
