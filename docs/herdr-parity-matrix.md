# Herdr behavior parity matrix

Plugin hook note: high-volume `pane.updated` and `layout.updated` hooks are
intentionally rejected and ignored, matching Herdr's
`PLUGIN_HOOK_EVENT_KINDS` in `src/api/schema/events.rs`.

Reference: `C:\Users\Palguna\.opensrc\repos\github.com\herdrdev\herdr\master`.

Integration parity now includes Mastracode. `spindle integration status` also
reports `current`, `outdated`, or `not-installed` using managed hook version
markers, following Herdr `src/integration/registry.rs`. Antigravity CLI
availability follows Herdr's `agy` executable name while retaining Spindle's
`antigravity-cli` label.

Live-pane TUI parity now keeps Spindle's reserved status row rendered when a
connected pane is present. This follows Herdr's explicit modal/footer area
handling in `src/ui/widgets.rs` and is covered by the connected-pane renderer
test; interactive Windows Terminal verification remains pending.

Desktop bottom-tab parity now places prefix, copy, and resize mode bars in the
one-line bottom tab row, matching Herdr's `src/client/shell/composition.rs`
and `src/client/shell/config.rs` layout. Spindle's pane/tab constraints and
mode-bar geometry are covered by renderer tests; interactive Windows Terminal
verification remains pending.

First-run onboarding now follows Herdr's missing-setting default: Spindle
shows a welcome overlay until Enter or Esc continues, then persists the
top-level `onboarding = false` setting while preserving unrelated config.
This is covered by config and renderer tests; interactive Windows Terminal
verification remains pending.

Keybinding precedence now follows Herdr: a valid configured chord displaces a
conflicting built-in chord, including when the actions differ. The behavior is
covered by `configured_binding_wins_when_it_reuses_a_default_chord`; malformed
bindings still preserve the default.

Theme selection now exposes Herdr's full built-in theme list, including light,
One, Solarized, Kanagawa, Rose Pine, and Vesper variants. The new choices map
to matching Herdr accent colors and are covered by renderer tests; custom
palette token overrides remain to be ported.

The first custom palette slice is now supported: `[theme.custom] accent`
accepts `#rgb` and `#rrggbb` overrides on top of the selected base theme,
with config and renderer coverage.

Custom accents also accept Herdr's `rgb(...)`, named-color, and reset aliases;
invalid values retain the base-theme accent.

Modal surfaces now use the selected theme's Herdr-style panel background;
`[theme.custom] panel_bg` accepts the same color formats and is covered by
config and renderer tests.

Spindle now uses Herdr's Catppuccin palette when no theme is configured, and
the settings overlay selects that same default.

The selected theme accent now drives the visible sidebar, tabs, mobile header,
navigator, and drag indicators; broader palette tokens remain to be ported.

Notification parity now includes `spindle notification show <title>` with
Herdr-compatible `--body`, `--position`, and `--sound` arguments. Delivery
uses the existing Windows platform notification and sound adapters and works
without a session server.

Plugin action parity now preserves Herdr action descriptions and context tags
from `herdr-plugin.toml`. `spindle plugin action list --json` exposes the
structured metadata; the default text output remains backward compatible.

Plugin pane descriptions are preserved and exported in the pane invocation
context, matching Herdr's manifest metadata contract.

Plugin popup borders now display the pane label/title with a command fallback,
matching Herdr's declared popup title behavior.
Popup border hit testing now keeps the popup modal instead of allowing clicks
to fall through to the background layout.
Custom popup commands accept numeric cell sizes and percentage dimensions,
matching Herdr's `[[keys.command]]` configuration behavior.

Numeric manifest popup dimensions are honored for plugin panes; CLI dimensions
override manifest defaults. Width and height also accept Herdr's percentage
forms, such as `80%`, retained in session state and recomputed on terminal
resize for rendering, hit testing, and PTY dimensions.

API parity now includes `spindle api snapshot`, which returns the live session
snapshot as pretty JSON; Bash, Fish, Zsh, PowerShell, and Elvish completions
include the command. Windows server startup uses the Herdr-compatible direct
daemon launch path and was verified by the release Windows and installer smoke
checks.

Agent lifecycle state is now isolated in `src/pane/agent_detection.rs`, following
Herdr's `src/pane/agent_detection.rs`; process grace, idle confirmation, and
agent-exit confirmation keep the same behavior while the manager stays focused
on pane I/O.

Pane wait note: `spindle pane wait-output` now supports Herdr's `--regex`
matcher alongside literal `--match`, `visible`/`recent`/`recent-unwrapped`
sources, and `--raw`, including mutual-exclusion and invalid-pattern validation.

Agent hook parity now includes `spindle pane report-agent` and
`spindle pane release-agent`. Reports are source/sequence aware, and reported
state remains authoritative until that source releases the pane, matching
Herdr's pane hook lifecycle handlers.
Verified with the release binary on Windows: sequence 4 `working` was retained,
stale sequence 3 `idle` was ignored, and sequence 5 released the pane.

CLI parity now includes Herdr's `pane current --pane ID` and `--current`
selector forms, with `SPINDLE_PANE_ID`/`HERDR_PANE_ID` environment fallback.
The same selectors now work with `spindle pane read`.
`spindle pane list` now lists all session panes by default; `--workspace` still
narrows the result to one workspace.
Pane reads default to Herdr's `recent` source and accept explicit `visible`
or `recent` selection, plus the `recent-unwrapped`, `detection`, `--format`,
`--ansi`, and `--raw` forms; text output strips terminal escape sequences.
Recent reads default to the last 80 lines, matching Herdr; visible and
detection reads stay unbounded unless a line limit is requested.
`pane neighbor` and `pane edges` now expose directional layout metadata with
Herdr-style pane selectors.
`pane layout` exposes the active tab's layout tree as JSON.
`pane process-info` reports the live PTY PID and launch metadata. All five
generated shell completion scripts include these pane commands.
`pane focus --direction ... --pane ID|--current` now supports Herdr's
selector-aware directional focus.
`pane input ... --right-click herdr|pane` now explicitly sets right-click
passthrough for scripts, matching Herdr's pane input API.
Pane split also accepts Herdr's `--pane`/`--current`, `right|down`, `--cwd`,
and repeated `--env` options.
Its ratio, focus, and right-click controls are also supported.
Pane rename also supports Herdr's `--clear` form.
Pane resize also accepts directional `--amount` and pane selectors.
Move-to-tab also accepts Herdr's `--ratio` option.
Move-to-tab also accepts Herdr's `--focus` and `--no-focus` controls.

Latest CLI parity: `spindle pane move <id>` preserves the live pane process
while moving it to a new tab, an existing tab, or a new workspace. The
existing-tab form supports an optional target pane and right/down split; the
new-workspace form supports workspace/tab labels and focus control.
The new-tab form also supports Herdr's optional `--workspace` destination.
Move source IDs are resolved across all Spindle spaces, workspaces, and tabs.
`spindle tab create` now accepts Herdr's `--label`, `--workspace`, `--cwd`, repeated
`--env`, and `--focus`/`--no-focus` controls; it starts a PowerShell pane and
preserves the current workspace/tab by default.
`spindle tab list --workspace ID` lists tabs in an inactive workspace without
changing focus.
`spindle tab move ID INSERT_INDEX` reorders tabs within their workspace without
changing the focused tab, and emits the Herdr-compatible `tab.moved` plugin
lifecycle event.
`spindle workspace move ID INSERT_INDEX` reorders a workspace within its space
without changing the active workspace, preserves linked-worktree groups, and
emits `workspace.moved` for plugins; linked children cannot be moved alone.
Plugin lifecycle parity now also covers the Herdr `tab.closed` and
`workspace.focused`, `workspace.created`, `workspace.renamed`, `workspace.moved`, and
`workspace.closed`, plus `pane.moved`, `layout.updated`, `pane.agent_detected`,
and `pane.agent_status_changed` event hooks;
the lifecycle row below records the remaining workspace and layout gaps.
Statuses describe the current Spindle checkout, not the target. “Partial” means
some server/model support exists but the user-facing behavior is missing or
does not yet match Herdr.

Agent navigation parity note: Spindle now accepts Herdr-compatible
`previous_agent` and `next_agent` bindings and wraps across detected panes.
The Herdr-compatible `[ui] sidebar_start_collapsed` setting is also honored
on startup, while saved client preferences still take precedence.

Sidebar layout parity note: the nested `[sidebar.agents]` and `[sidebar.spaces]`
row-token layouts now drive the rendered sidebar, scrolling, scrollbar mapping,
hit testing, and workspace drop targets. The default two-line workspace and
agent rows follow Herdr's `src/config/sidebar.rs` and
`src/client/shell/sidebar.rs`; focused agent styling is retained across both
lines. Configured state, workspace, branch, agent, machine, tab, pane, title,
and custom metadata tokens are resolved in the renderer, while missing values
are omitted. Agent-specific `rows_by_agent` layouts use canonical Herdr agent
keys. Rust tests, release build, Windows smoke, and installer smoke pass.
Configured `row_gap` spacing is included in scroll metrics only between the
same entry classes and leaves blank, non-selectable visual rows, matching
Herdr’s separated row geometry.

| User behavior | Spindle status | Herdr reference | Evidence / next verification |
| --- | --- | --- | --- |
| Filtered navigator branches show expanded state | Present | Herdr `src/client/shell/aggregate_navigation.rs` (`filtering` expands workspace children) | Spindle marks matching space/workspace rows expanded while filtering; regression coverage verifies collapsed branches expose descendants with open markers. |
| Workspace navigation shortcuts | Partial | Herdr `src/config/model.rs` defaults `workspace_picker` to `prefix+w` and `goto` to `prefix+g`; `src/client/shell/actions.rs`, `overlay_input.rs`, `aggregate_navigation.rs`, `overlays.rs`, `mouse.rs`; `docs/preview/website/src/content/docs/keyboard.mdx` | Spindle `Ctrl-b w` previews workspaces without switching until Enter. `Ctrl-b g` opens a searchable space/workspace/tab/pane tree, starts at the current pane, and supports Herdr-style movement, status filters, expansion, switching, cancellation, and pointer selection. Automated navigator and mouse hit-test tests pass. Release-binary PTY checks verified startup with a focused PowerShell pane, workspace preview, navigator search, collapsed-branch auto-expansion, the working-status filter, and clean detach. Windows Terminal side-by-side checks remain. |
| Agent startup status grace | Implemented | Herdr `src/pane/agent_detection.rs` (`AGENT_STARTUP_GRACE_WINDOW`); Spindle `src/pane/manager.rs` | Both keep a newly detected agent `Unknown` for three seconds before classifying screen state, preventing stale previous-process text from briefly showing an incorrect status. Spindle unit tests cover the grace deadline and removal behavior. |
| Agent identity across tab/workspace switches | Present | Herdr `src/pane/agent_detection.rs`; Herdr `src/app/actions.rs::mark_active_tab_seen`; Spindle `src/server/session.rs` (`refresh_snapshot`, `switch_tab`, `switch_workspace`) and `src/pane/manager.rs` | A regression test assigns distinct agent states to panes across two tabs and two workspaces, switches focus through each container, and verifies the states remain keyed to their pane IDs. Completed agent panes are marked seen when their tab becomes active, matching Herdr's active-tab behavior. Live UI and re-detection-after-recovery verification remain. |
| Start or attach to a persistent session | Partial | `docs/preview/website/src/content/docs/quick-start.mdx`, `src/app/mod.rs` (`ensure_default_workspace`), `src/server/headless.rs`, `src/workspace.rs` (`Workspace::new`), `src/pane/terminal.rs` | Herdr creates its first shell with the workspace and recreates a default workspace when a client connects to an empty session. Spindle creates a default workspace and PowerShell pane on attach, including after the last workspace was closed; a live server/client test covers that recovery. Spindle restarts saved `Interrupted` panes in place and answers PowerShell's cursor-position query. Fresh release-binary PTY verification showed `pane-1`, focused state, sidebar, tab strip, PowerShell prompt, and clean detach; Windows Terminal-specific visual verification remains. |
| Start with a live shell; create shell-backed tabs/workspaces | Partial | `src/workspace.rs` (`Workspace::new`, `create_tab_with_runtime`) | Herdr creates the initial shell with the workspace. Spindle ensures one after attach and container creation, using the active workspace repository path; concurrent ensure calls are tested to produce one pane. Integration tests cover tab/workspace/space creation, idempotency, and focus; live Windows TUI verification remains. |
| Manage sessions and terminal resources from scripts | Partial | Herdr `docs/preview/website/src/content/docs/cli-reference.mdx` (Sessions, Workspaces, Tabs, Panes); Herdr `src/cli/completion.rs`, `src/cli/api.rs`; `src/main.rs` (`--version`, `--help`); Herdr `src/cli/pane.rs`; Spindle `src/cli.rs`, `src/cli/workspace.rs`, `src/cli/tab.rs`, `src/cli/pane.rs`, `src/cli/completion.rs`, `src/cli/api.rs`, `docs/api/spindle-api.schema.json`, `src/server/control.rs` | Spindle's CLI supports project-local `start`, `attach`, `stop`, `list`, and `doctor`, config discovery, workspace list/create/get/focus/rename/close, tab list/create/get/focus/rename/close, pane list with `--workspace`, current/get/focus-by-id/focus-by-direction/rename/stop/restart/zoom with toggle/on/off modes/close/send-text/send-keys/run/read with visible or bounded recent sources/swap by direction or explicit source/target/split/resize/wait-output, completions for Bash, Elvish, Fish, PowerShell, and Zsh, and `spindle api schema [--json|--output PATH]`. Workspace create supports `--cwd`, `--label`, repeated `--env KEY=VALUE`, and Herdr-style default no-focus behavior; `--focus` opts into selecting the new workspace. It starts a PowerShell pane in the requested or current directory while preserving the previous workspace when requested. Close resolves workspace IDs globally. The bundled schema reports protocol 1 and 38 current control operations. Pane listing and mutations use the active tab's server focus scope, while pane neighbor, edges, and layout inspection resolve pane IDs across the session; workspace listing includes all tabs in the selected workspace. Stable IDs and release Windows smoke and installer smoke pass. Richer context-aware completions, Herdr's remaining pane selectors/read formats/agent reporting, and schema parity with Herdr's larger API remain. |
| Navigate spaces/workspaces and tabs in a sidebar | Partial | `src/client/shell/sidebar.rs`, `src/client/shell/workspace_navigation.rs`, Herdr `src/client/shell/config.rs`, `src/client/shell/preferences.rs`, Herdr `src/client/shell/sidebar.rs`, `src/client/shell/mouse.rs` | Spindle renders a space/workspace sidebar and tab strip; clicks switch containers. The sidebar footer now has Herdr's clickable `new` workspace control and `menu` launcher. `Ctrl-b w` previews a workspace; `Ctrl-b g` opens a searchable tree across spaces, workspaces, tabs, and panes. `Ctrl-b b` and the sidebar footer toggle compact mode, where markers remain clickable; the choice is saved per project. Long lists scroll with the wheel; track clicks jump and thumb drags preserve the grab offset. Resize-driven layout polish and live verification remain. |
| Workspace sidebar status reflects agent attention | Present | Herdr `src/client/shell/sidebar.rs` (`displayed_workspace_status`), `src/client/shell.rs` (`status_priority`) | Spindle aggregates agent states attached to each workspace and renders the highest-priority Herdr marker (`blocked`, `done`, `working`, `idle`, `unknown`); collapsed linked-worktree groups aggregate their members. `workspace_sidebar_uses_the_highest_priority_agent_state` covers the rendering path. |
| Render pane layouts with focus, terminal state, correct geometry | Partial | Herdr `src/ui/panes.rs`, `src/client/shell/render.rs::render_mode_bar`, `src/layout.rs` | Shared pane rectangles drive drawing, clicks, and per-PTY sizes. Spindle now propagates terminal cursor visibility and positions the host cursor in the focused pane; it hides the cursor for overlays and scrollback, matching Herdr's focused/live-pane rule. Connected terminal panes use the full height and have no permanent status row, while connection, empty-state, and navigation chrome remain available. Cursor placement and small-terminal/many-pane geometry tests pass. Fresh release-binary PTY verification rendered the focused pane and prompt; Windows Terminal-specific geometry verification remains. |
| Click panes/tabs/workspaces; drag split borders | Partial | `src/client/shell/mouse.rs`, `src/client/shell/composition.rs`, `src/client/shell/sidebar.rs`, `src/client/shell/tabs.rs`, `src/layout.rs` | Spindle handles clicks, tab/workspace row reordering, and divider drags. Tab and workspace drops use Herdr's insertion-index semantics and show live cyan drop indicators; a plain click still switches the container. Drag updates the exact nested split by tree path, saves the ratio, and triggers per-pane PTY resizing. Unit and server-client tests cover hit paths, collapsed-sidebar geometry, persisted nested ratios, and insertion order; manual Windows Terminal verification remains. |
| Right-click context menus for pane/tab/workspace actions | Partial | Herdr `src/workspace.rs` (`remove_pane`); `src/app/actions.rs` (`close_selected_workspace`); `src/app/api/panes.rs` (`handle_pane_zoom`, `handle_pane_swap`, `handle_pane_resize`); `src/app/api/tabs.rs` (`handle_tab_close`); `src/client/shell/context_menu.rs` | Spindle removes a tab when its last pane closes and a sibling tab exists; closing the final tab or pane closes that workspace and selects a sibling when available. Tab close, pane close, pane zoom, same-tab pane swap, and explicit pane resize requests now resolve IDs across spaces and workspaces, activating the target container first. With no workspace left, the attached client recreates a current-project workspace and shell, matching Herdr's automatic recovery; the empty server state still survives restart before a client attaches. Integration tests cover closing, restart, recovery, and inactive-workspace zoom/swap/resize. Pane split, rename/clear-name, focus, swap-with-focused, toggle-zoom, and right-click passthrough are implemented. Workspace context menus now detect linked worktree children, label the action as a grouped close, and send Herdr's explicit `close_group` confirmation. Git workspace menus also expose typed New worktree, Open worktree, and Remove worktree checkout actions through the existing CLI workflows. Group menus can toggle child visibility, with the same collapsed row model used by scrolling, hit-testing, and drag targets. Empty tabs now explain shell/tab recovery. Unit tests cover swap layout/focus, menu visibility, grouped-close detection, grouped row hiding, and worktree action labels; live Windows Terminal verification remains. |
| Select/copy terminal text and route terminal mouse input | Partial | Herdr `src/client/shell/mouse.rs`, `selection.rs`; Spindle `src/client/mouse.rs`, `src/client/selection.rs`, `src/client/app.rs`, `src/server/session.rs` | Spindle supports visible-screen drag selection, double-click word selection, and copy; wheel and mode-aware PageUp/PageDown expose up to 4,096 rows reconstructed from its bounded PTY history. Deep offsets beyond one viewport are covered by the scrollback regression test; upgrading to `vt100` 0.16.2 fixes its visible-row offset panic. Mouse requests from foreground apps retain priority. Pane hit testing and Ctrl-click URL lookup now exclude the scrollbar gutter and reuse the renderer's pane inner-area geometry, matching Herdr's separate terminal and scrollbar rectangles. Forwarded PTY mouse coordinates now use that same inner-rect origin and one-based encoding. New tests cover pane hit boundaries, scrollbar-gutter exclusion, inner-rect coordinate mapping, drag capture beyond pane bounds, event-mode routing, and right-click passthrough. Unlike Herdr's backend scroll model, Spindle history is bounded by its recent-output byte ring and rebuilt at current pane dimensions; live Windows Terminal verification remains. |
| Keyboard copy mode | Partial | Herdr `docs/preview/website/src/content/docs/keyboard.mdx` (Copy mode), `src/client/shell/copy_mode.rs`, `src/app/api/panes.rs`, `src/pane/terminal.rs`; Spindle `src/client/copy_mode.rs`, `src/client/input.rs`, `src/client/app.rs`, `src/client/renderer.rs` | Spindle binds `Ctrl-b [` and supports Herdr-style cursor/word/paragraph motions, forward/backward case-aware search, character/line selection, clipboard copy, and live-output viewport pinning. Tests cover input mapping, search, selection, Unicode cells, retained-history limits, and new output during browsing. Unlike Herdr's revision-checked server motions over retained terminal rows, Spindle reconstructs a local view from its bounded 64 KiB PTY byte ring. Automated tests and Windows smoke pass; interactive Windows Terminal verification remains. |
| Ctrl-click links in panes | Partial | Herdr `src/client/shell/mouse.rs`, `src/app/actions.rs` (`url_at_pane_surface_cell`, `url_at_column`), `src/app/api/plugins/mod.rs` (`handle_pane_link_activate`), quick-start "Use the mouse" | Spindle opens visible `http://` and `https://` URLs from the pane screen, maps clicks through terminal display columns, trims sentence punctuation, tracks OSC 8 hyperlinks across chunks, and invokes matching trusted local manifest actions before the browser fallback. Plugin link actions now receive Herdr-style pane ID and working-directory context through JSON and environment variables. `spindle plugin link/list/unlink/enable/disable` now manages global local registrations. Herdr's remote install, action/pane commands, and richer invocation context remain. Live Ctrl-click verification in Windows Terminal remains. |
| Discover and use keyboard and mouse controls | Partial | `src/main.rs` `[keys]`, `docs/preview/website/src/content/docs/keyboard.mdx`, `src/client/shell/context_menu.rs` | Spindle now matches Herdr's defaults for `Ctrl-b c` new tab, `Ctrl-b x` close pane, `Ctrl-b Shift+x` close tab, `Ctrl-b n/p` tab navigation, `Ctrl-b 1–9` direct tab selection, `Ctrl-b Tab/Shift-Tab` pane cycling, `Ctrl-b Shift+t` tab rename, `Ctrl-b Shift+p` pane rename, `Ctrl-b v/-` splits, `Ctrl-b h/j/k/l` pane focus, `Ctrl-b Shift+h/j/k/l` pane swapping, `Ctrl-b z` zoom, `Ctrl-b r` resize mode, `Ctrl-b e` editor launch for focused scrollback, `Ctrl-b s` Settings, `Ctrl-b o` notification target, and `Ctrl-b q` detach. Bare `q` is forwarded to the shell; `Ctrl-b d` remains an extra detach alias. Stop-pane is still available through the command palette or an explicit `stop_pane` binding. Fallback terminal input preserves Ctrl/Alt/Shift for control characters, modified arrows, Home/End, Insert/Delete, Page, and function-key CSI sequences, matching Herdr's terminal encoding behavior. Herdr exposes right-click passthrough in the pane menu but has no default key binding; Spindle also makes it available in the keyboard command palette. Its in-app guide lists the shortcuts. Configurable bindings and manual Windows verification remain. |
| Show detected agent identity and unknown/idle/working/blocked/done state | Partial | `src/detect.rs`, `src/detect/agents/`, `src/pane/manager.rs`, Herdr `src/detect/manifests/pi.toml`, `qodercli.toml`, `droid.toml`, `kilo.toml`, `antigravity.toml`, `hermes.toml`, `qwen.toml`, `grok.toml`, `maki.toml`, `muse.toml`, `kiro.toml`, `kimi.toml`, `cline.toml`, `devin.toml`, `cursor.toml`, `amp.toml`, `claude.toml`, `codex.toml`, `gemini.toml`, `opencode.toml`, `github-copilot.toml`, `src/pane/agent_detection.rs`, `src/client/shell/agent_sidebar.rs`, `src/client/shell.rs` (`status_priority`) | Spindle identifies direct Pi/Qoder CLI/Droid/Grok/Hermes/Kilo/Antigravity/Maki/Muse/Qwen/Kiro/Kimi/Cline/Devin/Cursor/Amp/Claude/Codex/Gemini/OpenCode/GitHub Copilot CLI process names plus supported Windows command shims; Pi's Node/Bun package launchers, Cursor's bundled Windows Node path, and Muse's versioned Windows binaries are recognized. Pi follows Herdr's literal `Working...` cue; Qoder CLI follows its confirmation blockers, cancel hint, and braille-spinner working cue; Droid follows its confirmation-menu blockers and `Esc to stop` working cue; Kilo follows its permission menu and `Esc interrupt` cue; Antigravity follows its permission/confirmation blocker, braille-spinner, and background-task cues; Hermes follows OSC warning/hourglass/check titles, dangerous-command and clarification/credential/confirmation blockers, and interrupt/cancel working cues; Qwen follows prioritized status titles, localized confirmation cues, cancel/timer lines, OSC tool progress, and composer-idle signals; Grok follows action-required and option-panel blockers, pinned background-work chips, OSC title/progress states, and footer status cues; Maki follows permission and plan-complete blockers, spinner/idle status bars, and narrow-pane prompt cues; Muse follows workspace trust, pickers and paired approvals, working/idle prompts, and leaves status unchanged while user menu overlays are open; Kiro follows its approval blockers, working markers, and two-part idle prompt; Kimi follows approval/question blockers, background-agent and spinner cues; Cline follows its tool-approval prompts and visible-output working fallback; Devin follows trust/permission prompts, tool activity, and prompt-idle signals; Cursor follows its file/command approval blockers and stop/task/spinner working cues; Amp follows OSC title, approval-footer, and status-footer cues. GitHub Copilot follows Herdr's selection blocker, cancel-hint working state, and background-agent working cue. Gemini follows Herdr's explicit apply/allow/confirmation blockers and `esc to cancel` working cue. Claude detection follows Herdr's OSC 9 idle progress, idle title, spinner and live-turn cues, background-agent/MCP work, `/btw` overlay, and high-confidence blocker prompts; OSC evidence clears when the detected agent changes. Agent rows retain identity across one confirmed absent-process scan, then become done after a second; incomplete or unavailable Windows process scans preserve identity and reset the absence confirmation. This approximates Herdr's process-exit-to-idle lifecycle with polling because Spindle does not receive Herdr's process-exit event. Plain working-to-idle transitions follow Herdr's 100 ms recheck, three confirmations, and 700 ms cap; visible idle signals, agent changes, and process exits bypass the hold. OpenCode permission detection follows Herdr's control hints and interrupt line. Codex trust requires Herdr's visible directory header, and its blocker checks are scoped after the current prompt marker; tests cover stale blockers in scrollback. Each detected pane has a focused sidebar row, grouped by workspace by default; clicking the sidebar title switches between grouped and priority modes, which is saved per client. Priority mode follows Herdr's blocked/done/working/idle/unknown order and shows workspace/tab. Selecting a row switches to and focuses that pane. Full manifest coverage and live side-by-side verification remain. |
| Configure keybindings, themes, and UI preferences | Partial | Herdr `docs/preview/website/src/content/docs/configuration.mdx` (Keybindings, Theme, UI and sidebar); Herdr `src/client/shell/global_menu.rs`, `settings.rs`; Spindle `src/config.rs`, `src/client/input.rs`, `src/client/preferences.rs`, `src/client/global_menu.rs`, `src/client/settings.rs`, `src/client/app.rs`, `src/client/renderer.rs` | Spindle reads optional `%APPDATA%\\Spindle\\config.toml` keybindings and `[theme] name` at startup and refreshes the keymap during the client event loop. It supports Herdr-style `prefix+key`, direct modified chords, arrays of alternatives, invalid-entry fallback, `spindle config path`, and `spindle config default`, whose starter file now includes the supported Settings and reload bindings. Built-in `terminal`, `catppuccin`, `dracula`, `gruvbox`, `nord`, and `tokyo-night` accents drive the footer, navigation marker, and focused pane border. The sidebar menu now includes a Herdr-style settings overlay with keyboard and mouse navigation for theme, notification delivery, and sound sections; selections persist to the user config and are reloaded by the client loop. `Ctrl-b Shift-r` and the menu's reload item now show explicit reload confirmation. The help overlay shows active bindings. Custom palettes and broader UI settings remain. |
| Notify when agents need attention or finish | Partial | Herdr `docs/preview/website/src/content/docs/configuration.mdx` (Notifications, Sound), `src/client/notifications.rs`; Spindle `src/client/notifications.rs`, `src/client/renderer.rs`, `src/terminal_notify.rs`, `src/platform/windows.rs` | Spindle detects background-pane transitions to blocked or finished, queues multiple in-app events in arrival order, suppresses the active tab, and supports Herdr, terminal, system, and off delivery. Herdr-style bounded `[notifications].delay_seconds` (default 1 second) now applies to in-app visibility and terminal/system delivery, with separate visibility and expiry deadlines and ordered pending external events. `[notifications].sound` is enabled by default and plays distinct attention/finished sounds when delivery becomes visible. Terminal delivery selects Herdr-compatible OSC 9/99 backends and tmux passthrough. System delivery uses the Windows notification-area desktop toast path. Unit coverage verifies transition policy, queue order, delayed visibility, delayed external delivery, expiry, terminal sequences, and Windows text bounds. Notification history remains. |
| Agent integrations, CLI automation, plugins, and remote sessions | Partial | `src/integration/`, `src/cli/`, `src/app/api/plugins/`, `src/remote/` | Spindle now has local global plugin registration, filtered/JSON plugin listing, bounded launch logs, shallow GitHub plugin installation/replacement/uninstallation, enable/disable, config directories, Windows platform filtering, manifest action listing/invocation, Herdr-default temporary zoomed overlay panes plus split/tab/zoomed placements with repeated `--env`, session-modal popup pane opening with explicit dimensions, transient overlay/popup cleanup and focus restoration, pane focus/close, runtime context variables, and OSC 8/URL link-handler dispatch based on Herdr manifests. Local linking accepts Herdr's explicit `--disabled` and `--enabled` modes. Plugin pane opening also accepts Herdr's `--workspace`, `--target-pane`, and `--direction right|down` placement controls, with `--no-focus` restoring the prior workspace and tab. CLI action and pane invocation now enrich JSON and environment context from the active session snapshot with workspace, tab, and focused-pane identity/cwd, matching Herdr's invocation context shape. Open popup panes now own keyboard input and `Ctrl-b x` closes the popup before the background pane. Integrations and remote sessions remain outside the local Windows-first implementation. |
| Windows-first process, terminal, installer, and recovery behavior | Partial | Herdr `src/platform/windows.rs`, `src/pty/backend.rs`, `src/session.rs` (`stop_socket_with_timeout`), `scripts/windows_*` | Spindle assigns Windows PTY children to kill-on-close Job Objects, terminating child tools with their pane while safely falling back when assignment is unavailable. `stop` waits for endpoint cleanup; Windows named-pipe wake retries handle the gap between requests. Server daemon launch now follows Herdr's WMI detached-process path and waits up to five seconds for readiness. Release Windows smoke and installer smoke both pass, including repeated start, stale-endpoint recovery, and clean shutdown. Interactive Windows Terminal behavior and deeper process/input parity still need live verification. |

Update a row only after checking the matching Herdr code and testing the
corresponding Spindle behavior. Any deliberate difference needs a reason and a
usable alternative, not just a missing implementation.

| Additional parity check | Status | Herdr reference | Spindle evidence |
| --- | --- | --- | --- |
| Sidebar width and narrow chrome | Partial | Herdr `src/config/model.rs` (`UiConfig::default`, 26-column expanded width, 64-column mobile threshold), `src/client/shell/config.rs`, and `src/client/shell/mobile.rs` | Spindle uses Herdr's 26-column expanded default at normal terminal sizes, accepts `[ui] sidebar_width`, `sidebar_min_width`, `sidebar_max_width`, and `mobile_width_threshold`, safely falls back from inverted bounds, and uses a narrow single-column pane with Herdr's compact workspace/tab header, local agent summary, and clickable `switch` control. The switch and mobile `Ctrl-b w`/`Ctrl-b b` navigation actions open a full-screen switcher with detected agents first, then workspace tree, tabs, create, search, scrolling, close, and global-menu actions; renderer, hit-test, keyboard, menu-geometry, agent-summary, and agent-row regressions cover the behavior. Endpoint-specific presentation remains. |
| Detect Hermes launched through versioned Python | Present | `src/detect/mod.rs` (`wrapped_agent_name_from_runtime_argv`, `is_python_runtime`; `identify_agent_in_job_detects_python_version_wrapped_hermes`) | Spindle reads command lines for versioned Python descendants and recognizes the launched agent name. Tests cover a Hermes executable and ensure Python `-c` and `-m` arguments do not create false positives. |
| Search across collapsed navigator branches | Present | Herdr `src/client/shell/aggregate_navigation.rs` (`filtering` expands workspace children) | Spindle search now expands matching space/workspace ancestors without changing saved expansion state; a regression test confirms a pane remains searchable after both branches are collapsed. |
| Clear process state when restarting a pane | Present | Herdr `src/app/api.rs` (`respawn_shell_for_launch_pane`); `src/terminal/state.rs` (`clear_agent_runtime_identity_after_respawn`) | Spindle restarts the same pane ID but clears the previous agent, scrollback, title, cursor, and terminal modes; it keeps the user label and right-click preference. A Windows test covers the reset. Re-detection after restart still needs live verification. |
| Normal terminal chrome and pane height | Present | Herdr `src/client/shell/render.rs::render_mode_bar` and `src/client/shell/config.rs::layout` | Herdr returns no mode bar in normal terminal mode and gives the pane surface the full remaining height. Spindle follows that geometry; a renderer test covers the full-height pane and connected rendering no longer paints a permanent status row. |
| Shift+Tab keybinding normalization | Present | Herdr `src/config/keybinds.rs::normalize_key_combo` and `terminal_key_matches_combo` | Spindle treats `Tab` with Shift and `BackTab` with or without Shift as the same canonical binding, including configured bindings and labels. Input tests cover both terminal event forms. |
| Prefix-mode feedback bar | Present | Herdr `src/client/shell/render.rs::render_mode_bar` (`ClientShellMode::Prefix`) | Spindle shows a transient bottom bar while the prefix is active, using the active prefix, workspace-picker, and help bindings; a renderer regression test covers the visible controls. |
| Diagnose missing, reachable, and stale server endpoints | Present | Herdr `src/cli/status.rs` (`print_server_status_body`) | Spindle `doctor` distinguishes endpoint state and suggests `spindle start` or `spindle attach` when endpoint metadata is stale; the recorded PID and identity are labeled as historical metadata. Unit tests cover endpoint classification. |
| Identify the client executable and protocol | Present | Herdr `src/cli/status.rs` (`print_client_status`) | Spindle `doctor` prints the running client version, executable path, and protocol. Windows smoke asserts those values identify the binary under test. |
| Windows plugin command launching | Present | Herdr `src/plugin_command.rs` (`command_for_argv_in_dir`) | Spindle resolves relative plugin commands from the manifest directory and launches `.cmd`/`.bat` handlers through `ComSpec`; Windows regression coverage executes a batch fixture. |
| Run plugin actions from the command palette | Present | Herdr `src/app/api/plugins/mod.rs` (`invoke_plugin_action_from_keybind`), `src/client/shell/actions.rs` | Spindle's palette accepts an action ID or `plugin.id.action`, resolves enabled Windows actions, launches them from the plugin directory, and passes active workspace/tab/pane context through JSON and Herdr-compatible environment variables. |
| Configured shell, pane, popup, and plugin-action commands | Present | Herdr `src/config/keybinds.rs` (`CustomCommandKeybind`), `src/app/custom_commands.rs` | Spindle parses `[[keys.command]]`, binds direct or prefix keys, runs shell commands through `cmd.exe /d /c` on Windows, opens pane commands as temporary overlays, opens popup commands in the transient centered popup, invokes installed plugin actions, and passes active workspace/tab/pane context plus the focused pane cwd. |
| Plugin build commands | Present | Herdr `src/app/api/plugins/manifest.rs`, `src/app/api/plugins/mod.rs` (`build`) | Spindle parses `[[build]]` manifest entries and runs supported commands from the temporary GitHub checkout before registration, using Herdr-style argv resolution and bounded failure output; local links remain author-built. |
| Plugin startup hooks | Present | Herdr `src/app/api/plugins/manifest.rs`, `src/app/api/plugins/runtime.rs` (`run_plugin_startup_hooks`) | Spindle parses `[[startup]]` manifest entries and runs supported hooks once after the first control request has made the server API usable, with Herdr-compatible startup event/context variables; hook failures do not stop server operation. |
| Plugin lifecycle event hooks | Partial | Herdr `src/app/api/plugins/manifest.rs`, `src/app/api/plugins/runtime.rs` (`run_plugin_event_hooks`), `src/app/api/plugins/runtime.rs` (`HERDR_PLUGIN_EVENT_JSON`), `src/api/schema/events.rs` | Spindle parses `[[events]]` entries, rejects unknown hook names like Herdr, and asynchronously dispatches supported workspace, tab, pane, agent, and worktree hooks from server events with event payload and session context. Event hooks now receive serialized protocol event data through `HERDR_PLUGIN_EVENT_JSON` and an explicit `invocation_source` context field. Worktree create/open/remove emit Herdr-compatible `worktree.created`, `worktree.opened`, and `worktree.removed` events, including worktree path and branch data. Tab rename emits `tab.renamed` from both active and cross-workspace paths, workspace reorder emits Herdr's `workspace.reordered` (while preserving the separate `workspace.moved` mapping), tab reorder emits `tab.moved`, and workspace branch refresh emits `workspace.updated`; high-volume `pane.updated` and `layout.updated` hooks are intentionally rejected like Herdr. | 
| Discover Git worktrees | Partial | Herdr `src/cli/worktree.rs`, `src/worktree.rs`, `src/api/schema/worktrees.rs`, `src/client/shell/sidebar.rs`, `src/client/shell/preferences.rs` | `spindle worktree list [--workspace ID | --cwd PATH]` parses `git worktree list --porcelain`, reports branch/detached/bare/prunable state, identifies linked checkouts, and maps open checkouts to Spindle workspaces when a server snapshot is available. Worktree create/open/remove now create or remove shell-backed workspaces, and linked workspaces persist their membership flag; the sidebar groups linked children under their main checkout, shows branch identity for normal checkouts, and uses a compact branch cue for children. Parent-group collapse is available from the context menu and its keys persist in the project `client.json`, matching Herdr's persisted collapsed-group behavior. Workspace branch metadata refreshes on a Herdr-inspired 1.5-second cadence. Full Herdr status metadata remains. |

Keybinding compatibility note: Spindle accepts Herdr action names such as
`goto`, `new_workspace`, `close_workspace`, `focus_pane_*`, `swap_pane_*`,
`copy_mode`, and `zoom` alongside its existing aliases.

Workspace group lifecycle note: `spindle workspace close ID --group` now closes
all Spindle workspaces sharing the linked-worktree group; closing a primary
workspace with linked children requires the explicit flag, matching Herdr.

New-tab behavior note: keyboard, command-palette, context-menu, and mobile
switcher new-tab actions now prompt for a tab name by default, matching
Herdr's `prompt_new_tab_name`; setting `[ui] prompt_new_tab_name = false`
retains the `Activity` fallback.

Workspace-name behavior note: Herdr's `prompt_new_workspace_name` is now
supported across keyboard, command palette, mobile navigator, and sidebar
new-workspace actions; it defaults to false.

Mouse-selection behavior note: `[ui] copy_on_select` now matches Herdr and
defaults to true; setting it to false keeps completed selections available
for explicit copy instead of copying immediately.

Mouse-capture behavior note: `[ui] mouse_capture` now controls the client
mouse UI and tracking modes, defaulting to true like Herdr and allowing the
outer terminal to handle mouse input when disabled.

Host-cursor behavior note: `[ui] host_cursor` now accepts Herdr's `auto`,
`native`, and `drawn` modes; `auto` preserves Spindle's Windows-first drawn
cursor default.

Mouse-wheel behavior note: `[ui] mouse_scroll_lines` now controls pane
scrollback and navigator wheel movement, defaulting to three lines like Herdr.

Workspace-close behavior note: `[ui] confirm_close` now controls confirmation
for keyboard and context-menu workspace closing, defaulting to true like Herdr.

Single-tab layout note: `[ui] hide_tab_bar_when_single_tab` now controls the
desktop tab row, defaulting to false like Herdr and updating pane geometry and
mouse hit testing when enabled.

Tab-bar placement note: `[ui] tab_bar_position = "top"|"bottom"` now moves the
desktop tab row and pane surface together, matching Herdr's default and
alternate placement.

Desktop tab note: the tab strip now exposes a clickable `+` control that uses
the same named-tab prompt and creation path as Herdr.

Action parity note: Herdr action names `move_tab_previous`, `move_tab_next`,
and `resize_pane_left|down|up|right` are now accepted and execute their matching
operations; they remain unset by default like Herdr.

Last-pane note: the `last_pane` action now tracks the prior focused pane across
tabs and workspaces and restores it when the pane still exists.

Right-click note: `[ui] right_click_passthrough_modifier` now matches Herdr's
modifier forms and forwards a matching right-click to a mouse-reporting pane,
stripping the configured modifier before encoding the terminal event.

Focus redraw note: `[ui] redraw_on_focus_gained` now defaults to true and
controls repainting after the outer terminal reports focus gained, matching
Herdr's setting.

Sidebar mode note: `[ui] sidebar_collapsed_mode` now accepts Herdr's `compact`
and `hidden` modes; compact remains the default and hidden removes the sidebar
column while preserving keyboard access to the toggle.

Pane chrome note: `[ui] pane_borders = "auto"|"always"|"off"` now follows
Herdr's lone-pane and split-pane policy; `pane_outer_borders` and `pane_gaps`
also control the visible frame and shared split edges.

Pane scrollbar note: `[ui] pane_scrollbars` now reserves Herdr's stable
one-column gutter, keeps PTY width in sync, and draws a scrollback indicator
when retained history exceeds the visible pane. Track clicks use the thumb
center and thumb drags preserve the grab offset, matching Herdr's
`src/ui/scrollbar.rs` mapping. Rendering and mouse handling share the
categorized `src/client/scrollbar.rs` helper. Focused and full Rust tests pass;
Pane terminal hit testing and Ctrl-click URL lookup exclude the gutter and reuse
the renderer's inner-area geometry, matching Herdr's separate terminal and
scrollbar rectangles. Focused and full Rust tests pass; Windows release smoke
and installer smoke pass. Alternate-screen panes now reclaim that gutter like
Herdr's `src/ui/panes.rs::terminal_inner_rect`; PTY sizing, rendering, cursor
placement, selection, URL lookup, and mouse hit testing all follow the reclaimed
width. The regression test
`alternate_screen_reclaims_scrollbar_column_like_herdr` passes.

Agent session note: pane agent reports now accept Herdr-style session IDs or
paths, and `pane report-agent-session` exposes the separate session identity
report path. Session metadata is returned in pane views, rejects stale
same-source sequence numbers, and is cleared on agent release or server
restart. The focused server/client parity test passes.

Display metadata note: `pane report-metadata` now supports Herdr-style
display-agent labels with separate source/sequence ordering. Labels appear in
pane titles, sidebar rows, and navigator rows without changing lifecycle
state; clear and release behavior are covered by the server/client test.
Custom display titles now override only the rendered title while preserving
the terminal title in pane state; title set/clear behavior is also covered.
Custom state labels now use the same source/sequence authority and appear in
pane titles and agent navigator rows without changing semantic lifecycle state;
clear and release behavior are covered by the server/client test. Metadata
tokens now support per-source sequence ordering, individual clears, bounded
key storage, and TTL expiry in the pane poll loop; direct expiry and API
coverage are tested. The agent sidebar also renders the `summary` token beside
the agent identity, matching Herdr's user-visible metadata path. Workspace
metadata tokens are now accepted through the API and `spindle workspace
report-metadata`, carried in snapshots, expired by TTL, and shown beside
workspace rows.
| Scriptable agent report | Partial | Herdr `src/app/api/agents.rs` (`handle_agent_list`, `handle_agent_get`, `handle_agent_focus`, `handle_agent_start`, `handle_agent_read`, `handle_agent_send_keys`, `handle_agent_prompt`, `handle_agent_rename`); Herdr `src/api/wait.rs`; Spindle `src/cli/agent.rs`, `src/cli/pane.rs` | `spindle agent list`, `spindle agent get <target>`, and `spindle agent focus <target>` report or focus detected agents across all spaces with pane, workspace, tab, focus, state, cwd, and title. `spindle agent start <name> --kind KIND --pane PANE_ID [--timeout MS] [-- AGENT_ARGS...]` launches a supported agent in an existing shell pane and waits for detection. Every existing-agent command accepts a pane ID or a unique case-insensitive live agent name; ambiguous names require a pane ID. `spindle agent wait <target>` waits up to 30 seconds by default for idle/done/blocked, or selected states with `--until` and `--timeout`. `spindle agent read <target>` validates the agent and reuses pane read's visible/recent/detection, line-limit, ANSI, and raw output modes. `spindle agent send-keys <target> <key>...` validates the agent and reuses pane key encoding. `spindle agent prompt <target> <text>...` validates the agent, rejects blocked agents, and sends text followed by Enter; `--wait` can wait for idle/done/blocked or repeated `--until` states with `--timeout`. `spindle agent rename <target> <name>|--clear` validates the agent and reuses pane labels. Named targets currently use Spindle's detected agent-kind label; richer Herdr lifecycle metadata and prompt activity/stall handling remain. |
OpenCode working-state parity note: the detector now follows Herdr's case-insensitive interrupt hints and four-or-more repeated `■`/`⬬` progress glyph rule.

Theme surface parity note: `[theme.custom] sidebar_bg` and `active_row_bg` now
apply to the live sidebar background and focused/preview rows, following
Herdr's `src/config/theme.rs` and `src/client/shell/sidebar.rs`.

Navigator selection now uses Herdr's `[theme.custom] selection_bg` token on
desktop and mobile switcher layouts.

Navigator panels now use the selected Herdr `panel_bg` surface on desktop and
mobile layouts.

Tab-strip rendering now follows Herdr's surface treatment: inactive tabs use
`surface0`, while the active tab uses the accent-filled state; the
`[theme.custom] surface0` override is supported.

Tab labels and the new-tab control now use Herdr's `overlay0`/`overlay1`
contrast tokens, with matching `[theme.custom] overlay0` and `overlay1`
overrides.

Sidebar active and selected rows now use Herdr's per-theme `active_row_bg` and
`selection_bg` values; custom overrides and terminal reset selection are
supported.

Sidebar separators, summaries, and scrollbars now use Herdr's `overlay0` and
`surface_dim` palette tokens, including the `▕` scrollbar glyph.

Pane title indicators and unfocused pane borders now use Herdr's theme
semantic colors for running, completed, and halted/interrupted states.

Sidebar agent state icons now use the matching themed status colors and
`overlay0` for unknown state.

Narrow tab strips now keep a minimum usable tab width and keep the active tab
visible; render, click, and drag geometry share the same visible-tab window.
Edge ellipses indicate when tabs are hidden outside that window.
Mouse-wheel scrolling over the strip changes the visible window while keeping
rendering and click geometry aligned.
Overflowing strips also expose clickable `<` and `>` controls for the same
window movement.
Scroll limits and drag indicators remain aligned with the control-reserved
content area.

Sidebar workspace and agent metadata now use Herdr's `text` and `subtext0`
tokens, including case-insensitive theme names and `[theme.custom] text` and
`subtext0` overrides.
