## Planner Ideas (#2295-abd3) (continued)
- [ ] Markdown selection to AI Chat (#2361-8099): on a real iPhone, adjust the native selection handles inside a table cell, tap Send to AI Chat, and confirm the draft is focused without sending. Browser-mode automated selection and source-row checks cover the shared path. _(HUMAN 2026-11-05: real iPhone native selection handles. Code seen: markdown send-to-chat path exercised by FilesScreen.send.test.tsx (passes); no device.)_

- [ ] After the next manual Rust rebuild/restart, verify Windows worktree cleanup uses its selected Git installation launcher and a forty-worktree MCP listing remains responsive while fresh dirty fingerprints change (#2310-6e76). The daily backend was not restarted; Rust changes do not hot-reload. _(HUMAN 2026-11-05: needs Windows + Git installation launcher; code check: forty-worktree MCP listing not run. Not reachable on macOS under rules.)_

- [ ] After a manual backend restart, select AI Chat conversations on desktop and iPad in both directions, then reconnect the iPad and confirm it catches up. Confirm the session CLI | Chat choice does not follow. Rust selection changes require `make dev` restart or a rebuilt release (#2403-e711). _(HUMAN 2026-11-05: needs desktop + iPad (second device) reconnect catch-up. Code/tests: ai_chat_selection.rs:13,30,59 (read/select/publish) nextest ai_chat_selection 2 pass; CLI|Chat choice is separate UI state (not touched by select).)_

## Reviewed event issue publication (#2325-10c4)

- [ ] After a manual `make dev` restart or release rebuild, configure an allowlisted sandbox event with a published story workflow revision and passing deterministic checks. Verify native story/run links in automation history, independent review, draft findings and stale-push refusal. No live GitHub publication was authorized in the implementation assignment. _(HUMAN 2026-11-05: needs authorized live GitHub sandbox publication. Code-verified: automations/store/tests.rs:566 two_pushes_publish_one_review_for_latest_head; completion/tests.rs:437,498,526 publication tests; all pass in automations:: batch.)_
## Workspace Delete alternative (#2446-2de7)

Isolated Chrome screenshot capture timed out after 35 seconds; visual rendering is not yet verified. Focused component, coordinator and backend tests pass.

- [ ] Event outcomes (#2326-98f1): after a manual Rust restart, inspect a permitted event run through `automations` run details; verify the PR URL and decision persist after restart and unknown publication does not create a second PR. No live GitHub write or desktop launch was performed in this implementation. _(HUMAN 2026-11-05: needs live GitHub write/PR. Code-verified: automations/completion/tests.rs:526 publication_refusal_is_durable..., store tests; automations:: batch 244/244 pass (loopback mocks). Restart persistence only via store tests.)_

## App log fixes (2568-a5d5, 2569-b40f)

  native notification bootstrap no longer logs a denied `is_permission_granted`
  command and repository structure loads with `chore/ego-e2e-322-d` present.
  The current running Rust backend and capabilities do not hot-reload.
- [ ] Plugin investigation 2567-ee9c: capture an isolated module-load failure _(NOT VERIFIED 2026-11-05: investigation item (capture an isolated module-load failure), no pass/fail criterion; needs a reproduced reload race. Code seen: plugins/pluginLoader.ts:5 import("plugin://id/main.js"). No reload of the daily app allowed.)_
  with request URL/status and document lifecycle before selecting a fix. The
  retained errors alone do not prove that the merge caused a reload race; all
  twelve plugins were subsequently loaded. No running app reload was performed.

## WF08 post-land proof service

- [ ] Scroll through terminal web links and soft-wrapped file URLs: dotted underlines and Cmd-hover strokes must stay on their text, including when the backend viewport trails a smooth gesture (#2582-13dd). _(HUMAN 2026-11-05: canvas-rendered underlines under smooth scroll are not observable over HTTP/DOM (AGENTS.md "Canvas rendering … needs a visual check"). Not code-verified beyond existence.)_

## Automation completion tab policy (#2873-4635)

- [ ] After a manual `make dev` restart or release rebuild, enable **Close the run tab after confirmed completion** on a throwaway automation. A positively completed run closes only its own tab through the existing countdown; default-off, failed, unknown and input-blocked runs stay open. Worktrees and sibling tabs stay intact. Rust does not hot-reload; the daily app was not restarted. Targeted backend and UI evidence is recorded in the story worklog.

## Sidebar H view (#2916-87da)

- [ ] With one live agent tab on a branch, a shell-only tab on another and an idle repo, click **H** in the toolbar: only the agent branch (with its repo and group) remains; end the agent and the branch disappears with no refresh; with nothing live the empty state offers **Show all**. Layout and badges of the kept rows match the unfiltered sidebar (visual check; not driven through a real backend by the author).

## Chat full text (#2917-a225)

- [ ] After a manual `make dev` restart, in a Claude terminal paste a prompt over 20 000 characters, open Chat view (desktop, then mobile): the message ends in `… [truncated, N chars total]` and shows **Show full text (N chars)**; the button opens a scrollable panel with all N characters (scroll to the end), **Close** returns. Rust does not hot-reload; not driven through a real backend by the author.

## Smart prompt base branch (#2932-f821)

- [ ] After a manual `make dev` restart or release rebuild, run **Merge Main Into Branch** from the toolbar and Git branches panel in a throwaway feature checkout whose base exists only on origin and whose `origin/HEAD` names that base. The prompt must name the remote default and feature branch without an unresolved-variable refusal. Set an explicit repository base and confirm it takes priority. Rust does not hot-reload; the daily app was not restarted.

## Idle disk traffic (#2922-f66c, #2923-42ab)

- [ ] After a manual `make dev` restart, let the app idle with several agents for 5 minutes, then run `python3 -I ~/Gits/.tmp/disk/shm-cycles.py 30` and read the process disk I/O twice 60 s apart (`proc_pid_rusage` written/read bytes). Before the fixes: about 83 SQLite open/close cycles a second, about 2 MB/s written, about 1.3 MB/s read. Expect cycles near 0 and writes far under 300 KB/s; if writes stay above that, open a new story from the measurement.

## CLI diff removal (#2940-793a)

- [ ] After the next manual CLI rebuild/install, confirm `tuic --help` no longer lists `diff`. The obsolete comparison command was removed; file opening and tmux attach remain. The installed CLI was not replaced by this implementation.
## Explicit CLI instance precedence (#2958-e0cf)

- [ ] After rebuilding and installing the CLI sidecar, run a read-only `tuic --instance <isolated-id> ls --json` with a different inherited `TUIC_SOCKET`. Verify that stderr names the isolated socket and the response comes from that instance. Do not use destructive actions. The installed CLI was not replaced during implementation; source CLI integration tests passed on rb (four targeted tests). This change does not hot-reload an already installed CLI.

## Shared provider quota store (#2991-e0b0)

- [ ] After the next manual Rust rebuild/restart, open two sessions bound to different Claude or Codex credentials. Switch between them and check that each status-bar quota follows its subscription without a provider read. Open mobile session usage and the matching dashboard; check that the same quota windows appear. Close a bound session and check that its binding retracts. The daily backend was not restarted; Rust changes require a manual `make dev` restart or a rebuilt release. Grok's billing dashboard is unchanged pending 2999-998f.

## Agent tab usage badge (#3068-c03b)

- [ ] After this frontend change lands on the main checkout (Vite HMR, no Rust restart), focus a Claude tab, a Codex tab and an ego tab bound to an anthropic or openai-codex subscription. Check that the agent badge shows the window percentage (for example `7d: 78%`) and that the ego badge uses the provider icon. Switch from Claude to Codex and check that the Claude percentage never shows on the Codex tab. Vitest covers the rule; the live check could not run from the worktree because the running frontend is the main checkout's Vite server.
## Agent badge shows registry usage (#3068-c03b)

- [ ] Frontend part (hot-reloads once on main): on a Claude tab and on an ego tab bound to anthropic, the status-bar agent badge text holds the 7d percentage (live 2026-11-15: registry had Claude 78%, ego/anthropic 77%, but no badge showed it). Check with `debug invoke_js` that `[class*="agentBadge"] [class*="agentUsage"]` contains the percentage.
- [ ] Rust part, after a manual `make dev` restart: a Codex tab launched by TUIC (argv carries `-c projects=… -c check_for_update_on_startup=false -c developer_instructions=…`) appears in `get_provider_quotas` under `codex-cli/openai-codex` with its session id, and its badge shows the percentage. Before the fix the registry had no Codex view at all, because `codex_profile` refused every `-c` override.
- [ ] After this frontend change lands on the main checkout (Vite HMR, no Rust restart), focus a Claude tab, a Codex tab and an ego tab bound to an anthropic or openai-codex subscription. Check that the agent badge shows the weekly percent with its reset and the monthly remaining (for example `7d: 78% -4d · mo: 99% left`; Codex/ego-Codex `credits: N left`), no tool name, and that it does not flicker across refreshes and that the ego badge uses the provider icon. Switch from Claude to Codex and check that the Claude percentage never shows on the Codex tab. Vitest covers the rule; the live check could not run from the worktree because the running frontend is the main checkout's Vite server.

- [ ] **Linux ARM64 desktop 2.1.2 (3084-a4ba):** On the produced native Ubuntu
  22.04 ARM64 package, record OS/architecture, artifact SHA256 and package metadata;
  install it and open a desktop window with a working local terminal. Check ARM64
  updater selection and CPU dictation without a usable Vulkan GPU. Requires an
  actual ARM64 Linux desktop/display and the final native CI artifacts.
  Native initial-desktop evidence now exists (2026-11-18) for the preserved
  unsigned internal 2.1.1 DEB on Ubuntu 22.04.5, RPM on Fedora 44 and original
  AppImage extract-and-run on Ubuntu 22.04.5. Each displayed the app and confirmed
  actual WebView DOM readiness; this does not verify a working local terminal,
  dictation, updater signatures/selection or default FUSE launch. Keep this item
  open until the signed 2.1.2 candidate passes. See `docs/release-checklist.md`.

- [ ] After a manual backend rebuild/restart, inspect `progress` in full and
  collapsed MCP discovery. Check that it distinguishes finished outcomes from
  questions that stop work, still asks for a chat reply, and exposes optional
  Yes/No metadata without making discovery of the tool a prerequisite.

## Story backend selection after rebuild (2026-11-18)

Install/update Stories Ticker 1.1.0 only after rebuilding the matching 2.1.2 host; old hosts do not enforce plugin `minAppVersion` and cannot load the new `stories:read` capability. The daily app and installed plugin were left untouched.

- [ ] After a manual backend rebuild/restart, open Plans and Stories for an automatically detected project with an already approved installed Wiz connector and unfinished native plans. The compact header disclosure beside Workflow must say Wiz; All plans must be selected by default, with optional plan filtering. Its open, non-archived All plans view must show the same open stories as the Stories ticker (no story-type filter). Choose native to confirm the old native records remain, then restore automatic selection. This changes the displayed backend; do not switch during a native workflow run.

## Contextual Progress replies (2026-11-18)

- [ ] After a manual backend rebuild/restart, report from two different agents and reply from the toast and journal while another repository stays selected. Confirm delivery to each original author, inline Reply with no blank closed row, shared drafts, and explicit Yes/No answers. Rust changes do not load through frontend hot reload.

## Global scratchpad (2026-11-21)

- [ ] After a manual backend rebuild/restart, click the notepad icon in the toolbar. An editor tab `scratchpad.md` opens, pinned. Write a line, save, switch to another repository: the same tab and text show there. Close and reopen it from the toolbar: the text is still there. Rust changes do not load through frontend hot reload.
- [ ] In the scratchpad, select a block with the mouse and with Shift+arrows. A bar appears under the selection: **Add to Ideas** adds it as a global idea with a toast; **Send to agent** sends it to the active agent terminal and is disabled with no terminal session. The bar does not appear in other editor tabs.

## MCP batch tool (2026-11-21)

- [ ] After a manual backend rebuild/restart and an MCP reconnect, an agent lists `batch` among the TUIC tools (direct and collapsed surface). Call `batch` with `calls=[{tool:"session",arguments:{action:"list"}},{tool:"repo",arguments:{action:"list"}}]`: `results` holds both replies in order and `errors` is 0. Rust changes do not load through frontend hot reload.

## Release review P2 fixes (2026-11-22)

- [ ] After a manual backend rebuild/restart, ask a Discussion side question on a Grok terminal, then on a second Grok session: both questions and answers appear in the journal. Rust changes do not load through frontend hot reload.
- [ ] After the restart, a saved tab that has not been reopened for more than 30 days keeps its operator capability when it is restored.
- [ ] In the Progress dialog, open **All projects** and click a row: the dialog leaves the all-projects view and shows that project's entries.

## Provider gateway — Rust restart required

The gateway is Rust code. It does not load through hot reload: do these checks after a manual `make dev` restart (or `make build`). Design and code map: `docs/backend/provider-gateway.md`. Read the traffic with `curl http://localhost:9876/diagnostics/provider-gateway` (`?reset=true` starts a new window) and the log with `GET http://localhost:9876/logs?source=gateway`.

### Phase 0 — request storm stopped (stories 3120 to 3123)

- [ ] Reset the gateway stats, open Tasks on a GitHub repository and keep the list visible for 5 min without input. The stats show at most one collection (2 search calls). Each explicit Refresh adds one. A preference save in a second window does not reload the list.
- [ ] Make a Tasks load fail (go offline): the error stays visible, and the view does not retry by itself.

### Phase 3 — core GitHub client (stories 3135 to 3138, 3150)

- [ ] `provider_cache.sqlite3` exists in the app data dir (mode 0600 on unix), and the log has no "provider cache unavailable" warning.
- [ ] The stats return `counters`, `recent` and `buckets`. `?reset=true` empties `counters` on the next read while `buckets` stays. MCP `debug action=gateway` returns the same shape.
- [ ] GitHub traffic (poller, PR panel, issue close/reopen, CI logs) is in the stats with callers of the form `<module>.<fn>` (`github.poll`, `github.issue_close`), never an empty or `test` caller. `provider_cache.sqlite3` fills and the poll cadence still runs. GitHub diagnostics report `circuit_breaker_open: false` while the quota is healthy.
- [ ] A REST `GET` that repeats inside its TTL (30 s to 60 s for lists and items) counts as `cacheHits`, not `sent`; a read after a write is `sent` again. GraphQL calls always count as `sent`. A fresh GitHub login clears an earlier rate-limit hold.
- [ ] **A write clears the PR status (3138).** With a repo open and its PRs shown in the sidebar, close or comment on a PR/issue through TUIC (or the plugin). The PR badge for that repo must refresh at once, not after up to 60 s. Another repo's badges must stay.
- [ ] **Signals (3149).** Log out of GitHub (or remove a GitHub account in Settings): no "gateway" error is in the log, the cached entries of that account are dropped (the next read is `sent`), and a PR/issue poll update makes the next list read of that repo `sent`. Unit coverage: `signal_tests::pr_update_event_invalidates_that_repo_lists_only`, `head_changed_invalidates_branch_pr`, `logout_clears_account`.

### Phase 4 — auth, verification and downloads (story 3139)

- [ ] Start a GitHub device-flow login, an account verification, an update check and an ego/mdkb install. The stats list callers `github_auth.*`, `accounts.verify`, `updater.fetch_channel_manifest` and `release_install.download_and_install` with `generic`/`github` providers. A download shows its bytes and no cache hits. Unit coverage: `release_download_is_uncached_and_counted`, `device_flow_poll_honours_slow_down_and_failure_memory`, `no_production_reqwest_send_to_github_hosts_outside_the_gateway`.

### Phase 5 — plugin egress and workflow connectors (stories 3140 to 3142)

- [ ] Open the GitHub Work plugin Tasks view twice within a minute: the second load makes no new GitHub request (caller `plugin` shows `cacheHits`) and the list still renders.
- [ ] A plugin `httpFetch` to a URL outside its `allowedUrls`, a private-IP literal, and an account request with a caller `Authorization` header all still fail with the same messages as before the change.
- [ ] A plugin request to a URL that redirects to a host the plugin did not declare fails with the "require explicit allowedUrls" text; a redirect to a declared host succeeds and the target sees no `Authorization`.
- [ ] Run a workflow that reads the same Jira issue or GitLab MR two times in a minute: caller `neutral_jira.request` or `neutral_gitlab.fetch` shows two `sent` (a `304` also counts as `revalidated`) and no `cacheHits` (workflow reads are always forced fresh). More connector checks: `to-test/workflows-stories-5.md`.

### Phase 6 — typed limits and Refresh (stories 3143, 3144)

- [ ] **Typed limits reach the UI (3143).** With a GitHub account whose `core` or `graphql` bucket is exhausted (or a plugin 429): the Tasks list, board and roadmap show "Rate limited · resets hh:mm" with no Retry button and no automatic reload before that time; stale rows show a muted "cached hh:mm". A merge refused by a limit in the PR popover shows the same reset time, not `rate-limit:{"kind":…}` text. A GitHub primary-limit 403 (x-ratelimit-remaining 0) shows the reset time too. Check placement against STYLE_GUIDE.md by screenshot.
- [ ] **Refresh forces a fresh read (3144).** In Tasks, switch between List, Board and Roadmap within 30 s: a list request that is already loaded shows `cacheHits` and no new `sent`. Click Refresh one time: one new `sent` per list request.
## Prerelease feed without this platform (2026-11-22, #161)

- [ ] After a manual backend rebuild/restart, on the Nightly channel, check for updates while the nightly `latest.json` has no entry for this platform (as when the macOS build fails at notarization): no update is announced. When the entry is present, the newer nightly is announced as before. Rust changes do not load through frontend hot reload.
## Sidebar rich rows and Quiet section (2026-11-22, #3154-de6f)

- [ ] [VISUAL] In R, branches show ⎇, merged branches a purple ⌥; a dirty merged worktree shows a lock in its Merged pill with the reason in the tooltip; hovering still offers Clean. Help > UI legend lists Merged.
- [ ] Click the branch icon of a branch with agents: the agents hide and show; clicking the name selects the branch. Done subagents fold under **› N done**, long-idle sessions under **› N idle**, level with the repository name.
- [ ] [VISUAL] In R, a long branch such as `feat/pgw-3141-plugin-egress-admission` shows in full with a muted prefix and wraps; its facts move to a right-aligned second line. A short name keeps its facts on the same line. In C the name stays on one ellipsized line.
- [ ] In R and C, a branch with no session, diff, dirty files, PR or lifecycle chip folds under **Quiet · N branches · oldest X** at the end of the repo; the main checkout and the selected branch never fold. H shows no Quiet header. Click the header: the rows appear and keep their ⌘^N numbers.
- [ ] On a merged worktree summary, hover or tab to the **Merged** pill: it reads **Clean**; click it and the removal confirmation opens. Cancel keeps the worktree.
- [ ] Click the yellow `● N` dirty badge: the Changes tab opens for that worktree. The dot is centred on the digits.
