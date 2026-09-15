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
- [x] Keep mouse hit-testing, routing decisions, and URL target lookup in the
  same dedicated module, while leaving request-producing handlers in `app.rs`.
- [x] Open the visible notification target with Herdr's `Ctrl-b o` binding;
  global pane focus activates its space, workspace, and tab first.
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
- [x] Keep space and workspace expansion markers open while navigator filtering
  exposes matching descendants, matching Herdr's filtered tree behavior.
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
- [x] Add Herdr's sidebar `menu` launcher with a modal menu for help, the
  command palette, config reload, and detach actions.
- [x] Add Herdr's clickable sidebar `new` control for creating and focusing a
  workspace from the current project directory.
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
  pane environment overrides. GitHub installs now run supported `[[build]]`
  commands in the temporary plugin checkout before registration and reject
  failed builds with bounded output, while local links remain author-built;
  enabled plugins now also run Herdr-style `[[startup]]` hooks once after the
  server endpoints are ready, with isolated failures and startup context /
  environment variables;
  Herdr-style manifest event hooks now support `pane.created`, `pane.focused`,
  `pane.closed`, `pane.updated`, `pane.exited`, `pane.moved`, `pane.agent_detected`, `pane.agent_status_changed`, `layout.updated`, `tab.created`, `tab.focused`, `tab.closed`, `workspace.focused`, `workspace.created`, `workspace.renamed`, and `workspace.closed`, delivered from the server event history with the
  triggering event and active session context; other Herdr event kinds remain
  to be ported;
  `spindle pane current` now accepts Herdr's `--pane` and `--current` selector
  forms and honors the Spindle/Herdr pane environment IDs;
  `spindle pane read` accepts the same selectors while keeping the positional
  pane ID form;
  `spindle pane list` now lists all session panes by default, matching Herdr;
  `--workspace` remains available to narrow the result;
  `spindle pane read` now defaults to Herdr's `recent` source while preserving
  explicit `--source visible` reads;
  pane reads also accept Herdr's `recent-unwrapped`, `detection`, `--format`,
  `--ansi`, and `--raw` forms, stripping terminal escapes for text output;
  recent and recent-unwrapped reads default to Herdr's last 80 lines, while
  visible and detection reads remain unbounded unless `--lines` is supplied;
  `spindle pane neighbor` and `spindle pane edges` expose Herdr-style
  directional layout metadata for scripts;
  `spindle pane layout` exposes the active tab's layout tree as JSON;
  generated shell completions expose all four pane introspection commands;
  `spindle pane process-info` reports live PTY PID and launch metadata without
  persisting process details in session snapshots;
  `spindle pane focus --direction ... --pane ID|--current` can start from an
  explicit pane, matching Herdr's selector-aware directional focus;
  `spindle pane input ... --right-click herdr|pane` explicitly sets the
  Herdr-compatible right-click routing policy for a pane;
  pane split accepts Herdr's `--pane`/`--current`, `right|down`, `--cwd`, and
  repeated `--env` options while retaining the legacy split form;
  split options also support Herdr's ratio, focus, and right-click controls;
  pane rename accepts Herdr's `--clear` label form;
  pane resize accepts Herdr's directional `--amount` and pane selector form;
  pane move-to-tab accepts Herdr's `--ratio` option;
  pane moves accept Herdr's `--focus` and `--no-focus` controls, including
  moving to a new tab in another workspace with `--workspace`;
  pane move source IDs resolve across spaces, workspaces, and tabs;
  moving a live pane into a new Herdr-style workspace with `--label` and
  `--tab-label`;
  popup panes now use a transient server popup slot, preserve the tiled
  background focus, render centered terminal content, accept popup placement
  from the plugin CLI, and close when the process exits. Overlay panes now
  follow Herdr's temporary zoomed split behavior and restore background focus
  and zoom state when they close. Popup mouse routing is implemented; native
  popup actions and richer overlay actions remain. CLI action invocation now
  best-effort enriches plugin context from the active server snapshot with
  workspace, tab, and focused-pane identity/cwd, and exports Herdr-compatible
  context environment variables. Plugin actions and link handlers now use a
  Herdr-style Windows command launcher so relative executables and `.cmd`/
  `.bat` entrypoints work from the plugin directory. The command palette now
  exposes a Herdr-style plugin action entry point that accepts an action ID,
  supports `plugin.id.action` disambiguation, and passes active workspace/tab/
  pane context to the launched process. Plugin panes now receive the same
  active workspace/tab/focused-pane context and Herdr-compatible environment
  variables;
- [x] Route keyboard input to an open plugin popup before the background pane,
  and make the normal close-pane binding close that popup first, matching
  Herdr's popup input ownership.
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
- [x] Provide a scriptable agent report. `spindle agent list` reports every
  detected agent across spaces with pane, workspace, tab, focus, state, cwd,
  and terminal title, following Herdr's agent-listing direction. Agent commands
  also accept a unique case-insensitive detected agent name, matching Herdr's
  pane-ID-or-name target behavior; ambiguous names require a pane ID. Prompts
  support Herdr-style `--wait`, repeated `--until`, and `--timeout` options.
  `agent start` launches a supported agent kind in an existing shell pane and
  waits for detection, matching Herdr's existing-pane startup workflow.
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
- [x] Add a Herdr-style settings overlay from the global menu for built-in
  themes, notification delivery, and notification sound; changes persist to
  the user config and take effect on the next client loop iteration.
- [x] Match Herdr's default `Ctrl-b s` Settings binding; stop-pane remains
  available through the command palette and explicit `stop_pane` bindings.
- [x] Preserve terminal key modifiers in fallback input. Ctrl-C and modified
  cursor keys now reach the PTY as control/CSI sequences instead of losing
  their modifiers.
- [x] Extend fallback input encoding to Herdr's modified Home/End, Insert/
  Delete, Page, and function-key CSI sequences, plus plain special keys.
- [x] Add Herdr's `Ctrl-b Shift-r` reload-config binding and show a short
  confirmation when reload is invoked from the keyboard or global menu.
- [x] Keep `spindle config default` aligned with the supported Settings and
  reload bindings so the starter file is discoverable and usable.
- [x] Add Herdr-style `[[keys.command]]` commands with direct or prefix key
  bindings. Shell commands use Windows `cmd.exe /d /c`; pane commands use the
  temporary overlay lifecycle and popup commands use the transient centered
  popup. Plugin-action commands use the installed plugin launcher. All
  command types receive focused-pane cwd and active workspace/tab/pane
  environment variables.
- [x] Add mouse navigation to the Settings overlay for section and value
  selection, with outside-click dismissal.
- [x] Keep the normal narrow sidebar title compact so workspace controls stay
  readable at the default terminal width, matching Herdr's compact sidebar
  header.
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
  splitting in the active workspace. Tab focus, rename, close, and workspace
  deletion now resolve IDs across the session's spaces and workspaces. Pane
  close also resolves pane IDs outside the active tab while preserving the
  existing workspace recovery rules. The CLI now also supports Herdr-style
  workspace create/close, and workspace creation starts a PowerShell pane in
  the requested directory. Workspace creation follows Herdr's default no-focus
  behavior; `--focus` opts into selecting the new workspace. Repeated
  `--env KEY=VALUE` options are applied to its initial PowerShell pane. Tab
  creation also accepts Herdr's label, workspace destination, cwd, environment, and
  focus controls while preserving the current workspace/tab by default.
  Tab listing accepts `--workspace` for inactive-workspace inspection without
  changing focus.
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
