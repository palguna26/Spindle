# installed by spindle
# managed by spindle; reinstalling or updating the integration overwrites this file.
# SPINDLE_INTEGRATION_ID=copilot
# SPINDLE_INTEGRATION_VERSION=1

$enabled = $env:SPINDLE_ENV
if ([string]::IsNullOrWhiteSpace($enabled)) { $enabled = $env:HERDR_ENV }
if ($enabled -ne "1") { exit 0 }
$paneId = $env:SPINDLE_PANE_ID
if ([string]::IsNullOrWhiteSpace($paneId)) { $paneId = $env:HERDR_PANE_ID }
if ([string]::IsNullOrWhiteSpace($paneId)) { exit 0 }
$socket = $env:SPINDLE_SOCKET_PATH
if ([string]::IsNullOrWhiteSpace($socket)) { $socket = $env:HERDR_SOCKET_PATH }
if ([string]::IsNullOrWhiteSpace($socket)) { exit 0 }

$inputText = [Console]::In.ReadToEnd()
try {
    $payload = if ([string]::IsNullOrWhiteSpace($inputText)) { @{} } else { $inputText | ConvertFrom-Json }
} catch { $payload = @{} }

function First-Text {
    param([object[]]$Names)
    foreach ($name in $Names) {
        $value = $payload.$name
        if ($value -is [string] -and -not [string]::IsNullOrWhiteSpace($value)) { return $value }
    }
    return $null
}

function Normalize-Event {
    param([string]$Event)
    if ([string]::IsNullOrWhiteSpace($Event)) { return "" }
    return $Event.Replace("_", "").Replace("-", "").ToLowerInvariant()
}

$eventName = First-Text @("hook_event_name", "hookEventName")
if ($eventName -and (Normalize-Event $eventName) -ne "sessionstart") { exit 0 }
if (-not $eventName -and (($payload.PSObject.Properties.Name -contains "prompt") -or (First-Text @("tool_name", "toolName", "notification_type", "notificationType", "stop_reason", "stopReason", "reason")))) { exit 0 }

$sessionId = First-Text @("session_id", "sessionId")
if ([string]::IsNullOrWhiteSpace($sessionId)) { exit 0 }
$seq = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
$bin = $env:SPINDLE_BIN_PATH
if ([string]::IsNullOrWhiteSpace($bin)) { $bin = $env:HERDR_BIN_PATH }
if ([string]::IsNullOrWhiteSpace($bin)) { $bin = "spindle" }
& $bin pane report-agent-session $paneId --source spindle:copilot --agent copilot --agent-session-id $sessionId --seq $seq 2>$null | Out-Null
