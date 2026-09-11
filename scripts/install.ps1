param(
    [string]$Binary = (Join-Path $PSScriptRoot "..\target\release\spindle.exe"),
    [string]$Destination = (Join-Path $env:LOCALAPPDATA "Spindle\bin"),
    [switch]$AddToUserPath
)

$Binary = [IO.Path]::GetFullPath($Binary)
$Destination = [IO.Path]::GetFullPath($Destination)
if (-not (Test-Path -LiteralPath $Binary -PathType Leaf)) {
    throw "Release binary not found: $Binary"
}

New-Item -ItemType Directory -Path $Destination -Force | Out-Null
$installedBinary = Join-Path $Destination "spindle.exe"
Copy-Item -LiteralPath $Binary -Destination $installedBinary -Force

if ($AddToUserPath) {
    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    $entries = @($userPath -split ';' | Where-Object { $_ })
    if ($entries -notcontains $Destination) {
        [Environment]::SetEnvironmentVariable("Path", (($entries + $Destination) -join ';'), "User")
        Write-Output "Added $Destination to the user PATH. Open a new shell to use it."
    }
}

Write-Output "Installed Spindle to $installedBinary"
