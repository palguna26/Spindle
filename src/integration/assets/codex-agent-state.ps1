# installed by spindle
# managed by spindle; reinstalling or updating the integration overwrites this file.
# SPINDLE_INTEGRATION_ID=codex
# SPINDLE_INTEGRATION_VERSION=1

param([string]$Action = "")

if ($Action -ne "session") { exit 0 }
if ($env:SPINDLE_ENV -ne "1" -and $env:HERDR_ENV -ne "1") { exit 0 }
$paneId = if ([string]::IsNullOrWhiteSpace($env:SPINDLE_PANE_ID)) { $env:HERDR_PANE_ID } else { $env:SPINDLE_PANE_ID }
if ([string]::IsNullOrWhiteSpace($paneId)) { exit 0 }

$inputText = [Console]::In.ReadToEnd()
try {
    $payload = if ([string]::IsNullOrWhiteSpace($inputText)) { $null } else { $inputText | ConvertFrom-Json }
} catch {
    exit 0
}

if ($payload.hook_event_name -and $payload.hook_event_name -ne "SessionStart") { exit 0 }
$sessionId = $payload.session_id
if ([string]::IsNullOrWhiteSpace($sessionId)) { exit 0 }
if ([string]::IsNullOrWhiteSpace($payload.transcript_path)) { exit 0 }
if (-not [string]::IsNullOrWhiteSpace($env:CODEX_THREAD_ID) -and $env:CODEX_THREAD_ID -ne $sessionId) { exit 0 }

$binary = if ([string]::IsNullOrWhiteSpace($env:SPINDLE_BIN_PATH)) { "spindle" } else { $env:SPINDLE_BIN_PATH }
$seq = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
try {
    & $binary pane report-agent-session $paneId --source spindle:codex --agent codex --seq "$seq" --agent-session-id "$sessionId" 2>$null | Out-Null
} catch {
}
