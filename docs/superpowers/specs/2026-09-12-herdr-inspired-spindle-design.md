# Herdr-inspired Spindle design

## Goal

Make Spindle a complete, Windows-first coding-session workspace with the
everyday interaction quality of Herdr, then improve the areas where Spindle's
Windows-first and local-first focus can serve users better. Herdr's source at
`C:\Users\Palguna\.opensrc\repos\github.com\herdrdev\herdr\master` is the
behavior reference for every slice. This is a behavior port, not a copy of
Herdr's implementation or assets.

## Evidence and current gap

Herdr's quick-start guide describes an attached persistent session, automatic
workspace creation, mouse-first navigation, pane splitting and tab creation
from context menus, and agent-state visibility. Its `Workspace::new` creates
the first tab and starts a shell. The sidebar and pane behavior are implemented
in `src/ui/sidebar.rs`, `src/ui/panes.rs`, `src/client/shell/mouse.rs`, and
`src/client/shell/context_menu.rs`.

Spindle already has a Windows PTY server, persistent session model, workspaces,
spaces, tabs, pane layouts, keyboard actions, and a command palette. Its UI
does not yet expose Herdr's sidebar and mouse-driven controls. Its default
session starts with an empty tab. Commit `68d86ad` adds an attach-time default
PowerShell pane, but does not establish full startup, new-tab, or new-workspace
behavior.

The previously referenced `SPEC.md` and `Implementation_plan.md` are not in
the current checkout. This document therefore uses the current Spindle source
and the named Herdr checkout as its references.

## Product principles

- Keep Spindle Windows-first and local-first; verify every feature on Windows.
- Keep the session server authoritative for workspaces, tabs, panes, focus, and
  process state. The client renders and sends user actions through the control
  API.
- Keep ordinary shell and agent commands under user control. Add agent
  visibility and workflow support without making LLM calls.
- Keep keyboard access while making common actions discoverable and usable
  with a mouse, as in Herdr.
- Work in small, tested slices. Each slice must compare its behavior against
  the corresponding Herdr source before implementation and runtime checks.

## Delivery sequence

The objective remains full parity with the relevant Herdr user experience plus
Spindle-specific improvements. The sequence below orders the work; it does not
drop later areas from the goal.

### 1. Daily workspace loop

- Start with a live shell in the initial workspace and in newly created tabs
  and workspaces. Reattach without duplicating shells.
- Render the workspace/space sidebar, active tab controls, pane canvas, and
  concise status area.
- Support mouse selection of spaces, workspaces, tabs, and panes; split-border
  dragging; pane context menus; text selection and copy.
- Keep keyboard equivalents and visible key help.
- Preserve detach/reattach and saved layout behavior.

### 2. Agent-aware workspaces

- Detect supported coding-agent processes and represent their state in panes
  and the sidebar.
- Show useful states such as working, blocked, done, and idle, and keep status
  changes tied to pane/workspace identity.
- Keep integrations optional and make detection failures visible without
  breaking shell use.

### 3. Full reference inventory and Spindle improvements

- Audit the remaining user-facing Herdr source areas (CLI, notifications,
  configuration, integrations, remote sessions, and extensions) against
  Spindle's Windows-first/local-first product scope.
- Port the relevant behaviors in reviewed slices; document deliberate
  differences instead of silently omitting them.
- Prioritize improvements that fit Spindle's purpose: reliable Windows process
  and terminal handling, clear recovery after server/process failure, and
  useful Codex/OpenCode workflows without hidden agent launches.

## Architecture

- Split client rendering into focused sidebar, tab-bar, pane-layout, status,
  and overlay modules. Keep input routing separate from rendering.
- Derive hit targets and pane rectangles from the same layout calculation used
  to draw the frame. Route focus, split, close, and resize actions through the
  session control API so multiple clients see consistent state.
- Add agent detection as a separate module that reports state to the session
  model; do not mix detection logic into terminal rendering.
- Keep persistent state versioned. A failed shell start or unsupported mouse
  event must leave the session usable and provide a clear error or keyboard
  fallback.

## Validation

- Unit-test layout geometry, focus targets, sidebar/tab hit testing, context
  menu actions, and agent-state mapping.
- Use Ratatui's test backend for rendered workspace states and overlays.
- Add server/client integration tests for initial shell creation, new
  tab/workspace shell creation, concurrent attach idempotency, and reattach.
- Run formatting, all Rust tests, strict Clippy, release build, Windows smoke
  and installer tests.
- Manually compare the running Spindle UI with the relevant Herdr source
  behavior on Windows. Automated tests alone do not prove interaction parity.

## Acceptance for the first slice

- A fresh attach shows a usable shell rather than an empty session view.
- New tabs and workspaces open with a shell and focus it.
- The workspace sidebar and tab controls are visible and mouse-selectable.
- Clicking a pane focuses it; dragging its split changes the actual PTY size;
  right-click opens actions for that pane.
- Keyboard equivalents remain available, and detach/reattach retains the
  session without duplicate panes.

## Decisions still open

- Which additional behavior beyond Herdr parity is most valuable after the
  daily workspace loop. The current recommendation is Windows-first reliability
  and first-class visibility for Codex/OpenCode sessions.
- Which Herdr integrations and remote/plugin capabilities should be included
  in Spindle's local-first product rather than treated as optional extensions.
