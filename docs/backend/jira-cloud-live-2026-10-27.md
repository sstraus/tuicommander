# Jira Cloud: recorded responses and native plugin evidence

Date: 2026-10-27. Stories: `1694-161f` and `1756-4d42`.
Core baseline: `9fce3f176c`. Plugin evidence commit: `c19f8af`.

## Authority and isolation

All vendor requests used the operator-owned test site
`https://straus-team.atlassian.net`. No other Atlassian site or Atlassian MCP
account was used. Writes were authorized on this site only.

The coordinator changed the credential source to the existing test configuration
in the orchestrator checkout. Only the three named Jira test variables were
selected into process memory. No value was printed or copied into this repository.
The native account was added through `/accounts/action` over the isolated Unix
socket; the account service verified it and stored its token. The plugin then
used its ordinary `host.httpFetch` account path and Rust credential loading.
No credential store was read by the agent. Boss gave **zero interactive approvals**.

The native app was built and started with:

```sh
make agent-test AGENT_TEST_INSTANCE=agent-test-jiralive AGENT_TEST_PORT=19888
```

The dry run was checked first. `mbx doctor` reported zero failures and warnings.
The isolated socket was
`~/Gits/.tuic/tuic-at/0da69ad430b8fe57/tuic-mcp-0da69ad430b8fe57.sock`.
No daily-instance endpoint, replacement host, injected catalogue or browser-only
loader was used. This is native macOS evidence, not an rb result. `file` identifies
the executable as Mach-O arm64; its SHA-256 is
`3c6fed4b0cceb7bbf2ec2e90a06ce6c31d69b1c7200db7d7c7c37b49d2139314`.

## Recorded REST boundaries — 1694

The REST setup created scratch project `TUIC` (`10001`) and issues `TUIC-1` and
`TUIC-2`. The scrubbed `plugins/jira/fixtures/cloud.json` replaces the former
synthetic file. It contains real response bodies and method/path/time/status/
relevant-header provenance for:

- Basic verification, project creation and issue-type enumeration.
- Full Task create metadata and successful issue creation.
- Issue detail with persisted ADF lists and code.
- Legal transitions and a successful transition response.
- Two independently fetched comment pages after real comment writes.
- Two enhanced-search pages and their opaque cursor.
- A real malformed-JQL 400.
- The real scratch Scrum board configuration.

Account IDs, user display names, email addresses, avatar URLs and self links were
scrubbed. The exact test email, token and Basic-auth payload were scanned before
commit. No raw recording or environment file is committed.

The old synthetic values remain as explicitly labeled `cloud-contract.json` for
existing edge cases; they are not relabeled as recordings. Four new provider
replay tests use the real `cloud.json` for search/error, comment/ADF, board move
and create-schema boundaries. Each names the plausible defect it protects.

## Native registered-provider flow — 1756 candidate 9, Jira

The executable harness is `scripts/jira-cloud-desktop-e2e.mjs`. It installs the
actual plugin from this checkout, reloads it through the production loader, and
gets the existing account-scoped provider from the production registry singleton.
It neither constructs a provider nor replaces a host method.

1. The account binding has no project override.
2. The actual Jira Settings input and blur handler save default project `10001`.
   No settings file is edited and no settings write service is called directly.
3. After reload, exactly one registered Jira provider accepts the default.
4. Authenticated `createFields()` returns the real project's five issue types;
   the selected Task type (`10006`) returns its real field schema.
5. The registered provider creates `TUIC-3`, titled `Native Jira plugin E2E`.
6. A later provider detail read returns the written lists, JavaScript code block
   and explicit hard break without changing the expected Markdown.
7. The provider adds a comment and reads it back, then uses transition `31` to
   move the issue to **Done** and reads back that state.
8. The provider searches for the new issue and returns it.

The machine-readable result is [jira-cloud-live-2026-10-27.json](jira-cloud-live-2026-10-27.json).
The [window-only screenshot](jira-cloud-live-2026-10-27.png) was captured with
`screencapture -x -o -l 26898`, after matching that window to this instance's
recorded PID `65761`. OCR confirms the Jira plugin settings and saved `10001`
input are present. The maccontrol visible-window path refused the non-activating
test window; it did not capture the daily app or the whole screen. No pixel-level
visual design claim is made from OCR.

### Failure and continuation

The first account-add attempt reported `Account provider is unavailable; load its
plugin first`. Loading the actual plugin resolved that prerequisite.

The first complete native run reached creation of `TUIC-3` but its immediate
three-request detail fan-out failed with macOS `Network is down (os error 50)`.
The write was not replayed. The harness gained an explicit `--resume=TUIC-3`
option and continued against that existing issue; the final run exited zero and
completed all read-back, comment, transition and search checks. No network root
cause or production fix is claimed.

## Validation

Dependencies were verified with `pnpm install --offline --frozen-lockfile`.
The single targeted local run went through `build-slot.sh` and the repository
test-temp wrapper:

```sh
scripts/with-test-tmp.sh node --test plugins/jira/*.test.js
scripts/with-test-tmp.sh pnpm vitest run \
  src/components/Tasks/create/__tests__/TasksCreate.jira.test.tsx \
  src/components/Tasks/create/__tests__/TasksCreate.jiraUsers.test.tsx \
  src/components/Tasks/list/JiraCapabilities.test.tsx \
  src/components/Tasks/roadmap/JiraRoadmap.test.tsx
```

Results: **60 Node tests passed**, zero failures/skips (160.46 ms); **29 Vitest
tests passed across four files** (14.18 s). No full suite, Rust unit suite,
mutation run or clippy run was requested or performed. Production code did not
change, so no new pre-fix RED is claimed.

Logs retained under `~/Gits/.tmp/results/`: `tuic-jira-live-agent-test.log`,
`tuic-jira-live-tests.log`, `tuic-jira-live-desktop.log`,
`tuic-jira-live-resume.log` and `tuic-jira-live-window-ocr.log`.

The matching `make agent-test-stop` was requested after capture. A subsequent
exact-PID check found neither native PID `65761` nor any task-started background
process still running; the instance's `desktop.pid` was removed. No daily app
restart was requested. The scratch site/project/issues remain as authorized test
resources; no other site was modified.

## Remaining limits

- No natural 429 was observed. No load was generated to force one.
- The recorded board has one status per column, not a multi-status column.
- Data Center PAT/context paths, multiple live sites, richer unsupported ADF,
  additional required/custom-field workflows and stale transition rejection are
  not newly proven by this scratch setup.
- Candidate 9 is newly proven past credentials for **Jira**, not GitLab.
- Candidate 12 is the GitHub core review/actions port criterion and was already
  checked by prior authenticated GitHub evidence. Jira credentials cannot prove
  it; no new GitHub review/action run is claimed here.
- The original morning run did not exercise URL-resolver cancellation. The
  afternoon continuation below supplies that separate native evidence.
- The morning run did not change story criteria. The afternoon audit checks the
  recorded-fixture criterion and the applicable candidate criteria; review proofs
  and story completion remain separate.

## Reproduce the loaded-provider continuation

First add a verified Basic account through the named instance's supported
account service, without printing credentials. Pass only its non-secret account
ID to the harness. On the same scratch site/project:

```sh
node scripts/jira-cloud-desktop-e2e.mjs \
  agent-test-jiralive '<account-id>' 10001 10006 \
  "$PWD/docs/backend/jira-cloud-live-2026-10-27.json" \
  --write-straus-team-only --resume=TUIC-3
```

Omit `--resume` to create a new scratch issue. The continuation still adds a
comment and requests the legal Done transition; it is not read-only.

## Native resolver continuation — 1756 candidate 10

The afternoon continuation uses the same isolated `agent-test-jiralive` instance,
rebuilt from this worktree. Its native health response identifies macOS arm64 and
binary SHA-256 `7d09ec7b6520e8727cdc33cf24aa37edaece7c6aca9a63f007d8b353a4714aa9`.
The actual account remains in the supported account service; no credential store
or environment value was read for this continuation. No Jira write was sent.

The harness now accepts `--read-only-resolver` instead of the write consent flag:

```sh
node scripts/jira-cloud-desktop-e2e.mjs \
  agent-test-jiralive '<account-id>' 10001 10006 \
  "$PWD/docs/backend/jira-resolver-live-2026-10-27.json" \
  --read-only-resolver
```

This mode still installs the real plugin, binds the selected account without a
project override, and first seeds a different baseline project (`99999`), then uses the actual
Settings input and blur handler to save `10001`. Authenticated project issue-type metadata succeeds. This repeats Jira's
settings-to-vendor path without creating another issue. Existing GitLab settings
persistence and default-consumption evidence remains credential-independent;
this run does not claim authenticated GitLab acceptance.

The harness imports the already-loaded production URL registry singleton. It
neither constructs a provider nor replaces the registry, host, native IPC, vendor
responses or timers. The real registry resolves
`https://straus-team.atlassian.net/browse/TUIC-3` to its account-bound link and
start-work draft. For cancellation, it starts another lookup, waits for the
scheduled abort point, and first requires that the lookup is still pending.
It aborts the signal and requires a null result within 1000 ms. This proves
native aborted-result suppression only: the host throws AbortError after native
HTTP settles even if `plugin_http_cancel` did nothing, so this run is not a
request-stop receipt. Request stop rests on the module/transport cancellation tests. A pre-aborted lookup also returns null,
and a fresh signal resolves TUIC-3 afterward.

The final machine-readable evidence is
[jira-resolver-live-2026-10-27.json](jira-resolver-live-2026-10-27.json).
The abort ran after **141 ms** (the requested timer delay is a minimum, not a
scheduling guarantee); the pending lookup settled **133 ms** after abort. The
production `PluginHost.httpFetch` awaits native HTTP before its aborted result
settles (`src/plugins/pluginRegistry.ts`), while Rust cancellation drops the
in-flight HTTP future (`src-tauri/src/plugin_http.rs`). The request-stop
proof rests on the existing module/transport cancellation regressions, not on
this run. It does not claim vendor-side request rollback or an exact count of sockets.

### Diagnosed attempts and cleanup

The first harness attempt inspected dispatch before asynchronous registry
traversal reached Jira. A subsequent attempt tried passive native IPC
observation, but the native `invoke` property is non-writable and non-configurable;
that approach was removed, not bypassed. No replacement IPC remained in the final
harness or run. Automatic plugin reloads also raced early setup requests; the
harness now waits for a stable registered provider across the native watcher's
500 ms debounce window before it begins vendor reads.

Two later attempts encountered the existing macOS `Network is down (os error 50)`
transport failure. A shell curl check reached the test site with HTTP 302, which
does not explain or fix the native transport failure. The final native run
succeeded without a production change. Failed logs are retained as
`tuic-1756-resolver-e2e*.log`; the successful run is
`~/Gits/.tmp/results/tuic-1756-resolver-e2e-final.log` (exit 0).

The matching `make agent-test-stop` stopped the task-owned instance. Its
`desktop.pid` was removed and the supervisor exited with make status 2 after the
requested application termination. No daily instance, browser or Vite server was
started or stopped. No visual/CSS change was made; the earlier Settings screenshot
remains the visual evidence. Claude `/wiz:review` proof criteria remain open.

### Continuation validation

The one final targeted plugin run used `build-slot.sh` and
`scripts/with-test-tmp.sh node --test` with these four files:

- `plugins/jira/main.test.js`
- `plugins/jira/recorded-cloud.test.js`
- `plugins/jira/provider.test.js`
- `plugins/jira/adf.test.js`

All **43 tests passed**, zero failures, cancellations or skips, in **124.59 ms**
after a **420 s** shared build-slot wait. The retained log is
`~/Gits/.tmp/results/tuic-1756-plugin-tests.log`. The real native E2E also exited
zero. This is targeted local Node and native macOS evidence, not a new rb,
Vitest, full-suite, mutation or clippy result. No production behavior changed;
previous production RED/GREEN evidence remains applicable. The new harness passes
`node --check`; it was formatted through Biome stdin using an included `.mjs`
path because the normal scripts path is excluded by Biome configuration.
