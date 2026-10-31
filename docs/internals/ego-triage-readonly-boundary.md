# Ego issue triage: read-only execution boundary

Date: 2026-10-29. Story: 2450-6845. Status: design blocked; not implemented.

## Contract

An ego triage evaluator must receive no write tools and no push credentials.
TUIC must enforce this before the evaluator receives untrusted issue text.
The evaluator returns an accepted, rejected or needs-input decision. Only the
existing host publisher may add a GitHub comment or label. The existing managed
executor keeps its operator-attested guarantee and remains the default.

**No current TUIC launch policy establishes the complete ego guarantee.** Do not
select the existing unattended ACP helper and describe it as credential isolation.
This note records the missing boundary, not a shipped executor option.

## Verified sources

TUIC source baseline: `911779b58ff9bbdfaacc4e152809d004359392d7`.
Ego source baseline: `67ed09908dc7cc0ad2541049c726cfe4d210becb`.
Ego paths below are relative to its separate repository.

| Boundary | Evidence | Actual guarantee |
| --- | --- | --- |
| Current triage configuration | TUIC `src-tauri/src/automations/model.rs:43-58`, `:239-240` | Requires `read_only_attested`; does not remove credentials. |
| Current launch | TUIC `src-tauri/src/automations/dispatcher/native.rs:44-61` | Launches the named managed run configuration. |
| Ego configuration ownership | TUIC `src-tauri/src/ego_cli.rs:1-7`, `src-tauri/src/acp_commands.rs:36-47` | Ego owns provider configuration and keys; TUIC selects its executable and profile. TUIC does not currently supply a scoped provider credential. |
| Unattended authority | TUIC `src-tauri/src/acp/manager.rs:1588-1592` | Removes the session's MCP servers only; preserves directory authority. |
| Native tools | Ego `crates/ego-cli/src/cli.rs:694-700`, `crates/ego-cli/src/run/composition.rs:278-353` | `--no-tools` selects an empty native registry. TUIC's existing ACP launch does not pass this flag (`src-tauri/src/acp/mod.rs:199-208`). |
| Environment | TUIC `src-tauri/src/acp/manager.rs:281-300`; agent-client-protocol 2.0.0 `src/acp_agent.rs:261-263` | SDK adds environment entries to an inherited environment. Its configuration API has no environment-clear operation. An absent `env` request is not an empty child environment. |
| Bare configuration | Ego `crates/ego-core/src/config/resolve.rs:333-346` | Bare removes project/local configuration, not user/system configuration. |
| Hooks | Ego `crates/ego-core/src/hook.rs:1284-1322`, `crates/ego-cli/src/run/port_adapters.rs:369-370` | System/user hooks can be admitted independently of a model's native tool selection. |
| Prompt helper | Ego `crates/ego-core/src/config/resolve.rs:389-392`, `crates/ego-cli/src/run/headless_driver.rs:289-305` | User `mdkb_prompt_command` has an execution path separate from the native tool registry. |
| Standalone MCP | Ego `crates/ego-cli/src/run/composition_root.rs:977-996` | Standalone execution attaches enabled configured MCP servers independently of the native tool selection. Do not substitute `ego run --no-tools` for an exclusive no-tool policy. |
| Publication | TUIC `src-tauri/src/automations/triage.rs:32-100`, `src-tauri/src/automations/store/triage.rs:20-66` | Host validates the policy and bound decision, then publishes separately. This authority must never reach the evaluator. |

No credential store or keychain was read. These findings describe source contracts,
not the contents of the operator's environment, hooks or credentials.

## Required policy

Use a host-issued, immutable **data-only triage policy** for the first ego executor.
A no-tool evaluator is sufficient when TUIC supplies the issue and required
repository context. Do not introduce a generic shell with a command denylist.

The launch boundary must provide all of these properties together:

1. Start a fresh evaluator with an explicitly constructed environment, not the
   inherited environment plus overrides. Pass only necessary platform values and
   an explicitly selected model-provider credential. Do not pass GitHub tokens,
   SSH-agent sockets, credential-helper settings or TUIC control-plane identity.
2. Exclude ambient user/project execution configuration, MCP servers, hooks,
   skills, prompt commands and delegated execution. `--bare` alone is insufficient.
   Preserve administrator restrictions, but fail closed if an administrator
   configuration requires an effect incompatible with data-only evaluation.
3. Offer an empty native tool registry and no MCP tools. Refuse unexpected host
   requests. Do not give the model filesystem access to Git, gh or SSH credential
   stores. Model-provider authentication belongs to the trusted runtime only.
4. Return structured decision data to the host. Do not give this evaluator the
   generic automations MCP tool to report its decision; that would reopen the
   host control plane. Bind the returned decision to the admitted run internally.
5. Apply the existing deadline, cancellation, restart and immutable-decision
   rules. Recheck the selected executor/policy before dispatch and publication.

The current ACP SDK cannot express item 1. TUIC needs an environment-clearing
transport adapter or an upstream SDK extension. Ego also needs a host-owned
exclusive execution policy for item 2; a named user profile is still mutable
operator configuration. Work in the ego repository and SDK is outside this
assignment's write scope. Approval of that scope and the provider-credential
handoff is required before the executor can be enabled.

## Boundary tests required before activation

Record the short reality contract in the story worklog before implementation.
The test must observe the actual child launch and session admission, not only a
helper that returns the expected flags. Use a local protocol fixture, not GitHub
writes or a paid provider call.

- Seed fake push credentials, an SSH socket and unrelated environment values;
  capture the child environment and assert that none reaches it. Check exact
  values without logging secrets. Check Windows case-insensitive names too.
- Supply ambient write-capable MCP configuration, a hook and a prompt helper;
  assert that the evaluator neither starts them nor receives their tools.
- Attempt native write, exec and delegated tools from the fixture; assert that
  the admitted evaluator cannot dispatch them. A provider omitting tools from
  its response is not proof of enforcement.
- Check cancellation, policy withdrawal, malformed decisions and the existing
  external executor. Ensure data-only evaluation never calls the managed PTY
  launcher and never grants the evaluator host publication credentials.

Until these tests can exercise a real admitted policy, executor selection remains
unimplemented. No behavioral test or GREEN result is claimed by this note.

## Enforced versus attested

Today, TUIC enforces repository allowlists, author-association admission, durable
policy-version deduplication, bound immutable decisions and separate host
publication. The external evaluator's read-only profile and absence of push
credentials remain operator-attested, as documented in story 2324-424d.

The proposed ego executor must enforce tool and credential exclusion at launch.
The trusted ego executable, model-provider transport and correctness of the
model's triage decision remain trust assumptions. This policy does not claim
operating-system isolation of the trusted ego executable itself.
