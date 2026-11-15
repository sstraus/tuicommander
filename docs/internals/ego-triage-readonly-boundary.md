# Ego issue triage: read-only execution boundary

Story: 2450-6845. Status: implemented.

## Contract and executor selection

An admitted issue triage definition can select:

- `triage_executor: { "kind": "managed" }` (the default): its `run_config`
  names an external managed executor. Its absence of write tools and push
  credentials is **operator-attested**, not enforced by TUIC.
- `triage_executor: { "kind": "ego", "provider_env": "OPENROUTER_API_KEY" }`:
  TUIC enforces a data-only ACP launch with a cleared initial child environment.
  Ego enforces the exclusive read-only policy. `provider_env` is optional and
  accepts only `OPENROUTER_API_KEY`, `OPENAI_API_KEY`, `ANTHROPIC_API_KEY` or
  `GEMINI_API_KEY`. Only the selected provider variable is forwarded.

The ego executor uses the configured `ego_executable`, not `run_config` or the
AI Chat profile. Profiles are incompatible with ego's exclusive policy. Keep
`run_config` nonblank for the stored definition schema; it is ignored by ego
triage. `read_only_attested` can be false for this executor. Only an admitted
triage run may use it; manual, scheduled and implementation launches cannot.
Native workflow import refuses this executor rather than silently replacing its
read-only guarantee with a different workflow.

## Launch and authority

The exact process arguments are:

```text
ego acp -C <workspace> --exclusive-read-only [--env-allow <provider_env>]
```

TUIC resolves the configured executable and uses `Command::env_clear()` before
spawn. It constructs the child environment from platform values plus, at most,
the selected provider variable. Platform values are `PATH`, `HOME`, `EGO_HOME`,
`TMPDIR`, `TMP`, `TEMP`, `LANG`, `TZ`, `LC_*`, certificate paths
(`SSL_CERT_FILE`, `SSL_CERT_DIR`), Windows user/system directories and macOS
`__CF_USER_TEXT_ENCODING`. Windows names compare without case. Proxy variables
are not forwarded. No GitHub token, SSH agent socket, git credential-helper
setting or TUIC peer/control-plane identity is forwarded. An absent provider
variable is not replaced with another provider's credential. Ego may use its
own configured provider authentication; TUIC does not read a credential store.

The host's dedicated ACP driver sends `initialize` (protocol version 1), then
`session/new` with `cwd=<workspace>` and `mcpServers=[]`, then one
`session/prompt` containing the acceptance policy and issue title/body. Before
launch, the host fetches that issue using its own GitHub API, checks the workspace
remote against the admitted repository and compares the requirements digest. A
changed issue or missing admission digest fails closed, before ego is spawned.
It refuses every agent-to-host request, including filesystem, terminal and
permission requests. It does not register a terminal, managed peer or host MCP
server. Total protocol output is bounded to 4 MiB.

The generic ACP SDK launcher is deliberately not used for this executor: SDK
2.0.0 adds environment overrides but inherits the parent environment. An ego
restart that clears inherited variables is too late to protect the initial
child. Other ACP callers keep their existing behavior.

Ego story 950-32d4 (landed in `a6b47016e`) supplies the immutable
`--exclusive-read-only` policy: no native/MCP tools, hooks, skills or prompt
commands; incompatible profile/tool settings fail closed. Its ACP session
rejects nonempty `mcpServers`. Source: ego
`docs/internals/950-acp-exclusive-readonly-design.md` and
`crates/ego-cli/src/exclusive_env.rs`.

## Decision and lifecycle

The evaluator returns only JSON with `outcome` (`accepted`, `rejected` or
`needs_input`) and a nonempty `reason` of at most 4000 bytes. TUIC accepts only
an ACP `end_turn`, validates the JSON, and records the decision internally for
the host-owned run. The evaluator receives no decision-reporting MCP tool and
no GitHub publication credentials. The existing host publisher alone can add a
comment or label, using the existing issue digest and publication ledger.

The run's saved definition pins the executor and provider selection. Dispatch
rechecks the policy and definition; cancellation and policy withdrawal stop
the owned child. The definition's maximum duration covers ACP startup and the
turn. Timeout is recorded as `timed_out`. Restart recovery never retries an
interrupted evaluator. Publication also checks the current executor selection.
The external managed executor and its caller-bound reporting path are unchanged.

## Evidence and limits

The single launch regression uses the existing local ACP fixture executable.
It re-executes only its own test to seed fake `GH_TOKEN`, `SSH_AUTH_SOCK`,
`GIT_CONFIG_COUNT`, `TUIC_SESSION` and an unselected provider key in the actual
parent environment, without racing other tests through environment mutation.
The fixture requires the exact exclusive-policy/provider flags and empty MCP
session, checks that forbidden values are absent and the selected fake provider
value is present, and returns a structured decision. It makes no model call or
GitHub write. Ego's own policy tests cover tool, hook and push refusal; TUIC does
not duplicate those tests.

The trusted ego executable, provider transport and the model's decision quality
remain assumptions. This policy is not operating-system isolation of the trusted
ego executable. No credential store or keychain was read during implementation.
