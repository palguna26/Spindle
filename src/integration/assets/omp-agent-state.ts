// installed by spindle
// managed by spindle; reinstalling or updating the integration overwrites this file.
// SPINDLE_INTEGRATION_ID=omp
// SPINDLE_INTEGRATION_VERSION=1
// @ts-nocheck

import { spawn } from "node:child_process";

const source = "spindle:omp";
let sequence = Date.now() * 1000;
let rootSession = false;
let active = false;
let blocked = 0;
let lastState;

function report(command, args) {
  const pane = process.env.SPINDLE_PANE_ID || process.env.HERDR_PANE_ID;
  if (!pane) return;
  const binary = process.env.SPINDLE_BIN_PATH || process.env.HERDR_BIN_PATH || "spindle";
  spawn(binary, ["pane", command, pane, ...args], { stdio: "ignore", windowsHide: true });
}

function sessionRef(ctx) {
  const id = ctx?.sessionManager?.getSessionId?.();
  const path = ctx?.sessionManager?.getSessionFile?.();
  if (typeof path === "string" && path) return ["--agent-session-path", path];
  if (typeof id === "string" && id) return ["--agent-session-id", id];
  return [];
}

function publish(ctx) {
  const state = blocked > 0 ? "blocked" : active ? "working" : "idle";
  if (state === lastState) return;
  lastState = state;
  report("report-agent", [
    "--source", source, "--agent", "omp", "--state", state,
    "--seq", String(++sequence), ...sessionRef(ctx),
  ]);
}

export default function (pi) {
  if (process.env.SPINDLE_ENV !== "1" && process.env.HERDR_ENV !== "1") return;
  pi.on("session_start", async (event, ctx) => {
    if (ctx?.mode !== "tui" && ctx?.hasUI !== true) return;
    rootSession = true;
    const ref = sessionRef(ctx);
    if (ref.length) report("report-agent-session", ["--source", source, "--agent", "omp", "--seq", String(++sequence), ...ref, ...(event?.reason ? ["--session-start-source", String(event.reason)] : [])]);
    active = ctx?.isIdle?.() === false;
    publish(ctx);
  });
  pi.on("agent_start", (_event, ctx) => {
    if (!rootSession) return;
    active = true;
    publish(ctx);
  });
  pi.on("agent_end", (_event, ctx) => {
    if (!rootSession) return;
    active = false;
    publish(ctx);
  });
  pi.on("agent_settled", (_event, ctx) => {
    if (!rootSession || ctx?.isIdle?.() !== true) return;
    active = false;
    publish(ctx);
  });
  pi.events.on("herdr:blocked", (data) => {
    if (!rootSession) return;
    blocked = data?.active ? blocked + 1 : Math.max(0, blocked - 1);
    publish(data?.ctx);
  });
}
