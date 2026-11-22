## Sidebar child-session guides

- [ ] In Rich mode, open a branch with an agent and several child sessions. Check that the guide starts under the parent dot, meets each child's row, ends at the last child's dot level, and does not continue into the next unrelated row. Switch to Compact mode and check that the guide disappears.

## Ego interactive spawn fallback — Rust restart required

- [ ] After a manual `make dev` restart or release rebuild, use a throwaway session to call `agent action=spawn agent_type=ego prompt="<task>"` with no explicit args or named profile. Verify the task runs once in the interactive terminal and the session remains available for a second task. Repeat with a configured model and with an explicit headless profile; the explicit profile must remain authoritative. The argv regression tests cover the mapping, not live provider submission or persistence. Do not restart the daily instance until losing its current sessions is acceptable.

## Managed Claude background output (1012-f12e) — backend rebuild required (continued)
- [ ] #1596-df57: In macOS desktop, focus a terminal on a branch with an automatically opened PR detail popover. Wheel over the sidebar without clicking; it must scroll, and typing must still reach the terminal. Click outside the popover: it closes and that first click activates the underlying control. Repeat with no popover, after a pane resize, and after a tab drag. Browser hit-testing reproduces the old full-window overlay; native wheel and visuals need Boss’s next-build check. _(audit 2026-10-13: HUMAN — Exact check acts on Boss active native window/repository or named pre-existing worktree; no live interaction outside owned disposable sessions authorized.)_ _(triage 2026-10-14: WAITS — Prior browser hit-testing evidence retained; actual native wheel, typing focus and first-click dispatch require available desktop interaction.)_ _(HUMAN 2026-11-05: Native macOS wheel/hit-testing; not reachable in browser mode.)_

- [ ] #1597-fab6: In macOS desktop, with each of the status-bar ticker popover (right-click a ticker), the status info balloon (click truncated status text), the sidebar GitHub panel (GitHub badge on a repo) and the Smart Prompts dropdown open, wheel over the sidebar without clicking; it must scroll. Click a sidebar control outside the popup: the popup closes and that first click activates the control. Clicking the GitHub badge or the info text again must close its popup (not re-open it). Browser hit-testing reproduces the old overlays; native wheel needs Boss’s next-build check. _(audit 2026-10-13: HUMAN — Exact check acts on Boss active native window/repository or named pre-existing worktree; no live interaction outside owned disposable sessions authorized.)_ _(triage 2026-10-14: WAITS — Prior browser overlay hit-testing evidence retained; native wheel and first-click activation remain unobserved, with desktop interaction unavailable.)_ _(HUMAN 2026-11-05: Native macOS wheel/popups; not reachable.)_
## Agent background indicator

- [ ] In an isolated agent session, run two tracked background jobs that finish at different times. At a ready composer, confirm the tab and sidebar stay steady blue and Activity Dashboard reads Background until the last job settles. Verify a question takes priority and an unseen completion returns to purple.

## Recent worktree allocation protection — Rust restart required

- [ ] After a manual `make dev` restart or release rebuild, use a disposable repository to create a worktree from the default tip by SHA. Confirm automatic cleanup keeps it during the first five minutes, including when an old checkout is reused. Confirm an explicit removal still works. Do not use active worktrees.

## Agent update process identity (3087-5ad5) — Rust restart required

- [ ] After a manual `make dev` restart or release rebuild, use an isolated session with a disposable native agent executable. Remove that executable while its agent remains at an idle composer. Confirm process detection still recognizes its current OS name and the existing guarded submit/queued-notice path can proceed. Repeat with a returned shell and a foreground child: neither may receive unintended agent input. Native regression tests cover real signed executable unlink, root ownership, unknown names and process exit; do not restart the daily app or modify its live queues for this check.

## Goose reply completion (1812-022d) — Rust restart required

- [ ] After Boss restarts `make dev` or installs `make build`, send two prompts to Goose 1.49.0 with ollama. Each reply must return the tab to idle shortly after the ready composer appears; the second submit must be accepted without a five-minute wait or 409. The recorded first-turn replay covers the backend transition; this live check remains open until restart. _(triage 2026-10-14: WAITS — Needs Goose 1.49.0 and a running ollama model.)_ _(HUMAN 2026-11-05: needs Goose 1.49.0 + ollama. Replay tests PASS: pty::tests::captured_goose_submitted_turn_returns_idle_without_protocol_stale (core_tests.rs:9549), goose_idle_capture_reads_ready (:9663).)_

- [ ] #1792-7450: After Boss manually restarts `make dev` (or rebuilds release), confirm Claude Chat still loads and follows its transcript. After installing/updating and trusting TUIC native Codex hooks, open two Codex tabs in one cwd: each CLI / Chat view must show only its own completed conversation. Replacing a thread resets Chat; leaving Codex retracts availability. Without native hooks, a heuristic/resume ID must not enable Chat. Native hook capture was verified without a model call; automated boundary tests cover exact binding and revocation. No desktop instance was launched for this change. _(triage 2026-10-14: WAITS — Exact-binding hook capture/regression evidence retained; full two installed Codex tabs/Claude follow/replacement/revocation needs trusted native hooks and owned vendor sessions. No Boss hooks or sessions modified.)_ _(HUMAN 2026-11-05: needs trusted native Codex hooks and two real Codex tabs. Code/tests: pty/codex_tool_tests.rs:35 codex_tool_edges_restore_busy_without_rebinding_identity PASS; agent_question/tests.rs:144 codex_chat_projection_gains_no_question_row exists; exact binding/revocation not driven live.)_
## Session usage menu (#2150-4e0c)

  and ego sessions. Check the live profile, shared quota/reset labels or exact ego
  session tokens/cost, unavailable and stale states. Never restart the daily app
  during active sessions for this check. Rust changes do not hot reload.

## Queued terminal replies (2680-2de2) — Rust rebuild/restart required

- [ ] After a manual backend rebuild/restart, queue a voice reply while speaking: _(HUMAN 2026-11-05: needs audible TTS playback ordering and browser audio. Code seen: dictation/adapters.rs:91-99 accepts_voice/write_voice; vitest stores/dictation.test.ts 109 pass but no test named for #2683 queue-wait ordering was found by search.)_
  new playback must wait for the utterance to close and its transcript to submit;
  repeat with a held draft/question, and with browser audio. Idle replies should
  still start immediately. Accepted barge-in can still cancel obsolete replies
  (#2683-5b0f). This requires loading the Rust change; Vite HMR cannot load it.

## Idle Claude worker wakes after its turn (2837-df6a) — Rust rebuild/restart required

- [ ] After a manual rebuild/restart when losing current sessions is acceptable, let a managed Claude worker finish a turn. Then send it mail. The log shows `hook-idle`, then `working-screen`, then an idle that is not `silence-timeout-agent`, and the `[TUIC] message available` notice is typed without `idle_unconfirmed` deferrals.
## Refused Claude question (2839-9095) — Rust rebuild/restart required

- [ ] After a manual backend rebuild or restart, refuse a Claude AskUserQuestion dialog with Escape. The session must return to idle, and the desktop status bar and mobile Current tool must not show AskUserQuestion. Do not restart while active sessions must be preserved.

## Answer-marker line guidance (2861-0410) — Rust rebuild/restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, inspect the effective generated launch guidance for a fresh terminal agent. It must say to start a new line before each additional `💬 ` marker, while a list-item marker stays after its list prefix. A pre-restart agent keeps its old instructions. A real agent reply can still disobey guidance; do not treat one model response as proof of a renderer fix.

## Absent-stage health warning (2876-3c58) — Rust rebuild/restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, open Activity Dashboard with a managed agent that has no current live stage. It must not show `Unknown` / `No current live stage is available.` Explicit Failed, Stalled, Input stalled and Blocked problems must remain visible. Targeted backend tests cover the projection; no daily instance was restarted. Rust does not hot-reload.

## Restored Claude state hooks (2858-2b1c) — Rust rebuild/restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, restore a Claude terminal whose saved launch uses an absolute executable. Its process arguments must include `--settings` and the single generated `agent-hooks/claude.json` path, including paths with spaces; one prompt/tool/stop cycle must record native busy/idle receipts. Repeat with explicit settings or Native status signals disabled: TUIC must not add settings. The launch-builder regression and resume command tests cover argument construction; no live sessions were changed. Rust does not hot-reload.

- [ ] After a manual Rust rebuild/restart, steer a working Claude Code, Codex and ego from Chat and the Compose queued list. Each must stop its active turn and receive the selected owner text, not defer it behind a tool. Repeat during a Claude Stop-hook loop and with `agent send urgency=urgent`; drafts and native dialogs must stay intact. Rust does not hot-reload; restart only when losing current sessions is acceptable. (#2869-3e82)

- [ ] After a manual backend rebuild/restart, open a new TUIC terminal and launch Claude with `env CLAUDE_CONFIG_DIR=<throwaway-profile> claude --dangerously-skip-permissions`, then resume the same conversation. Confirm status hooks report activity and the generated init guidance is present. Isolated native PTY argv recorders prove both flags are delivered; no vendor account was launched for story 2878-fee0. Rust and existing shell environments do not hot-reload.
- [ ] After rebuilding and restarting the Rust backend when session loss is acceptable, open a new TUIC terminal with `alias codex="codex --dangerously-bypass-approvals-and-sandbox --no-alt-screen"`. Start it manually and from a prepared agent launch. Both must start without the duplicate screen-flag error. Check Grok with `--no-alt-screen` and OpenCode with `--mini` aliases too. Existing shells retain loaded integration functions; reopen them after the update (#2877-5cec).

- [ ] After the next daily rebuild, run an ego background child and a background command. The tab stays working after the parent turn ends and becomes idle/completed only after the last owned job settles. Awaiting a person still shows the question badge. Native completed turns must not reopen on a retained spinner or stay working due to process infrastructure. Check an uninstrumented agent still uses its ready-screen fallback (#2930-c02d). Rust does not hot-reload; no daily instance was restarted.

- [ ] After a manual Rust rebuild/restart when losing sessions is acceptable, spawn an ego 0.4.7 terminal child with an initial brief and pending mail. The empty startup composer must receive the brief without manual input; mail must not overtake it. Repeat with a retained draft or open dialog: automated input stays parked. The captured startup/queue regression covers the backend boundary. Rust does not hot-reload; no daily instance was restarted (#2948-fc2a).

## Idle PTY ticker — Rust rebuild/restart required

- [ ] After a manual Rust rebuild/restart, compare `top -l 3 -s 2 -pid <TUIC_PID> -stats pid,csw,cpu,threads` with the pre-fix observation of about 3,800 context switches/s for 48 idle PTY tickers. Keep the same quiet terminal count, verify that context switches fall, then type in a dormant terminal, scroll, and leave a terminal query pending in canonical mode: output, scrolling and the reply after raw-mode entry must still work. CPU context switches are not a direct wakeup count; compare under equivalent load. Do not restart while active sessions matter.

- [ ] After an authorized rebuild/restart, spawn a throwaway agent command that prints a startup diagnostic and exits nonzero. Confirm its parent can read a failed `agent action=task_get` result with the real exit code and `session action=output` after the tab closes. Also check zero exit remains completed and signal exit uses `128 + signal`. Rust does not hot-reload; no running instance was restarted for story 2953-abb3.
## Mail wake after inbox observation (#2952-6322)

- [ ] After a manual `make dev` restart or `make build`, when losing active sessions is acceptable, send mail to a busy managed agent and read its inbox before it becomes idle. No stale message-available turn must follow; a later unread message must wake it once. The targeted recording-writer regression covers the delayed handoff and retry boundaries. No running app was restarted; Rust does not hot reload.

- [ ] After a manual Rust rebuild/restart when losing sessions is acceptable, send urgent mail to a working throwaway Codex child. Its interrupted composer must become idle and receive the message, or accept a later normal submit. No raw input workaround should be needed (#2961-8724). Rust does not hot-reload; no daily instance was restarted.

- [ ] After a manual rebuild/restart, verify Claude and Codex session quota and dashboard views share a refresh without duplicate quota requests. Check Claude extra usage/plan fields and Codex credit/model flags. Codex token statistics must still load independently. Rust does not hot-reload; no running app was restarted.
## Windows process argv (3000-aa7f) — Windows runner required

- [ ] On Windows, run `read_windows_child_profile_argv`: the live child profile must preserve spaces, quotes, Unicode and an empty argument. Then rebuild/restart when active sessions may be lost and verify ego/Codex named-profile sessions bind to the correct quota account. Cross-target compilation on macOS does not verify Windows runtime behavior. Rust does not hot-reload; no daily app was restarted. _(test part verified 2026-11-15 on the Windows lab VM: `read_windows_child_profile_argv` fails on the pre-implementation `None` stub and passes on the implementation; the app-level ego/Codex quota binding on Windows stays open.)_

## Provider quota bindings (2989-9fa6) — Rust rebuild/restart required

- [ ] After the registry transport and adapters are integrated and the app is rebuilt, verify two sessions sharing a CLI config show the same account quota; different config directories remain separate. An ego session must not show either CLI account quota. Do not restart the daily app while its sessions must be preserved.

## Claude status line relay (2992-0150) — Rust rebuild/restart required

- [ ] After a rebuild, start Claude in a TUIC tab with your own `statusLine` (user or project settings, with `padding`/`refreshInterval`). The status line looks and refreshes as before. After the first response, the tab quota shows the same 5h/7d percentages that Claude passes to the status line, with no `/api/oauth/usage` request in the log. Without a user status line, no extra status row appears.

## pi provider quota channel (2994-a1d0) — Rust rebuild/restart required

- [ ] After a rebuild, reinstall the Pi extension in Settings (it shows **Outdated**). Start pi signed in to Claude Pro/Max or ChatGPT in a TUIC tab and send one prompt: the tab shows that subscription quota, bound to `pi:<provider>:~/.pi/agent`. A pi session on an API key shows no subscription quota. With `/diagnostics/capture` on, the `pi-event` frame holds only quota headers. Copilot shows no quota (no endpoint recorded).

## Typed child_report at a silent stop (3014-6b1c) — Rust rebuild/restart required

- [ ] After a rebuild, spawn a throwaway Claude, Codex and ego child with a short task and tell each to end its turn without RESULT. Within about 20 s the parent inbox gets one `child_report` mail (`state: ended_without_result`, `last_text`, `resume`) per child, and `agent action=subscribe filter=report` selects it. A child that sends RESULT gets none; a child that waits on its own `tuic bg` job gets none until the job ends.

## One silence rule for every agent session (3050-589e) — Rust rebuild/restart required

- [ ] After a rebuild, start `TUIC_AGENT_STALL_MINUTES=1 make agent-test ...`, open a Claude tab by hand (no parent) and leave its turn open on a hidden dialog (for example press ctrl+o while a permission dialog waits). After about 1 min `session action=status` shows `stage: stalled`, `stalled: true`; the tab icon and the sidebar dot turn the warning color (no busy pulse); Boss attention lists it as Stalled; no mail reaches any inbox. Any output or answer clears it at once. An idle tab never turns stalled.

## ego input after a turn that leaves owned work running (3064-3141) — Rust rebuild/restart required

- [ ] After a rebuild, in a throwaway ego tab start a background exec that keeps running (for example `sleep 600` in exec background) and let the turn end. `session action=list` shows `shell_state: idle`, `agent_state: working`, no `queue_blocker`; `session action=submit` and a normal `agent send` reach the composer at once. When the exec ends, ego's collection turn runs and the tab returns to idle.
## Grok quota through the registry (2999-998f) — Rust rebuild/restart required

- [ ] After a rebuild, open a Grok tab with a valid `grok login`: the status-bar quota shows one Grok window from the billing period, and the Grok dashboard shows the same billing data. Refresh the dashboard twice within a minute: no second `grok agent stdio` process starts. With an expired login the badge shows the login state and no new CLI process starts until `grok login` rewrites `~/.grok/auth.json`.

## Ego named-account quota binding (3092-e0ee) — Rust restart required

- [ ] After Boss chooses to rebuild/restart the backend and installs the coordinated ego account-publication change, open two ego terminal sessions with distinct named OAuth accounts for the same vendor. Confirm each badge shows its own quota, refresh stays on that account, and passive usage updates do not replace the other account or a remaining legacy account. Repeat with an account ID equal to its vendor name. Automated checks cover synthetic distinct quotas without network/auth tokens; real provider values and the daily desktop were not exercised. Do not restart the daily app during active sessions.

- [ ] After the 2.1.2 candidate backend is rebuilt, repeat the named-account and legacy-account quota check above. The release CI correction replaces nested optional connection states with explicit legacy, named and invalid states; account selection must stay unchanged. Rust does not hot-reload, and no daily instance was restarted.

## Ego Settings machine credential inventory (3093-8d1b) — Rust restart required

- [ ] After Boss chooses to rebuild/restart the backend and installs the coordinated Ego machine-inventory producer, read Ego Settings with named and legacy credentials for the same vendor, including equal-spelling IDs and an ID longer than 64 characters. Confirm independent presence, missing/empty declared sources unset, and no named-account legacy auth edit targets. Automated replay uses actual isolated producer output; the daily app and authenticated sources were not exercised. Do not restart the daily app during active sessions.

- [ ] [HUMAN] Copy a non-image file (a `.mov` screen recording or a PDF) in Finder, focus a Claude Code terminal and press Cmd+V. Confirm the file reaches Claude as with Ctrl+V. Plain-text Cmd+V must still paste the text. Cross-app clipboard, so it needs a human. Covered by `CanvasTerminal-paste.test.tsx` (non-image file → `\x16`).

- [ ] [VISUAL] **Needs you** (frontend, arrives by HMR): with two agents waiting in different repositories, the toolbar centre shows an orange pulsing **Needs you 2** pill; clicking it lists `repo / branch · question · age` oldest first, and a row opens that session. Cmd+U visits the same order. Close one repository: its header shows `1 need you` and the sidebar banner `N need you · <repos> ↓` appears; clicking the banner opens the repository and the session. With nothing waiting, the pill and the banner disappear. Component render was checked headless in Chrome; the real toolbar and sidebar layout still need a look.

- [ ] After a manual `make dev` restart (Rust change, no hot reload): in a Claude Code session, make the agent run a command that fails (exit 1) and then continue with another step. **Needs you** and the red badge must not appear while the agent keeps working. A turn that really stops on the failed row still raises the badge once the session is idle. Covered by `test_tool_error_waits_for_turn_end_while_agent_keeps_working`.

## Ego session without a named account under ego ≥ 0.4.15 — Rust restart required

- [ ] After the backend restart, keep an ego session that started before its account was named (manifest without `connection`). Its badge must read unavailable with "ego session has no named account; restart it", not a contract error, and never another account's quota. A new ego session on the named account must show its own quota. Unit coverage: `ego_legacy_key_without_legacy_publication_is_unavailable`.

## Spawned agents are no longer capped — Rust restart required

- [ ] After a manual `make dev` restart: have the orchestrator spawn a worker with no `capabilities`, then have that worker spawn its own agent and `git push` a scratch branch to a throwaway remote. Both must work with no `capability_denied` and no "lacks the push capability" message. Unit coverage: `child_without_capabilities_may_spawn`, `spawned_child_may_spawn_but_not_orchestrate`.

## Discussion in the Progress journal (stories 3107–3111, 3115) — Rust restart required

- [ ] After the `make dev` restart, ask five questions ending in `?` directly in a Claude terminal, then one each in a Codex, a Grok and an ego terminal. Every question must appear in Progress with its answer (the capture now reads the Chat view and retries up to ~7 s when the final record lands after the idle). Answers must show no `💬`, `intent:` or `suggest:` markers; a fork answer to `Cmd+Alt+Q` likewise. The ask field and the Ask button sit on one row, and an open thread hides the row text it repeats. Entry 9 filed before the fix still carries its markers.

- [ ] After a manual `make dev` restart, in a Claude terminal that is working: press Cmd+Alt+Q. Progress opens on that terminal with the ask box focused. Ask "what are you doing now?": a `question` entry appears at once, then an `answer` entry with a toast. The main agent's turn is not interrupted and its transcript gains no message. **Thread** shows the full answer without the header line; `/logs?source=discussion` has no "no classification header" warning.
- [ ] In the same thread send a follow-up: it continues the fork (shown `warm`). Ask "create a file x.txt": no file appears (plan mode).
- [ ] Cancel a running question: an `answer` entry with step `cancelled`. Three questions at once in one terminal: the third is refused with "2 questions are already running". Restart TUIC while one runs: it answers `failed` "interrupted by a restart".
- [ ] Type to the Claude agent directly "which file holds the parser?" and let it answer: when the turn ends, a `question` (step `direct`) and its `answer` land in Progress, once. A plain instruction ("fix the parser") files nothing.
- [ ] The default list hides intents, messages and delegations; the Activity toggle shows them; Flow is unchanged. "All projects" lists entries of two repositories with kind and unread filters; a click opens the project and terminal.
- [ ] A Grok terminal shows "Side questions support Claude, Codex and ego terminals for now." Visual check of the ask box, thread and All projects view (screenshot).
- [ ] Codex backend (story 3112), after the restart, in a Codex terminal that is working, with its session id from the tab: `curl -X POST localhost:9876/sessions/<id>/discussion -H 'Content-Type: application/json' -d '{"question":"what are you doing now?"}'`. A `question` entry, then an `answer` whose turn in `GET /discussion/<entry_id>` has `cacheReadTokens` close to the main session's context size. `POST /discussion/<entry_id>/follow-up` with "what did I ask before?" names the first question. The Codex turn is not interrupted, `codex resume` lists no new session, and "create a file x.txt" creates nothing.
- [ ] ego backend (story 3114), after the restart, with an ego release later than 0.4.18 (it must contain the prefix cache key), in an ego terminal that is working: press Cmd+Alt+Q and ask "what are you doing now?". A `question` entry, then an `answer` whose turn in `GET /discussion/<entry_id>` has `cacheReadTokens` close to the main session's context size (with ego 0.4.18 the first fork reads 0). A follow-up "what did I ask before?" names the first question and reads the cache again. The ego turn is not interrupted, "create a file x.txt" creates nothing, and `ego ls` in that workspace lists one new session per thread (the stored fork). An AI Chat ego conversation has no ask box target.

## Interrupted subagents — Rust restart required

- [ ] In the rich sidebar, open the agent row for a session with interrupted subagents (or interrupt a subagent in a disposable session). Verify the row reads **Interrupted**, is not counted as returned, and is not highlighted as running. A subagent that reports normally still appears under the returned count.
## Ego session without a named account under ego ≥ 0.4.15 — Rust restart required

- [ ] After the backend restart, keep an ego session that started before its account was named (manifest without `connection`). Its badge must read unavailable with "ego session has no named account; restart it", not a contract error, and never another account's quota. A new ego session on the named account must show its own quota. Unit coverage: `ego_legacy_key_without_legacy_publication_is_unavailable`.

## Interrupted or rejected dialog returns to idle — Rust restart required

- [ ] After the backend restart, in a disposable Claude session with hooks, run a prompt that asks for a Bash permission, then reject it (option 3/No) and, in a second run, press Esc. After the "Interrupted" banner the tab must reach idle within a few seconds without sending another prompt. Repeat with a Codex approval and Esc. Unit coverage: `claude_permission_cancel_or_reject_returns_to_idle_without_stop`, `codex_canceled_approval_capture_returns_to_idle_without_stop`.

## Needs you clears when the agent process dies — Rust restart required

- [ ] After the backend restart, open a permission dialog in a disposable Claude session, then kill the agent process (`kill -9` its pid). The tab must drop "Needs you" as soon as the session shows exited. Unit coverage: `process_exit_clears_a_confident_awaiting_badge`.
