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
  recreate the default workspace when a client remains attached or reattaches,
  following Herdr's `ensure_default_workspace` behavior.
- [x] Closing the only pane in a tab removes that tab when sibling tabs exist
  and focuses the remaining active tab, matching Herdr `Workspace::remove_pane`.
- [x] Make active workspace optional so closing the final workspace does not
  leave a stale ID; persist/reload the empty state and recreate a default
  workspace with a usable shell when a client remains connected or reattaches,
  following Herdr's automatic workspace recovery.
- [x] Show shell-start errors in the UI and allow retry or detach.
- [x] Wire attach, tab/workspace/space creation, and container switching to
  idempotent shell creation using the active workspace repository path.
- [x] Verify a failed shell executable leaves an empty session that accepts a
  later valid shell request.
- [x] Repair stale pane IDs in the active layout before ensure-shell, following
  Herdr's invariant that layout panes and pane state must match.
- [x] Verify concurrent clients ensuring the initial pane create exactly one
  shell, matching Herdr's single live pane at workspace startup.
- [x] Recreate a default workspace and shell when attaching to a persisted
  session whose last workspace was closed, following Herdr's server loop.
- [x] Cover new tab/workspace/space shell creation and focus in integration
  tests. The ensure-shell flow verifies the active pane after creation and
  retries/reports an error instead of leaving a connected empty canvas.
- [x] Exercise the real client attach and detach/reattach flow end-to-end. On
  Windows, the installed release showed the PowerShell prompt, accepted a
  command, and reattached to the same live pane with its output intact.
- [x] Match Herdr's detach behavior: `Ctrl-b q` detaches, while a bare `q` is
  sent to the focused shell. Keep `Ctrl-b d` as a Spindle compatibility alias.

Reference: Herdr `Workspace::new`, `create_tab_with_runtime`,
`App::ensure_default_workspace`, and quick-start "Create a workspace" /
"Detach and come back". Spindle: `Session::default`,
`Session::create_tab`, control operations, and `client/app.rs` startup. Review
commit `68d86ad` as partial startup work; do not count it as completion.

## 2. Build the everyday workspace layout

- [x] Add a workspace/space sidebar with active selection and useful names.
- [x] Add visible tab controls and a focused-pane/status area.
- [x] Save focused pane per tab and restore it when switching tabs or
  workspaces, matching Herdr's focus in each tab layout.
- [x] Render the active pane tree in the remaining area with clear focus and
  lifecycle states; show shell and workspace recovery actions when that tree is
  empty.
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
- [x] Keep client mouse state and pane hit models in the dedicated
  `src/client/mouse.rs` module, following Herdr's focused shell mouse module.
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
  the matching Herdr action exists, including swapping a clicked pane with the
  previously focused pane.
- [x] Bind focused-pane zoom to `Ctrl-b z`, matching Herdr's default.
- [x] Align tab creation, pane/tab close, tab navigation, split, and pane-focus
  shortcuts with Herdr's default bindings.
- [x] Add Herdr's default `Ctrl-b`, then `Shift+n` new-workspace shortcut;
  creating from the focused project starts and focuses its first shell.
- [x] Add Herdr's default `Ctrl-b`, then `Shift+w` rename-workspace shortcut.
- [x] Add Herdr's default `Ctrl-b`, then `Shift+d` close-workspace shortcut;
  keep Spindle's type-name confirmation before stopping its panes.
- [ ] Match Herdr's workspace navigation bindings: `Ctrl-b w` previews
  workspaces; `Ctrl-b g` opens a searchable space/workspace/tab/pane tree with
  Herdr-style movement, filters, expansion, activation, and cancellation. The
  navigator is implemented; searches now auto-expand collapsed branches to
  expose matches and select the matching workspace/pane rather than Spindle's
  extra parent-space row. The workspace preview accepts Herdr's `1`–`9` direct
  workspace selection keys and now cycles across all local spaces. Windows
  checks verified `Ctrl-b w`, direct `1` selection, `Ctrl-b g`,
  search-to-workspace activation, and `Esc`. Live filter and collapsed-branch
  checks remain before marking parity complete.
- [x] Support drag-select/copy and terminal mouse passthrough without stealing
  input from applications that request it. Wheel and mode-aware PageUp/PageDown
  now browse recent PTY history, which the same selection/copy path can read;
  history is bounded by the PTY byte ring and reconstructed at the current
  pane size.
- [x] Add keyboard copy mode on the matching prefix+[ binding, with Herdr-style
  cursor motion, search, character/line selection, and copy. Keep PTY output
  live while browsing and preserve the viewport until copy mode exits.
- [x] Test copy-mode input, history boundaries, search results, selection text,
  Unicode cell mapping, and output arriving while the user browses history.
- [x] Keep keyboard command-palette actions for pane, tab, and workspace
  actions available through the mouse, including clearing a pane's manual name.
- [x] Add in-app help that explains both keyboard shortcuts and mouse controls.
- [x] Ctrl-click visible `http://` and `https://` URLs in panes to open them;
  use terminal display columns for hit testing and trim sentence punctuation.
- [ ] Add the full Herdr plugin system. OSC 8 hyperlink metadata is now tracked
  per terminal cell, carried in pane snapshots, persisted with recent history,
  and preferred by Ctrl-click. Spindle also dispatches matching local
  `herdr-plugin.toml` link-handler actions with clicked-URL environment
  variables. Local `plugin link/list/unlink/enable/disable` registration and
  `plugin action list/invoke`, `plugin config-dir`, local manifest pane opening
  (`split`, `tab`, `zoomed`), and plugin pane focus/close are now available,
  with plugin config/state/context variables; shallow GitHub install is now
  available, with bounded plugin launch logs, Windows platform filtering, and
  pane environment overrides;
  popup panes now use a transient server popup slot, preserve the tiled
  background focus, render centered terminal content, accept popup placement
  from the plugin CLI, and close when the process exits. Overlay panes now
  follow Herdr's temporary zoomed split behavior and restore background focus
  and zoom state when they close. Popup mouse routing is implemented; native
  popup actions and richer overlay actions remain.
- [ ] Test input routing, hit regions, drag boundaries, menu actions, and
  passthrough behavior. Tests now cover pane edge hits, captured drags outside
  pane bounds, event-mode routing, and right-click passthrough; existing tests
  cover menu items and split-drag limits. Manual Windows Terminal verification
  remains.

Reference: Herdr `src/client/shell/mouse.rs`, `copy_mode.rs`, `context_menu.rs`,
`selection.rs`, `src/api/schema/panes.rs` (`PaneCopyMotion`, `PaneCopySearch`),
`src/app/api/panes.rs`, `src/pane/terminal.rs` (copy motions),
`src/app/actions.rs` (`url_at_column`, `url_at_pane_surface_cell`),
`src/app/api/plugins/mod.rs` (`handle_pane_link_activate`), and quick-start
"Use the mouse". Spindle: `src/client/app.rs`, `src/client/links.rs`,
`src/client/input.rs`, and `src/client/renderer.rs`.

Copy-mode difference: Herdr asks the server's retained terminal for
revision-checked motions and searches. Spindle builds a local `vt100` text
buffer from its existing 64 KiB pane byte ring, so no protocol change is
needed; history is limited to that retained ring. Keyboard flow, motions,
search direction/repeat and case rules, selection/copy, and live-output
viewport pinning are covered by client tests. Full tests, strict Clippy, release
build, Windows smoke, and installer smoke passed. Direct interactive Windows
Terminal verification is still pending.

## 4. Add agent-aware workspace status

- [ ] Port process/terminal agent detection using Herdr's manifests and state
  transitions as behavioral references, not copied implementation. Partial:
  Recent Amp, Antigravity, Cline, Copilot, Cursor, Devin, Gemini, Droid, Grok, Hermes, Kilo, Kiro, Kimi, Maki, Muse, OpenCode, Qoder CLI, and Qwen rule functions are now grouped under
  `src/detect/agents/`, matching Herdr's per-agent manifest organization.
  direct and wrapped Pi/Qoder CLI/Droid/Grok/Hermes/Kilo/Kiro/Kimi/Maki/Muse/Qwen/Cline/Devin/Cursor/Amp/Antigravity/Claude/Codex/Gemini/
  OpenCode/GitHub Copilot processes, Herdr-aligned Pi `Working...`, Kimi
  approval/question panels and working cues, Qoder CLI
  and Droid blocker/working cues, Kiro blocker/working/idle cues, Cline approval
  prompts and visible-output working rule, Devin prompt/state cues, Cursor's
  Windows Node install-path recognition and status cues, Amp OSC-title/footer
  cues, Kilo permission/interrupt cues, Antigravity confirmation/spinner/background-task cues, Hermes OSC-title and multi-panel
  blocker cues, Qwen prioritized title/screen/OSC state rules, Grok permission and title/OSC/footer cues, Maki permission/status-bar/prompt cues, Muse approval/idle rules and menu state retention, and Copilot selection blocker,
  cancel-hint and background-agent working cues, Claude OSC title/progress
  and selected prompt signals, Claude live-turn/background-agent/MCP and `/btw`
  working cues, selected manifest blocker prompts, and done status after two
  consecutive confirmed process scans while retaining the pane's identity;
  unavailable process scans do not change status,
  Herdr-aligned OpenCode permission/interrupt, Gemini confirmation/cancel cues,
  Codex directory-trust and transcript-viewer rules, and Claude
  transcript/model-picker status preservation plus MCP input-request blockers
  and dynamic-workflow blockers
  are implemented; plain working-to-idle transitions now use Herdr's 100 ms
  recheck, three confirmations, and 700 ms cap. Newly detected agents remain
  unknown for Herdr's three-second startup grace so stale prior-screen text is
  not reported as the new agent's state. Herdr-style versioned Python wrappers
  are checked for Hermes commands, while Python `-c`/`-m` commands are excluded
  to avoid shell false positives. Full manifest rules remain.
- [x] Expose agent kind and unknown/idle/working/blocked/done state in pane chrome and
  the sidebar. Pane chrome shows identity/state; the sidebar lists each detected
  agent under its workspace with a state marker and tab, and selecting a row
  opens that space/workspace/tab and focuses the pane; within each grouped tab,
  the sidebar supports Herdr-style grouped and priority views, ordered
  blocked/done/working/idle/unknown in priority mode. The selected view is
  saved per client. Richer
  lifecycle transitions remain.
- [ ] Keep state associated with the correct pane, tab, and workspace through
  switches, restarts, and recovery. A session test now verifies status remains
  attached to its pane across tab and workspace switches; a restart test now
  verifies stale agent identity, scrollback, title, cursor, and terminal modes
  are cleared while pane labels and user preferences survive. Re-detection
  after recovery still needs live verification.
- [x] Make detection optional/fault-tolerant so normal shell use is unaffected.
  Detection runs only for recognized agents; unavailable process scans preserve
  the last known identity/status, and detection is separate from PTY input,
  output, and terminal parsing. `pane::manager::tests::unavailable_process_scans_do_not_change_agent_identity_or_status`
  verifies the unavailable-scan behavior, matching Herdr's best-effort scan
  boundary in `src/pane/agent_detection.rs`.
- [ ] Test detection fixtures and status transitions; verify Codex and OpenCode
  side-by-side on Windows.

Reference: Herdr `src/pane/agent_detection.rs`, `src/detect/`,
`src/ui/sidebar.rs`, and `src/client/shell/agent_sidebar.rs`. Spindle currently
describes agents as user-run commands in panes; preserve that unless a separate
approved design changes it.

## 5. Audit and port the remaining user-facing Herdr behavior

- [ ] Extend the parity matrix across CLI, configuration/keybindings/themes,
  notifications, session recovery, integrations, remote sessions, and plugins.
- [x] Keep client orchestration categorized like Herdr's shell modules; startup
  failure actions now live in `src/client/startup.rs` instead of the main event
  loop module.
- [x] Add a read-only `spindle workspace list` CLI command, matching Herdr's
  workspace-list entry point and showing active state across spaces.
- [x] Add `spindle workspace focus <workspace_id>`, matching Herdr's CLI and
  allowing focus across Spindle spaces.
- [ ] Port each behavior that fits Spindle's Windows-first/local-first purpose
  in a separately tested commit. Pane movement now supports Herdr's
  existing-tab destination as well as a new tab, with optional target-pane
  splitting in the active workspace.
- [ ] For behavior not included, record the concrete reason and an equivalent
  Spindle workflow; do not silently omit it.
- [ ] Improve Windows-specific process launch, terminal input/output, resizing,
  recovery, installer, and diagnostics based on observed gaps in the matrix.
  `spindle doctor` now labels endpoints missing/reachable/stale and gives a
  start/attach recovery hint for stale metadata, matching Herdr's explicit
  running/not-running server status. It also prints the client version, binary
  path, and protocol so users can identify which build they launched.
  Windows PTY children now use kill-on-close Job Objects so child tools do not
  outlive their pane; assignment safely falls back when Windows denies it.
- [x] Make `spindle stop` wait for endpoint cleanup and retry Windows named-pipe
  wakeups through the listener handoff race, following Herdr's bounded server
  stop wait. The Windows and installer smoke scripts pass after stale-endpoint
  recovery.

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
