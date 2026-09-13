# Spindle empty-startup investigation

- Symptom: Screenshot showed a connected session with no focused pane and zero panes.
- Herdr comparison: Herdr's `App::ensure_default_workspace` creates a shell-backed workspace when none exists. Spindle's attach path calls `ensure_active_default_pane`, which creates or repairs the active pane and verifies the resulting snapshot.
- Evidence: The focused regression test `startup_restores_a_workspace_and_shell_after_the_last_workspace_was_closed` passes. A freshly built Windows release attached with a visible PowerShell pane (`focused: pane-1`, `panes: 1`) and detached cleanly. No Spindle process remained afterward; doctor reported only stale endpoint metadata.
- Root-cause hypothesis: The screenshot came from a launch/build/state not reproduced by the current checkout. This remains unconfirmed because the original launch command and executable path are unknown.
- Fix: None. No source change is justified without reproducing the blank state.
- Regression test: `src/client/app.rs`, `startup_restores_a_workspace_and_shell_after_the_last_workspace_was_closed`.
- Status: DONE_WITH_CONCERNS; the reported scenario still needs its exact launch path to reproduce.
