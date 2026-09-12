# Herdr behavior parity matrix

Reference: `C:\Users\Palguna\.opensrc\repos\github.com\herdrdev\herdr\master`.
Statuses describe the current Spindle checkout, not the target. “Partial” means
some server/model support exists but the user-facing behavior is missing or
does not yet match Herdr.

| User behavior | Spindle status | Herdr reference | Evidence / next verification |
| --- | --- | --- | --- |
| Start or attach to a persistent session | Partial | `docs/preview/website/src/content/docs/quick-start.mdx`, `src/session.rs` | Spindle has per-project server and attach; verify the first visible shell and warm reattach in a real terminal. |
| Start with a live shell; create shell-backed tabs/workspaces | Partial | `src/workspace.rs` (`Workspace::new`, `create_tab_with_runtime`) | Spindle now ensures a shell after attach, container creation, and switching, using the active workspace repository path. Integration test covers initial, tab, workspace, and space shell creation, idempotency, and focus; live TUI verification remains. |
| Navigate spaces/workspaces and tabs in a sidebar | Partial | `src/client/shell/sidebar.rs`, `src/client/shell/workspace_navigation.rs` | Spindle now renders a space/workspace sidebar and tab strip; click and keyboard navigation switch active containers. Agent status, sidebar resizing/collapse, and larger-list scrolling remain. |
| Render pane layouts with focus, terminal state, correct geometry | Partial | `src/ui/panes.rs`, `src/layout.rs` | Spindle stores/restores focus per tab and renders visible pane focus. Shared geometry now drives pane drawing and click hit-testing; `resize_panes` still resizes every PTY to whole-terminal size. Verify actual pane rectangles and per-pane PTY dimensions. |
| Click panes/tabs/workspaces; drag split borders | Partial | `src/client/shell/mouse.rs` | Spindle handles left-click focus/navigation for panes, tabs, spaces, and workspaces; hit targets are unit-tested. Split-border dragging, terminal-app mouse passthrough, and a manual Windows Terminal check remain. |
| Right-click context menus for pane/tab/workspace actions | Missing | `src/client/shell/context_menu.rs` | No menus in Spindle renderer/input. Verify menu targets, actions, keyboard fallback, and screen-edge placement. |
| Select/copy terminal text and route terminal mouse input | Missing | `src/selection.rs`, `src/client/shell/mouse.rs` | Spindle forwards key codes only and has no selection model. Test copy, scrollback, and mouse passthrough. |
| Show detected agent identity and working/blocked/done/idle state | Missing | `src/pane/agent_detection.rs`, `src/detect/`, `src/client/shell/agent_sidebar.rs` | Spindle treats agents as ordinary commands. Test supported-agent fixtures and Codex/OpenCode side by side. |
| Configure themes, sidebar, keybindings, and notifications | Missing / partial | `src/config/`, `src/client/notifications.rs`, `src/client/shell/notifications.rs` | Spindle keybindings are static and has no settings UI. Audit exact behavior and test config/default fallback. |
| Agent integrations, CLI automation, plugins, and remote sessions | Missing | `src/integration/`, `src/cli/`, `src/app/api/plugins/`, `src/remote/` | Spindle is local-only and has no integration/plugin system. Inventory command and API behavior before choosing port details. |
| Windows-first process, terminal, installer, and recovery behavior | Partial | Herdr `src/platform/windows.rs`, `src/pty/backend.rs`, `scripts/windows_*` | Spindle has Windows PTY and smoke tests; add side-by-side process/input/recovery checks and record concrete improvements. |

Update a row only after checking the matching Herdr code and testing the
corresponding Spindle behavior. Any deliberate difference needs a reason and a
usable alternative, not just a missing implementation.
