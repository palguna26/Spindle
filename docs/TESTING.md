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
```

The smoke flow uses an isolated `LOCALAPPDATA`, starts and stops a server,
checks diagnostics, and verifies endpoint and PID markers are removed.

Before a release, also manually verify two PowerShell or fake CLI panes,
detach/reattach, a second client, workspace switching, pane restart, and
recovery from a server restart.
