# installed by spindle
# SPINDLE_INTEGRATION_ID=devin
# SPINDLE_INTEGRATION_VERSION=1
param([string]$Action = "")
if ($Action -ne "session") { exit 0 }
$enabled = $env:SPINDLE_ENV
if ([string]::IsNullOrWhiteSpace($enabled)) { $enabled = $env:HERDR_ENV }
if ($enabled -ne "1") { exit 0 }
$paneId = $env:SPINDLE_PANE_ID
if ([string]::IsNullOrWhiteSpace($paneId)) { $paneId = $env:HERDR_PANE_ID }
if ([string]::IsNullOrWhiteSpace($paneId)) { exit 0 }
$inputText = [Console]::In.ReadToEnd()
try { $payload = if ([string]::IsNullOrWhiteSpace($inputText)) { $null } else { $inputText | ConvertFrom-Json } } catch { $payload = $null }
$sessionId = $null
if ($null -ne $payload -and $payload.session_id -is [string]) { $sessionId = $payload.session_id }
if ($null -ne $payload -and [string]::IsNullOrWhiteSpace($sessionId) -and $payload.sessionId -is [string]) { $sessionId = $payload.sessionId }
if ([string]::IsNullOrWhiteSpace($sessionId)) { exit 0 }
$bin = $env:SPINDLE_BIN_PATH
if ([string]::IsNullOrWhiteSpace($bin)) { $bin = $env:HERDR_BIN_PATH }
if ([string]::IsNullOrWhiteSpace($bin)) { $bin = "spindle" }
try { & $bin pane report-agent-session $paneId --source spindle:devin --agent devin --seq ([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()) --agent-session-id $sessionId 2>$null | Out-Null } catch {}
