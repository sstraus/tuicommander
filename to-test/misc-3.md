## Still needs a human (continued)
- [ ] [HUMAN] Install `zed`, put a comment, a trailing comma and hand-tuned indentation _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_ _(audit 2026-10-13: HUMAN — Requires restart/reload/freeze or network interruption prohibited by audit brief; not attempted. Source: to-test.md:2356)_ _(triage 2026-10-14: WAITS — Needs Zed installed and its user configuration fixture.)_ _(HUMAN 2026-11-05: needs zed installed (`command -v zed` = none) and Boss's ~/.config/zed; MCP config writes to agent configs forbidden. Code-verified: backups go to config_dir()/mcp-backups (agent_mcp.rs:622).)_
  in `~/.config/zed/settings.json`, install the bridge from Settings → Agents, and
  confirm Zed still starts, still shows every setting, and lists the `tuicommander`
  context server — `diff` against `<config dir>/mcp-backups/zed-settings.json.orig`
  must show only the added member. Then press **Remove all MCP integrations** and
  confirm each client lost only its `tuicommander` entry and a relaunch does not put
  it back. Zed is not installed here, and a real client reading the file afterwards
  is the one thing the splice tests cannot cover. (issue #115)
- [ ] [HUMAN] Compare OSC 133 gutter marks side by side, browser at `:9876` against the _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_ _(audit 2026-10-13: VISUAL-OWED — Real canvas/vendor rendering or subjective visual observation unavailable. Related tests run where applicable but do not prove rendered native surface; no screenshot attempts.)_ _(triage 2026-10-14: WAITS — Side-by-side native canvas rendering requires available desktop capture; API extraction and renderer tests cannot prove appearance.)_ _(HUMAN 2026-11-05: needs the desktop app and the browser visible at once (desktop is Boss's live window; canvas painting also did not settle reliably in the shared headless-ish Chrome screenshots today). Code touched: CanvasTerminal.tsx:858-860,933,987,1046 (commandBlocks / activeBlock drive the OSC 133 marks); browser attach to a PTY verified live (PTY resized to 35 rows by the browser).)_
  desktop app, on the same session: same rows, same size, neither client stealing the
  other's dirty rows. Canvas painting is not observable over HTTP, and both clients
  have to be visible at once. (Port corrected 2026-09-07 from `:9877` — only one
  instance runs, and it serves 9876; a browser pointed at 9877 gets nothing.)
- [ ] [HUMAN] Open `vim` or `htop`: the wheel still goes to the app, `Shift+wheel` _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_ _(audit 2026-10-13: HUMAN — Requires hardware, authentication/provider state, or credential mutation; prohibited by audit brief. Source: to-test.md:2369)_ _(triage 2026-10-14: WAITS — Actual trusted wheel and Shift+wheel routing in the native terminal needs desktop interaction; locked browser pointer cannot exercise this path.)_ _(HUMAN 2026-11-05: real wheel/Shift+wheel in vim needs a native trackpad/mouse. Live attempt: MCP-created PTY, `vim -u NONE -c "set mouse=a"` (PTY emitted ESC[?1002h), CDP `scroll down` over the canvas did not move vim (row0 stayed "1") — inconclusive harness limit, not a defect claim. vitest CanvasTerminal-wheel-forward.test.tsx 11/11 pass today (forwards SGR notches, Shift+wheel never forwarded and scrolls TUIC history: tests at :186,:194).)_
  scrolls TUIC history, and quitting restores the shell scrollback unchanged. The
  enter/exit half is covered by the `gh-run-watch.raw` replay test; mouse-reporting
  forwarding needs a real wheel. `lazygit` is not installed.
- [ ] [HUMAN] Print a fullwidth char and overwrite half of it — `printf '\e[1;5H中'` _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_ _(audit 2026-10-13: VISUAL-OWED — Real canvas/vendor rendering or subjective visual observation unavailable. Related tests run where applicable but do not prove rendered native surface; no screenshot attempts.)_ _(triage 2026-10-14: WAITS — Fullwidth overwrite renderer behavior has automated coverage retained; the requested native glyph/cursor appearance still needs desktop capture.)_ _(HUMAN 2026-11-05: canvas appearance needs eyes (my browser screenshots of the canvas painted only partially). Live grid half PASS: PTY `printf '\e[3;5H中'; printf '\e[3;6HX'` → GET /sessions/<id>/terminal/row-text?row=2 = "     X" (no ghost 中 in the server grid; terminal/lines agrees). Renderer-side ghost needs a visual check.)_
  then `printf '\e[1;6HX'` — and confirm no ghost `中` survives beside the `X`. Scroll
  away and back to prove it is not just hidden by a later full-row reship. Canvas
  painting.
- [ ] [HUMAN] Raise an MCP `ui action=confirm` and answer it on a phone at _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_ _(audit 2026-10-13: HUMAN — Requires hardware, authentication/provider state, or credential mutation; prohibited by audit brief. Source: to-test.md:2377)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_ _(HUMAN 2026-11-05: needs a real phone + push subscription (not raising ui confirm: it would block Boss's desktop dialog). Code-verified: mcpConfirm.ts:88-100 first client to answer wins, "mcp-confirm-resolved" event clears the other client's dialog; McpConfirmHost mounted in mobile MobileApp.tsx:329.)_
  `/mobile.html`: the desktop dialog must disappear by itself, and the reverse must
  work too. Then, with a push subscription registered and the PWA closed, confirm the
  push carries the title. Needs a real phone and a real subscription.
- [ ] [HUMAN] Boss's call: **Trim** the real `src-tauri/target` row in Build Cleaner. It _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_ _(audit 2026-10-13: HUMAN — Requires hardware, authentication/provider state, or credential mutation; prohibited by audit brief. Source: to-test.md:2385)_ _(triage 2026-10-14: WAITS — MacControl Screen Recording: needs native desktop/cross-app observation.)_ _(HUMAN 2026-11-05: Boss's call, no agent may Trim. Code: plugins/build-cleaner/main.js:302-304 (Trim arms before confirming), :404 Trim offered only when separable intermediates exist. src-tauri/target is a symlink to the mbx view (…/.mbx/targets/v1/1512a2…, du 59G today).)_
  is 58 GiB and a Trim forces a full rebuild of the running dev app, so no agent may
  run it. Trim against other repos' `target/` is covered.
- [ ] Rust change, needs a `make dev` restart (story #650-b0a0). With the app started _(NOT VERIFIED 2026-09-29: Needs a valid external relay server URL/token and killing the relay server; external service.)_ _(audit 2026-10-13: HUMAN — Requires hardware, authentication/provider state, or credential mutation; prohibited by audit brief. Source: to-test.md:2399)_ _(triage 2026-10-14: WAITS — Needs an owned externally reachable relay daemon/server pair; rb unavailable and no Boss relay/network teardown authorized.)_ _(HUMAN 2026-11-05: needs a reachable relay server + toggling Cloud Relay (credentials/tunnels forbidden); /system/relay-status live = {enabled:false,connected:false}. Code-verified log strings relay_client.rs:321 "connecting to", :342 "shutting down", :263 "stopped", :350/:359 "reconnecting in {backoff}s", backoff reset to INITIAL after a successful connection :330-337.)_
  while **Cloud Relay is off**, turn it on in Settings → Services with a valid relay URL
  and token: the status dot must go green with no app restart, and `GET :9876/logs`
  must show `relay: connecting to …`. Turn it off: the dot goes grey and the log shows
  `relay: shutting down` then `relay: stopped`. Turn it on again — the supervisor must
  still be watching after a stop. Then kill the relay server (or pull the network) while
  connected: every reconnect log must read `reconnecting in 1s` for the first attempt
  after each *successful* connection, growing 1→2→4… only across consecutive failures.
- [ ] Rust change, needs a `make dev` restart (story #654-bfc1). Put a stub earlier on _(NOT VERIFIED 2026-09-29: partial — Stub cannot take priority: resolve_cli probes /usr/local/bin then /opt/homebrew/bin (real gh there; code comment says a PATH stub is never picked up) and I may not touch system dirs. Observed on headless tuic-remote (PATH stub first, sandbox HOME): HTTP bound 0.28s after start, deferred probe logged off boot path. Not checked: 10s 'did not answer' )_ _(audit 2026-10-13: HUMAN — Requires restart/reload/freeze or network interruption prohibited by audit brief; not attempted. Source: to-test.md:2417)_ _(triage 2026-10-14: WAITS — Needs isolated backend startup with deferred GitHub token probe; targeted the_deferred_probe_nudges_the_poller_once_the_token_lands remains unexecuted while rb SSH is unavailable. No credential store read.)_ _(HUMAN 2026-11-05: needs app relaunch with a gh stub (forbidden). Code: github_auth.rs:726-729 boot no longer reads credential store, :774-818 spawn_deferred_token_resolution nudges poller with PollerCmd::ForceResync; test the_deferred_probe_nudges_the_poller_once_the_token_lands (:1180) not run. NOTE: the logged text "gh auth token did not answer in time" no longer exists anywhere in src-tauri (rg), so that check string is outdated.)_
  the resolved `gh` path (`/opt/homebrew/bin/gh` or `/usr/local/bin/gh`, whichever
  `resolve_cli` finds first) containing `#!/bin/sh` + `sleep 600`, unset `GH_TOKEN` and
  `GITHUB_TOKEN`, then launch the app: the window must appear at the usual speed instead
  of waiting on the stub. `GET :9876/logs?source=github` must then show
  "`gh auth token` did not answer in time" about 10s later (5s for the `gh_token` crate's
  own spawn, 5s for ours) and the app must stay usable with no GitHub token. Restore the
  real `gh`, relaunch, and confirm PRs/issues populate within a second or two without
  touching Settings — the deferred probe, not boot, is what fills them now, and it has to
  nudge the poller (`ForceResync`) to re-run the cycle it missed; a sidebar that stays
  empty for a full minute means that nudge did not land. Same stub check against
  `tuic-remote` (headless): the HTTP server must bind immediately instead of waiting on
  `gh`, and `GET /repo/issues` must answer once the probe lands.
- [ ] Rust change, needs a `make dev` restart (story #642-3741). `mod dictation_routes` _(NOT VERIFIED 2026-09-29: partial: /dictation/status, /models, /devices, /config, /system/relay-status and /system/check-update?channel=nightly answer JSON (unix-socket router); browser dictation record/inject and output device need a microphone/speaker)_ _(audit 2026-10-13: HUMAN — Partial live JSON route proof: dictation-status/models/devices/config-live.json and system-relay-status-live.json over supported UDS…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_ _(HUMAN 2026-11-05: routes half PASS live today over the socket: /dictation/status, /models, /devices, /config, /system/relay-status and /system/check-update?channel=nightly all return JSON (200 application/json). Browser dictation record→inject and chosen output device need microphone/speaker (not available).)_
  was never declared, so 12 handlers never compiled, and 7 more COMMAND_TABLE paths hit
  no route at all. Against the restarted build: `curl :9876/dictation/status` and
  `/dictation/models`, `/dictation/devices`, `/dictation/config`, `/system/relay-status`
  must answer JSON (not a 404 or the SPA shell), and
  `curl ':9876/system/check-update?channel=nightly'` must return an `UpdateCheckResult`.
  Then open the app in a browser at `:9876` and use dictation end to end: record, stop,
  and confirm the transcript is injected — browser mode reads `inject_text` as a bare
  string now, not `{text}`. Last, set a non-default audio output in notification
  settings and trigger a notification from the browser tab: it must play on the chosen
  device, which is what the added `device` field in the HTTP body carries.
- [ ] Rust change, needs a `make dev` restart (story #670-b9a2). Grid delivery got three _(NOT VERIFIED 2026-09-29: Needs the desktop WebView with tauri::ipc::Channel, blocking the app JS thread via devtools; not reachable from HTTP/MCP on a test instance.)_ _(audit 2026-10-13: VISUAL-OWED — Rendering/timing requires images or real pointer interaction; image viewing and MacControl unavailable. No screenshots attempted. Source: to-test.md:2441)_ _(triage 2026-10-14: WAITS — Exact blocked-WebView/grid-channel observation requires an owned desktop; peers cannot launch it or block Boss UI. Prior backend regression evidence retained.)_ _(HUMAN 2026-11-05: needs the desktop WebView with tauri::ipc::Channel and a blocked JS thread (Boss's live app). Nothing in browser mode exercises the Channel path; no code run.)_
  changes that only show up in a live WebView. (1) **Frame ordering:** zoom/resize a busy
  session repeatedly (the resize path cuts a FULL frame off-thread while the ticker cuts
  deltas) — no blank or half-stale screen may survive the zoom, and any frame that loses
  the race must come back as a full repaint on the next tick rather than vanishing.
  (2) **Browser not starved by a stalled desktop:** open the same session in the app and
  in a browser tab at `:9876`, block the app's JS thread (devtools breakpoint, or a heavy
  panel), and confirm the browser tab keeps painting at the normal rate instead of
  freezing with it. (3) **Desktop repair:** release that breakpoint — the app window must
  repaint the whole screen in one go, with no rows left stale from the frames it missed.
  Tests cover the Rust side of all three; what they cannot reach is the frame actually
  crossing `tauri::ipc::Channel` into the WebView.
  background costs from the boot audit: (1) the AI cron scheduler's 30s tick loop
  (`ai_agent::scheduler`) now only spawns when `ai-cron.json` has at least one enabled
  job, and stops when the last one is disabled/removed via `save_scheduler_config` —
  with zero jobs configured (the default), confirm no "Scheduler stopped"/tick log lines
  ever appear; add one enabled job via the Scheduler UI (or `PUT /ai/scheduler/config`)
  and confirm it fires on schedule; then delete/disable it and confirm the loop actually
  stops (no further tick activity) rather than continuing to poll. (2) Knowledge persist
  (`ai_agent::knowledge`) now skips the `spawn_blocking` dispatch on a 2s tick when no
  session has dirty knowledge — run a normal terminal session (commands recorded via
  knowledge tracking), confirm history still persists to `ai-sessions/` correctly (no
  regression from the skip). (3) `app_logger::init_tracing`'s file appender is now
  wrapped in `tracing_appender::non_blocking` instead of writing to `logs/tuic.log.*`
  synchronously on the calling thread (including from async tokio tasks) — confirm the
  daily-rotated log file still receives entries during normal use and on graceful
  shutdown (no lines silently dropped by the leaked `WorkerGuard`).
- [ ] Rust change, needs a `make dev` restart. Dictation speech gates: the transcriber _(NOT VERIFIED 2026-09-29: Needs real microphone silence/speech and Whisper gating.)_ _(audit 2026-10-13: HUMAN — Requires hardware, authentication/provider state, or credential mutation; prohibited by audit brief. Source: to-test.md:2469)_ _(triage 2026-10-14: WAITS — Needs the named native platform/CI runtime; this machine is macOS.)_ _(HUMAN 2026-11-05: needs real microphone silence/speech + Whisper. Live: GET /dictation/config returns rms_threshold 0.002205 (settings field exists, config loads). Not driven.)_
  now rejects a window in three steps — the RMS floor, Whisper's own
  `no_speech_probability`, and the phrase filter — and the first two read their
  thresholds from `dictation-config.json` (`rms_threshold`, `no_speech_threshold`)
  rather than constants. (1) **The reported leak:** with the headset a metre away,
  start dictation, say nothing, stop. No "Grazie"/"Thank you" may reach the terminal —
  before this change the repeated form ("Grazie. Grazie.") produced by the final
  full-buffer pass slipped past the filter, while the short streaming windows caught
  the single form. (2) **No over-rejection:** dictate a normal command and confirm it
  still lands, and that a sentence that merely starts with thanks ("Grazie, ora
  committa") is not eaten. (3) **Settings > Dictation > Voice tuning:** the meter must
  move with your voice, the marker must sit where the level gate is, "Start test
  recording" must show the transcript in the panel and never type it into the terminal
  behind it, and a rejected recording must print its reason ("Rejected: no speech
  detected (no_speech 0.91 > 0.60)"). (4) **Both sliders persist** across an app
  restart, and a `dictation-config.json` written before this change keeps the defaults
  (0.001 / 0.60) instead of reading 0.
- [ ] Rust change, needs a `make dev` restart. Per-tab agent resume (issue #119): with _(NOT VERIFIED 2026-09-29: Needs several real Claude tabs in one folder to verify per-tab session discovery)_ _(audit 2026-10-13: HUMAN — Real per-tab resume across app restart and registry/config-dir checks not performed; restart forbidden and credential-store boundaries apply. No runtime pass claimed.)_ _(triage 2026-10-14: WAITS — Exact process-identity/resume evidence retained; whole multi-live-tab reconnect check needs owned vendor sessions and rebuilt lifecycle. rb unavailable, no Boss session restarted.)_ _(HUMAN 2026-11-05: (2) needs Cmd+Q restart, (3) grok, (4) codex — not allowed/not run. Part (1) PASS live: 3 claude tabs in one folder (scratch repo) each with its own prompt: window.__TUIC__.terminals() agentSessionId 850a810a…/801a4540…/5f37ba10… all distinct, each equal to sessionId in ~/.claude/sessions/<leaf-pid>.json (pids 58643/58943/59324 from GET /sessions/<id>/leaf-pid). Tabs closed.)_
  several Claude tabs open in the SAME folder, each tab must now hold its own session.
  Before this change discovery took "the newest unclaimed transcript in the project
  dir", so tabs stole each other's session or got none — measured live on 6 Claude
  tabs: 3 had `agentSessionId: null` and one held a different tab's id, which is why
  every tab resumed with `claude --continue` into the same conversation. (1) Open three
  Claude tabs in one repo, give each a distinct conversation, then check
  `curl -X POST localhost:9876/debug/invoke_js -d '{"script":"return
  JSON.stringify(window.__TUIC__.terminals())"}'` — every claude tab must show a
  DISTINCT non-null `agentSessionId`, and each must equal the `sessionId` in that
  tab's own `$CLAUDE_CONFIG_DIR/sessions/<pid>.json` (pid from
  `GET /sessions/<id>/leaf-pid`). (2) Quit TUIC (Cmd+Q), relaunch, click each resume
  banner: each tab must reopen ITS conversation, and `ps -ax -o args=` must show
  `claude --resume <uuid>` with three different uuids — not `claude --continue`.
  (3) Same check for a grok tab (binding comes from `~/.grok/active_sessions.json`).
  (4) No regression for Codex/Gemini, which have no pid registry and keep the old
  heuristic: a single Codex tab must still resume its own session.
- **DELETED 2026-09-19 — two items, both unrunnable.** **Detached AI Chat
  window** (`700-4d5d`) asked for a stream to follow the panel into its own window,
  and **agent runs persist with the conversation** (`705-57fa`) asked for a
  `schema_version: 3` migration and tool cards surviving a reload. #784-0aec deleted
  the engine under both: no conversation store, no
  `<config_dir>/ai-chat-conversations/`, no autonomous agent goal, no tool card, no
  *Explain this error* context-menu entry. The one surviving check — the panel
  detaches and comes home — is listed under *The embedded AI engine is gone
  (#784-0aec)* at the end of this file. The streaming half returns with 785-58ca and
  will be written against ego rather than restored from here.
  pushed, not polled (story `687-be9d`). The desktop no longer calls
  `list_active_sessions` on a 1 Hz timer; the backend emits `session-state-changed`
  once per real transition, on the Tauri window and on `/events` SSE. (1) **Badges
  still move:** with a claude tab open, send it a prompt — the tab must go busy, then
  show the awaiting badge on a question, then clear when answered, all as fast as
  before. Same for the Activity Dashboard. (2) **The poll is gone:** with the app
  idle and the window VISIBLE, `curl -X POST http://localhost:9876/diagnostics -d
  '{"enabled":true}' -H 'content-type: application/json'`, wait a minute, then
  `curl 'http://localhost:9876/logs?source=diagnostics'` — no periodic IPC at idle.
  Before this it polled once a second forever whenever the window was on screen.
  (3) **A reload still converges:** with a long-idle, silent agent tab, reload the
  window (desktop: Cmd+R / reopen). The badge must be correct immediately — that is
  the one mount-time `list_active_sessions` catch-up, the only call left.
  (4) **Browser parity:** open `http://localhost:9876/` in a browser and repeat (1);
  the SSE arm carries the same payload.
- **DELETED 2026-09-19 — unrunnable.** **Provider availability is now rendered**
  (`701-b6ac`) asked for a visual check of the reachable / not-detected / wrong-port
  rows in `Settings > Providers`. #784-0aec deleted `detect_ollama` with the rest of
  the provider registry and removed the tab itself — no row, no icon, no reason line.
  Provider configuration moves into ego as 786-4a6d, which brings its own checks.
  picks it up; no `make dev` restart needed. I could not screenshot it: the
  orchestrator instance on :9876 does not run this build, and no worktree dev
  instance was up. (1) **It appears:** `Settings > General > Terminal` now shows
  a third toggle, **Show scrollbar marks**, below **Block folding**, on by
  default — check it lines up with the other two and the hint wraps sanely.
  (2) **It is searchable:** type "scrollbar" in the settings search box; the
  entry must appear and jump to the Terminal section. (3) **It actually gates
  the marks:** in a terminal with several command blocks, turn it off — the
  blue/red block ticks **and** the green user-prompt ticks disappear, and they
  must go on the flip itself, not on the next scroll. (An early return used to
  skip the repaint that erases them, so they stayed painted forever; 723-6b02
  fixed that, and this is the check for it.) (4) **Search ticks must SURVIVE**
  — with the toggle OFF, run a terminal search (Cmd+F): the orange match ticks
  must still be drawn. A search that silently marks nothing is the failure
  723-6b02 exists to prevent; the flag covers command history only.
  (5) **It persists:** turn it off, restart, confirm `config.json` carries
  `"show_scrollbar_marks": false` and the toggle is still off.

- **DELETED 2026-09-19 — unrunnable.** **Agent tool-log bound, measured on a real
  run** (`718-aebf`) asked for the file size and rewrite rate of a long autonomous
  agent session. #784-0aec deleted `conversationStore.ts`, the 512 KB tool-log
  ceiling it enforced, and the agent run that filled it; there is no conversation
  file left to measure and no tool card to reload. ego keeps its own transcript, so
  bounding it is ego's problem, not TUICommander's.

  needs a rebuild.** `claude_usage_cache` carried `#[cfg(feature = "desktop")]`
  while `build_router` mounts `/claude/usage` and `/claude/usage/timeline`
  unconditionally, so `cargo build --bin tuic-remote --no-default-features` did
  not compile at all. The gate is gone. After a rebuild, start `tuic-remote` and
  check `curl http://127.0.0.1:<port>/claude/usage` answers instead of 404/500.
  Desktop behaviour must be unchanged — the same endpoint on :9876 still works.
## The connection runtime moved to Rust (#790-ef85) — needs a `make dev` restart

The health probe, the token exchange, the 5s status poll and the SSH tunnel now
run in the backend; `remoteConnections.ts` only renders what the backend pushes.
Every item below must behave exactly as it did before the move — that is the
point of the story — plus the two things only the backend can do.

- [ ] Quit TUICommander with an SSH-transport connection live. No orphan `ssh` _(NOT VERIFIED 2026-09-30: blocked — Second physical machine (SSH transport to mac-mint) plus quitting the desktop app; neither available headless.)_ _(audit 2026-10-13: HUMAN — App quit with actual SSH connection requires Boss own window/restart and SSH credentials. Shared teardown audited/tested, but process-exit orphan sweep cannot be exercised under no-kill/no-restart…)_ _(triage 2026-10-14: WAITS — Needs the specified remote daemon/SSH fixture and coordinated rollout; no live daemon restart.)_ _(HUMAN 2026-11-05: needs quitting the live app with an SSH-transport connection (forbidden). Code-verified: lib.rs:2862-2880 RunEvent::Exit waits tunnel_manager.shutdown_all_and_wait(); remote_runtime.rs:4214 .kill_on_drop(true) on the ssh child; remote_runtime.rs:1488 comment: the old __remote_<id> profile file is no longer written.)_
      process and no `__remote_<id>` profile file is left behind.
## The embedded AI engine is gone — needs a `make dev` restart (#784-0aec)

~24k lines of Rust and ~44 frontend files were deleted. Nothing below is a new
feature: each item confirms that removing the engine did not take a *surviving*
feature with it. All of it needs the rebuilt backend, so run it after the
restart, not before.
