// Neutral workflow definitions used by the scenarios. Built from the wire
// contract (docs/backend/workflows.md "Neutral workflow transport").
const field = (value_type, required = true) => ({ value_type, required, default: null });
const POLICY = {
  version: 1, timeout_secs: 10, input_bytes: 65536, stdout_bytes: 16384, stderr_bytes: 16384,
  artifact_limit: 4, resources: [], environment_refs: {}, secret_refs: {}, mutation: false,
  retry: "never", reconciliation: { type: "read_only" }, output: { type: "structured" },
};

/** A saved script operation: `body` is Python run with `q` (the request) in scope and must set `data`. */
export function scriptOperation(name, inputs, outputs, body) {
  const script = `import json,sys\nq=json.load(sys.stdin)\n${body}\nprint(json.dumps(dict(version=1,identity=q['identity'],outcome='ok',data=data,artifacts={})))`;
  return {
    id: { namespace: "e2e", name, version: 1 },
    purpose: `E2E script ${name}`,
    handler: { name: "host.script", version: 1 },
    target: { type: "script", invocation: { type: "shell", program: "python3", script, version: 1 } },
    config: { fields: { script: { value_type: "object", required: true, default: POLICY } } },
    inputs: { fields: inputs }, subject: null,
    outputs: { fields: outputs }, outcomes: ["ok"],
    required_capabilities: [], execution_policies: ["local"],
  };
}
export const ids = (name) => ({ namespace: "e2e", name, version: 1 });
export const fieldInt = field("integer");
export const fieldStr = field("string");
export const fieldBool = field("boolean");

const base = (extra) => ({
  subject: null, collections: {}, loops: {}, forks: {}, subflows: [], ...extra,
});

/** Script, condition, end. Completes `done` when value+1 == 8, else `blocked`. */
export function scriptConditionDefinition() {
  return base({
    control: {
      nodes: {
        start: { type: "start" },
        calc: { type: "activity", activity: 0 },
        check: { type: "condition", condition: "is_eight" },
        done: { type: "end", outcome: "done" },
        blocked: { type: "end", outcome: "blocked" },
      },
      edges: {
        start: { next: "calc" },
        calc: { ok: "check" },
        check: { true: "done", false: "blocked", unknown: "blocked" },
      },
    },
    inputs: { fields: { value: fieldInt } },
    outputs: { fields: { next: fieldInt } },
    outcomes: ["done", "blocked"],
    instances: [{ descriptor: ids("calc"), config: {}, execution_policy: "local" }],
    input_bindings: { 0: { value: { source: "workflow_input", field: "value" } } },
    conditions: {
      is_eight: {
        operator: "compare", comparison: "equal",
        left: { source: "activity_output", activity: 0, field: "next" },
        right: { source: "constant", value: 8 },
      },
    },
    output_bindings: { next: { source: "activity_output", activity: 0, field: "next" } },
  });
}

/** Approval, then one script effect that appends `tag` to the marker file. */
export function approvalDefinition() {
  return base({
    control: {
      nodes: {
        start: { type: "start" },
        ask: { type: "activity", activity: 0 },
        check: { type: "condition", condition: "approved" },
        write: { type: "activity", activity: 1 },
        done: { type: "end", outcome: "done" },
        denied: { type: "end", outcome: "denied" },
      },
      edges: {
        start: { next: "ask" },
        ask: { complete: "check" },
        check: { true: "write", false: "denied", unknown: "denied" },
        write: { ok: "done" },
      },
    },
    inputs: { fields: { text: fieldStr, marker: fieldStr } },
    outputs: { fields: { approved: fieldBool } },
    outcomes: ["done", "denied"],
    instances: [
      { descriptor: { namespace: "inbox", name: "approval", version: 1 }, config: {}, execution_policy: "local" },
      { descriptor: ids("append"), config: {}, execution_policy: "local" },
    ],
    input_bindings: {
      0: { text: { source: "workflow_input", field: "text" } },
      1: { marker: { source: "workflow_input", field: "marker" } },
    },
    conditions: {
      approved: {
        operator: "compare", comparison: "equal",
        left: { source: "activity_output", activity: 0, field: "approved" },
        right: { source: "constant", value: true },
      },
    },
    output_bindings: { approved: { source: "activity_output", activity: 0, field: "approved" } },
  });
}

export const appendOperation = (tag) =>
  scriptOperation("append", { marker: fieldStr }, { ok: fieldBool },
    `open(q['inputs']['marker'],'a').write('${tag}\\n')\ndata=dict(ok=True)`);
export const calcOperation = () =>
  scriptOperation("calc", { value: fieldInt }, { next: fieldInt }, "data=dict(next=q['inputs']['value']+1)");

export const LIMITS = { max_activations: 20, max_concurrency: 1, max_duration_ms: 120000 };
