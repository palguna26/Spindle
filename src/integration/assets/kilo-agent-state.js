// installed by spindle
// SPINDLE_INTEGRATION_ID=kilo
// SPINDLE_INTEGRATION_VERSION=1
import net from "node:net";

const SOURCE = "spindle:kilo";
const AGENT = "kilo";
let seq = Date.now() * 1000;
const nextSeq = () => ++seq;
const sessionId = (properties) => typeof properties?.sessionID === "string" && properties.sessionID ? properties.sessionID : undefined;
const state = (value) => ["active", "busy", "pending", "running", "streaming", "working"].includes(String(value).toLowerCase()) ? "working" : String(value).toLowerCase() === "idle" ? "idle" : undefined;
function request(method, params) {
  const pane = process.env.SPINDLE_PANE_ID || process.env.HERDR_PANE_ID;
  const socket = process.env.SPINDLE_SOCKET_PATH || process.env.HERDR_SOCKET_PATH;
  if (!pane || !socket) return Promise.resolve();
  const endpoint = process.platform === "win32" ? `\\\\.\\pipe\\${socket}` : socket;
  return new Promise((resolve) => { const client = net.createConnection(endpoint, () => client.write(`${JSON.stringify({ id: `${SOURCE}:${Date.now()}`, method, params: { pane_id: pane, source: SOURCE, agent: AGENT, seq: nextSeq(), ...params } })}\n`)); const done = () => { client.destroy(); resolve(); }; client.setTimeout(500, done); client.on("data", done); client.on("error", done); client.on("end", done); client.on("close", resolve); });
}
const report = (value, id) => request("pane.report_agent", { state: value, ...(id ? { agent_session_id: id } : {}) });
const reportSession = (id) => id ? request("pane.report_agent_session", { agent_session_id: id, session_start_source: "startup" }) : Promise.resolve();
export const SpindleAgentStatePlugin = async () => {
  if ((process.env.SPINDLE_ENV || process.env.HERDR_ENV) !== "1") return {};
  return { "chat.message": async ({ sessionID }) => report("working", sessionID), event: async ({ event }) => { const type = event?.type; const props = event?.properties || {}; const id = sessionId(props); if (type === "session.created" || type === "session.updated") return reportSession(id); if (type === "session.status") return report(state(props.status) || "idle", id); if (["permission.asked", "question.asked", "session.error"].includes(type)) return report("blocked", id); if (type === "session.idle") return report("idle", id); if (["tool.execute.before", "tool.execute.after", "permission.replied", "question.replied", "question.rejected", "session.compacted"].includes(type)) return report("working", id); } };
};
