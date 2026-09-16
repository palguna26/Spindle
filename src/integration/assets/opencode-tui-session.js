// installed by spindle
// managed by spindle; reinstalling or updating the integration overwrites this file.
// SPINDLE_INTEGRATION_ID=opencode-tui
// SPINDLE_INTEGRATION_VERSION=1

import { spawn } from "node:child_process";

const SOURCE = "spindle:opencode";
const AGENT = "opencode";
const RETRY_DELAYS = [100, 400, 1000];

function report(sessionID) {
  const pane = process.env.SPINDLE_PANE_ID || process.env.HERDR_PANE_ID;
  if (!pane || !sessionID) return Promise.resolve();
  const binary = process.env.SPINDLE_BIN_PATH || process.env.HERDR_BIN_PATH || "spindle";
  return new Promise((resolve) => {
    const child = spawn(binary, [
      "pane", "report-agent-session", pane,
      "--source", SOURCE, "--agent", AGENT,
      "--seq", String(Date.now()),
      "--agent-session-id", sessionID,
    ], { stdio: "ignore", windowsHide: true });
    child.once("error", resolve);
    child.once("close", resolve);
  });
}

export default {
  id: "spindle.opencode.session-selection",
  tui: async (api) => {
    if (process.env.SPINDLE_ENV !== "1" && process.env.HERDR_ENV !== "1") return;

    let selected;
    let retryIndex = 0;
    let nextReportAt = 0;
    let pending = false;

    const sync = async () => {
      const route = api.route.current;
      const id = route?.name === "session" ? route.params?.sessionID : undefined;
      const session = typeof id === "string" && id ? api.state.session.get(id) : undefined;
      if (!session || session.parentID) {
        selected = undefined;
        retryIndex = 0;
        nextReportAt = 0;
        return;
      }
      if (id !== selected) {
        selected = id;
        retryIndex = 0;
        nextReportAt = 0;
      }
      if (pending || Date.now() < nextReportAt) return;

      pending = true;
      try { await report(id); } finally { pending = false; }
      if (selected !== id) return;
      const delay = RETRY_DELAYS[retryIndex++];
      nextReportAt = delay === undefined ? Number.POSITIVE_INFINITY : Date.now() + delay;
    };

    await sync();
    const timer = setInterval(() => void sync(), 100);
    api.lifecycle.onDispose(() => clearInterval(timer));
  },
};
