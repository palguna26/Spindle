# Spindle implementation plan: Herdr parity plus Windows-first improvements

Herdr source of truth: `C:\Users\Palguna\.opensrc\repos\github.com\herdrdev\herdr\master`.
Before each task, inspect the matching Herdr source and document any intentional
Spindle difference. Do not treat this sequence as a reduced scope: the goal is
to inventory and implement Herdr's user-facing behavior, then improve it for
Spindle's Windows-first, local-first use.

The original `SPEC.md` is not present in the current checkout. This plan is
based on the committed design in
`docs/superpowers/specs/2026-09-12-herdr-inspired-spindle-design.md`, the current
Spindle code, and the Herdr reference checkout.

## Working rules

- Make small commits, one independently testable behavior at a time.
- Keep the session server authoritative; do not add separate client-only state
  for workspace, tab, pane focus, or layout.
- Keep keyboard access while porting Herdr's mouse-first interactions.
- Preserve working-tree user files such as `image.png`; stage only task files.
- For each milestone, run focused tests first, then the full Rust suite and
  Windows checks when the milestone changes runtime behavior.
- Do not claim UX parity from unit tests alone. Run Spindle beside Herdr on
  Windows and verify the interactions listed below.

## 0. Establish the parity matrix

- [ ] Compare Herdr's README, quick-start, keyboard guide, CLI guide, and
  user-facing `src/ui`, `src/client/shell`, `src/workspace.rs`, and `src/detect`
  behavior with Spindle's README, usage guide, CLI, renderer, client, server,
  session, and tests.
- [ ] Record each behavior as present, missing, partial, or intentionally
  different, with a source path and a verification method.
- [ ] Use that matrix to keep later work tied to actual Herdr behavior rather
  than assumptions or visual imitation alone.

Reference starting points: Herdr `docs/preview/website/src/content/docs/quick-start.mdx`,
`src/workspace.rs`, `src/ui/sidebar.rs`, `src/ui/panes.rs`,
`src/client/shell/mouse.rs`, `src/client/shell/context_menu.rs`,
`src/pane/agent_detection.rs`, and `src/detect/`. Spindle starting points:
`src/client/renderer.rs`, `src/client/app.rs`, `src/server/session.rs`,
`src/server/control.rs`, `src/cli.rs`, and `tests/`.

## 1. Make session startup and container creation match Herdr

- [x] Fresh attach starts one usable PowerShell pane; reattach does not
  duplicate it. If a server restart interrupted the saved pane, ensure-shell
  restarts that pane in place and keeps its layout entry.
- [x] Answer terminal cursor-position queries from PowerShell so its first
  prompt is not blocked waiting for the host.
- [x] Creating a tab, workspace, or space starts and focuses its first shell.
- [x] Closing the last pane/tab follows Herdr's workspace lifecycle: close the
  workspace when its final tab/pane closes, select a sibling when present, and
  show a clear empty-state with a shortcut to create a workspace when none remain.
- [x] Closing the only pane in a tab removes that tab when sibling tabs exist
  and focuses the remaining active tab, matching Herdr `Workspace::remove_pane`.
- [x] Make active workspace optional so closing the final workspace does not
  leave a stale ID; persist/reload the empty state and allow workspace creation
  to restore a usable shell.
- [x] Show shell-start errors in the UI and allow retry or detach.
- [x] Wire attach, tab/workspace/space creation, and container switching to
  idempotent shell creation using the active workspace repository path.
- [x] Verify a failed shell executable leaves an empty session that accepts a
  later valid shell request.
- [x] Repair stale pane IDs in the active layout before ensure-shell, following
  Herdr's invariant that layout panes and pane state must match.
- [x] Verify concurrent clients ensuring the initial pane create exactly one
  shell, matching Herdr's single live pane at workspace startup.
- [x] Cover new tab/workspace/space shell creation and focus in integration
  tests. The ensure-shell flow verifies the active pane after creation and
  retries/reports an error instead of leaving a connected empty canvas.
- [x] Exercise the real client attach and detach/reattach flow end-to-end. On
  Windows, the installed release showed the PowerShell prompt, accepted a
  command, and reattached to the same live pane with its output intact.

Reference: Herdr `Workspace::new`, `create_tab_with_runtime`, and quick-start
"Create a workspace" / "Detach and come back". Spindle: `Session::default`,
`Session::create_tab`, control operations, and `client/app.rs` startup. Review
commit `68d86ad` as partial startup work; do not count it as completion.

## 2. Build the everyday workspace layout

- [x] Add a workspace/space sidebar with active selection and useful names.
- [x] Add visible tab controls and a focused-pane/status area.
- [x] Save focused pane per tab and restore it when switching tabs or
  workspaces, matching Herdr's focus in each tab layout.
- [ ] Render the active pane tree in the remaining area with clear focus and
  lifecycle states.
- [x] Track terminal cursor visibility and place the host cursor in the focused
  pane; hide it for overlays and while viewing scrollback, following Herdr's
  `src/ui/panes.rs` behavior.
- [x] Calculate pane rectangles once and share the results between rendering,
  hit testing, and PTY resize requests.
- [x] On terminal resize or layout change, send each PTY its actual pane size,
  not the full terminal size.
- [x] Add small-terminal and many-pane geometry tests using Ratatui TestBackend.

Reference: Herdr `src/ui.rs`, `src/ui/sidebar.rs`, `src/ui/panes.rs`, and
`src/layout.rs`. Spindle: `src/client/renderer.rs`, `src/model/layout.rs`, and
`resize_panes` in `src/client/app.rs`.

## 3. Port mouse-first navigation and pane controls

- [x] Click spaces, workspaces, tabs, and panes to switch/focus them.
- [x] Toggle a compact four-column sidebar from the Herdr `prefix+b` binding
  or the sidebar footer control; compact rows remain clickable.
- [x] Save the sidebar's manually selected collapsed state with the local
  project's client preferences, matching Herdr's chrome-preference behavior.
- [x] Scroll long sidebar lists with the mouse wheel and jump by clicking the
  scrollbar track or dragging its thumb, following Herdr's grab-offset behavior.
- [x] Drag split borders to resize, updating both the saved layout and PTY
  dimensions.
- [x] Right-click workspaces, tabs, and panes for context-specific actions.
- [x] Support pane split, close, rename, zoom/focus actions from menus where
  the matching Herdr action exists.
- [x] Bind focused-pane zoom to `Ctrl-b z`, matching Herdr's default.
- [x] Align tab creation, pane/tab close, tab navigation, split, and pane-focus
  shortcuts with Herdr's default bindings.
- [x] Support drag-select/copy and terminal mouse passthrough without stealing
  input from applications that request it. Wheel and mode-aware PageUp/PageDown
  now browse recent PTY history, which the same selection/copy path can read;
  history is bounded by the PTY byte ring and reconstructed at the current
  pane size.
- [x] Keep keyboard command-palette actions for pane, tab, and workspace
  actions available through the mouse, including clearing a pane's manual name.
- [x] Add in-app help that explains both keyboard shortcuts and mouse controls.
- [ ] Test input routing, hit regions, drag boundaries, menu actions, and
  passthrough behavior; manually verify in Windows Terminal and PowerShell.

Reference: Herdr `src/client/shell/mouse.rs`, `context_menu.rs`, `selection.rs`,
and quick-start "Use the mouse". Spindle: `src/client/app.rs`,
`src/client/input.rs`, and `src/client/renderer.rs`.

## 4. Add agent-aware workspace status

- [ ] Port process/terminal agent detection using Herdr's manifests and state
  transitions as behavioral references, not copied implementation. Partial:
  direct and known wrapped Codex/OpenCode processes, selected manifest blocker
  prompts, Herdr-aligned OpenCode permission/interrupt and Codex directory-trust
  rules, and initial screen-state signals are detected; full manifest rules
  and transition stabilization remain.
- [ ] Expose agent kind and unknown/idle/working/blocked state in pane chrome and
  the sidebar. Partial: pane chrome shows identity/state and workspace rows show
  per-state counts; multi-agent rows and richer lifecycle transitions remain.
- [ ] Keep state associated with the correct pane, tab, and workspace through
  switches, restarts, and recovery.
- [ ] Make detection optional/fault-tolerant so normal shell use is unaffected.
- [ ] Test detection fixtures and status transitions; verify Codex and OpenCode
  side-by-side on Windows.

Reference: Herdr `src/pane/agent_detection.rs`, `src/detect/`,
`src/ui/sidebar.rs`, and `src/client/shell/agent_sidebar.rs`. Spindle currently
describes agents as user-run commands in panes; preserve that unless a separate
approved design changes it.

## 5. Audit and port the remaining user-facing Herdr behavior

- [ ] Extend the parity matrix across CLI, configuration/keybindings/themes,
  notifications, session recovery, integrations, remote sessions, and plugins.
- [ ] Port each behavior that fits Spindle's Windows-first/local-first purpose
  in a separately tested commit.
- [ ] For behavior not included, record the concrete reason and an equivalent
  Spindle workflow; do not silently omit it.
- [ ] Improve Windows-specific process launch, terminal input/output, resizing,
  recovery, installer, and diagnostics based on observed gaps in the matrix.

Reference areas: Herdr `src/main.rs`, `src/config/`, `src/terminal/`,
`src/client/notifications.rs`, `src/integration/`, `src/remote/`,
`src/plugin_paths.rs`, `src/plugin_command.rs`, `src/cli/plugin.rs`, and
`src/app/api/plugins/`.
Use the reference repository's user docs to confirm the behavior before each
port.

## Verification gates

For every code commit:

1. Re-read the matching Herdr source and the affected Spindle code.
2. Add a regression test for the behavior.
3. Run `cargo fmt --all`, `cargo test`, and strict Clippy.
4. For UI/runtime work, build release and run the Windows smoke and installer
   smoke scripts.
5. Update this plan and the parity matrix with observed results.

Before calling the overall objective complete, verify each matrix row and
acceptance item against the running app on Windows, including mouse use,
Codex/OpenCode status, multi-client attach, detach/reattach, and recovery.
