# installed by spindle; managed by spindle integration install mastracode
# SPINDLE_INTEGRATION_ID=mastracode
# SPINDLE_INTEGRATION_VERSION=1

param([string]$Action = "")
if ($Action -notin @("session", "working", "idle", "blocked")) { exit 0 }
$enabled = $env:SPINDLE_ENV
if ([string]::IsNullOrWhiteSpace($enabled)) { $enabled = $env:HERDR_ENV }
if ($enabled -ne "1") { exit 0 }
$pane = $env:SPINDLE_PANE_ID
if ([string]::IsNullOrWhiteSpace($pane)) { $pane = $env:HERDR_PANE_ID }
if ([string]::IsNullOrWhiteSpace($pane)) { exit 0 }
$inputText = [Console]::In.ReadToEnd()
try { $payload = if ([string]::IsNullOrWhiteSpace($inputText)) { $null } else { $inputText | ConvertFrom-Json } } catch { $payload = $null }
$session = if ($null -ne $payload -and $payload.session_id -is [string]) { $payload.session_id } else { $null }
$bin = $env:SPINDLE_BIN_PATH
if ([string]::IsNullOrWhiteSpace($bin)) { $bin = $env:HERDR_BIN_PATH }
if ([string]::IsNullOrWhiteSpace($bin)) { $bin = "spindle" }
$seq = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
try {
    if ($Action -eq "session") {
        if ([string]::IsNullOrWhiteSpace($session)) { exit 0 }
        & $bin pane report-agent-session $pane --source spindle:mastracode --agent mastracode --agent-session-id $session --session-start-source startup --seq $seq 2>$null | Out-Null
    } else {
        & $bin pane report-agent $pane --source spindle:mastracode --agent mastracode --state $Action --seq $seq 2>$null | Out-Null
    }
} catch {}
