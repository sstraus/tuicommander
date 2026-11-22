// Scenarios 10-13: automated development and testing pipelines built only from
// neutral controls and saved script operations, driven through HTTP and MCP.
import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import assert from "node:assert/strict";
import { makeBoundRepo, bindMcp, neutral, neutralRaw, waitFor } from "./lib.mjs";
import * as D from "./definitions.mjs";

const refOf = (pub) => ({ project: pub.project, id: pub.id, revision: pub.revision });
const startReq = (pub, request_id, inputs) => ({ publication: refOf(pub), request_id, inputs, subject: null, limits: D.LIMITS });
const inspect = async (project, runId) => (await neutral(project, { action: "inspect", run_id: runId, after_sequence: 0, limit: 500 })).value;
const runToEnd = async (project, pub, requestId, inputs) => {
  const runId = (await neutral(project, { action: "start", request: startReq(pub, requestId, inputs) })).value.snapshot.run.id;
  return waitFor(`terminal ${requestId}`, async () => {
    const i = await inspect(project, runId);
    return i.snapshot.status.type !== "running" ? i : null;
  }, { timeoutMs: 60000 });
};
const save = (project, descriptor) => neutral(project, { action: "save_operation", expected_version: 0, descriptor });
async function author(project, def, name) {
  const v = (await neutral(project, { action: "validate", definition: def })).value;
  assert.equal(v.valid, true, `validate ${name}: ${JSON.stringify(v.diagnostics)}`);
  const draft = (await neutral(project, { action: "create_draft", name, definition: def })).value;
  return (await neutral(project, { action: "publish", id: draft.id, expected_revision: draft.revision })).value;
}
const git = (repo, ...a) => execFileSync("git", a, { cwd: repo, stdio: "pipe" }).toString().trim();
const commits = (repo) => Number(git(repo, "rev-list", "--count", "HEAD"));

// A tiny project under test: calc.add plus a unittest suite.
function seedProject(repo, { broken = false } = {}) {
  writeFileSync(join(repo, "calc.py"), broken ? "def add(a, b):\n    return a - b\n" : "def add(a, b):\n    return a + b\n");
  writeFileSync(join(repo, "test_calc.py"),
    "import unittest\nfrom calc import add\nclass T(unittest.TestCase):\n    def test_add(self):\n        self.assertEqual(add(2, 3), 5)\nif __name__ == '__main__':\n    unittest.main()\n");
}

// Script bodies run with `q` (request) in scope; `repo` comes from the workflow input.
// Python runs with -B: the scenarios rewrite calc.py between `a - b` and `a + b`, which
// have the same size, so a same-second rewrite would otherwise reuse the stale .pyc.
const RUN = "import subprocess\nr=q['inputs']['repo']\nrun=lambda *a: subprocess.run(list(a),cwd=r,capture_output=True,text=True)";
const lintOp = () => D.scriptOperation("lint", { repo: D.fieldStr }, { passed: D.fieldBool },
  `${RUN}\np=run('python3','-B','-c','import sys\\n[compile(open(f).read(),f,"exec") for f in sys.argv[1:]]','calc.py','test_calc.py')\ndata=dict(passed=p.returncode==0)`);
const testOp = () => D.scriptOperation("unittest", { repo: D.fieldStr }, { passed: D.fieldBool },
  `${RUN}\np=run('python3','-B','-m','unittest','-q')\ndata=dict(passed=p.returncode==0)`);
const commitOp = () => D.scriptOperation("commit", { repo: D.fieldStr, message: D.fieldStr }, { sha: D.fieldStr },
  `${RUN}\nrun('git','add','-A')\nrun('git','commit','-q','-m',q['inputs']['message'])\ndata=dict(sha=run('git','rev-parse','HEAD').stdout.strip())`);

/** start -> fork{lint, unittest} -> lint_ok -> test_ok -> commit -> committed; any failure -> rejected. */
function gatedCommitDefinition() {
  const passed = (activity) => ({ operator: "compare", comparison: "equal",
    left: { source: "activity_output", activity, field: "passed" }, right: { source: "constant", value: true } });
  return {
    subject: null, collections: {}, loops: {}, subflows: [],
    forks: { checks: { branches: { lint: { activity: 0, required: true }, test: { activity: 1, required: true } }, max_concurrency: 2 } },
    control: {
      nodes: {
        start: { type: "start" }, checks: { type: "fork", scope: "checks" },
        lint_ok: { type: "condition", condition: "lint_ok" }, test_ok: { type: "condition", condition: "test_ok" },
        commit: { type: "activity", activity: 2 },
        committed: { type: "end", outcome: "committed" }, rejected: { type: "end", outcome: "rejected" },
      },
      edges: {
        start: { next: "checks" },
        checks: { complete: "lint_ok", blocked: "rejected" },
        lint_ok: { true: "test_ok", false: "rejected", unknown: "rejected" },
        test_ok: { true: "commit", false: "rejected", unknown: "rejected" },
        commit: { ok: "committed" },
      },
    },
    inputs: { fields: { repo: D.fieldStr, message: D.fieldStr } },
    outputs: { fields: { tests_passed: D.fieldBool } },
    outcomes: ["committed", "rejected"],
    instances: ["lint", "unittest", "commit"].map((n) => ({ descriptor: D.ids(n), config: {}, execution_policy: "local" })),
    input_bindings: {
      0: { repo: { source: "workflow_input", field: "repo" } },
      1: { repo: { source: "workflow_input", field: "repo" } },
      2: { repo: { source: "workflow_input", field: "repo" }, message: { source: "workflow_input", field: "message" } },
    },
    conditions: { lint_ok: passed(0), test_ok: passed(1) },
    output_bindings: { tests_passed: { source: "activity_output", activity: 1, field: "passed" } },
  };
}

/** Bounded correction loop: each traversal applies one simulated fix attempt and reruns the suite. */
// Carried loop fields are also body inputs: `attempts` flows from one traversal to the next.
// `.attempts` on disk independently counts real body executions.
const fixAttemptOp = (fixOnAttempt) => D.scriptOperation("fix_attempt",
  { repo: D.fieldStr, attempts: D.fieldInt, repeat: { value_type: "boolean", required: false, default: null } },
  { repeat: D.fieldBool, attempts: D.fieldInt },
  `${RUN}\nimport os\nn=q['inputs']['attempts']+1\nc=os.path.join(r,'.attempts')\nopen(c,'a').write('x')\n` +
  `if n>=${fixOnAttempt}: open(os.path.join(r,'calc.py'),'w').write('def add(a, b):\\n    return a + b\\n')\n` +
  "p=run('python3','-B','-m','unittest','-q')\ndata=dict(repeat=p.returncode!=0,attempts=n)");
function correctionLoopDefinition(maxIterations) {
  return {
    subject: null, collections: {}, forks: {}, subflows: [],
    loops: { fix: {
      body: 0, max_iterations: maxIterations,
      carried: { fields: { repeat: { value_type: "boolean", required: false, default: null }, attempts: D.fieldInt } },
      initial: { attempts: { source: "constant", value: 0 } },
      result: { repeat: { source: "activity_output", activity: 0, field: "repeat" }, attempts: { source: "activity_output", activity: 0, field: "attempts" } },
    } },
    control: {
      nodes: {
        start: { type: "start" }, fix: { type: "loop", scope: "fix", condition: "red" },
        green: { type: "end", outcome: "green" }, operator: { type: "end", outcome: "operator" }, failed: { type: "end", outcome: "failed" },
      },
      edges: { start: { next: "fix" }, fix: { complete: "green", exhausted: "operator", unknown: "operator", failed: "failed" } },
    },
    inputs: { fields: { repo: D.fieldStr } },
    outputs: { fields: { attempts: D.fieldInt } },
    outcomes: ["green", "operator", "failed"],
    instances: [{ descriptor: D.ids("fix_attempt"), config: {}, execution_policy: "local" }],
    input_bindings: { 0: { repo: { source: "workflow_input", field: "repo" } } },
    conditions: { red: { operator: "compare", comparison: "equal",
      left: { source: "scope_output", scope: "fix", field: "repeat" }, right: { source: "constant", value: true } } },
    output_bindings: { attempts: { source: "scope_output", scope: "fix", field: "attempts" } },
  };
}

/** Per-module suite: collection over module names, bounded concurrency, failures collected in source order. */
const moduleTestOp = () => D.scriptOperation("module_test", { repo: D.fieldStr, module: D.fieldStr, index: D.fieldInt }, { passed: D.fieldBool, module: D.fieldStr },
  `${RUN}\nimport time\nm=q['inputs']['module']\ntime.sleep(0.3 if q['inputs']['index']==0 else 0)\np=run('python3','-B','-m','unittest','-q',m)\ndata=dict(passed=p.returncode==0,module=m)`);
function moduleSuiteDefinition(expectedPassing) {
  return {
    subject: null, loops: {}, forks: {}, subflows: [],
    collections: { suite: { collection: { source: "workflow_input", field: "modules" }, item_type: "string", body: 0,
      item_input: "module", index_input: "index", max_items: 8, max_concurrency: 3, empty: "complete", failure: "collect" } },
    control: {
      nodes: { start: { type: "start" }, suite: { type: "collection", scope: "suite" }, all: { type: "condition", condition: "all" },
        green: { type: "end", outcome: "green" }, red: { type: "end", outcome: "red" } },
      edges: { start: { next: "suite" }, suite: { complete: "all", blocked: "red" }, all: { true: "green", false: "red", unknown: "red" } },
    },
    inputs: { fields: { repo: D.fieldStr, modules: { value_type: "array", required: true, default: null } } },
    outputs: { fields: { items: { value_type: "array", required: true, default: null } } },
    outcomes: ["green", "red"],
    instances: [{ descriptor: D.ids("module_test"), config: {}, execution_policy: "local" }],
    input_bindings: { 0: { repo: { source: "workflow_input", field: "repo" } } },
    conditions: { all: { operator: "compare", comparison: "equal",
      left: { source: "scope_output", scope: "suite", field: "completed" }, right: { source: "constant", value: expectedPassing } } },
    output_bindings: { items: { source: "scope_output", scope: "suite", field: "items" } },
  };
}

/** A published test suite: start -> unittest -> end, returning tests_passed. */
function suiteDefinition() {
  return {
    subject: null, collections: {}, loops: {}, forks: {}, conditions: {}, subflows: [],
    control: {
      nodes: { start: { type: "start" }, test: { type: "activity", activity: 0 }, end: { type: "end", outcome: "completed" } },
      edges: { start: { next: "test" }, test: { ok: "end" } },
    },
    inputs: { fields: { repo: D.fieldStr } }, outputs: { fields: { tests_passed: D.fieldBool } }, outcomes: ["completed"],
    instances: [{ descriptor: D.ids("unittest"), config: {}, execution_policy: "local" }],
    input_bindings: { 0: { repo: { source: "workflow_input", field: "repo" } } },
    output_bindings: { tests_passed: { source: "activity_output", activity: 0, field: "passed" } },
  };
}

/** A parent that reuses the published suite as one subflow activity. */
function parentDefinition(subflowId) {
  return {
    subject: null, collections: {}, loops: {}, forks: {}, conditions: {}, subflows: [],
    control: {
      nodes: { start: { type: "start" }, suite: { type: "activity", activity: 0 }, end: { type: "end", outcome: "checked" } },
      edges: { start: { next: "suite" }, suite: { completed: "end" } },
    },
    inputs: { fields: { repo: D.fieldStr } }, outputs: { fields: { green: D.fieldBool } }, outcomes: ["checked"],
    instances: [{ descriptor: subflowId, config: {}, execution_policy: "local" }],
    input_bindings: { 0: { repo: { source: "workflow_input", field: "repo" } } },
    output_bindings: { green: { source: "activity_output", activity: 0, field: "tests_passed" } },
  };
}

export const devScenarios = {
  async "10 dev pipeline: parallel lint+tests gate an automatic commit"(ctx) {
    const repo = await makeBoundRepo("s10"); ctx.cleanup(repo.cleanup);
    for (const op of [lintOp(), testOp(), commitOp()]) await save(repo.path, op);
    const pub = await author(repo.path, gatedCommitDefinition(), "gated commit");
    seedProject(repo.path);
    const base = commits(repo.path);
    const ok = await runToEnd(repo.path, pub, "s10-green", { repo: repo.path, message: "feat: add calc" });
    assert.deepEqual(ok.snapshot.status, { type: "completed", outcome: "committed" }, JSON.stringify(ok.snapshot.status));
    assert.equal(commits(repo.path), base + 1, "green checks must produce exactly one commit");
    assert.equal(git(repo.path, "log", "-1", "--format=%s"), "feat: add calc");
    // Red suite: the fork completes, the gate rejects, nothing is committed.
    seedProject(repo.path, { broken: true });
    const red = await runToEnd(repo.path, pub, "s10-red", { repo: repo.path, message: "broken" });
    assert.deepEqual(red.snapshot.status, { type: "completed", outcome: "rejected" });
    assert.deepEqual(red.snapshot.outputs.values, { tests_passed: false });
    assert.equal(commits(repo.path), base + 1, "a red suite must not commit");
    assert.ok(git(repo.path, "status", "--porcelain").includes("calc.py"), "the broken change stays uncommitted");
  },

  async "11 test-fix correction loop: bounded, green on attempt 2, exhaustion goes to operator"(ctx) {
    const repo = await makeBoundRepo("s11"); ctx.cleanup(repo.cleanup);
    seedProject(repo.path, { broken: true });
    await save(repo.path, fixAttemptOp(2));
    const pub = await author(repo.path, correctionLoopDefinition(3), "fix loop");
    const green = await runToEnd(repo.path, pub, "s11-green", { repo: repo.path });
    assert.deepEqual(green.snapshot.status, { type: "completed", outcome: "green" }, JSON.stringify(green.snapshot.status));
    assert.deepEqual(green.snapshot.outputs.values, { attempts: 2 }, "the loop stops on the first green traversal");
    // A fixer that never succeeds: the bound is honoured and the run never reports success.
    const repo2 = await makeBoundRepo("s11b"); ctx.cleanup(repo2.cleanup);
    seedProject(repo2.path, { broken: true });
    await save(repo2.path, fixAttemptOp(99));
    const pub2 = await author(repo2.path, correctionLoopDefinition(3), "fix loop");
    const ex = await runToEnd(repo2.path, pub2, "s11-ex", { repo: repo2.path });
    assert.deepEqual(ex.snapshot.status, { type: "completed", outcome: "operator" }, JSON.stringify(ex.snapshot.status));
    const attempts = readFileSync(join(repo2.path, ".attempts"), "utf8").length;
    ctx.note(`max_iterations=3 executed ${attempts} body traversals before exhaustion`);
    assert.equal(attempts, 3, "exactly max_iterations body traversals before exhaustion");
    assert.equal(readFileSync(join(repo.path, ".attempts"), "utf8").length, 2, "the green run stops after 2 traversals");
    // max_iterations 0 is not a usable bound and must be refused at validation.
    const zero = (await neutral(repo2.path, { action: "validate", definition: correctionLoopDefinition(0) })).value;
    assert.equal(zero.valid, false, "a zero iteration bound must not validate");
    ctx.note(`max_iterations=0 diagnosis: ${JSON.stringify(zero.diagnostics[0])}`);
    // Type drift between carried state and the body input is caught before publication.
    const drift = correctionLoopDefinition(3); drift.loops.fix.carried.fields.attempts = D.fieldStr;
    assert.equal((await neutral(repo2.path, { action: "validate", definition: drift })).value.valid, false);
  },

  async "12 per-module test collection: ordered results, partial failure collected"(ctx) {
    const repo = await makeBoundRepo("s12"); ctx.cleanup(repo.cleanup);
    seedProject(repo.path);
    writeFileSync(join(repo.path, "test_slow.py"), "import unittest\nclass S(unittest.TestCase):\n    def test_ok(self):\n        self.assertTrue(True)\n");
    writeFileSync(join(repo.path, "test_bad.py"), "import unittest\nclass B(unittest.TestCase):\n    def test_bad(self):\n        self.assertEqual(1, 2)\n");
    await save(repo.path, moduleTestOp());
    const modules = ["test_slow", "test_calc", "test_bad"];
    const pub = await author(repo.path, moduleSuiteDefinition(3), "module suite");
    const res = await runToEnd(repo.path, pub, "s12", { repo: repo.path, modules });
    ctx.note(`status=${JSON.stringify(res.snapshot.status)}`);
    const items = res.snapshot.outputs?.values?.items ?? [];
    assert.deepEqual(items.map((i) => i.outputs?.module), modules, "aggregate keeps source order despite completion order");
    assert.deepEqual(items.map((i) => i.outputs?.passed), [true, true, false], "a failing module is reported, not hidden");
    assert.deepEqual(res.snapshot.status, { type: "completed", outcome: "green" }, "all three items completed their activity");
    // Bound: more modules than max_items must not silently truncate.
    const over = await runToEnd(repo.path, pub, "s12-over", { repo: repo.path, modules: Array.from({ length: 9 }, () => "test_calc") });
    ctx.note(`9 items over max_items=8: ${JSON.stringify(over.snapshot.status)}`);
    assert.notDeepEqual(over.snapshot.status, { type: "completed", outcome: "green" }, "exceeding max_items must not pass");
    // Empty collection follows the declared empty policy (complete), then the count condition decides.
    const empty = await runToEnd(repo.path, pub, "s12-empty", { repo: repo.path, modules: [] });
    assert.deepEqual(empty.snapshot.status, { type: "completed", outcome: "red" }, "0 completed != 3 -> red");
  },

  async "13 MCP authors and inspects the dev pipeline end to end"(ctx) {
    const repo = await makeBoundRepo("s13"); ctx.cleanup(repo.cleanup);
    const mcp = await bindMcp(repo.path); ctx.cleanup(() => mcp.close());
    const viaMcp = (input) => mcp.tool("workflow", { action: "neutral", input });
    for (const op of [lintOp(), testOp(), commitOp()]) {
      const r = await viaMcp({ action: "save_operation", expected_version: 0, descriptor: op });
      if (r.error) { ctx.finding(`MCP save_operation refused: ${r.error}`); await save(repo.path, op); }
    }
    const cat = await viaMcp({ action: "catalog" });
    const names = JSON.stringify(cat);
    for (const n of ["lint", "unittest", "commit"]) assert.ok(names.includes(`"${n}"`), `catalog over MCP must list saved operation ${n}`);
    // A broken edit surfaces a diagnosis an agent can act on.
    const bad = gatedCommitDefinition(); delete bad.control.edges.checks.blocked;
    const v = await viaMcp({ action: "validate", definition: bad });
    assert.equal(v.value.valid, false);
    ctx.note(`missing fork port diagnosis: ${JSON.stringify(v.value.diagnostics[0])}`);
    assert.ok(JSON.stringify(v.value.diagnostics).includes("checks"), "the diagnosis must name the offending node");
    const draft = await viaMcp({ action: "create_draft", name: "gated commit (mcp)", definition: gatedCommitDefinition() });
    const pub = await viaMcp({ action: "publish", id: draft.value.id, expected_revision: draft.value.revision });
    assert.equal(pub.type, "publication", JSON.stringify(pub));
    seedProject(repo.path);
    const base = commits(repo.path);
    const done = await runToEnd(repo.path, pub.value, "s13", { repo: repo.path, message: "chore: mcp pipeline" });
    assert.deepEqual(done.snapshot.status, { type: "completed", outcome: "committed" });
    const viaM = await viaMcp({ action: "inspect", run_id: done.snapshot.run.id, after_sequence: 0, limit: 500 });
    assert.deepEqual(viaM.value.snapshot.status, done.snapshot.status, "MCP inspect sees the same terminal state");
    assert.equal(commits(repo.path), base + 1);
    const runs = await neutralRaw(repo.path, { action: "list_runs" });
    ctx.note(`list_runs over HTTP: ${runs.status} ${JSON.stringify(runs.body).slice(0, 100)}`);
  },
  async "14 published workflow reused as a subflow: registered by the backend, pinned, run by a parent"(ctx) {
    const repo = await makeBoundRepo("s14"); ctx.cleanup(repo.cleanup);
    const mcp = await bindMcp(repo.path); ctx.cleanup(() => mcp.close());
    const viaMcp = (input) => mcp.tool("workflow", { action: "neutral", input });
    await save(repo.path, testOp());
    const child = await author(repo.path, suiteDefinition(), "test suite");
    const ref = { project: child.project, id: child.id, revision: child.revision };
    const id = { namespace: "project", name: "test-suite", version: 1 };
    // Registered over MCP, naming only the publication: the backend pins its digest and interface.
    const op = await viaMcp({ action: "save_subflow", expected_version: 0, id, publication: ref });
    assert.equal(op.type, "operation", JSON.stringify(op));
    const target = op.value.target;
    assert.equal(target.type, "subflow");
    assert.deepEqual(target.publication.reference, ref);
    assert.match(target.publication.sha256, /^[0-9a-f]{64}$/, "the backend computes the content pin");
    assert.deepEqual(op.value.outputs, suiteDefinition().outputs, "the interface is the child's");
    // Re-registering the same id at the same expected version is refused, not overwritten.
    const again = await neutralRaw(repo.path, { action: "save_subflow", expected_version: 0, id, publication: ref });
    assert.ok(again.status >= 400, `duplicate registration must be refused: ${again.status}`);
    // A publication reference into another project is refused.
    const foreign = await neutralRaw(repo.path, { action: "save_subflow", expected_version: 1, id: { ...id, version: 2 }, publication: { ...ref, project: "/elsewhere" } });
    assert.ok(foreign.status >= 400, `foreign publication must be refused: ${foreign.status}`);
    const parent = await author(repo.path, parentDefinition(id), "suite gate");
    seedProject(repo.path);
    const green = await runToEnd(repo.path, parent, "s14-green", { repo: repo.path });
    assert.deepEqual(green.snapshot.status, { type: "completed", outcome: "checked" }, JSON.stringify(green.snapshot.status));
    assert.deepEqual(green.snapshot.outputs.values, { green: true }, "the child's return reaches the parent");
    seedProject(repo.path, { broken: true });
    const red = await runToEnd(repo.path, parent, "s14-red", { repo: repo.path });
    assert.deepEqual(red.snapshot.outputs.values, { green: false }, "a red child suite is reported by the parent");
  },
};
