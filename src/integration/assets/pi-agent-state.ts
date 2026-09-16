// installed by spindle
// managed by spindle; reinstalling or updating the integration overwrites this file.
// SPINDLE_INTEGRATION_ID=pi
// SPINDLE_INTEGRATION_VERSION=1
// @ts-nocheck

import { spawn } from "node:child_process";

const source = "spindle:pi";
let sequence = Date.now() * 1000;
let rootSession = false;
let active = false;
let blocked = 0;
let blockedMessage;
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

function reportSession(ctx, reason) {
  const ref = sessionRef(ctx);
  if (!ref.length) return;
  report("report-agent-session", [
    "--source", source, "--agent", "pi", "--seq", String(++sequence), ...ref,
    ...(reason ? ["--session-start-source", String(reason)] : []),
  ]);
}

function publish(ctx) {
  const state = blocked > 0 ? "blocked" : active ? "working" : "idle";
  if (state === lastState) return;
  lastState = state;
  report("report-agent", [
    "--source", source, "--agent", "pi", "--state", state,
    "--seq", String(++sequence), ...sessionRef(ctx),
  ]);
}

export default function (pi) {
  if (process.env.SPINDLE_ENV !== "1" && process.env.HERDR_ENV !== "1") return;

  pi.on("session_start", async (event, ctx) => {
    if (ctx?.mode !== "tui") return;
    rootSession = true;
    reportSession(ctx, event?.reason);
    active = ctx?.isIdle?.() === false;
    publish(ctx);
  });
  pi.on("agent_start", (_event, ctx) => {
    if (!rootSession) return;
    active = true;
    reportSession(ctx);
    publish(ctx);
  });
  pi.on("agent_settled", (_event, ctx) => {
    if (!rootSession || ctx?.isIdle?.() !== true) return;
    active = false;
    publish(ctx);
  });
  pi.events.on("herdr:blocked", (data) => {
    if (!rootSession) return;
    if (data?.active) {
      blocked += 1;
      blockedMessage = data.label;
    } else {
      blocked = Math.max(0, blocked - 1);
      if (!blocked) blockedMessage = undefined;
    }
    publish(data?.ctx);
  });
}
