# installed by spindle
# SPINDLE_INTEGRATION_ID=droid
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
if ($null -eq $payload -or [string]::IsNullOrWhiteSpace($payload.session_id)) { exit 0 }
$bin = $env:SPINDLE_BIN_PATH
if ([string]::IsNullOrWhiteSpace($bin)) { $bin = $env:HERDR_BIN_PATH }
if ([string]::IsNullOrWhiteSpace($bin)) { $bin = "spindle" }
try { & $bin pane report-agent-session $paneId --source spindle:droid --agent droid --agent-session-id $payload.session_id --seq ([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()) 2>$null | Out-Null } catch {}
