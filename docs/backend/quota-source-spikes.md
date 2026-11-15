# Quota source spikes

## S2: Gemini CLI 0.66.0 (2026-11-12, story 2995-6cc1)

### Result

No quota method is exposed by the installed ACP agent. The CLI has an internal,
non-generating Code Assist quota reader, but using it from TUICommander needs the
credential-access choice in decision D4. D4 is still open. No adapter, invented
quota fixture, credential read, or generating request was added.

### Live probe

The installed `gemini --acp` ran in a new workspace and a new home under the
Gits scratch directory. `HOME` and `GEMINI_CLI_HOME` pointed there; inherited
API keys and token variables were removed. No authentication or session prompt
was submitted. The process was stopped and reaped after the probe.

- `initialize` returned protocol version 1 and agent version `0.66.0`.
- Capabilities were `loadSession`, image/audio/embedded-context prompts, and
  HTTP/SSE MCP. No quota capability was advertised.
- `methods/list` returned JSON-RPC `-32601` (method not found). ACP has no
  method-list endpoint here; the source dispatch table supplies the list below.
- `quota/read` returned JSON-RPC `-32601` (method not found).

The recorded request/response transcript is retained at
`~/Gits/.tmp/batch-d-quota-spikes/gemini-acp.json`. It contains no credential.

### Installed-source check

The launcher at `/opt/homebrew/lib/node_modules/@google/gemini-cli/bundle/gemini.js`
loads `gemini-22EOGL6O.js`. Its agent dispatch table (line 12507) lists:

- `initialize`, `authenticate`
- `session/new`, `session/load`, `session/list`, `session/fork`, `session/resume`,
  `session/close`, `session/cancel`, `session/prompt`
- `session/set_mode`, `session/set_model`, `session/set_config_option`

The concrete `GeminiAgent` implementation starts at line 16672. The dispatch
has no quota arm. `gemini --help` lists no quota subcommand or output option.
These checks cover the installed version, not future or private extensions.

The active core chunk, `chunk-RZ3ZVTZB.js`, implements `retrieveUserQuota` at
line 312138 and consumes it at line 382477. It reads Code Assist quota buckets
and their `modelId` / `remainingFraction` values. This is internal application
code, not an ACP surface. Source inspection is not a captured service response.
A real authenticated quota response is still required before adding the
per-model parser regression.

### Decision D4 (2026-11-12)

D4 was open after this spike: permit a read-only Gemini OAuth-file reader and a
non-generating `retrieveUserQuota` request, or leave this owner's quota
unavailable.

Decided by the coordinator, reversible: no Gemini quota for now. The direct
reader needs a recorded authenticated `retrieveUserQuota` response for its
per-model parser. Managed agents may not read the Gemini OAuth file to record
one, and a fixture must not be invented. No `GeminiCli` owner is registered and
binding creates no Gemini key, so Gemini sessions show no quota.

<!-- DEFERRED (2026-11-12) — GeminiCli adapter waits for Boss to authorize one recorded non-generating retrieveUserQuota response; then implement the read-only OAuth-file reader with per-model windows from remainingFraction. -->

## S3: Antigravity local quota source (2026-11-12, story 2996-22d1)

### Result and decision

A local RPC server exists, but this spike did not establish an authenticated
quota response contract. Defer the owner adapter until `agy` is a TUICommander
AgentType and its owned process can provide connection metadata. This is not
proof that Antigravity has no quota source. No product code was added.

<!-- DEFERRED (2026-11-12) — adapter waits for an agy AgentType, owned RPC port/CSRF metadata, and a recorded authenticated quota response. -->

### CLI and startup evidence

`agy --help` has no quota or user-status subcommand. The initial installed
cask path was `/opt/homebrew/Caskroom/antigravity-cli/1.0.1,5826024320139264/antigravity`.
The binary contained `GetUserStatus`, `quotaInfo` and `fetchAvailableModels`.
Those names alone do not define a usable API.

An isolated startup used a new home and workspace under Gits, with no inherited
API keys or token variables and no submitted prompt. Its own log and a scoped
`lsof` snapshot showed an HTTPS gRPC listener on `127.0.0.1:61061` and an HTTP
listener on `127.0.0.1:61062`. The logs reported that no Antigravity account
was logged in. Both ports were dynamically assigned, not fixed discovery ports.

### Updater anomaly and bounded follow-up

The initial startup unexpectedly spawned its built-in background updater
(PID 12539 in the owned startup log). The installed executable changed at
18:42:09. No installation or update was requested by this spike. The updater
process had exited when checked; no rollback was attempted.

A follow-up set `AGY_CLI_DISABLE_AUTO_UPDATE=true`. The log confirmed that
updates were disabled and identified the language server as **1.5.7**. The
binary SHA-256 stayed unchanged before and after this follow-up:

`e872cc54b5381f401c798fc947937a0a930876b849bc3fee654996ba4ab236a8`

This process listened on gRPC port 61842 and HTTP port 61843. The current
binary contains the route
`/exa.language_server_pb.LanguageServerService/GetUserStatus`. A non-generating
HTTP POST with JSON `{}` to that route returned:

```json
{"code":"unauthenticated","message":"unauthenticated: missing CSRF token"}
```

The HTTP status was 401. This proves a protected local RPC listener, not a
successful GetUserStatus dispatch: authentication can run before route lookup.
The isolated process was not logged in, and its quota manager logged that it
skipped refresh for that reason. No credential store was read to bypass either
precondition. The process was stopped and reaped after the probe.

### Remaining work

When an agy AgentType exists, inspect only its owned process for port and CSRF
metadata, then record one authenticated, non-generating status response. Verify
which response carries `quotaInfo`, its model boundaries and reset semantics
before designing an adapter. Never discover this source by scanning unrelated
localhost ports or by using another owner's credential.

Raw evidence and reproducible probe scripts are retained under
`~/Gits/.tmp/batch-d-quota-spikes/`: `agy.log`, `agy-probe.json`,
`agy-symbols.json`, `agy-rpc.log`, `agy-rpc-probe.json` and the probe scripts.
The first binary observation and the 1.5.7 follow-up are separate evidence sets.

## S5: Claude Code status line relay (2026-11-12, story 2992-0150)

### Result

An injected `statusLine` relay can chain the user's own status line. Claude
Code gives the status line command the subscription windows of the session
account after the first response. The relay design of plan §3.5 is used; the
active read stays as the fallback.

### Live probe

Claude Code **2.1.318** ran interactively in a pseudo terminal (120x40) in a
throwaway project under the Gits scratch directory. The launch was
`claude --setting-sources project,local --settings <flag file> --strict-mcp-config`,
so no user hook, plugin or MCP server ran. The project
`.claude/settings.json` held the "user" status line
`{"command": "printf 'USER-SL'", "padding": 4, "refreshInterval": 2}`. The
flag file held only `{"statusLine": {"type": "command", "command": "sh '<relay>'"}}`.
The relay logged each call and its stdin, then ran the user command with the
same stdin. One prompt ("Reply with the single word OK") was sent to get a
response. Evidence: `~/Gits/.tmp/tuic-2992/spike/` (`run.py`, `relay-*.log`,
`screen-*.raw`).

| Question | Observation |
|---|---|
| Setting precedence | Sources merge low to high: user, project, local, flag (`--settings`), policy (binary `["userSettings","projectSettings","localSettings","flagSettings","policySettings"]`). The flag `statusLine` replaced the project command: the relay ran on every update and the screen showed the chained `USER-SL`. A managed policy `statusLine` would replace the relay. |
| Object merge | `statusLine` merges field by field. The flag object without `padding` and `refreshInterval` kept the project values: the screen showed four spaces before `USER-SL`, and the relay ran every 2 s while idle (10 calls in about 18 s). With `"padding": 0` in the flag object the spaces were gone. So the relay must set only `type` and `command`. |
| `refreshInterval` | Inherited from the user's setting (above). Without one, the relay ran only on events: 3 calls in the same idle period. Repeated calls send the same `rate_limits` again. |
| Shell | The command runs under `/bin/sh` (`$0` = `/bin/sh`, `$BASH_VERSION` = 3.2.57 on macOS), not the login shell, with the session cwd and environment. The relay runs the user command the same way. |
| Empty output | When there is no user status line, the relay prints nothing and Claude draws no status row. The footer is the same as without a status line. |
| `rate_limits` | Absent before the first response. After it: `{"five_hour": {"used_percentage": 11, "resets_at": 1794519600}, "seven_day": {"used_percentage": 29, "resets_at": 1794754800}}`, percent 0-100, epoch seconds. The embedded documentation names the same fields and an optional `spend_limit` for gateways. The payload has no observation time. Recorded body: `src-tauri/src/provider_quota/fixtures/claude-status-line-stdin.json`. |
| Trust | The status line runs only after workspace trust ("Status line command skipped: workspace trust not accepted"). |
| Windows | Not observed: no Windows host. On Windows TUICommander hooks run through `tuic.exe agent-state`, which has no relay. The relay is not injected there, so the user status line stays unchanged and the quota stays with the active read. |

### Decision

- The generated `agent-hooks/claude.json` gets a `statusLine` with only `type`
  and `command` (POSIX hosts). It runs `agent-hooks/claude-statusline.sh`.
- The relay reads stdin exactly, posts it to the local socket route
  `POST /sessions/{id}/claude-status-line` with the `CLAUDE_CONFIG_DIR` and
  `HOME` of the agent, and receives the user's own command: the highest of
  `<project>/.claude/settings.local.json`, `<project>/.claude/settings.json`,
  `<config dir>/settings.json` that has a `statusLine.command`. It runs that
  command under `/bin/sh` with the same stdin and prints its output.
- TUICommander keeps the newest `five_hour` / `seven_day` windows per
  credential; the `claude-code` owner returns them from `observed()`. A repeated
  identical payload keeps its first observation time, so an old value ages
  normally and the active read can still run after the cadence.
- Limit: when the TUICommander socket does not answer, the relay prints
  nothing, so the user status line is not shown for that update.

<!-- DEFERRED (2026-11-12) — Windows relay through tuic.exe agent-state; needs a Windows Claude Code status line observation first. -->

## S6: pi provider responses (2026-11-14, story 2994-a1d0)

### Result

pi 0.84.2 gives every extension the HTTP status and all response headers of
each provider request (`after_provider_response`). The header names are
lowercase: each pi-ai transport copies `Response.headers.entries()`
(`utils/headers.js`). `ctx.model.provider` names the provider (`anthropic`,
`openai-codex`, `github-copilot`). So the managed extension can forward the
subscription headers, and does (kind `provider_response` on the `pi-event`
channel, `docs/backend/pty.md`).

No pi subscription login exists on this machine:
`pi auth check --provider <p> --json --no-refresh` answered
`credentials_not_configured` for `anthropic`, `openai-codex` and
`github-copilot`. No subscription header was therefore seen through pi, and no
auth file was read. The forwarded names are the provider families of the same
endpoints pi calls:

| Provider | Header names (no values) | Evidence |
|---|---|---|
| `openai-codex` | `x-codex-plan-type`, `x-codex-active-limit`, `x-codex-{primary,secondary}-{used-percent,window-minutes,reset-at,reset-after-seconds}`, `x-codex-primary-over-secondary-limit-percent`, `x-codex-credits-{has-credits,balance,unlimited}` | recorded live response of `chatgpt.com/backend-api/codex/responses` (ego S1, `ego-ai/tests/fixtures/subscription_quota/openai-codex-response.json`) |
| `anthropic` | `anthropic-ratelimit-unified-{status,representative-claim}`, `anthropic-ratelimit-unified-{5h,7d,overage}-{status,utilization,reset}` | TUICommander's existing unified-header parser; ego S1 got a 429 without them, so they are not recorded live |
| `github-copilot` | none known | — |

`x-codex-turn-state` is an opaque per-turn value (redacted in the ego
recording) and is not forwarded. No other header crosses the channel.

The pi transport itself is recorded: real pi ran the generated managed
extension against a local server that sent quota headers next to
`authorization`, `set-cookie` and `x-codex-turn-state`. The frame carried only
the quota headers. Fixture and method:
`src-tauri/src/fixtures/agent_prompts/pi-provider-response-2994.md`.

### GitHub Copilot quota endpoint

None recorded. pi reads no Copilot quota: its Copilot code calls only the
token exchange (`copilot_internal/v2/token`), model enablement and the model
list. Without a pi Copilot login no candidate endpoint could be recorded. The
`pi` owner reports Copilot keys as unavailable.

<!-- DEFERRED (2026-11-14) — Copilot quota needs a recorded non-generating response from a pi Copilot login; Anthropic header names need a successful pi or ego response. -->
