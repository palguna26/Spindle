# installed by spindle
# SPINDLE_INTEGRATION_ID=qwen
# SPINDLE_INTEGRATION_VERSION=1
param([string]$Action = "")
if ($Action -ne "session") { exit 0 }
$enabled = $env:SPINDLE_ENV
if ([string]::IsNullOrWhiteSpace($enabled)) { $enabled = $env:HERDR_ENV }
if ($enabled -ne "1") { exit 0 }
$paneId = $env:SPINDLE_PANE_ID
if ([string]::IsNullOrWhiteSpace($paneId)) { $paneId = $env:HERDR_PANE_ID }
$socket = $env:SPINDLE_SOCKET_PATH
if ([string]::IsNullOrWhiteSpace($socket)) { $socket = $env:HERDR_SOCKET_PATH }
if ([string]::IsNullOrWhiteSpace($paneId) -or [string]::IsNullOrWhiteSpace($socket)) { exit 0 }
$inputText = [Console]::In.ReadToEnd()
try { $payload = if ([string]::IsNullOrWhiteSpace($inputText)) { $null } else { $inputText | ConvertFrom-Json } } catch { $payload = $null }
if ($null -eq $payload -or [string]::IsNullOrWhiteSpace($payload.session_id)) { exit 0 }
$bin = $env:SPINDLE_BIN_PATH
if ([string]::IsNullOrWhiteSpace($bin)) { $bin = $env:HERDR_BIN_PATH }
if ([string]::IsNullOrWhiteSpace($bin)) { $bin = "spindle" }
$args = @("pane","report-agent-session",$paneId,"--source","spindle:qwen","--agent","qwen","--agent-session-id",[string]$payload.session_id,"--seq",[string]([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()))
if ($payload.source -in @("startup", "resume", "clear", "compact", "branch")) { $args += @("--session-start-source", [string]$payload.source) }
try { & $bin @args 2>$null | Out-Null } catch {}
