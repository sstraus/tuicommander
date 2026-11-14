## Managed Claude background output (1012-f12e) — backend rebuild required (continued)
- [ ] #1596-df57: In macOS desktop, focus a terminal on a branch with an automatically opened PR detail popover. Wheel over the sidebar without clicking; it must scroll, and typing must still reach the terminal. Click outside the popover: it closes and that first click activates the underlying control. Repeat with no popover, after a pane resize, and after a tab drag. Browser hit-testing reproduces the old full-window overlay; native wheel and visuals need Boss’s next-build check. _(audit 2026-10-13: HUMAN — Exact check acts on Boss active native window/repository or named pre-existing worktree; no live interaction outside owned disposable sessions authorized.)_ _(triage 2026-10-14: WAITS — Prior browser hit-testing evidence retained; actual native wheel, typing focus and first-click dispatch require available desktop interaction.)_ _(HUMAN 2026-11-05: Native macOS wheel/hit-testing; not reachable in browser mode.)_

- [ ] #1597-fab6: In macOS desktop, with each of the status-bar ticker popover (right-click a ticker), the status info balloon (click truncated status text), the sidebar GitHub panel (GitHub badge on a repo) and the Smart Prompts dropdown open, wheel over the sidebar without clicking; it must scroll. Click a sidebar control outside the popup: the popup closes and that first click activates the control. Clicking the GitHub badge or the info text again must close its popup (not re-open it). Browser hit-testing reproduces the old overlays; native wheel needs Boss’s next-build check. _(audit 2026-10-13: HUMAN — Exact check acts on Boss active native window/repository or named pre-existing worktree; no live interaction outside owned disposable sessions authorized.)_ _(triage 2026-10-14: WAITS — Prior browser overlay hit-testing evidence retained; native wheel and first-click activation remain unobserved, with desktop interaction unavailable.)_ _(HUMAN 2026-11-05: Native macOS wheel/popups; not reachable.)_
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

- [ ] On Windows, run `read_windows_child_profile_argv`: the live child profile must preserve spaces, quotes, Unicode and an empty argument. Then rebuild/restart when active sessions may be lost and verify ego/Codex named-profile sessions bind to the correct quota account. Cross-target compilation on macOS does not verify Windows runtime behavior. Rust does not hot-reload; no daily app was restarted.

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
