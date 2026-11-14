# Internal structured script activity

The staged neutral adapter is `run/neutral_script.rs`. A trusted host registers
`host.script@1` as a reusable Script handler with its own current admission
adapter. Registration describes availability; it grants no execution rights.
The installed native provider registers that handler and dispatches saved script
targets through the same owned-process adapter. A production `script_descriptor`
constructor and the protocol fixtures share policy/target construction. Caller
binding requires an authenticated owner or an agent with captured and current
Operator authority. Public starts bind that backend caller before dispatch. Existing Designer
forms are not changed by this adapter.

A custom activity uses an ordinary versioned `ActivityDescriptor`, with config,
input, optional subject and output schemas, named outcomes and supported
execution policies. `ReusableTarget::Script` pins the existing version 1 inline
interpreter/script invocation in that descriptor. Resolved inputs cannot select
another executable. This pins invocation bytes, not the installed interpreter's
bytes or an external file's future contents.

## Configuration

The descriptor's config schema declares a required object field `script`, often
with a default. Other config fields use the same generic schema. The `script`
object has these required fields; unknown fields are refused:

| Field | Contract |
| --- | --- |
| `version` | `1` |
| `timeout_secs` | 1–86400, covers owned process execution |
| `input_bytes` | 1–1048576, bounds serialized request before entry |
| `stdout_bytes`, `stderr_bytes` | 1–1048575 each; larger captures fail |
| `artifact_limit` | 0–64 named artifact output references |
| `resources` | Up to 16 resolved resource names, each at most 4096 bytes |
| `environment_refs`, `secret_refs` | Combined maximum 32 environment-name-to-daemon-reference mappings; no literal values, no overlapping destinations |
| `mutation` | Declares effect semantics, never a permission grant |
| `retry` | `"never"`; no automatic replay or inferred idempotency |
| `reconciliation` | `{"type":"read_only"}` or `{"type":"manual","procedure":"…"}`; mutations require a nonempty procedure and owned resources |
| `output` | `{"type":"structured"}` or explicit `{"type":"exit_status","outcome":"…"}` |

The neutral ledger claims resources and applies concurrency limits. There is no
second queue or scheduler. Resource limits here bound named claims and artifact
references, not OS CPU/RAM quotas or a sandbox. The process executor retains at
most 1 MiB per stream while draining it; the declared output budgets decide
whether the result can be accepted, rather than stopping a pipe at that byte.
The timeout stops the owned process tree. At most 1 MiB of serialized stdin and
2 MiB of captured stream bytes are retained before JSON parsing/redaction.

Environment references resolve immediately before entry and have nonempty
values of at most 16384 bytes. The process starts with the command executor's
existing limited daemon environment, execution PATH and explicit references.
Only resolved secret-reference values use the shared capture redactor; ordinary
environment-reference values remain unchanged in results and diagnostics. Neither
reference map places resolved values in requests.
The host supplies the working directory and private input directory; graph
inputs do not bind either path. The private stdin snapshot lives until stop and
reap and is then removed.

## Request and result

Stdin contains one UTF-8 JSON request:

```json
{
  "version": 1,
  "identity": {
    "fence": {
      "run": "run-id", "execution": "execution-id",
      "activation": 1, "attempt": 1, "scope": null
    },
    "digest": "retained-effect-digest"
  },
  "config": {},
  "inputs": {},
  "subject": null
}
```

The actual `config` includes the resolved `script` policy and descriptor defaults.
`identity` is the exact retained EffectKey, including a scoped fence when present.
The neutral owner commits preparation and dispatch before the process enters.
No script owns successor selection, retries, approval or the outer workflow.

Structured stdout contains exactly one result object:

```json
{
  "version": 1,
  "identity": {
    "fence": {
      "run": "run-id", "execution": "execution-id",
      "activation": 1, "attempt": 1, "scope": null
    },
    "digest": "retained-effect-digest"
  },
  "outcome": "ok",
  "data": {"count": 7},
  "artifacts": {
    "report": {"reference": "artifact", "id": "report-id", "revision": "r1"}
  }
}
```

Version and the full identity must match. Outcomes and output fields must be
declared. Artifact entries are named output ports with typed artifact references;
they cannot duplicate data fields. The merged fields pass the descriptor's typed
output schema before reaching NeutralExecution. JSON null, arrays and nested
objects stay JSON values. Artifact reference syntax is checked; actual artifact
existence/freshness remains the owning host service's responsibility.

Exit zero with missing, malformed, foreign, wrong-shaped or oversized structured
output is a protocol failure. Stderr remains a separate diagnostic string and is
never parsed as result data. Explicit exit-status mode synthesizes an empty-data
result for its declared outcome after exit zero; required outputs must therefore
have defaults or be absent. It still enforces bounds and process stop.

## Uncertainty and cancellation

For a read-only activity, rejected output or failed execution yields a failed
activity. For a mutation, rejected/lost output, nonzero exit, timeout or cancelled
execution yields uncertainty, with the actual stop flag recorded separately.
A pre-entry refusal can prove no process started and is a known failed activity.
A failure of owned process-tree stop is uncertain even for a read-only command.

Reconciliation only inspects retained observation/identity and returns the
original correlation plus the declared procedure. It never invokes the script.
Without original completion evidence, manual effect reconciliation is required;
no adapter automatically retries or claims rollback. Cancellation signals the
existing per-effect owned-process registry; signalling alone is not stop proof.
The original worker independently reports stop when the executor returns.
Uncertain mutations retain the neutral ledger's conflicting resource claims even
when the process is stopped. No process-local handle is used as restart proof.

The published descriptor is renderer input through the generic catalog contract.
Designer visual proof for a custom script is inherited by story 014-5d73, which
owns neutral catalog rendering. This adapter does not add a per-script form or a
kernel node kind.

Renderer handoff: the [captured custom descriptor](../../src-tauri/src/workflows/fixtures/custom_script_descriptor.json)
comes from the published backend serializer. The targeted Rust test
`published_custom_script_descriptor_matches_shared_renderer_fixture` checks
that its complete wire value still matches. Capture provenance accompanies the
fixture. Story 014 owns its rendering and visual verification.
