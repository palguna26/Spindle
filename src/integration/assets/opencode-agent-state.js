// installed by spindle
// managed by spindle; reinstalling or updating the integration overwrites this file.
// SPINDLE_INTEGRATION_ID=opencode
// SPINDLE_INTEGRATION_VERSION=1

import { spawn } from "node:child_process";

const SOURCE = "spindle:opencode";
const AGENT = "opencode";
let sequence = Date.now() * 1000;
let chain = Promise.resolve();
let rootSession;
const children = new Map();

const CHILD_STATES = new Map([
  ["permission.asked", "blocked"],
  ["question.asked", "blocked"],
  ["permission.replied", "working"],
  ["question.replied", "working"],
  ["question.rejected", "working"],
]);

const STATUS_STATES = new Map([
  ["idle", "idle"],
  ["active", "working"],
  ["busy", "working"],
  ["pending", "working"],
  ["retry", "working"],
  ["running", "working"],
  ["streaming", "working"],
  ["working", "working"],
]);

function nextSequence() {
  sequence += 1;
  return sequence;
}

function runReport(command, args) {
  const binary = process.env.SPINDLE_BIN_PATH || process.env.HERDR_BIN_PATH || "spindle";
  const pane = process.env.SPINDLE_PANE_ID || process.env.HERDR_PANE_ID;
  if (!pane) return Promise.resolve();
  return new Promise((resolve) => {
    const child = spawn(binary, ["pane", command, pane, ...args], {
      stdio: "ignore",
      windowsHide: true,
    });
    child.once("error", resolve);
    child.once("close", resolve);
  });
}

function request(command, args) {
  const pending = chain.then(() => runReport(command, args));
  chain = pending.catch(() => {});
  return pending;
}

function reportSession(sessionID) {
  if (!sessionID) return Promise.resolve();
  return request("report-agent-session", [
    "--source", SOURCE,
    "--agent", AGENT,
    "--seq", String(nextSequence()),
    "--agent-session-id", sessionID,
  ]);
}

function reportState(state, sessionID) {
  const args = ["--source", SOURCE, "--agent", AGENT, "--state", state, "--seq", String(nextSequence())];
  if (sessionID) {
    rootSession = sessionID;
    args.push("--agent-session-id", sessionID);
  }
  return request("report-agent", args);
}

function sessionID(properties) {
  return typeof properties?.sessionID === "string" && properties.sessionID
    ? properties.sessionID
    : undefined;
}

function stateFor(status) {
  const kind = typeof status === "string" ? status : status?.type;
  return typeof kind === "string" ? STATUS_STATES.get(kind.toLowerCase()) : undefined;
}

export const SpindleAgentStatePlugin = async () => {
  if (
    process.env.SPINDLE_ENV !== "1" &&
    process.env.HERDR_ENV !== "1"
  ) {
    return {};
  }

  return {
    "chat.message": async ({ sessionID: id }) => {
      if (!children.has(id)) await reportState("working", id);
    },
    event: async ({ event }) => {
      const type = event?.type;
      const properties = event?.properties ?? {};
      const id = sessionID(properties);
      const info = properties.info;
      if (info?.id && info.parentID) children.set(info.id, info.parentID);

      if (id && children.has(id)) {
        const state = CHILD_STATES.get(type);
        if (state) {
          let root = id;
          while (children.has(root)) root = children.get(root);
          await reportState(state, root);
        }
        return;
      }

      switch (type) {
        case "session.created":
          rootSession = id;
          break;
        case "session.updated":
          if (id && id !== rootSession) await reportSession(id);
          break;
        case "session.status": {
          const state = stateFor(properties.status);
          if (state) await reportState(state, id);
          else await reportSession(id);
          break;
        }
        case "permission.asked":
        case "question.asked":
        case "session.error":
          await reportState("blocked", id);
          break;
        case "session.idle":
          await reportState("idle", id);
          break;
        case "tool.execute.before":
        case "tool.execute.after":
        case "permission.replied":
        case "question.replied":
        case "question.rejected":
        case "session.compacted":
          await reportState("working", id);
          break;
        default:
          break;
      }
    },
  };
};
