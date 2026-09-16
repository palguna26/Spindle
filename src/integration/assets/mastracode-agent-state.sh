#!/bin/sh
# installed by spindle; managed by spindle integration install mastracode
# SPINDLE_INTEGRATION_ID=mastracode
# SPINDLE_INTEGRATION_VERSION=1
action="${1:-}"
case "$action" in session|working|idle|blocked) ;; *) exit 0 ;; esac
[ "${SPINDLE_ENV:-${HERDR_ENV:-}}" = "1" ] || exit 0
[ -n "${SPINDLE_PANE_ID:-${HERDR_PANE_ID:-}}" ] || exit 0
bin="${SPINDLE_BIN_PATH:-${HERDR_BIN_PATH:-spindle}}"
pane="${SPINDLE_PANE_ID:-${HERDR_PANE_ID:-}}"
input=$(cat 2>/dev/null || true)
session=$(printf '%s' "$input" | python3 -c 'import json,sys; d=json.load(sys.stdin) if sys.stdin.readable() else {}; print(d.get("session_id", ""))' 2>/dev/null || true)
if [ "$action" = session ]; then
  [ -n "$session" ] || exit 0
  "$bin" pane report-agent-session "$pane" --source spindle:mastracode --agent mastracode --agent-session-id "$session" --session-start-source startup --seq "$(date +%s%3N)" >/dev/null 2>&1 || true
else
  "$bin" pane report-agent "$pane" --source spindle:mastracode --agent mastracode --state "$action" --seq "$(date +%s%3N)" >/dev/null 2>&1 || true
fi
