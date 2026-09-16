# installed by spindle
# SPINDLE_INTEGRATION_ID=kimi
# SPINDLE_INTEGRATION_VERSION=1
param([string]$Action = "")
if (@("session", "working", "blocked", "idle") -notcontains $Action) { exit 0 }
$enabled = $env:SPINDLE_ENV
if ([string]::IsNullOrWhiteSpace($enabled)) { $enabled = $env:HERDR_ENV }
if ($enabled -ne "1") { exit 0 }
$paneId = $env:SPINDLE_PANE_ID
if ([string]::IsNullOrWhiteSpace($paneId)) { $paneId = $env:HERDR_PANE_ID }
if ([string]::IsNullOrWhiteSpace($paneId)) { exit 0 }
$inputText = [Console]::In.ReadToEnd()
try { $payload = if ([string]::IsNullOrWhiteSpace($inputText)) { $null } else { $inputText | ConvertFrom-Json } } catch { $payload = $null }
$sessionId = if ($null -ne $payload -and $payload.session_id -is [string]) { $payload.session_id } else { $null }
$bin = $env:SPINDLE_BIN_PATH
if ([string]::IsNullOrWhiteSpace($bin)) { $bin = $env:HERDR_BIN_PATH }
if ([string]::IsNullOrWhiteSpace($bin)) { $bin = "spindle" }
try {
    if ($Action -eq "session") { if (-not [string]::IsNullOrWhiteSpace($sessionId)) { & $bin pane report-agent-session $paneId --source spindle:kimi --agent kimi --agent-session-id $sessionId --session-start-source startup --seq ([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()) 2>$null | Out-Null } }
    elseif ([string]::IsNullOrWhiteSpace($sessionId)) { & $bin pane report-agent $paneId --source spindle:kimi --agent kimi --state $Action --seq ([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()) 2>$null | Out-Null }
    else { & $bin pane report-agent $paneId --source spindle:kimi --agent kimi --state $Action --agent-session-id $sessionId --seq ([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()) 2>$null | Out-Null }
} catch {}
