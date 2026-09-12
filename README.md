# Spindle

Spindle is a Windows-first Rust terminal multiplexer for persistent parallel
coding-agent sessions. Run Codex, OpenCode, PowerShell, or any other command
manually inside its panes. Spindle owns the terminal processes and session
state; it does not launch agents or make LLM calls.

## Install on Windows

Build with the Visual Studio C++ build tools and Windows SDK:

```powershell
cmd /c 'call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat" -arch=x64 && cargo build --release'
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\install.ps1 -AddToUserPath
```

Open a new PowerShell or CMD window after adding the install directory to PATH.

## Use

Run `spindle` in a repository. It starts a per-project server, attaches the
client, and opens a PowerShell pane in an empty active tab. The server remains
alive when the client detaches.

New tabs and workspaces also start a PowerShell pane in the workspace's
repository directory.

The screen shows spaces and workspaces on the left, tabs above the panes, and
supports clicking those items or a pane to switch focus. Right-click menus,
drag resizing, and mouse input inside terminal apps are not supported yet.

```text
spindle          attach (and start when needed)
spindle start    start the server
spindle stop     stop the server
spindle doctor   show server health and state paths
spindle list     show project identity and state path
```

Press `Ctrl-b`, then:

| Keys | Action |
| --- | --- |
| `n` | New tab |
| `?` | Command palette |
| `d` | Detach |
| `"` / `%` | Horizontal / vertical split |
| `o` / `p` | Next / previous pane |
| Arrow keys | Directional pane focus |
| `<` / `>` | Resize pane |
| `x` / `r` | Stop / restart pane |
| `]` / `[` | Next / previous tab |
| `}` / `{` | Next / previous space |
| `w` | Next workspace |

The command palette can create more PowerShell panes, rename objects,
switches to a workspace by name, and requires typing the exact name before
deleting a space or workspace.

## State and recovery

Project state is stored under `%LOCALAPPDATA%\Spindle\projects\<project-id>`.
Session metadata is in `session.json`; terminal history is separate in
`session-history.json`. A server restart restores metadata and terminal state,
but marks previously live processes as `interrupted`; arbitrary child
processes cannot be resumed automatically.

See [docs/USAGE.md](docs/USAGE.md) and [docs/TESTING.md](docs/TESTING.md) for
details.

## Scope

Spindle is local-only and Windows-first. Environment variables are used to
start a process but are not persisted. Codex, OpenCode, and other agents are
ordinary commands run by the user inside panes.
