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
    $createdWorkspace = ((& $Binary workspace create --label "No focus" --cwd $PWD --env SPINDLE_SMOKE=ok) -join "`n") | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($createdWorkspace.workspace_id)) {
        throw "workspace create did not return a workspace ID: $createdWorkspace"
    }
    $workspacesAfterCreate = (& $Binary workspace list) -join "`n"
    if ($LASTEXITCODE -ne 0 -or $workspacesAfterCreate -notmatch '(?m)^\*\s+workspace-1\s+Current project') {
        throw "workspace create --no-focus changed the active workspace: $workspacesAfterCreate"
    }
    Invoke-Spindle @("workspace", "close", $createdWorkspace.workspace_id)
    $createdTab = ((& $Binary tab create "No focus tab" --cwd $PWD --env SPINDLE_TAB_SMOKE=ok) -join "`n") | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($createdTab.tab_id)) {
        throw "tab create did not return a tab ID: $createdTab"
    }
    $tabsAfterCreate = (& $Binary tab list) -join "`n"
    if ($LASTEXITCODE -ne 0 -or $tabsAfterCreate -notmatch '(?m)^\*\s+\S+\s+Main') {
        throw "tab create changed the active tab: $tabsAfterCreate"
    }
    Invoke-Spindle @("tab", "close", $createdTab.tab_id)
    $targetWorkspace = ((& $Binary workspace create --label "Tab target" --cwd $PWD --focus) -join "`n") | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($targetWorkspace.workspace_id)) {
        throw "workspace create --focus did not return a workspace ID: $targetWorkspace"
    }
    $createdTab = ((& $Binary tab create "Remote tab" --workspace workspace-1 --cwd $PWD --env SPINDLE_REMOTE_TAB=ok) -join "`n") | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($createdTab.tab_id)) {
        throw "tab create --workspace did not return a tab ID: $createdTab"
    }
    $workspaceAfterRemoteTab = (& $Binary workspace list) -join "`n"
    if ($LASTEXITCODE -ne 0 -or $workspaceAfterRemoteTab -notmatch "(?m)^\*\s+$([regex]::Escape($targetWorkspace.workspace_id))\s+Tab target") {
        throw "tab create --workspace changed the active workspace: $workspaceAfterRemoteTab"
    }
    Invoke-Spindle @("tab", "close", $createdTab.tab_id)
    Invoke-Spindle @("workspace", "close", $targetWorkspace.workspace_id)
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
    $createdTab = ((& $Binary tab create Logs --focus) -join "`n") | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($createdTab.tab_id)) {
        throw "tab create did not return a tab ID: $createdTab"
    }
    $tabId = $createdTab.tab_id
    $tabs = (& $Binary tab list) -join "`n"
    if ($LASTEXITCODE -ne 0 -or $tabs -notmatch "(?m)^\*\s+$([regex]::Escape($tabId))\s+Logs\s+\[workspace-1\]$") {
        throw "tab list did not mark the created tab: $tabs"
    }
    $tab = (& $Binary tab get $tabId) -join "`n"
    if ($LASTEXITCODE -ne 0 -or $tab -notmatch '(?m)^name: Logs$') {
        throw "tab get did not return the created tab: $tab"
    }
    Invoke-Spindle @("tab", "rename", $tabId, "Build logs")
    Invoke-Spindle @("tab", "focus", $tabId)
    $renamedTabs = (& $Binary tab list) -join "`n"
    if ($LASTEXITCODE -ne 0 -or $renamedTabs -notmatch "(?m)^\*\s+$([regex]::Escape($tabId))\s+Build logs\s+\[workspace-1\]$") {
        throw "tab rename/focus did not update the active tab: $renamedTabs"
    }
    Invoke-Spindle @("tab", "close", $tabId)
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
