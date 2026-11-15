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
