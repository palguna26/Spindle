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
supports clicking to switch focus. Drag split borders to resize, right-click
for pane/tab/workspace actions, and drag terminal text to select and copy it.
Mouse-aware terminal apps receive mouse input when they request it.

```text
spindle          attach (and start when needed)
spindle start    start the server
spindle stop     stop the server
spindle doctor   show server health and state paths
spindle list     show project identity and state path
spindle --help   show commands
spindle --version  show the build version
```

Press `Ctrl-b`, then `?` for the in-app mouse and keyboard guide. Press `:` to
open the command palette. Other common keys:

| Keys | Action |
| --- | --- |
| `c` | New tab |
| `x` / `Shift+x` | Close pane / close tab |
| `n` / `p` | Next / previous tab |
| `?` | In-app help |
| `:` | Command palette |
| `q` / `d` | Detach |
| `v` / `-` | Vertical / horizontal split |
| `z` | Zoom / restore focused pane |
| `h` / `j` / `k` / `l` or arrows | Directional pane focus |
| `o` / `O` | Next / previous pane |
| `<` / `>` | Resize pane |
| `s` / `r` | Stop / restart pane |
| `]` / `[` | Next / previous tab (alternate) |
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
