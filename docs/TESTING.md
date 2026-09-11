# Testing Spindle

Run the Rust suite from a Visual Studio developer shell so the MSVC linker is
available:

```powershell
cmd /c 'call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat" -arch=x64 && cargo test'
cmd /c 'call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat" -arch=x64 && cargo clippy --all-targets --all-features -- -D warnings'
```

Build and run the Windows smoke flow:

```powershell
cmd /c 'call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat" -arch=x64 && cargo build --release'
powershell -NoProfile -ExecutionPolicy Bypass -File .\tests\windows-smoke.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File .\tests\installer-smoke.ps1
```

The smoke flow uses an isolated `LOCALAPPDATA`, starts and stops a server,
checks diagnostics, and verifies endpoint and PID markers are removed.
It also verifies warm-start behavior and recovery from a dead endpoint marker.

The Rust integration suite covers separate control and interactive IPC, PTY
input/output/resize, exit statuses, detach with multiple live panes, client
loss and geometry takeover, screen and scrollback recovery, layout recovery,
workspace switching, and safe space/workspace deletion.

To test the installer without changing the user PATH, pass a temporary
destination:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\install.ps1 -Destination $env:TEMP\spindle-install-test
```

Before a release, manually verify two PowerShell or fake CLI panes,
detach/reattach, a second client, workspace switching, pane restart, recovery
from a server restart, and Codex/OpenCode side by side in a real repository.
