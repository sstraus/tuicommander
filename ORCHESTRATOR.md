# Root Orchestrator

## Invariants

- One root orchestrator; at most three managed children.
- Use TUICommander `agent`, `session`, and `repo` actions.
- Children must not spawn agents.
- Delegate bounded, independent work with non-overlapping ownership.
- Preserve unrelated and uncommitted work. Do not branch, commit, push, revert, or overwrite without authorization.
- Use managed worktrees for concurrent edits.
- Verify the effective model after spawning; reject fallbacks.
- Require a result or blocker through the parent inbox. Terminal output is an anomaly fallback.

## Root lifecycle

```text
agent action=register orchestrator=true name=root-orchestrator project=<repo>
```

Use the returned identity. On completion, close finished sessions, remove only safe inactive
worktrees, and unregister the orchestrator.

## Claude one-shot

Use an exact model and absolute paths:

```text
agent action=spawn
  name=<stable-name>
  agent_type=claude
  model=claude-sonnet-5
  cwd=<absolute-path>
  args=[
    "--print",
    "--setting-sources", "",
    "--no-session-persistence",
    "--mcp-config", "<absolute-path>/claude-mcp.json",
    "--strict-mcp-config",
    "--append-system-prompt-file", "<absolute-path>/subagent-system.md",
    "--no-chrome",
    "--dangerously-skip-permissions"
  ]
  prompt=<bounded-task>
```

Rules:

- Do not use `--bare` with Claude.ai OAuth; it ignores OAuth and Keychain credentials.
- `--setting-sources ""` removes user/project settings without disabling OAuth.
- `--no-session-persistence` requires `--print`.
- `--mcp-config <path>` supplies the file; `--strict-mcp-config` is a separate boolean flag.
  Reversing or omitting these arguments can make Claude treat the JSON path as the prompt.
- Use `--dangerously-skip-permissions` only for a bounded managed directory or isolated
  worktree. For read-only work, prefer `--permission-mode dontAsk` plus explicit
  `--allowedTools`.
- Keep `claude-mcp.json` minimal: TUICommander handoff plus task-required servers only.

For direct CLI experiments, the equivalent form is:

```sh
claude --print --output-format json \
  --model claude-sonnet-5 \
  --setting-sources "" \
  --no-session-persistence \
  --mcp-config ./claude-mcp.json \
  --strict-mcp-config \
  --append-system-prompt-file ./subagent-system.md \
  --no-chrome \
  --dangerously-skip-permissions \
  "<task>"
```

## Persistent Claude

Omit `--print` and `--no-session-persistence`. Reuse one named session for related
follow-ups. Use the configured wrapper only when its private settings are required:

```text
binary_path=/Users/stefano.straus/.local/bin/claude-c2
```

Do not pass Codex's `--dangerously-bypass-approvals-and-sandbox` to Claude.

## Appended system prompt

```markdown
You are a bounded subagent managed by a root orchestrator.

- Work only in the assigned directory and scope.
- Do not spawn agents.
- Preserve unrelated and pre-existing changes.
- Follow the task's mutation and validation permissions.
- Send the complete result or blocker to the parent through TUICommander.
```

Put scope, acceptance criteria, exclusions, ownership, and required evidence in the task
prompt—not the system file.

## Routing

| Work | Exact model |
|---|---|
| implementation, debugging, refactoring | `claude-sonnet-5` |
| bounded research, audit, archaeology | `gpt-5.6-terra` |
| tests, builds, lint, CI/CD, delivery | `gpt-5.6-luna` |

Verify process arguments or the settled footer. Close and replace any fallback.

## Supervision

- Use one event-driven `agent action=wait`.
- Lifecycle messages are state, not results.
- Use `agent action=task_get` beyond the wait ceiling.
- Inspect status only for missing or anomalous delivery.
- Read terminal output only if inbox handoff failed.
- Answer awaiting input through `session action=input`, or stop the child.
- Send corrections through `agent action=send`; do not duplicate inbox content in the PTY.

## Completion

- Reconcile every result and blocker.
- Resolve ownership conflicts without discarding unique work.
- Obtain repository-required validation from the required model.
- Report failures and uncertainty.
- Rust changes under `src-tauri/**` require the rebuild story and a manual `make dev`
  restart notice.
