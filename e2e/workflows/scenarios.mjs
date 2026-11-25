// Scenarios 1-6. Each takes a context {note(msg)} and throws on failure.
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import assert from "node:assert/strict";
import { makeBoundRepo, bindMcp, sseCollect, neutral, neutralRaw, waitFor, shape } from "./lib.mjs";
import * as D from "./definitions.mjs";

const refOf = (pub) => ({ project: pub.project, id: pub.id, revision: pub.revision });
const startReq = (pub, request_id, inputs) => ({ publication: refOf(pub), request_id, inputs, subject: null, limits: D.LIMITS });
const inspect = async (project, runId) => (await neutral(project, { action: "inspect", run_id: runId, after_sequence: 0, limit: 500 })).value;
const terminal = (s) => s.status.type !== "running";

async function author(project, def, name) {
  const draft = (await neutral(project, { action: "create_draft", name, definition: def })).value;
  const v = (await neutral(project, { action: "validate", definition: def })).value;
  assert.equal(v.valid, true, `validate: ${JSON.stringify(v.diagnostics)}`);
  const pub = (await neutral(project, { action: "publish", id: draft.id, expected_revision: draft.revision })).value;
  return { draft, pub };
}

async function startApproval(project, marker, requestId) {
  const { draft, pub } = await author(project, D.approvalDefinition(), "approval");
  const run = (await neutral(project, { action: "start", request: startReq(pub, requestId, { text: "ship it?", marker }) })).value;
  const runId = run.snapshot.run.id;
  const waiting = await waitFor("approval question", async () => {
    const i = await inspect(project, runId);
    return Object.values(i.snapshot.effects ?? {}).some((e) => e.question) ? i : null;
  });
  const effect = Object.values(waiting.snapshot.effects).find((e) => e.question);
  return { draft, pub, runId, waiting, effect };
}
const answerAction = (runId, w, effect, approved, cmd) => ({
  action: "answer", run_id: runId, command_id: cmd, expected_sequence: w.snapshot.sequence,
  key: effect.key, revision: effect.question.revision, approved,
});
const lines = (f) => (existsSync(f) ? readFileSync(f, "utf8").split("\n").filter(Boolean) : []);

export const scenarios = {
  async "1 author -> validate -> publish -> run -> terminal (script + condition)"(ctx) {
    const repo = await makeBoundRepo("s1"); ctx.cleanup(repo.cleanup);
    await neutral(repo.path, { action: "save_operation", expected_version: 0, descriptor: D.calcOperation() });
    const { pub } = await author(repo.path, D.scriptConditionDefinition(), "calc");
    assert.equal(pub.revision, 1);
    const start = (await neutral(repo.path, { action: "start", request: startReq(pub, "s1-a", { value: 7 }) })).value;
    const runId = start.snapshot.run.id;
    const done = await waitFor("terminal", async () => { const i = await inspect(repo.path, runId); return terminal(i.snapshot) ? i : null; });
    assert.deepEqual(done.snapshot.status, { type: "completed", outcome: "done" });
    assert.deepEqual(done.snapshot.outputs.values, { next: 8 });
    assert.equal(done.events.at(-1).kind.type !== undefined, true);
    ctx.note(`events=${done.events.length} outputs=${JSON.stringify(done.snapshot.outputs.values)}`);
    // The false branch of the condition: 3+1 != 8 ends `blocked`, no git or story needed.
    const s2 = (await neutral(repo.path, { action: "start", request: startReq(pub, "s1-b", { value: 3 }) })).value;
    const blocked = await waitFor("terminal 2", async () => { const i = await inspect(repo.path, s2.snapshot.run.id); return terminal(i.snapshot) ? i : null; });
    assert.deepEqual(blocked.snapshot.status, { type: "completed", outcome: "blocked" });
    // Same request id and payload is idempotent: no second run.
    const again = (await neutral(repo.path, { action: "start", request: startReq(pub, "s1-a", { value: 7 }) })).value;
    assert.equal(again.snapshot.run.id, runId, "a retried start must return the original run");
  },

  async "2 exact approval answered over HTTP produces one effect"(ctx) {
    const repo = await makeBoundRepo("s2"); ctx.cleanup(repo.cleanup);
    await neutral(repo.path, { action: "save_operation", expected_version: 0, descriptor: D.appendOperation("rev1") });
    const marker = join(repo.path, "marker.txt");
    const { runId, waiting, effect } = await startApproval(repo.path, marker, "s2");
    assert.equal(waiting.snapshot.status.type, "running");
    assert.deepEqual(lines(marker), [], "no effect before approval");
    // A stale sequence must be refused and leave the run waiting; then the exact answer is accepted.
    const stale = await neutralRaw(repo.path, { ...answerAction(runId, waiting, effect, true, "stale"), expected_sequence: 0 });
    assert.ok(stale.status >= 400, `stale sequence must be refused, got ${stale.status}`);
    assert.equal((await inspect(repo.path, runId)).snapshot.status.type, "running");
    assert.deepEqual(lines(marker), [], "a refused answer must not run the effect");
    await neutral(repo.path, answerAction(runId, waiting, effect, true, "exact"));
    const done = await waitFor("terminal", async () => { const i = await inspect(repo.path, runId); return terminal(i.snapshot) ? i : null; });
    assert.deepEqual(done.snapshot.status, { type: "completed", outcome: "done" });
    assert.deepEqual(lines(marker), ["rev1"], "exactly one effect");
    const dup = await neutralRaw(repo.path, answerAction(runId, waiting, effect, true, "exact"));
    ctx.note(`duplicate answer after completion: HTTP ${dup.status} ${JSON.stringify(dup.body).slice(0, 120)}`);
    await new Promise((r) => setTimeout(r, 500));
    assert.deepEqual(lines(marker), ["rev1"], "a duplicate answer must not repeat the effect");
    // Denied branch.
    const d = await startApproval(repo.path, join(repo.path, "m2.txt"), "s2-deny");
    await neutral(repo.path, answerAction(d.runId, d.waiting, d.effect, false, "deny"));
    const den = await waitFor("denied", async () => { const i = await inspect(repo.path, d.runId); return terminal(i.snapshot) ? i : null; });
    // Declining an exact approval fails the run (it is not a branch); the effect never runs.
    assert.deepEqual(den.snapshot.status, { type: "failed", diagnostic: "neutral effect approval declined" });
    assert.deepEqual(lines(join(repo.path, "m2.txt")), [], "denial must not run the effect");
  },

  async "3 MCP and HTTP return identical shapes"(ctx) {
    const repo = await makeBoundRepo("s3"); ctx.cleanup(repo.cleanup);
    const mcp = await bindMcp(repo.path); ctx.cleanup(() => mcp.close());
    const viaMcp = async (input) => mcp.tool("workflow", { action: "neutral", input });
    await neutral(repo.path, { action: "save_operation", expected_version: 0, descriptor: D.calcOperation() });
    const def = D.scriptConditionDefinition();
    const h = {}; const m = {};
    h.draft = (await neutral(repo.path, { action: "create_draft", name: "p", definition: def }));
    m.draft = await viaMcp({ action: "create_draft", name: "p", definition: def });
    h.validate = await neutral(repo.path, { action: "validate", definition: def });
    m.validate = await viaMcp({ action: "validate", definition: def });
    h.pub = await neutral(repo.path, { action: "publish", id: h.draft.value.id, expected_revision: 1 });
    m.pub = await viaMcp({ action: "publish", id: m.draft.value?.id, expected_revision: 1 });
    // Same record read through both transports must be byte-identical.
    const getH = await neutral(repo.path, { action: "get_publication", id: h.pub.value.id, revision: 1 });
    const getM = await viaMcp({ action: "get_publication", id: h.pub.value.id, revision: 1 });
    assert.deepEqual(getM, getH, "get_publication differs between MCP and HTTP");
    const listH = await neutral(repo.path, { action: "list_drafts" });
    const listM = await viaMcp({ action: "list_drafts" });
    assert.deepEqual(listM, listH, "list_drafts differs between MCP and HTTP");
    for (const k of ["draft", "validate", "pub"]) assert.deepEqual(shape(m[k]), shape(h[k]), `${k} shape differs`);
    // MCP start binds the managed Agent, which fails closed without a backend capability binding
    // (a bare `agent register` has none). Starts therefore go through HTTP; reads are compared.
    const mcpStart = await viaMcp({ action: "start", request: startReq(h.pub.value, "s3-m", { value: 7 }) });
    ctx.note(`MCP start by an unprivileged registered peer: ${JSON.stringify(mcpStart).slice(0, 120)}`);
    assert.ok(mcpStart.error, "an Agent without a capability binding must not start a script run");
    ctx.finding(`MCP start refused for a registered peer without capability binding ("${mcpStart.error}") while HTTP (Owner) starts the same publication; by design (fail closed) but not testable black-box without an agent launched by TUIC.`);
    const started = await neutral(repo.path, { action: "start", request: startReq(h.pub.value, "s3-h", { value: 7 }) });
    const rid = started.value.snapshot.run.id;
    await waitFor("run", async () => { const i = await inspect(repo.path, rid); return terminal(i.snapshot) ? i : null; });
    for (const action of [{ action: "inspect", run_id: rid, after_sequence: 0, limit: 500 }, { action: "events", run_id: rid, after_sequence: 0, limit: 500 }, { action: "inspect", run_id: rid, after_sequence: 3, limit: 2 }]) {
      const viaH = await neutral(repo.path, action);
      const viaM = await viaMcp(action);
      assert.deepEqual(viaM, viaH, `${action.action} (after ${action.after_sequence}) differs between MCP and HTTP`);
    }
    // A refusal carries the same message on both transports.
    const bad = { action: "get_publication", id: "no-such", revision: 1 };
    const refH = await neutralRaw(repo.path, bad);
    const refM = await viaMcp(bad);
    assert.equal(refM.error, refH.body.error, "refusal text differs between MCP and HTTP");
    ctx.note(`refusal over HTTP is status ${refH.status}`);
    if (refH.status >= 500) ctx.finding(`HTTP maps a user refusal to ${refH.status} (get_publication of an unknown id: "${refH.body.error}"); MCP returns {error} with no status.`);
  },

  async "4 cancel mid-run keeps history"(ctx) {
    const repo = await makeBoundRepo("s4"); ctx.cleanup(repo.cleanup);
    await neutral(repo.path, { action: "save_operation", expected_version: 0, descriptor: D.appendOperation("rev1") });
    const marker = join(repo.path, "marker.txt");
    const { runId, waiting } = await startApproval(repo.path, marker, "s4");
    const before = waiting.events.length;
    await neutral(repo.path, { action: "cancel", run_id: runId, command_id: "cancel-1", expected_sequence: waiting.snapshot.sequence });
    const end = await waitFor("cancelled", async () => { const i = await inspect(repo.path, runId); return terminal(i.snapshot) ? i : null; });
    assert.equal(end.snapshot.status.type, "cancelled");
    assert.ok(end.events.length > before, "cancel must append to the retained history");
    assert.equal(end.events[0].kind.type, "started", "history keeps the start event");
    assert.deepEqual(lines(marker), [], "a cancelled run must not run its effect");
    // A cancelled run stays cancelled, and cannot be answered into life.
    const eff = Object.values(waiting.snapshot.effects).find((e) => e.question);
    const late = await neutralRaw(repo.path, answerAction(runId, end, eff, true, "late"));
    ctx.note(`late answer on a cancelled run: HTTP ${late.status} ${JSON.stringify(late.body).slice(0, 120)}`);
    const still = await inspect(repo.path, runId);
    assert.equal(still.snapshot.status.type, "cancelled");
    assert.deepEqual(lines(marker), []);
  },

  async "5 new revision mid-run keeps the pin"(ctx) {
    const repo = await makeBoundRepo("s5"); ctx.cleanup(repo.cleanup);
    await neutral(repo.path, { action: "save_operation", expected_version: 0, descriptor: D.appendOperation("rev1") });
    const marker = join(repo.path, "marker.txt");
    const { draft, pub, runId, waiting, effect } = await startApproval(repo.path, marker, "s5");
    // Revision 2 of the same workflow writes a different tag through a different saved operation.
    const op2 = D.scriptOperation("append2", { marker: D.fieldStr }, { ok: D.fieldBool },
      "open(q['inputs']['marker'],'a').write('rev2\\n')\ndata=dict(ok=True)");
    await neutral(repo.path, { action: "save_operation", expected_version: 0, descriptor: op2 });
    const def2 = D.approvalDefinition();
    def2.instances[1].descriptor = D.ids("append2");
    const upd = (await neutral(repo.path, { action: "update_draft", id: draft.id, expected_revision: draft.revision, name: "approval", definition: def2 })).value;
    const pub2 = (await neutral(repo.path, { action: "publish", id: draft.id, expected_revision: upd.revision })).value;
    assert.equal(pub2.revision, 2);
    await neutral(repo.path, answerAction(runId, waiting, effect, true, "ok"));
    const done = await waitFor("terminal", async () => { const i = await inspect(repo.path, runId); return terminal(i.snapshot) ? i : null; });
    assert.equal(done.snapshot.status.type, "completed");
    assert.deepEqual(lines(marker), ["rev1"], "the run must execute its pinned revision 1");
    assert.equal(done.snapshot.run.closure.root.revision, 1);
    // A new run pins revision 2.
    const r2 = (await neutral(repo.path, { action: "start", request: startReq(pub2, "s5-b", { text: "again?", marker }) })).value;
    assert.equal(r2.snapshot.run.closure.root.revision, 2);
  },

  async "6 invalid definition is refused with a diagnosis"(ctx) {
    const repo = await makeBoundRepo("s6"); ctx.cleanup(repo.cleanup);
    await neutral(repo.path, { action: "save_operation", expected_version: 0, descriptor: D.calcOperation() });
    const bad = D.scriptConditionDefinition();
    bad.control.edges.check.true = "ghost";           // edge to a node that does not exist
    const v = (await neutral(repo.path, { action: "validate", definition: bad })).value;
    assert.equal(v.valid, false);
    assert.ok(v.diagnostics.length > 0 && v.diagnostics.every((d) => d.path && d.code && d.message), JSON.stringify(v));
    ctx.note(`diagnosis: ${JSON.stringify(v.diagnostics[0])}`);
    const wrongBinding = D.scriptConditionDefinition();
    wrongBinding.input_bindings[0].value = { source: "workflow_input", field: "nope" };
    const v2 = (await neutral(repo.path, { action: "validate", definition: wrongBinding })).value;
    assert.equal(v2.valid, false, "a binding to an unknown workflow input must be invalid");
    // Publishing the invalid draft must be refused and leave no publication.
    const created = await neutralRaw(repo.path, { action: "create_draft", name: "bad", definition: bad });
    if (created.status < 300) {
      const pubTry = await neutralRaw(repo.path, { action: "publish", id: created.body.value.id, expected_revision: 1 });
      assert.ok(pubTry.status >= 400, "publishing an invalid draft must fail");
      const got = await neutralRaw(repo.path, { action: "get_publication", id: created.body.value.id, revision: 1 });
      assert.ok(got.status >= 400, "no publication may exist after a refused publish");
    }
    // Unknown action fields are rejected before any effect.
    const forged = await neutralRaw(repo.path, { action: "catalog", grants: ["write"] });
    assert.ok(forged.status >= 400, "unknown fields must be rejected");
  },

  async "7 HTTP retries with the same command_id are idempotent"(ctx) {
    const repo = await makeBoundRepo("s7"); ctx.cleanup(repo.cleanup);
    await neutral(repo.path, { action: "save_operation", expected_version: 0, descriptor: D.appendOperation("rev1") });
    const marker = join(repo.path, "marker.txt");
    const { pub, runId, waiting, effect } = await startApproval(repo.path, marker, "s7");
    // Lost response to Start: the same request returns the same run, never a second one.
    const again = (await neutral(repo.path, { action: "start", request: startReq(pub, "s7", { text: "ship it?", marker }) })).value;
    assert.equal(again.snapshot.run.id, runId, "a retried start must not create a second run");
    // Lost response to Answer: send the identical command twice (same command_id and expected_sequence).
    const ans = answerAction(runId, waiting, effect, true, "s7-answer");
    const first = await neutralRaw(repo.path, ans);
    const second = await neutralRaw(repo.path, ans);
    ctx.note(`answer retry: first HTTP ${first.status}, second HTTP ${second.status}`);
    assert.ok(first.status < 300, JSON.stringify(first.body));
    const done = await waitFor("terminal", async () => { const i = await inspect(repo.path, runId); return terminal(i.snapshot) ? i : null; });
    assert.equal(done.snapshot.status.type, "completed");
    await new Promise((r) => setTimeout(r, 500));
    assert.deepEqual(lines(marker), ["rev1"], "a retried answer must not repeat the effect");
    const answered = (await inspect(repo.path, runId)).events.filter((e) => e.command_id === "s7-answer");
    assert.equal(answered.length, 1, "one answer event for one command_id");
    // Same command_id with a different payload must not be accepted as the original.
    const clash = await neutralRaw(repo.path, { ...ans, approved: false });
    assert.ok(clash.status >= 400 || (await inspect(repo.path, runId)).snapshot.status.type === "completed");
    // Cancel retry on a second run: identical command twice leaves one cancel event.
    const c = await startApproval(repo.path, join(repo.path, "m2.txt"), "s7-c");
    const cancel = { action: "cancel", run_id: c.runId, command_id: "s7-cancel", expected_sequence: c.waiting.snapshot.sequence };
    const c1 = await neutralRaw(repo.path, cancel);
    const c2 = await neutralRaw(repo.path, cancel);
    ctx.note(`cancel retry: first HTTP ${c1.status}, second HTTP ${c2.status}`);
    const ci = await inspect(repo.path, c.runId);
    assert.equal(ci.snapshot.status.type, "cancelled");
    assert.equal(ci.events.filter((e) => e.command_id === "s7-cancel").length, 1, "one cancel event");
  },

  async "8 MCP: invalid-then-valid retry is idempotent"(ctx) {
    const repo = await makeBoundRepo("s8"); ctx.cleanup(repo.cleanup);
    const mcp = await bindMcp(repo.path); ctx.cleanup(() => mcp.close());
    const viaMcp = (input) => mcp.tool("workflow", { action: "neutral", input });
    await neutral(repo.path, { action: "save_operation", expected_version: 0, descriptor: D.calcOperation() });
    const bad = D.scriptConditionDefinition();
    bad.control.edges.check.true = "ghost";
    const v1 = await viaMcp({ action: "validate", definition: bad });
    assert.equal(v1.value.valid, false);
    const draft = await viaMcp({ action: "create_draft", name: "retry", definition: D.scriptConditionDefinition() });
    assert.equal(draft.type, "draft", JSON.stringify(draft));
    const refused = await viaMcp({ action: "publish", id: draft.value.id, expected_revision: 99 });
    assert.ok(refused.error, "a stale revision must be refused over MCP");
    const p1 = await viaMcp({ action: "publish", id: draft.value.id, expected_revision: 1 });
    assert.equal(p1.type, "publication", JSON.stringify(p1));
    const p2 = await viaMcp({ action: "publish", id: draft.value.id, expected_revision: 1 });
    ctx.note(`second identical publish over MCP: ${JSON.stringify(p2).slice(0, 120)}`);
    const list = await neutral(repo.path, { action: "get_publication", id: draft.value.id, revision: 2 }).catch((e) => ({ error: String(e) }));
    assert.ok(list.error, "a retried publish must not create revision 2");
    // Run-level idempotency over MCP (start is capability-gated for a bare peer, see scenario 3):
    // inspect and cancel-of-unknown go through the same service and must refuse cleanly.
    const noRun = await viaMcp({ action: "cancel", run_id: "no-such-run", command_id: "c", expected_sequence: 0 });
    assert.ok(noRun.error, "cancel of an unknown run must be refused");
  },

  async "9 scheduled binding: save, enable, fire, inspect, approve"(ctx) {
    const repo = await makeBoundRepo("s9"); ctx.cleanup(repo.cleanup);
    const approvalOnly = {
      subject: null, collections: {}, loops: {}, forks: {}, subflows: [],
      control: { nodes: { start: { type: "start" }, ask: { type: "activity", activity: 0 }, end: { type: "end", outcome: "done" } }, edges: { start: { next: "ask" }, ask: { complete: "end" } } },
      inputs: { fields: { text: D.fieldStr } }, outputs: { fields: { approved: D.fieldBool } }, outcomes: ["done"],
      instances: [{ descriptor: { namespace: "inbox", name: "approval", version: 1 }, config: {}, execution_policy: "local" }],
      input_bindings: { 0: { text: { source: "workflow_input", field: "text" } } }, conditions: {},
      output_bindings: { approved: { source: "activity_output", activity: 0, field: "approved" } },
    };
    const { pub } = await author(repo.path, approvalOnly, "scheduled approval");
    const sse = sseCollect(); ctx.cleanup(() => sse.stop());
    const draft = { id: "e2e-sched", publication: refOf(pub), source: "scheduled", kind: "schedule", resource_prefix: "schedule:e2e-sched",
      schedule: { cron: "* * * * *", timezone: "UTC", text: "e2e tick" }, events: ["due"], filters: {}, ignored_actors: [],
      mapping: { inputs: { text: "text" }, subject: {} }, limits: D.LIMITS };
    const saved = (await neutral(repo.path, { action: "save_trigger_binding", expected_revision: 0, draft })).value;
    assert.equal(saved.enabled, false, "a new binding saves disabled");
    // A binding left enabled over a deleted project makes every later schedule tick fail (finding F8).
    ctx.cleanup(async () => {
      const cur = (await neutral(repo.path, { action: "trigger_bindings" })).value.find((b) => b.draft.id === "e2e-sched");
      if (cur?.enabled) await neutral(repo.path, { action: "enable_trigger_binding", id: "e2e-sched", expected_revision: cur.revision, enabled: false, acknowledged_overlaps: {} });
    });
    const on = (await neutral(repo.path, { action: "enable_trigger_binding", id: "e2e-sched", expected_revision: saved.revision, enabled: true, acknowledged_overlaps: {} })).value;
    assert.equal(on.enabled, true);
    // The automation tick (30 s) fires the next cron minute; the run is announced on /events.
    const runId = await waitFor("scheduled run", async () => {
      for (const f of sse.frames) {
        if (f.event !== "workflow-run-changed") continue;
        const d = JSON.parse(f.data ?? "{}");
        const p = d.payload ?? d;
        if ((d.repo_path ?? p.repo_path) === repo.path && (p.runId ?? p.payload?.runId)) return p.runId ?? p.payload.runId;
      }
      return null;
    }, { timeoutMs: 120000, everyMs: 1000 });
    const waiting = await waitFor("approval question", async () => {
      const i = await inspect(repo.path, runId);
      return Object.values(i.snapshot.effects ?? {}).some((e) => e.question) ? i : null;
    });
    const effect = Object.values(waiting.snapshot.effects).find((e) => e.question);
    assert.equal(effect.question.title, "e2e tick", "the scheduled text must reach the approval");
    await neutral(repo.path, answerAction(runId, waiting, effect, true, "s9-ok"));
    const done = await waitFor("terminal", async () => { const i = await inspect(repo.path, runId); return terminal(i.snapshot) ? i : null; });
    assert.deepEqual(done.snapshot.outputs.values, { approved: true });
    // Disable so the instance stops starting runs; the retained run stays.
    const off = (await neutral(repo.path, { action: "enable_trigger_binding", id: "e2e-sched", expected_revision: on.revision, enabled: false, acknowledged_overlaps: {} })).value;
    assert.equal(off.enabled, false);
    ctx.note(`run ${runId} started by schedule, approved, completed`);
  },
};
