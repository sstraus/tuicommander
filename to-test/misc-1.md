## Long answer rows survive resize in the log (2726-7833) — Rust rebuild/restart required

- [ ] After a manual rebuild/restart when losing current sessions is acceptable, ask Claude Code for an answer taller than the viewport and shrink the terminal height (and toggle a side panel to halve its width) while it streams. `session action=output format=log` must keep every answer row once. Rust does not hot-reload; no daily instance was restarted. _(FAILED 2026-11-05: repro (plain shell, throwaway PTY, not Claude): ./rows.sh prints ROW001..150 at 0.05s; resize 120c->10 rows, ->60c, ->30 rows/120c while streaming; GET /sessions/<id>/output?format=log lines show ROW102-ROW104 at idx 117-121 (cols 60) AND again at idx 166-168 (cols 120); ROW121-122 at 155-157 and 185-186: rows 102-122 logged twice. Other runs (height-only, 8 resizes) had each row once. Claude answer not tried (agent run).)_

## Early wake acknowledgement (2610-9b43) — Rust rebuild/restart required

- [ ] After a manual rebuild/restart when losing current sessions is acceptable, use an isolated voice conversation with a configured phrase. Unrelated speech must stay **Listening**. Say the phrase and continue speaking: a recognized prefix should show **Go ahead** before final **Transcribing**. Repeat from a browser-owned microphone. Record real wake-to-display latency and the matching `prefix_ms`/`inference_ms` log; deterministic tests do not establish acoustic accuracy or native latency. The daily instance was not restarted; Rust does not hot-reload. _(HUMAN 2026-11-05: needs mic + latency. Code: continuous.rs:412,430,954-967 wake_confirmed, :'prefix_ms' log field exists; DictationToast handsFreePhaseLabel.ts:11; wake tests PASS (tuic-dictation run).)_
## Shared globe artwork (2602-2e4d)

- [ ] Inspect the global-workspace globe at default and enlarged sizes in light and dark themes. Confirm symmetric openings, a continuous rim and a clear equator. Headless geometry and before/after crops cover the shared component. _(HUMAN 2026-11-05: visual. Code: components/GlobeIcon.tsx:4-8, test VisualIconGeometry.test.ts:55-59 is skipped without TUIC_TEST_CHROME (skipped in my vitest run); browser-mode attempt via shared Chrome only reached 'Server unreachable' screen.)_

## Visual identity artwork (2601-0477)

- [ ] Inspect Droid and OpenCode beside Claude in sidebar, Settings agent tiles and mobile session cards. Confirm equal apparent height, no clipped artwork and theme-inherited color. Headless before/after crops are retained in `visual-audit-20`; no daily desktop interaction was used. _(HUMAN 2026-11-05: visual. Code: components/ui/AgentIcon.tsx:35-56; geometry test VisualIconGeometry.test.ts:44-50 skipped without TUIC_TEST_CHROME (my vitest run); Settings/mobile rendering not seen.)_
## Readable secondary and muted text (2506-22b0) — backend rebuild/restart required

## CLI return focus (2484-9adb)

- [ ] After landing, check Chat → CLI by click in the desktop WKWebView and type without clicking the terminal. Worktree Chrome verified the real Terminal and CanvasTerminal focus plus typed write_pty data through an isolated IPC sink; the assignment did not launch a desktop instance, so WebKit-specific focus behavior is not verified. _(HUMAN 2026-11-05: needs desktop WKWebView focus. Code: components/Terminal/Terminal.tsx:244-247 canvasTerminalRef()?.focus(); TerminalChatView.test.tsx.)_

## Event-run publication prerequisites (2428-cd8d) — Rust restart required

- [ ] After Boss manually restarts a rebuilt `make dev` or installs `make build`, confirm completed allowlisted event runs still open draft PRs and report prerequisite failures. Rust does not hot-reload; this peer did not restart the daily app or write to GitHub. _(HUMAN 2026-11-05: needs real draft-PR publication on GitHub (external write prohibited); code: automations/publish.rs:24 draft PR creation, tests PASS (L360).)_
## Development backend command mismatch (2409-aa48)

- [ ] Visual check of the AI Chat mismatch message and absent Retry. An isolated browser fixture mounted the real component, but Chrome did not produce a screenshot before the 45-second bound; its own process and Vite server were stopped. Log: `~/Gits/.tmp/results/tuic-2409-visual.log`. No visual pass is claimed. _(HUMAN 2026-11-05: visual only. Code utils/backendMismatch.ts:1 + AIChatPanel.test.tsx:400 asserts text "Backend/frontend version mismatch", "Restart make dev", command name, and no Retry button (PASS). No screenshot taken.)_

## Complete labelled issue intake (2415-dccf) — Rust restart required

- [ ] After an authorized `make dev` restart or `make build` (or rebuilding and restarting `tuic-remote`), confirm an allowlisted labelled open issue outside the latest 30 unfiltered issues can start a run. Rust does not hot-reload; the daily app was not restarted. _(HUMAN 2026-11-05: needs a real GitHub repo with >30 newer issues and allowlisted label (external service). Code-verified: github.rs:695-708 intake query independent of the first:30 unfiltered section (:684) and cursor paging (test github.rs:1032-1043).)_
## App Nap opt-out (1638-c94c) — Rust restart required

- [ ] After Boss manually restarts a rebuilt backend, check the log line `App Nap disabled for the process lifetime` (source `app-nap`). Cover the TUIC window with a full-screen app while no agent is busy, wait two minutes, and check that `/logs` contains no `Sleep-wake detected` or `Frontend unresponsive` line for that period and that remote clients stay live. This Rust change does not hot-reload into the daily app. _(HUMAN 2026-11-05: live log (daily tuic.log.2026-11-04): "2026-11-04T22:50:13 App Nap disabled for the process lifetime source=\"app-nap\"" present (app_nap.rs:33); 0 "Frontend unresponsive" after that restart, but 7 "Sleep-wake detected" lines (not attributable). Full-screen occlusion 2-minute test needs the real window.)_
## Bounded input frame priority (2040-16b2) — Rust restart required

- [ ] After a manual rebuild/restart, check repeated input while output races frame delivery. Newer applied output must retain its bounded retry; this experimental branch trials four credits per 100 ms, with unchanged 8 ms spacing, receipt gates and synchronized redraws. Do not load this trial in the daily app: the completed A/B/A/B series improves idle p95 but is NOT LANDABLE under the strict all-statistic rule because B1 calibration rAF max is 40 ms versus A 26/36 ms; see `reports/tuicommander/2026-10-23-four-send-quota-trial.md`. Native idle/help proxy results and limits are in the browser-input measurement report; this is not a one-frame guarantee. _(HUMAN 2026-11-05: experimental trial marked NOT LANDABLE and "do not load in daily app"; code: nextest pty::frame_priority::tests::retained_send_retry_preserves_spacing_quota_and_expiry PASS. Needs input-latency measurement in native WebKit.)_

- [ ] After Boss manually restarts a rebuilt `make dev` or installs `make build`, type while a disposable terminal prints paced output. Check desktop and browser/PWA responsiveness, sustained output, and DEC-2026 complete redraws. This Rust change does not hot-reload into the running daily app. Isolated native trace timing is an untrusted-input-to-`fillText` proxy, not trusted keyboard or compositor proof; the original native REALITY criterion remains separate. _(HUMAN 2026-11-05: needs typing during paced terminal output on desktop/PWA with DEC-2026; nextest pty::frame_priority::tests::retained_send_retry_preserves_spacing_quota_and_expiry PASS (pty/frame_priority.rs). Not measurable here.)_

## Paired native plugin refresh (2039-428c) — rebuild required

- [ ] After Boss manually restarts a rebuilt backend or installs a signed package, verify notification permission and delivery, then the updater check/download/install flow and deferred relaunch. Rust/npm notification 2.7.0 and updater 2.14.0 are paired; Linux default-feature compilation and targeted tests pass, but macOS/Windows packaging and real OS integration still need release validation. This peer did not restart or replace the running app. _(HUMAN 2026-11-05: needs real OS notification permission/delivery and signed updater install/relaunch. Nothing code-verifiable beyond dependency pair; not run.)_

## Canvas base-grid repaint reductions (2040-16b2) — load landed frontend

- [ ] With the landed frontend in native WebKit, verify typing stays responsive during scrolling agent output and record keyboard-to-paint latency. Check normal history growth, a stationary spinner, colored trailing blanks, wide glyphs, combining marks and marker-filtered output. Worktree browser replay proves lower drawing-operation counts and identical reference pixels, not native timing or the ego printer's independent row-emission delay. No Rust restart is needed for these frontend-only changes; do not restart a live desktop merely for this check. _(HUMAN 2026-11-05: needs native WebKit keyboard-to-paint latency; frontend-only per item. Not code-verifiable beyond replay counts.)_
## Declaration into a newly discovered workspace (2044-31f3) — Rust restart required

- [ ] After Boss restarts the Rust backend, use an isolated instance and declare an existing linked worktree whose saved workspace row has no `savedTerminals`. Confirm declaration succeeds, the caller restore record moves without duplicating sibling records, and the caller tab appears under the declared workspace despite unchanged cwd. Repeat the declaration, cd into another repository or an unregistered directory, reload the frontend and send a cwd update; placement must persist without moving sibling tabs. The peer did not restart or mutate the installed instance; live sidebar verification is still pending. _(HUMAN 2026-11-05: needs isolated instance with a saved workspace row lacking savedTerminals and cd/reload placement; no unit test for that scenario identified.)_

## Durable peer-mail subscriptions (2004-4760) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, create two named actionable readers for an isolated peer. Send idle lifecycle traffic, then RESULT and BLOCKED mail; confirm each reader receives the result independently, explicit ack prevents reconnect replay, and an interrupted unacknowledged batch remains available. Check the HTTP SSE stream and ACP resource notification path. No installed binary or live desktop instance was replaced by this peer. _(NOT VERIFIED 2026-11-05: Isolated peers/SSE not run. mail_subscriptions tests (incl. actionable_subscription_overflow...) pass; mail_subscriptions.rs:38 ack/offered/batch_end columns.)_
## Generated launch modules — Rust restart required

- [ ] After Boss restarts the backend, launch a root and a spawned child. Inspect the delivered launch text: both get enabled modules and global then agent additions; only the child gets Children and omits suggest. Turn all off and verify a new launch has no TUIC playbook. Existing agent processes are not updated by hot reload. _(NOT VERIFIED 2026-11-05: Delivered-text check needs root+child launch; launch_instructions_tests (composition per agent) pass; Settings shows Children module OFF in Boss config, so a child would not get it today.)_

## Launch instruction preview critic fixes — Rust restart required

- [ ] After Boss restarts `make dev` or installs `make build`, verify launch instruction previews on a daemon-backed machine. Switch from a loaded local machine to a delayed or unavailable remote: inherited text must not carry over and Customize must stay disabled until that machine's preview arrives. The running desktop backend was not restarted. _(NOT VERIFIED 2026-11-05: Needs delayed/unavailable remote daemon; only configured remote is Mac-mint, shown Offline in status bar; switching machine in Settings would not give a loaded daemon-backed preview. launch_instructions.rs:25 preview metadata read.)_
## Run-owned launch timeout (2006-f5a1) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, use an isolated run-owned child stalled before its composer is ready. Read its durable launch receipt after 30 seconds: it must report blocked with a delivery-timeout reason and retain one queued prompt and the same child identity. No live desktop restart was performed by this peer. _(NOT VERIFIED 2026-11-05: Needs stalled run-owned child 30s. prompt_receipt tests pass (in 94-pass run); prompt_receipt.rs.)_
## Guided event configuration (2012-72ff)

- [ ] On a throwaway headless owner or browser-mode instance, send each Configure Smart Prompt to a test agent and verify the step-by-step interview stops on missing owner/profile/GitHub auth/skills and unsupported intake, without saving or launching work. Automated library tests verify the delivered prompt contract, not model compliance. _(NOT VERIFIED 2026-11-05: Needs test agent given Configure Smart Prompt; model-compliance. Entry settingsSearchIndex.ts:764 only.)_
- [ ] After beta, with Boss-approved repository/test issue and authorization on the owner, run one labelled-issue-to-draft-PR pilot. Retain independent reviewer/check receipts on the published commit, the real draft PR URL and reconciliation evidence. No live GitHub publication or credentials were used by this change. _(HUMAN 2026-11-05: Needs approved repo + GitHub authorization/draft PR; forbidden (no external publication).)_

## Continuous answer gutter (1998-d826)

- [ ] [VISUAL] Capture the worktree CLI gutter for the real Claude answer fixture with blank rows, section headings and marked list items; confirm one continuous line and no line on trailing unmarked prose or tool output. Both headless Chrome capture attempts failed to produce a PNG; the bounded retry stopped its own Chrome/Vite processes after 35 seconds. The production-plan regression passes, but it is not screenshot evidence. No Rust change or backend restart is required. _(NOT VERIFIED 2026-11-05: [VISUAL] screenshot not taken; ContentRenderer.tsx:66 gutter ranges for list items.)_

## Desktop standby input recovery (1986-1236) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, let an isolated plain-shell terminal enter standby after its foreground child exits. Confirm desktop typing wakes it and executes the command without switching tabs again. Repeat by viewing the terminal and by sending from the phone. The running backend was not restarted; the real-PTY regression is not evidence that Boss's current binary loaded this fix. _(NOT VERIFIED 2026-11-05: Standby shell needs foreground child exit + idle timer; pty/lifecycle.rs:1086 standby marker; not reproduced.)_

## Managed injection recovery (1957-38c1) — Rust restart required

- [ ] Run a full fake-agent delayed-echo E2E in an isolated headless test instance with a real PTY. Delay the fake agent's composer echo after Enter to force uncertain injection, queue a later BG DONE wake, then render a fresh stable empty ego composer. Verify the later wake reaches the fake agent once without replaying the earlier uncertain payload, raw input returns a bounded uncertainty error while the reader is stalled, and close terminates the stalled session. Use the recorded ego composer fixtures for screen semantics; the existing recording-writer regression is not this process-level E2E. Do not use Boss's instance or sessions. This check remains unverified. _(NOT VERIFIED 2026-11-05: Needs isolated headless E2E; pty/injection_tests.rs exist (urgent/bg filters pass).)_
- [ ] After Boss manually restarts `make dev` or installs `make build`, verify a later urgent send is not permanently suppressed by an abandoned notice reservation. The 30-second reservation lease does not replay the earlier payload. Also verify raw input returns an uncertainty error rather than hanging on a stalled reader, close still terminates that session, and a fresh stable empty ego composer releases a later queued BG wake without replaying earlier input. No live desktop was restarted by this peer. _(NOT VERIFIED 2026-11-05: Same; injection tests pass; no live reservation lease probe.)_
## App setting choices (1915-d77f) — Rust restart required

- [ ] After Boss manually restarts, use an isolated instance. Check bundled font choices and a custom theme agree with Settings UI. Explicitly refresh installed IDE choices and check their labels/availability on macOS, Windows and Linux. Describe must not launch discovery. No desktop instance or real user config was used by the agent. _(NOT VERIFIED 2026-11-05: Isolated instance + IDE discovery on 3 OSes.)_

## Persistent stall evidence (1638-c94c) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, retain the journal files after the next real desktop stall and build a metadata-derived replay input with `scripts/build-stall-replay.mjs`. Record independent backend-request timing separately. Targeted tests cover retention, rotation and numeric-only records; original real-stall RED/GREEN remain open. No managed peer restarts or stresses the live app. _(NOT VERIFIED 2026-11-05: Needs a real stall.)_

## Orphan removal threading (1638-c94c) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, remove a throwaway orphan worktree through the desktop cleanup UI. Record desktop heartbeat continuity and independent backend-request timings during removal. The module-boundary regression covers async/blocking-pool placement, not a live heartbeat replay. No second desktop instance was launched. _(NOT VERIFIED 2026-11-05: Needs throwaway orphan worktree via desktop UI (no native clicks).)_

## Suspend replacement identity (1806-ed18) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, suspend and resume a throwaway idle shell twice. The TUIC identity stays stable, each Resume has a fresh PTY id, and the second Suspend succeeds. On a remote repository, Resume and closing a suspended tab must act on that daemon. Automated targeted tests cover the lifecycle and routing; the running desktop requires a restart to load the Rust change. _(NOT VERIFIED 2026-11-05: Suspend/resume tab UI not driven; pty/injection.rs:1874 suspend refusal tests pass.)_
## Shell wrapper argument guards — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, open a new shell. With OpenCode native scrollback enabled, verify `opencode mcp list` receives no `--mini`; a bare interactive launch still does. Verify Goose `run`/`session` with `--name=work`, `-nwork`, `--path f.jsonl`, `--path=f.jsonl`, and `run --no-session` receive no TUIC `--name`. Existing shells keep their sourced wrappers until reopened. Automated argv tests cover bash, zsh and fish. _(NOT VERIFIED 2026-11-05: Needs new shell with opencode/goose installed; argv tests not run by me.)_

## One-time beta channel migration (1817-beab) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build` with a beta version, an old config with `update_channel=stable` and no `beta_channel_migrated` marker switches to Beta once. Selecting Stable afterwards survives restarts. Use a throwaway isolated instance; stable builds must preserve the saved channel. Targeted Rust tests cover the load/save migration; the running desktop needs a rebuild/restart to load it. _(NOT VERIFIED 2026-11-05: Isolated beta instance; beta_channel tests pass (in run); config.rs beta_channel_migrated.)_

## Continuous answer regions (1872-1870)

# To Test

- [ ] [HUMAN] Mobile PWA bottom band (#2116-0213): remove the old iPhone Home Screen install and add it again from updated Safari content to load the non-overlay status-bar metadata. Check keyboard closed/open/reopened, composer taps, Home Indicator interaction and background reaching the screen bottom. Keep the approved CLI bottom padding at 4px. iPhone 17 Pro / iOS 26.4 simulator screenshots and XCUITest confirm the band is removed and the composer accepts taps/draft text, including after keyboard dismissal through session navigation. Physical-phone ergonomics remain unverified. If a band remains, capture the device screen, device/iOS, orientation and keyboard state before reloading. _(HUMAN 2026-11-05: iPhone PWA bottom band.)_

## Work-item page total (1845-66f6) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, `WorkItemPage.total` / `WorkItemGroupedPage.total` are accepted by the Rust DTOs (optional, defaulted). Targeted `plugin_docs` and `work_items` tests pass; the Tasks list shows "N of total" once a loaded GitLab plugin build that sends `x-total` is installed (plugin branch parked, see story worklog). _(triage 2026-10-14: WAITS — Needs the named integration/plugin release to land and load before the consumer check.)_ _(NOT VERIFIED 2026-11-05: WAITS on GitLab plugin release; work_items/projections.rs total field exists.)_
## Screen stays awake while dictating (1767-3071)

- [ ] [HUMAN] On the iPhone home-screen PWA (iOS 18.4 or later), dictate into the draft, and separately run hands-free, for longer than the Auto-Lock time. The screen must stay on and the recording must continue; after Stop it must lock as usual. Turn "Keep screen awake while dictating" off in mobile Settings and confirm the screen locks again. Before iOS 18.4 the API does not work in home-screen web apps ([WebKit bug 254545](https://bugs.webkit.org/show_bug.cgi?id=254545)), so a failure on 18.3 or earlier is expected. Vitest covers acquire, release, re-acquire and the setting; the device is unverified. _(audit 2026-10-13: HUMAN — iPhone.)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_ _(HUMAN 2026-11-05: iPhone Auto-Lock; screenWakeLock.test.ts passes.)_
## Commit failing checks (1712-b11c) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, select a failing external check in a throwaway PR and confirm auto-heal uses its exact head and detail link. Confirm a successful rerun removes the stale failure. The lookup uses the bound repository account over GraphQL and does not depend on the first 100 open PRs. Recorded GitHub fixtures cover CheckRuns, external StatusContexts and pagination on rb; live account requests remain unverified. No desktop instance was started or restarted by this peer. _(audit 2026-10-13: HUMAN — Needs throwaway PR/live GitHub write.)_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_ _(HUMAN 2026-11-05: Throwaway PR/live GitHub write.)_
## Plugin account requests (1697-4cdc) — Rust restart required

- [ ] After Boss restarts `make dev` or installs `make build`: install a plugin whose manifest declares `"capabilities":["net:http"]` and a `providers` descriptor, confirm the provider appears in the account add flow, add a throwaway account, and call `host.httpFetch(<site>/<path>, { account })` from the plugin. The response arrives without the token; a URL outside the site and a plugin without that provider are refused. Uninstall the plugin and confirm the account remains listed. Local-server tests on rb cover injection, host rule, consent, redirects and redaction; a real Jira/GitLab/GitHub call is not verified. No desktop instance was launched by this peer. _(NOT VERIFIED 2026-10-12: needs the desktop WebView/Tauri UI, which a headless tuic-remote does not have, and this lane may not launch a desktop. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: HUMAN — Needs account add flow (token) + plugin install. Partial: rb plugin_http::account_tests (7) passed.)_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_ _(HUMAN 2026-11-05: Plugin install + account token.)_
## Core provider accounts (1679-1419) — Rust restart required

- [ ] After landing and Boss's manual `make dev` restart or `make build`, list accounts through `POST /accounts/action`; confirm existing GitHub IDs and repo bindings, plus presence-only credential status. OAuth success replies must omit `access_token`. No app was launched, restarted or killed by this peer. _(audit 2026-10-13: HUMAN — List would probe credential presence (keychain risk); HTTP auth blocked; add flow needs a token. rb not run for accounts.)_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_ _(NOT VERIFIED 2026-11-05: List probes credential presence/keychain; HTTP /accounts/action not called (credentials rule).)_
- [ ] After A2/A3 integration, use the private native add flow for a registered Jira/GitLab token provider, Test it, then remove a throwaway account/site. A1 tests the generic descriptor protocol on rb; live provider auth and native secret-entry privacy are not claimed. _(NOT VERIFIED 2026-10-12: needs the desktop WebView/Tauri UI, which a headless tuic-remote does not have, and this lane may not launch a desktop. Isolated headless tuic-remote built from branch chore/1706-a443-to-test-runs (main 67ccd72c3) and run on the rb box; no desktop, no Boss instance.)_ _(audit 2026-10-13: HUMAN — List would probe credential presence (keychain risk); HTTP auth blocked; add flow needs a token. rb not run for accounts.)_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_ _(HUMAN 2026-11-05: Native private add flow with token.)_

## Crate diet stage B (1648-2c54) — Rust restart required

## Confirmed dirty orphan removal (2926-2a6b) — Rust restart required

- [ ] After a manual `make dev` restart, review and remove a throwaway detached worktree with an untracked file. Confirm removal succeeds; separately confirm a failed removal displays its error without reopening the same review.

- [ ] After choosing to restart/rebuild, use a named isolated `make agent-test AGENT_TEST_HEAP_PROFILE=1` and follow `docs/backend/heap-profiling.md`; verify profiler status, two symbolized captures and separate backend/WebContent footprint. Normal builds retain system allocation. Rust does not hot-reload; no daily instance was restarted.
