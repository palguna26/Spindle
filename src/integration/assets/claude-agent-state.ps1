# installed by spindle
# managed by spindle; reinstalling or updating the integration overwrites this file.
# SPINDLE_INTEGRATION_ID=claude
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

$propertyNames = @($payload.PSObject.Properties.Name)
if ((Test-Path Env:CURSOR_VERSION) -or $propertyNames -ccontains "cursor_version") { exit 0 }
if (-not ($propertyNames -ccontains "hook_event_name") -or $payload.hook_event_name -isnot [string] -or $payload.hook_event_name -cne "SessionStart") { exit 0 }
if (-not [string]::IsNullOrWhiteSpace($payload.agent_id)) { exit 0 }

$sessionId = $payload.session_id
if ([string]::IsNullOrWhiteSpace($sessionId)) { exit 0 }
$binary = if ([string]::IsNullOrWhiteSpace($env:SPINDLE_BIN_PATH)) { "spindle" } else { $env:SPINDLE_BIN_PATH }
$seq = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
try {
    $args = @(
        "pane", "report-agent-session", $paneId,
        "--source", "spindle:claude", "--agent", "claude",
        "--seq", "$seq", "--agent-session-id", "$sessionId"
    )
    if ($payload.transcript_path -is [string] -and -not [string]::IsNullOrWhiteSpace($payload.transcript_path)) {
        $args += @("--agent-session-path", "$($payload.transcript_path)")
    }
    if ($payload.source -is [string] -and -not [string]::IsNullOrWhiteSpace($payload.source)) {
        $args += @("--session-start-source", "$($payload.source)")
    }
    & $binary @args 2>$null | Out-Null
} catch {
}
