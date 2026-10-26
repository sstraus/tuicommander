# Plugin review: native live evidence for 1756-4d42

Date: 2026-10-22. Worktree: `fix/1693-provider-closure`. Core baseline: `54ce1e8ff`. Plugin checkout: `d9d9f294dfceae27d6f797b8b421a00952cfe171`. No production implementation changed in this evidence update.

## Contract and isolation

Use a real loaded plugin, its production provider registration, native settings service and the published catalogue. Do not replace a host method, inject a catalogue, use a network double or supply a token. The check uses only the named `agent-test-ghlive` instance and its Unix socket. No daily-instance debug endpoint is used.

The native build and runtime target are `aarch64-apple-darwin`. The [recorded result](plugin-review-live-1756.json) includes the native binary digest, instance identity, loaded plugin manifests and actual UI row text. This is native desktop evidence, not an rb or cross-target result.

## Candidate 4 — PASS

The real `fetch_plugin_registry` path now returns published payload digests. This supersedes the missing-publication blocker recorded on 2026-10-20.

The production catalogue and installed GitHub Work package both report version `1.0.0` throughout:

| Installed payload | Native discovered SHA-256 | Actual Installed UI |
| --- | --- | --- |
| Original | `0fd734c9cfd8b4049800594501a5aca25e3ff66bddb5e91e47c6f5fb4a4f3aad` | No Update button |
| Harmless comment appended to installed `main.js` | `5c5a3ad43ee05d91fbe91bbd75eb0b0ef71d703a2c18d2b18815f43327137e90` | Update button and equal-version badge |
| Original bytes restored | `0fd734c9cfd8b4049800594501a5aca25e3ff66bddb5e91e47c6f5fb4a4f3aad` | No Update button |

The plugin remains loaded with no error. The test uses native rediscovery/reload and the existing catalogue singleton. It does not substitute a registry response or change the registry URL. Original installed bytes are restored in `finally`.

## Candidate 9 — PASS to the credential boundary; vendor result remains open

Both Jira and GitLab are installed and loaded by the real desktop loader. Each has one explicit, non-secret local Tasks account binding with no credential. The `.invalid` host is a local configuration input, not a recorded vendor fixture, and no request reaches it.

The test opens each actual plugin Settings page, uses its real input and blur handlers, reads back the native persisted configuration, and reloads the real provider:

| Plugin | Before setting the default | Value saved through Settings | Actual registered provider after reload |
| --- | --- | --- | --- |
| Jira | `Jira project is required` | `projectId = TUIC-1756` | `createFields()` reaches Rust and returns `Account not found` |
| GitLab | `GitLab create needs a configured project` | `project = sstraus/tuic-1756` | `createFields("issue")` reaches Rust and returns `Account not found` |

The change in the observable failure proves that the loaded provider consumes the configured default instead of taking its missing-project branch. Neither default is set by editing `config.json` or by invoking the settings write service directly. Only credential-free account bindings are seeded through the normal native plugin-data API.

This does **not** prove an authenticated metadata response, an accepted vendor project, account-specific override precedence against a vendor, or successful item creation. The full candidate remains unchecked pending that evidence.

## Candidate 12 — PASS to the credential boundary; authenticated review remains open

The check gets the existing `github/sstraus` provider from the production account-scoped registry. It does not import a new provider implementation or construct a test host.

For the real sandbox reference `sstraus/tuic-test-sandbox/pull/2`:

- `comments(reference, "threads:first", 50)` enters the installed plugin's core review-thread adapter and returns `Account not found` from Rust.
- `runPrAction(reference, "state")` enters the installed plugin's core PR-action adapter and returns the same credential-boundary error.
- The same action with a different account reference fails earlier with `Item belongs to another provider/account`.

Source inspection confirms that `plugin_github::execute` validates capability, manifest provider and PR identity before `accounts::load_credential`; successful service dispatch follows credential loading. The observed error is therefore beyond the absent-port branches, not a successful GitHub API result.

This does **not** prove review submission, merge/action mutation, successful B6/B7 data, or a core UI review operation. No write is attempted. Candidate 12 remains unchecked until the authenticated E2E is run.

## Reproduce

Prerequisites: a linked worktree, dependencies installed, the real GitHub Work plugin installed and loaded in the named instance, and its credential-free or authenticated `sstraus` binding restricted to `sstraus/tuic-test-sandbox`. The harness replaces Jira/GitLab bindings and defaults **inside that throwaway instance**. Do not use an instance whose plugin settings must be preserved.

```sh
export PATH="/opt/homebrew/bin:$HOME/Library/Application Support/mbx/bin:$PATH"
make agent-test AGENT_TEST_INSTANCE=agent-test-ghlive AGENT_TEST_PORT=19887 AGENT_TEST_DRY_RUN=1
make agent-test AGENT_TEST_INSTANCE=agent-test-ghlive AGENT_TEST_PORT=19887
```

After the native frontend answers an evaluator readiness probe, run from the same worktree:

```sh
node scripts/plugin-review-desktop-e2e.mjs \
  "$HOME/Gits/.tmp/tuic-at/416e20482e26b1f5/tuic-mcp-416e20482e26b1f5.sock" \
  agent-test-ghlive --isolated-desktop
```

Stop only that instance:

```sh
make agent-test-stop AGENT_TEST_INSTANCE=agent-test-ghlive AGENT_TEST_PORT=19887
```

The harness resolves the production reload export from the current embedded build and imports its existing asset modules. The registry and catalogue imports return the already-loaded singleton instances. No frontend or plugin method is replaced. The script checks socket/instance identity and confines payload mutation to the named instance.

## Execution and limitations

The final command passed all three local contracts and exited zero. Log: `~/Gits/.tmp/results/tuic-gh-1756-e2e-verified-run.log`. Earlier harness-development attempts exposed an overly strict navigation text locator and a settings-toggle race during plugin reload; the harness now uses the real plugin navigation title and checks panel presence separately from plugin-page readiness. These were harness failures, not product defects. An earlier native catalogue fetch also reported a transport error; the final real fetch succeeded without a source change. No transport root cause is claimed.

The supervisor reported exit `2` after the requested application termination (`make` wraps the expected terminated child). Its recorded PID `23171` is no longer running, and the instance `desktop.pid` was removed. No browser or Vite process was started for these checks. Task-created helper files are removed after evidence collection.

No Rust, Vitest or full-tree suite was run for this evidence-only update. Candidate 10 remains parked. GitHub login still needs Boss's Device Flow confirmation; no replacement code was requested and no credential store was inspected.
