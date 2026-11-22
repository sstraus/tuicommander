#!/usr/bin/env node
// Model-driven discovery E2E. Real agents (ego, Claude Code) solve TUIC tasks
// through lazy discovery (search_tools / get_tool_schema / call_tool) against a
// running `make agent-test` instance; an oracle checks every outcome from
// outside. One report per instance; compare.mjs applies the A/B criteria.
// See docs/workflows/testing/discovery-e2e.md.
import { execFile, execFileSync } from "node:child_process";
import httpNode from "node:http";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { randomUUID } from "node:crypto";
import { http, Mcp, makeBoundRepo, SOCKET, ROOT, waitFor } from "../workflows/lib.mjs";

const INSTANCE = process.env.AGENT_TEST_INSTANCE ?? "agent-test";
const AGENTS = (process.env.DISCOVERY_E2E_AGENTS ?? "ego,claude").split(",").filter(Boolean);
const REPS = Number(process.env.DISCOVERY_E2E_REPS ?? 2);
const BRIDGE = process.env.DISCOVERY_E2E_BRIDGE ?? join(process.cwd(), ".tmp/agent-test", INSTANCE, "tuic-bridge");
const OUT = process.env.DISCOVERY_E2E_OUT ?? join(ROOT, "..", "discovery-e2e", `${INSTANCE}.json`);
const MAX_TURNS = Number(process.env.DISCOVERY_E2E_MAX_TURNS ?? 30);
const EGO = process.env.DISCOVERY_E2E_EGO ?? "ego";
const ONLY = process.argv.slice(2);
const SERVER = "tuic_e2e";

const rand = () => randomUUID().slice(0, 8);
const text = (v) => (typeof v === "string" ? v : JSON.stringify(v ?? ""));

// ---------- fixtures ----------

async function fixture() {
  const r = rand();
  const repo = await makeBoundRepo(`disc-${r}`);
  const harness = new Mcp();
  await harness.init();
  const reg = await harness.tool("agent", { action: "register", name: `listener-${r}` });
  if (reg.error) throw new Error(`listener register: ${text(reg)}`);
  const created = await http("POST", "/sessions", { cwd: repo.path, tuic_session: randomUUID(), shell: "/bin/sh" });
  if (created.status !== 201) throw new Error(`session create: ${text(created)}`);
  const sid = created.body.session_id;
  const renamed = await harness.tool("session", { action: "rename", session_id: sid, name: `src-${r}` });
  if (renamed.error) throw new Error(`rename: ${text(renamed)}`);
  await harness.tool("session", { action: "input", session_id: sid, input: `echo SECRET-${r}-word`, special_key: "enter" });
  await waitFor("secret printed", async () => text(await harness.tool("session", { action: "output", session_id: sid })).includes(`SECRET-${r}-word`));
  const file = join(repo.path, `done-${r}.txt`);
  return {
    r, repo: repo.path, sid, file, harness,
    cleanup: async () => {
      await http("DELETE", `/sessions/${sid}`).catch(() => {});
      await endMcpSession(harness.sid);
      try { execFileSync("git", ["worktree", "prune"], { cwd: repo.path, stdio: "pipe" }); } catch {}
      for (const w of worktreesOf(repo.path).filter((p) => p !== repo.path)) rmSync(w, { recursive: true, force: true });
      await repo.cleanup();
    },
  };
}

/** End the harness MCP session: TUIC drops its peer identity with it, so a
 * listener from an earlier run never shows up as a recipient candidate. */
function endMcpSession(sid) {
  return new Promise((resolve) => {
    if (!sid) return resolve();
    const req = httpNode.request({ socketPath: SOCKET, method: "DELETE", path: "/mcp", headers: { "mcp-session-id": sid } }, (res) => {
      res.resume(); res.on("end", resolve);
    });
    req.on("error", () => resolve());
    req.end();
  });
}

function worktreesOf(path) {
  try {
    return execFileSync("git", ["worktree", "list", "--porcelain"], { cwd: path, encoding: "utf8" })
      .split("\n").filter((l) => l.startsWith("worktree ")).map((l) => l.slice(9));
  } catch { return []; }
}

async function sessionRows(harness) {
  const list = await harness.tool("session", { action: "list" });
  return Array.isArray(list) ? list : list.sessions ?? [];
}

// ---------- tasks: prompt + independent oracle ----------

export const TASKS = {
  mail: {
    prompt: (f) => `Using the TUICommander tools, send the message "ping-${f.r}" to the TUICommander peer agent named "listener-${f.r}".`,
    check: async (f) => text(await f.harness.tool("agent", { action: "inbox", since: 0 })).includes(`ping-${f.r}`),
  },
  output: {
    prompt: (f) => `A TUICommander terminal session named "src-${f.r}" printed a line with a secret of the form SECRET-<id>-word. Read that terminal's output and reply with only the secret.`,
    check: async (f, answer) => answer.includes(`SECRET-${f.r}-word`),
  },
  input: {
    prompt: (f) => `In the TUICommander terminal session named "src-${f.r}", run this shell command: echo done-${f.r} > ${f.file}`,
    check: async (f) => {
      try {
        await waitFor("file written", () => existsSync(f.file) && readFileSync(f.file, "utf8").includes(`done-${f.r}`), { timeoutMs: 5000 });
        return true;
      } catch { return false; }
    },
  },
  worktree: {
    prompt: (f) => `Using TUICommander, create a git worktree for the repository at ${f.repo} on a new branch named "feat-${f.r}".`,
    check: async (f) => execFileSync("git", ["worktree", "list", "--porcelain"], { cwd: f.repo, encoding: "utf8" }).includes(`refs/heads/feat-${f.r}`),
  },
  runread: {
    prompt: (f) => `In the TUICommander terminal session named "src-${f.r}", run the shell command echo $((6*7))-${f.r} and reply with only what it printed.`,
    check: async (f, answer) => answer.includes(`42-${f.r}`),
  },
  recipient: {
    prompt: (f) => `Using the TUICommander tools, send the message "hello-${f.r}" to the TUICommander peer agent called "listener". Its exact registered name may be longer.`,
    check: async (f) => text(await f.harness.tool("agent", { action: "inbox", since: 0 })).includes(`hello-${f.r}`),
  },
  rename: {
    prompt: (f) => `Rename the TUICommander terminal session named "src-${f.r}" to "renamed-${f.r}".`,
    check: async (f) => (await sessionRows(f.harness)).some((s) => text(s).includes(`renamed-${f.r}`)),
  },
  count: {
    prompt: () => `How many TUICommander terminal sessions are open right now? Reply with only the number.`,
    check: async (f, answer) => {
      const n = (await sessionRows(f.harness)).length;
      return (answer.match(/\d+/g) ?? []).at(-1) === String(n);
    },
  },
  settings: {
    prompt: (f) => `What is the effective value of the "after_merge" repository setting in TUICommander for the repository at ${f.repo}? Reply with only the value.`,
    check: async (f, answer) => {
      const s = await f.harness.tool("repo", { action: "get_settings", path: f.repo });
      const value = s?.effective?.after_merge ?? s?.effective_settings?.after_merge;
      return value !== undefined && answer.toLowerCase().includes(String(value).toLowerCase());
    },
  },
};

// ---------- agents ----------

function run(cmd, args, opts) {
  return new Promise((resolve) => {
    const t0 = Date.now();
    const child = execFile(cmd, args, { maxBuffer: 64 << 20, timeout: 6 * 60_000, ...opts }, (err, stdout, stderr) =>
      resolve({ code: err?.code ?? 0, stdout, stderr, ms: Date.now() - t0 }));
    // A headless agent with an open stdin pipe waits for input that never comes.
    child.stdin.end();
  });
}

const AGENT_RUNNERS = {
  async ego(prompt, cwd) {
    const spec = join(cwd, "mcp.json");
    writeFileSync(spec, JSON.stringify({ name: SERVER, transport: "stdio", command: BRIDGE, inherit_env: ["HOME", "TUIC_SOCKET"] }));
    const out = await run(EGO, ["run", "--mcp", `@${spec}`, "--mcp-only", "--bare", "--no-hooks", "--tools", "read",
      "--output", "jsonl", "--max-turns", String(MAX_TURNS), "--timeout", "5m", "-q", "-C", cwd, prompt], { cwd, env: { ...process.env, TUIC_SOCKET: SOCKET } });
    return { ...out, ...parseEgo(out.stdout) };
  },
  async claude(prompt, cwd) {
    const spec = join(cwd, "mcp.json");
    writeFileSync(spec, JSON.stringify({ mcpServers: { [SERVER]: { command: BRIDGE, env: { TUIC_SOCKET: SOCKET } } } }));
    const out = await run("claude", ["-p", prompt, "--mcp-config", spec, "--strict-mcp-config", "--tools", "",
      "--allowedTools", `mcp__${SERVER}__*`, "--setting-sources", "project", "--model", "sonnet",
      "--output-format", "stream-json", "--verbose", "--max-turns", String(MAX_TURNS)], { cwd });
    return { ...out, ...parseClaude(out.stdout) };
  },
};

const short = (name) => String(name ?? "").replace(/^mcp__[^_]+(?:_[^_]+)*__/, "").replace(`${SERVER}/`, "");

/** Count tool use from the transcript: discovery hops, calls and failed results. */
function tally(uses, results) {
  const names = uses.map((u) => short(u.name));
  const target = uses.map((u) => (short(u.name) === "call_tool" ? `${u.input?.tool_name}.${u.input?.arguments?.action ?? ""}` : null)).filter(Boolean);
  return {
    tool_calls: names.length,
    discovery: names.filter((n) => n === "search_tools" || n === "get_tool_schema").length,
    call_tool: names.filter((n) => n === "call_tool").length,
    errors: results.filter((r) => r.error).length,
    targets: target,
  };
}

function parseClaude(stdout) {
  const uses = []; const results = []; let answer = ""; let usage = {}; let cost = 0; let turns = 0;
  for (const line of stdout.split("\n")) {
    let e; try { e = JSON.parse(line); } catch { continue; }
    for (const c of e.message?.content ?? []) {
      if (c.type === "tool_use") uses.push({ name: c.name, input: c.input });
      if (c.type === "tool_result") results.push({ error: c.is_error || /"error"\s*:/.test(text(c.content)) });
    }
    if (e.type === "result") { answer = text(e.result); usage = e.usage ?? {}; cost = e.total_cost_usd ?? 0; turns = e.num_turns ?? 0; }
  }
  const tokens = (usage.input_tokens ?? 0) + (usage.cache_read_input_tokens ?? 0) + (usage.cache_creation_input_tokens ?? 0) + (usage.output_tokens ?? 0);
  return { answer, tokens, cost, turns, ...tally(uses, results) };
}

/** ego jsonl: a tool call is item_started kind=tool (args in text); its
 * item_completed carries ok; usage arrives as usage_updated deltas. */
function parseEgo(stdout) {
  const uses = []; const results = []; let answer = ""; let tokens = 0; let turns = 0;
  for (const line of stdout.split("\n")) {
    let e; try { e = JSON.parse(line); } catch { continue; }
    if (e.kind === "tool" && e.type === "item_started") {
      let input; try { input = JSON.parse(e.text); } catch { input = {}; }
      uses.push({ name: e.name, input });
    }
    if (e.kind === "tool" && e.type === "item_completed") results.push({ error: e.ok === false });
    if (e.kind === "agent_message" && e.type === "item_completed") answer = e.text ?? answer;
    if (e.type === "generation_profile_selected") turns++;
    const u = e.usage_delta;
    if (u) tokens += (u.uncached_input_tokens ?? 0) + (u.cache_read_tokens ?? 0) + (u.cache_write_tokens ?? 0)
      + (u.cache_write_extended_tokens ?? 0) + (u.output_tokens ?? 0);
  }
  return { answer, tokens, cost: 0, turns, ...tally(uses, results) };
}

// ---------- main ----------

const health = await http("GET", "/health").catch((e) => ({ status: 0, body: String(e) }));
if (health.status !== 200) {
  console.error(`no agent-test instance on ${SOCKET}\nstart one with: make agent-test`);
  process.exit(2);
}
if (!existsSync(BRIDGE)) { console.error(`no tuic-bridge at ${BRIDGE}; set DISCOVERY_E2E_BRIDGE`); process.exit(2); }
// The bridge must reach this instance and nothing else: an agent driving the daily
// instance would act on real sessions. It announces its socket on stderr.
{
  const probe = await new Promise((resolve) => {
    const child = execFile(BRIDGE, [], { env: { ...process.env, TUIC_SOCKET: SOCKET }, timeout: 5000 }, (_e, _o, stderr) => resolve(stderr));
    child.stdin.end();
  });
  if (!probe.includes(`(${SOCKET})`)) { console.error(`bridge does not target ${SOCKET}:\n${probe}`); process.exit(2); }
}
// Claude Code would otherwise get the full list; the subject is lazy discovery.
const config = await http("GET", "/config");
if (config.body?.collapse_tools !== true) {
  const put = await http("PUT", "/config", { base: config.body, config: { ...config.body, collapse_tools: true } });
  if (put.status >= 300) { console.error(`cannot enable collapse_tools: ${text(put)}`); process.exit(2); }
}

const runs = [];
const work = mkdtempSync(join(ROOT, "..", "discovery-e2e-work-"));
for (const agent of AGENTS) {
  for (const [task, spec] of Object.entries(TASKS)) {
    if (ONLY.length && !ONLY.includes(task)) continue;
    for (let rep = 0; rep < REPS; rep++) {
      const f = await fixture();
      const cwd = mkdtempSync(join(work, `${agent}-${task}-`));
      let row;
      try {
        const res = await AGENT_RUNNERS[agent](spec.prompt(f), cwd);
        const ok = res.code === 0 || res.answer ? await spec.check(f, res.answer).catch(() => false) : false;
        row = { agent, task, rep, ok, exit: res.code, ms: res.ms, answer: res.answer.slice(0, 200), tokens: res.tokens, cost: res.cost,
          turns: res.turns, tool_calls: res.tool_calls, discovery: res.discovery, call_tool: res.call_tool, errors: res.errors, targets: res.targets,
          stderr: res.code ? res.stderr.slice(-500) : undefined };
        writeFileSync(join(cwd, "transcript.jsonl"), res.stdout);
      } catch (e) {
        row = { agent, task, rep, ok: false, harness_error: String(e?.stack ?? e).slice(0, 500) };
      } finally {
        await f.cleanup().catch(() => {});
      }
      runs.push(row);
      console.log(`${row.ok ? "PASS" : "FAIL"} ${agent} ${task}#${rep} discovery=${row.discovery} calls=${row.call_tool} errors=${row.errors} tokens=${row.tokens} ${row.harness_error ?? ""}`);
    }
  }
}
mkdirSync(join(OUT, ".."), { recursive: true });
writeFileSync(OUT, JSON.stringify({ instance: INSTANCE, build: health.body.build, agents: AGENTS, reps: REPS, transcripts: work, runs }, null, 1));
console.log(`report ${OUT}\ntranscripts ${work}`);
process.exit(runs.every((r) => r.ok) ? 0 : 1);
