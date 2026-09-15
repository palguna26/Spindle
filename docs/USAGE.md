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

The screen shows spaces and workspaces in a left sidebar and tabs above the
panes. Click a space, workspace, tab, or pane to switch focus. Drag split
borders to resize. Right-click panes, tabs, and workspaces for actions. Drag
terminal text to select and copy it; double-click a word to select it. Use the
wheel inside the sidebar to browse long space/workspace lists, click its
scrollbar track to jump, or drag the thumb. Use the wheel or PageUp/PageDown
over a pane to browse recent output when the foreground app does not claim
those inputs. Mouse-aware terminal apps receive mouse input when they request
it.

Ctrl-click a visible `http://` or `https://` URL in a pane to open it in your
default browser. OSC 8 hyperlinks are not supported yet.

Useful commands:

```text
spindle start    Start the project server
spindle attach   Attach to the project server
spindle stop     Stop the project server
spindle list     Show project identity and state path
spindle doctor   Show client/server identity and flag stale endpoint metadata
```

## Configuration

Spindle reads optional keybindings from `%APPDATA%\Spindle\config.toml`.
Bindings use Herdr's `prefix+key` format; missing or invalid entries keep the
built-in defaults.

```toml
[keys]
prefix = "ctrl+b"
new_tab = "prefix+c"
next_tab = ["prefix+n", "ctrl+alt+right"]
rename_workspace = "prefix+shift+w"
```

The config is read when the client starts and refreshed while the client runs.
Supported action names include
`new_tab`, `close_pane`, `close_tab`, `next_tab`, `previous_tab`,
`workspace_picker`, `session_navigator`, `create_workspace`,
`rename_workspace`, `delete_workspace`, pane focus directions, splits, resize,
zoom, sidebar, copy mode, help, and the command palette.

Use `spindle config path` to print the path or `spindle config default` to
print a starter file.

Use `spindle completion <shell>` to generate completions for Bash, Elvish,
Fish, PowerShell, or Zsh.

Use `spindle api schema` to inspect the local control protocol, `--json` to
print the machine-readable schema, or `--output PATH` to save a copy.

Set `[theme] name` to `terminal`, `catppuccin`, `dracula`, `gruvbox`, `nord`,
or `tokyo-night` to change the main UI accent and focused-pane colors.

Background agent panes show a short in-app notification when they need
attention or finish. Notifications for the active tab are suppressed, like
Herdr's default behavior. Multiple background events are queued and shown in
arrival order. Set `[notifications] enabled = false` in the config to disable
these toasts; the setting reloads while the client is running.

Tabs can also be automated with `spindle tab list`, `create [label]`,
`get <id>`, `focus <id>`, `rename <id> <label>`, and `close <id>`.

Panes can be automated with `spindle pane list [--workspace <id>]`, `current [<id>]`, `get <id>`,
`focus <id>` or `focus --direction left|right|up|down`, `rename <id> <label>`, `stop <id>`, `restart <id>`, `zoom [<id>] [--toggle|--on|--off]`,
`close <id>`, `send-text <id> <text>`, `send-keys <id> <key>...`,
`run <id> <command>`, `read <id> [--source visible|recent] [--lines N]`, `swap <source> <target>`,
`wait-output <id> --match TEXT [--timeout MS] [--lines N]`,
`swap --direction left|right|up|down` or `--source-pane ID --target-pane ID`,
`move <id> --new-tab [--label TEXT]`,
`split <horizontal|vertical>`, and `resize <id> <delta>`. Pane listing and
mutations use the active tab's current server focus scope.

## Keybindings

`Ctrl-b` is the command prefix. Press `Ctrl-b`, then `?` for the in-app guide
to mouse and keyboard controls. Press `:` for the command palette. Other keys:

| Keys | Action |
| --- | --- |
| `q` / `d` | Detach |
| `c` | New tab |
| `x` | Close focused pane |
| `Shift+x` | Close active tab |
| `n` / `p` | Next / previous tab |
| `]` / `[` | Next / previous tab (alternate) |
| `w` | Next workspace |
| `}` / `{` | Next / previous space |
| `o` / `O` | Focus next / previous pane |
| `s` | Stop focused pane |
| `r` | Restart focused pane |
| `h` / `j` / `k` / `l` or arrows | Focus pane by direction |
| `-` | Horizontal split |
| `v` | Vertical split |
| `"` / `%` | Horizontal / vertical split (alternate) |
| `z` | Zoom / restore focused pane |
| `b` | Toggle compact sidebar (saved for this project) |
| `<` / `>` | Resize pane |
| `?` | Open mouse and keyboard help |
| `:` | Open command palette |

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
