# Agent-work triage — 2026-10-20

## Scope

Reconcile the three items in section 1 of the orchestrator's 2026-10-19 Boss-wait triage report: GitHub traffic (1995-279a), ego resize width (573-bd86), and keyboard-to-paint latency (2040-16b2). Do not restart Boss's app, capture its desktop pixels, start another desktop instance, or close criteria that still need native evidence.

This is an evidence report, not a production change. Story worklogs were updated through the Wiz CLI in each main checkout. The assigned worktrees use the coordinator-approved branch `chore/triage-3`.

## 1995: already measured and complete

The main-store story is already complete. The committed report [GitHub toolbar and request volume](../../reports/tuicommander/2026-10-18-github-toolbar-and-request-volume.md#joined-live-account-observation-2026-10-19) records the joined production browser App, HTTP router, poller and reqwest client observation. This is stronger evidence than inferring counts from configured intervals.

| Observed phase | Window | REST | GraphQL | GraphQL per minute |
| --- | ---: | ---: | ---: | ---: |
| Visible idle app, no PR panel | 60.002623 s | 0 | 1 batch | 0.99996 |
| Open GitHub panel, expand one PR, then leave mounted | 63.321365 s | 0 | 2: PR action state and CI checks | 1.89509 |

The recorded workload used one account and the public `tauri-apps/tauri` repository. Three cold-start queries (two concurrent viewer lookups and one batch) preceded the idle window. Two other batch arrivals occurred outside the numeric windows. All eight recorded upstream replies were HTTP 200.

The source still defines a 60-second visible base interval and 120-second hidden interval. After 15 minutes without changes, hot repositories use every fifth tick; dormant repositories use every tenth tick with jitter. Budget pressure and failure backoff can increase cadence intervals. Panel actions and additional pagination can create more requests: these intervals are not an account-wide request ceiling.

Source: `src-tauri/src/github_poller.rs` constants and `tiered_paths` / `current_interval`. Existing fixture evidence covers 403 and 429 pre-send backoff for REST and GraphQL. It does not prove that other applications cannot exhaust the same account budget, or that every secondary-limit condition is impossible.

No new GitHub request or credential access was necessary. This peer re-read the committed report, story worklog and source. The report's original active-worktree artifact path is no longer present at the tested path; this peer does not claim to have revalidated its raw receiver trace. The committed report and detailed original worklog remain the evidence cited here.

`story-close.sh --check` passed for 1995-279a without writing anything. No duplicate close was performed.

## 573: landed regression and viewport replay pass

The ego story is already `wontfix` by the coordinator's reversible not-reproducible decision. Its regression commit `8b2f28ec` is an ancestor of master `d6927ba517c271644a9150489db30e20deb89549`.

A fresh rb run of `live_width::tool_rows_follow_real_pty_growth_and_shrink` passed: one selected test, 4.76 seconds, exit 0, `stale=no`, `box_dirty=no`. It launches a real PTY and checks completed tool rows at 80, 200 and 60 columns. Log: `~/Gits/.tmp/results/triage-3-resize-regression.log`.

The companion ego report is `docs/internals/2026-10-20-resize-width-triage.md` on ego branch `chore/triage-3`. A native worktree binary built through build-slot/mbx in 1 minute 28 seconds. Real PTYs started at 80 columns, then changed to 47 and 173; completed tool headers used exactly 47 and 173 cells. A named stealth-wrapper browser session replayed those actual captured headers through the production grid renderer at 390×844 and 1400×900. The retained JSON records matching viewport/column/header widths, and matching PNG screenshots exist under `~/Gits/.tmp/triage-3-width/`. Controlled metrics are 8-pixel cells with 12 pixels of horizontal allowance. Own Vite, native children, loopback stub and named browser resources were stopped. A renderer replay cannot prove the normal full-app resize-dispatch path or native WebKit presentation; those distinctions remain explicit.

## 2040: prerequisites landed; native paint observation still missing

Git ancestry confirms that ego 571 (`e30e0e38`) and 574 (`b1d07ab2`) are already on master. Waiting for these source fixes is obsolete.

Existing measurements are not keyboard-to-paint measurements:

- Story 574: 371.629042 ms from TUIC submission to the final help row in session output; 100.912203 ms for draft echo during long output in its real-PTY test.
- Story 2040: direct cursor-position response median 0.01215 ms and application echo p95 0.12029 ms in TUIC.
- The landed discovery regression measures kernel PTY echo and independent frame consumption during an 800 ms filesystem delay. It does not observe presentation of a glyph.

A valid native measurement needs a trusted input timestamp, identification of the matching glyph update, and a native WebKit presentation observation. The current assignment forbids desktop pixels/interaction on Boss's instance and a second desktop instance. A worktree Vite browser replay can measure the browser renderer, not settle that native criterion. No synthetic draw timing is substituted for native keyboard-to-paint.

The latest existing story worklog already records a post-restart runtime and absence of discovery in a desktop main-thread sample. Therefore another restart is not demonstrated to be the remaining prerequisite. The remaining gap is the authorized native input/presentation observation itself. The original native GREEN and REALITY stay unchecked, and 2040 remains open.

## Story-close reconciliation

Initial preflight results:

- 1995-279a: pass.
- 573-bd86: refuses the original unchecked RED; the bug did not reproduce.
- 2040-16b2: refuses the original unchecked generic RED, despite later checked named regressions. Native GREEN and REALITY also remain unproved.

The coordinator authorized rejection of 573's original unreproduced RED and addition of a narrow verification GREEN; the `wontfix` status stays unchanged. The coordinator also authorized rejection of 2040's generic RED as superseded by its checked named renderer and discovery regressions. Its native GREEN and REALITY stay unchecked and the story stays blocked.

The named owner/prerequisite for the remaining 2040 work is an isolated `make agent-test` after story 2086 lands, or Boss's authorized native observation. This peer will not fabricate a RED, relabel unproved native criteria as passed, or change a creator's criterion to human-only merely to make a close gate pass.

## Validation scope and operational limits

This peer ran one targeted Rust regression on rb and one explicitly authorized native ego binary build for exploration, not a broad test suite. Native PTY and browser replay drivers both exited 0 after the initial retained probe-readiness failure was corrected. No paid model request, GitHub request, token read, production frontend change, new committed capture, installed-binary replacement, main/master source edit, merge or push occurred.

A read-only request for worktree warm status through the desktop HTTP API returned 401 Unauthorized; no credentials were read and no bypass attempted. The evidence-only Vite replay used the already available worktree dependencies. The authorized story GREEN addition also encountered the terminal-story CLI refusal described in the ego report; no criteria success is inferred from a failed command.

Final coordinator disposition: do not reopen terminal story 573 to add criteria; retain its current criteria and record the successful verification in its worklog only. Final preflight passes 1995, refuses 573 original broad GREEN, and refuses 2040 native GREEN. These refusals are explicit limitations, not test failures to suppress. Story states remain complete / wontfix / blocked respectively.


## 2040 native follow-up: isolated WebView, 2026-10-20

### Scope and provenance

The coordinator authorized `make agent-test AGENT_TEST_INSTANCE=at-2040
AGENT_TEST_PORT=19879` in the `chore/desk-2040` worktree at
`ba54df45f6678ef443ec8d9293b3ab31492c6a91`. The native frontend and Rust build
used the target's build-slot/mbx wrappers. The installed app and Boss's running
instance were not changed. No trusted native input was authorized.

TCP `/health` initially returned the Basic Auth challenge. The coordinator then
authorized the instance's own Unix socket. All subsequent workload and debug
requests used that socket, with no login, token-file read or credential copy:
`~/Gits/.tmp/tuic-at/71f77a801497f18a/tuic-mcp-71f77a801497f18a.sock`.

Raw evidence and the executable measurement scripts are retained in
`~/Gits/.tmp/desk-2040-native/`. The main files are `native-loaded.json`,
`native-loaded-backend.json`, `native-loaded-frame-workload.json`,
`native-loaded-*-output.json`, `summary.json`, `measure.js`, `run-measure.py`,
`eval.mjs` and `agent-test-socket.log`. A preceding ten-key, one-session smoke
is retained separately as `smoke*`; its samples are not pooled with the main run.

### Workload and observation

One throwaway PTY ran the unchanged `scripts/stall-workload/output.py` generator:
12 source-like rows per tick, a requested 100 ms pause, two-second startup pause
and default 15-second output window. This is paced synthetic output, not a
historical agent stall or an ego `/help` run. The session was selected through
its normal tab click handler. The terminal canvas had a nonzero 862 by 660
CSS-pixel rectangle, 1724 by 1320 device pixels, DPR 2.

The WebView reported `visibilityState=hidden` throughout both measurement
windows. No visibility override, focus workaround or native input was used.
The initial inspection before measurement had reported `visible`; a nonzero
canvas rectangle did not establish visible document scheduling later.

The main run dispatched 15 untrusted `KeyboardEvent` objects through the actual
terminal input element, using distinct circled-number characters. Its intended
150 ms pauses were delayed, with an observed maximum dispatch gap of 3747 ms.
An independent Python observer polled the session output over the Unix socket,
with a 20 ms pause after each completed request. It recorded the first response
containing each character. This gives a **JS dispatch to backend-output
observation upper bound**, not backend read time, IPC duration, paint latency
or trusted keyboard latency. It includes observer scheduling and request cost.
Both clocks are from the same Mac; the WebView timestamps use
`performance.timeOrigin + performance.now()` and the observer uses wall time.

### Main-run results

All durations below are milliseconds; quantiles use nearest rank.

| Metric | n | p50 | p95 | p99 | max |
| --- | ---: | ---: | ---: | ---: | ---: |
| JS dispatch to backend echo observation, matched samples only | 11 | 243.959 | 811.406 | 811.406 | 811.406 |
| 100 ms WebView timer gap | 16 | 671 | 3480 | 3480 | 3480 |
| Independent Unix-socket output request | 97 | 36.542 | 587.484 | 3777.371 | 3777.371 |
| Production frame decode | 26 | 0 | 4 | not retained | 50 |
| rAF callback / production paint | 0 | unavailable | unavailable | unavailable | unavailable |

Four dispatched characters (⑧, ⑨, ⑪ and ⑫) were absent from both the final
backend output and the observer's matches. The 11-sample echo distribution is
therefore censored, not a 15/15 pass. The layer responsible for the four absent
characters was not isolated. No production fix or regression claim is based
on this exploratory result.

The observer saw 207537 bytes of output growth over 17.146339 seconds, or
12103.867 bytes/s across that observation window. The final retained output
contains 792 generator row prefixes, spanning steps 0 through 65. The requested
120 rows/s is a generator setting, not a measured achieved rate; scheduling,
startup and observation boundaries prevent treating those two rates as equal.

The draw hook observed zero `fillText` calls on the selected terminal canvas,
and the rAF observer received zero callbacks. The production frame timing hook
also recorded zero paints. The bounded workload recorder retained 17 arrival
events and zero paints, with nine events dropped. Its arrival timestamp is the
frontend frame callback, not the backend PTY read timestamp. There is therefore
**no backend-read-to-WebView-paint measurement** in this run. The observations
are consistent with the known hidden-WebView prerequisite being tracked by
story 2090; they do not isolate a terminal renderer defect.

### Browser comparison and disposition

The earlier Chrome run in `scripts/stall-workload/RESULTS.md` used a visible
1878 by 1260 CSS-pixel terminal, four PTYs and a 30-second output window. Its
active-capture timer-gap p95 was 101.9 ms, rAF-gap p95 was 16.7 ms and maximum
rAF gap was 57.5 ms. Its renderer-only main replay paint p95 was 6.2 ms.
Those are not native keyboard measurements. The older triage numbers above
also measure PTY/application echo, not presentation.

The hidden native run's timer p95 of 3480 ms and absence of rAF/paint cannot be
used as an engine performance ratio against visible Chrome. Geometry, visibility,
workload duration and request/measurement paths differ. Native paint and original
`/help` REALITY/GREEN criteria remain unproved. Next prerequisites are the
coordinator-owned hidden-WebView resolution or an authorized visible observation,
and Boss's decision on trusted input/presentation capture. Do not close 2040 or
claim a pre-fix native RED from these proxy samples.

### Cleanup and validation

All workload sessions were deleted in the driver's `finally` block. The exact
`make agent-test-stop AGENT_TEST_INSTANCE=at-2040 AGENT_TEST_PORT=19879` command
returned `Stop requested for this agent-test instance only`. A subsequent
`ps -axo pid=,command= | grep "$PWD/.tmp/agent-test"` check found no matching
worktree agent-test process. No browser, Vite server or separate backend was
started for this run. No token or account login was used.

This follow-up changes documentation only. No production test, mutation run,
backend edit, installed executable replacement, merge or push was done. The
original story close check still refuses its unproved native GREEN criterion.
