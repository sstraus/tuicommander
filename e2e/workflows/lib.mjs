// Black-box helpers for the workflow E2E runner. Talks only to the isolated
// `make agent-test` instance over HTTP and its MCP endpoint.
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync, realpathSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { randomUUID } from "node:crypto";

import http_ from "node:http";
import { readFileSync } from "node:fs";

// The isolated instance serves TCP :19876 behind Basic auth that `make agent-test`
// never configures; agents use its Unix socket (scripts/agent-test.py "connect").
const INSTANCE = process.env.AGENT_TEST_INSTANCE ?? "agent-test";
export const SOCKET = process.env.WF_E2E_SOCKET ?? readFileSync(
  join(homedir(), "Library/Application Support/com.tuic.commander/instances", INSTANCE, "mcp-socket"), "utf8").trim();
if (/orchestrator|14319|9876/.test(SOCKET)) throw new Error(`refusing to drive ${SOCKET}`);
export const ROOT = join(homedir(), "Gits", ".tmp", "wf-e2e");

function request(method, path, body, headers = {}) {
  return new Promise((resolve, reject) => {
    const data = body === undefined ? undefined : JSON.stringify(body);
    const req = http_.request({ socketPath: SOCKET, method, path, headers: {
      "content-type": "application/json", ...(data ? { "content-length": Buffer.byteLength(data) } : {}), ...headers } }, (res) => {
      const chunks = [];
      res.on("data", (c) => chunks.push(c));
      res.on("end", () => resolve({ status: res.statusCode, headers: res.headers, text: Buffer.concat(chunks).toString("utf8") }));
    });
    req.setTimeout(60000, () => req.destroy(new Error(`timeout ${method} ${path}`)));
    req.on("error", reject);
    if (data) req.write(data);
    req.end();
  });
}

/** Throwaway git repository; returns {path, cleanup}. */
export function makeRepo(label) {
  mkdirSync(ROOT, { recursive: true });
  const path = realpathSync(mkdtempSync(join(ROOT, `${label}-`)));
  const git = (...a) => execFileSync("git", a, { cwd: path, stdio: "pipe" });
  git("init", "-q", "-b", "main");
  git("config", "user.email", "e2e@example.invalid");
  git("config", "user.name", "e2e");
  writeFileSync(join(path, "README.md"), "e2e\n");
  git("add", "-A");
  git("commit", "-q", "-m", "init");
  return { path, cleanup: () => rmSync(path, { recursive: true, force: true }) };
}

/** Repository registered with the instance (repo watcher) and cleaned up afterwards. */
export async function makeBoundRepo(label) {
  const repo = makeRepo(label);
  const q = `?path=${encodeURIComponent(repo.path)}`;
  const w = await http("POST", `/watchers/repo${q}`, { path: repo.path });
  if (w.status >= 300) throw new Error(`watch failed: ${JSON.stringify(w)}`);
  return { path: repo.path, cleanup: async () => { await http("DELETE", `/watchers/repo${q}`); repo.cleanup(); } };
}

/** MCP client bound to a throwaway managed PTY whose cwd is `repoPath`. */
export async function bindMcp(repoPath) {
  const tuic = randomUUID();
  const s = await http("POST", "/sessions", { cwd: repoPath, tuic_session: tuic, shell: "/bin/sh" });
  if (s.status !== 201) throw new Error(`session create failed: ${JSON.stringify(s)}`);
  const mcp = new Mcp();
  await mcp.init();
  const reg = await mcp.tool("agent", { action: "register", tuic_session: tuic, name: "wf-e2e", path: repoPath });
  if (!reg.ok) throw new Error(`register failed: ${JSON.stringify(reg)}`);
  mcp.close = () => http("DELETE", `/sessions/${s.body.session_id}`);
  return mcp;
}

export async function http(method, path, body) {
  const r = await request(method, path, body);
  let json;
  try { json = JSON.parse(r.text); } catch { json = r.text; }
  return { status: r.status, body: json };
}

/** Neutral workflow action over HTTP. Throws on a non-2xx reply. */
export async function neutral(project, action) {
  const r = await neutralRaw(project, action);
  if (r.status >= 300) throw new Error(`HTTP ${r.status} for ${action.action}: ${JSON.stringify(r.body)}`);
  return r.body;
}
export const neutralRaw = (project, action) =>
  http("POST", `/workflows/neutral/action?path=${encodeURIComponent(project)}`, action);

/** Minimal MCP streamable-HTTP client (JSON-RPC). */
export class Mcp {
  constructor() { this.sid = null; this.id = 0; }
  async rpc(method, params) {
    const res = await request("POST", "/mcp", { jsonrpc: "2.0", id: ++this.id, method, params }, {
      accept: "application/json, text/event-stream",
      ...(this.sid ? { "mcp-session-id": this.sid } : {}),
    });
    this.sid ??= res.headers["mcp-session-id"];
    const text = res.text;
    const data = text.startsWith("event:") || text.includes("\ndata:") || text.startsWith("data:")
      ? text.split("\n").filter((l) => l.startsWith("data:")).map((l) => l.slice(5).trim()).at(-1)
      : text;
    return JSON.parse(data);
  }
  async init() {
    const r = await this.rpc("initialize", {
      protocolVersion: "2025-03-26", capabilities: {}, clientInfo: { name: "wf-e2e", version: "1" },
    });
    if (r.error) throw new Error(`MCP initialize failed: ${JSON.stringify(r.error)}`);
    return r;
  }
  /** Call a tool; returns the parsed JSON text content. */
  async tool(name, args) {
    const r = await this.rpc("tools/call", { name, arguments: args });
    if (r.error) throw new Error(`MCP ${name}: ${JSON.stringify(r.error)}`);
    const text = r.result?.content?.[0]?.text ?? "null";
    try { return JSON.parse(text); } catch { return text; }
  }
}

export async function waitFor(label, fn, { timeoutMs = 30000, everyMs = 250 } = {}) {
  const end = Date.now() + timeoutMs;
  let last;
  while (Date.now() < end) {
    last = await fn();
    if (last) return last;
    await new Promise((r) => setTimeout(r, everyMs));
  }
  throw new Error(`timeout waiting for ${label}`);
}

/** Recursively replace volatile values so two replies compare by shape. */
export function shape(v) {
  if (Array.isArray(v)) return v.length ? [shape(v[0])] : [];
  if (v && typeof v === "object") return Object.fromEntries(Object.keys(v).sort().map((k) => [k, shape(v[k])]));
  return v === null ? "null" : typeof v;
}

/** Collect `/events` SSE frames until `stop()` is called. Returns {frames, stop}. */
export function sseCollect() {
  const frames = [];
  const req = http_.request({ socketPath: SOCKET, method: "GET", path: "/events", headers: { accept: "text/event-stream" } }, (res) => {
    let buf = "";
    res.on("data", (c) => {
      buf += c.toString("utf8");
      let i;
      while ((i = buf.indexOf("\n\n")) >= 0) {
        const frame = buf.slice(0, i); buf = buf.slice(i + 2);
        const ev = /^event:\s*(.*)$/m.exec(frame)?.[1]; const data = /^data:\s*(.*)$/m.exec(frame)?.[1];
        if (ev) frames.push({ event: ev, data });
      }
    });
  });
  req.on("error", () => {});
  req.end();
  return { frames, stop: () => req.destroy() };
}
