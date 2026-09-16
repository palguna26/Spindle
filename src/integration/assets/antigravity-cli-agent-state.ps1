# installed by spindle; managed by spindle integration install antigravity-cli
# SPINDLE_INTEGRATION_VERSION=1

param([string]$Action = "")

function Exit-Hook {
    Write-Output "{}"
    exit 0
}

if ($Action -ne "session") { Exit-Hook }
$envMode = $env:SPINDLE_ENV
if ([string]::IsNullOrWhiteSpace($envMode)) { $envMode = $env:HERDR_ENV }
if ($envMode -ne "1") { Exit-Hook }
$pane = $env:SPINDLE_PANE_ID
if ([string]::IsNullOrWhiteSpace($pane)) { $pane = $env:HERDR_PANE_ID }
if ([string]::IsNullOrWhiteSpace($pane)) { Exit-Hook }

$inputText = [Console]::In.ReadToEnd()
try {
    $payload = if ([string]::IsNullOrWhiteSpace($inputText)) { $null } else { $inputText | ConvertFrom-Json }
} catch { Exit-Hook }
if ($null -eq $payload) { Exit-Hook }

$conversationId = if ($payload.conversationId -is [string]) { $payload.conversationId } else { $null }
if ([string]::IsNullOrWhiteSpace($conversationId)) { Exit-Hook }

$seq = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
$bin = $env:SPINDLE_BIN_PATH
if ([string]::IsNullOrWhiteSpace($bin)) { $bin = $env:HERDR_BIN_PATH }
if ([string]::IsNullOrWhiteSpace($bin)) { $bin = "spindle" }
try {
    & $bin pane report-agent-session $pane --source "spindle:antigravity-cli" --agent "antigravity-cli" --seq "$seq" --agent-session-id "$conversationId" 2>$null | Out-Null
} catch { }
Exit-Hook
