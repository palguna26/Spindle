# Spindle usage

Spindle keeps local terminal processes alive while the client is detached.
It does not start agents or interpret their output.

## Build on Windows

Install the Visual Studio C++ build tools, including the Windows SDK. Then
open a Visual Studio developer shell and run:

```powershell
cargo build --release
.\target\release\spindle.exe help
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\install.ps1 -AddToUserPath
```

## Start and attach

From a repository directory:

```powershell
spindle start
spindle attach
```

Attaching to an empty active tab starts one PowerShell pane. Reattaching does
not add another pane when that tab already has one. New tabs and workspaces
also start a pane in the workspace's repository directory.

Running `spindle` without a server starts the server and attaches. The server
runs independently of the attached terminal.

Useful commands:

```text
spindle start    Start the project server
spindle attach   Attach to the project server
spindle stop     Stop the project server
spindle list     Show project identity and state path
spindle doctor   Show server health and identity
```

## Keybindings

`Ctrl-b` is the command prefix. Press `Ctrl-b`, then:

| Keys | Action |
| --- | --- |
| `d` | Detach |
| `n` | New tab |
| `c` | Close tab |
| `]` / `[` | Next / previous tab |
| `w` | Next workspace |
| `}` / `{` | Next / previous space |
| `o` | Focus next pane |
| `x` | Stop focused pane |
| `r` | Restart focused pane |
| `"` | Horizontal split |
| `%` | Vertical split |
| `<` / `>` | Resize pane |
| `?` | Open command palette |

The command palette supports arrow keys or `j`/`k`, Enter to select, and Esc
to close. It includes pane, tab, space, and workspace rename actions, named
new PowerShell pane creation, named workspace switching, and type-to-confirm deletion for the active space or
workspace. Deletion still requires all panes in the target to be stopped and
never permits deleting the last space or workspace.

## State and recovery

State is stored per project under:

```text
%LOCALAPPDATA%\Spindle\projects\<project-id>\
```

The server writes `session.json`, `server.json`, endpoint markers, and a PID
marker. It appends lifecycle records to `server.log`; PTY output is never
written to that log. `server.json` contains diagnostics only: PID, protocol
version, endpoints, and start time.

If the server stops unexpectedly, previously running panes are restored as
`interrupted`. Spindle does not claim to restore arbitrary child processes;
restart a pane explicitly from the palette or with `Ctrl-b`, `r`.

If a snapshot is corrupt or newer than the supported version, startup fails
without deleting it. Stop the server, copy the project state directory for
backup, and repair or move the snapshot before retrying.

## Limitations

- Spindle is local-only and Windows-first in v1.
- Environment variables are used to create a process but are not persisted.
- Terminal output is kept separately from the metadata snapshot and may
  contain secrets.
- Only the active client controls PTY dimensions; passive clients observe.
- Codex, OpenCode, and other tools are ordinary commands run inside panes.
