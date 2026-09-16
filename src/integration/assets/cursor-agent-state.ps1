# installed by spindle
# SPINDLE_INTEGRATION_ID=cursor
# SPINDLE_INTEGRATION_VERSION=1

param([string]$Action = "")

function Exit-Hook { Write-Output "{}"; exit 0 }
if ($Action -ne "session") { Exit-Hook }
$enabled = $env:SPINDLE_ENV
if ([string]::IsNullOrWhiteSpace($enabled)) { $enabled = $env:HERDR_ENV }
if ($enabled -ne "1") { Exit-Hook }
$paneId = $env:SPINDLE_PANE_ID
if ([string]::IsNullOrWhiteSpace($paneId)) { $paneId = $env:HERDR_PANE_ID }
if ([string]::IsNullOrWhiteSpace($paneId)) { Exit-Hook }

$inputText = [Console]::In.ReadToEnd()
$jsonStart = $inputText.IndexOf("{")
if ($jsonStart -gt 0) { $inputText = $inputText.Substring($jsonStart) }
try { $payload = if ([string]::IsNullOrWhiteSpace($inputText)) { $null } else { $inputText | ConvertFrom-Json } } catch { Exit-Hook }
if ($null -eq $payload) { Exit-Hook }
$event = if ($payload.hook_event_name -is [string]) { $payload.hook_event_name } else { $payload.hookEventName }
if (-not [string]::IsNullOrWhiteSpace($event) -and $event -ne "sessionStart") { Exit-Hook }

$sessionId = $null
foreach ($name in @("session_id", "sessionId", "conversation_id", "conversationId")) {
    $value = $payload.$name
    if ($value -is [string] -and -not [string]::IsNullOrWhiteSpace($value)) { $sessionId = $value; break }
}
if ([string]::IsNullOrWhiteSpace($sessionId)) { Exit-Hook }
$seq = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
$bin = $env:SPINDLE_BIN_PATH
if ([string]::IsNullOrWhiteSpace($bin)) { $bin = $env:HERDR_BIN_PATH }
if ([string]::IsNullOrWhiteSpace($bin)) { $bin = "spindle" }
try { & $bin pane report-agent-session $paneId --source spindle:cursor --agent cursor --seq $seq --agent-session-id $sessionId 2>$null | Out-Null } catch {}
Exit-Hook
