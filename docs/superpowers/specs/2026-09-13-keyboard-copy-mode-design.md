# Keyboard copy mode

## Goal

Add Herdr-style keyboard browsing, search, selection, and clipboard copy for
terminal history. This fills the unchecked copy-mode item in
`Implementation_plan.md` and preserves Spindle's existing mouse selection.

## Reference behavior

Use Herdr's `docs/preview/website/src/content/docs/keyboard.mdx` and
`src/client/shell/copy_mode.rs` as the behavior references. `prefix+[` enters
copy mode. Navigation includes arrows and `h/j/k/l`, `PageUp/PageDown`,
`ctrl+b/f/u/d`, `g/G`, `0/^/$`, `w/b/e`, `W/B/E`, and `{`/`}`. `/` and `?`
open forward/backward literal search; search is case-insensitive unless the
query has uppercase letters. `n/N` repeat search. `v` or Space begins character
selection; `V` begins line selection. `y` or Enter copies and exits; `q` exits
without copying. Escape clears active selection/search first, then exits.

## Design

Keep the server authoritative for PTY output and session state. Build a bounded
copy buffer in a focused client module from the current pane snapshot's retained
scrollback bytes. The input loop routes copy-mode keys to that module instead
of the PTY. The renderer draws the history viewport, cursor, selection, search
match, and a small mode hint. Copying uses Spindle's existing clipboard helper.
No control-protocol changes are needed because snapshots already contain the
retained terminal history. Keep the copy buffer aligned with the existing
limits: 64 KiB of PTY bytes per pane and at most 4,096 reconstructed rows. Cache
the parsed buffer by pane, bytes, and geometry, and rebuild only when one of
those inputs changes; the client refreshes snapshots every 100 ms.

Keep processing PTY output while copy mode is active. At the live bottom, the
view follows new output. Once the user moves into history, preserve the visible
content while new output arrives. If Spindle's bounded byte history evicts the
anchored content, clamp the cursor to retained history and clear stale search
or selection state. Do not send copy-mode navigation keys to the shell.

The `prefix+[` binding takes precedence over Spindle's alternate previous-tab
binding; `prefix+p` remains previous-tab. Other prefix behavior remains
unchanged. Copy mode is unavailable when there is no focused pane.

## Boundaries and tests

- `src/client/copy_mode.rs` owns the bounded text model, cell-aware motions,
  search, selection, and copy-mode state transitions.
- `src/client/app.rs` enters/exits the mode and routes keys while continuing
  snapshot refresh and PTY output processing.
- `src/client/renderer.rs` draws mode feedback without changing pane layout.
- Keep mouse drag selection and terminal mouse passthrough unchanged.

Test key routing, search case rules and wraparound, selection text, wide and
combining characters, deep-history navigation, byte-ring eviction, live-output
follow and viewport pinning, and copy/cancel behavior. Verify the interaction
in Windows Terminal with PowerShell, comparing each behavior with the Herdr
reference above.
