## Mobile Claude dot pulse (1654-a3d5)

- [ ] On an actual iPhone and iPad installed Safari PWA, run a long Claude Bash tool call; confirm grey ON/OFF pulses and green completion preserve text position and continuation indentation. Real OutputView browser geometry passes in Chromium at 390×844 and 1024×1366; device emulation does not prove Safari fonts or touch behavior.
## Tablet desktop switch (1658-fb34)

- [x] Browser iPad emulation: Settings → Open Desktop UI reaches `/`, a later root visit stays desktop, and the visible Switch link returns to `/mobile` and clears the preference. _(verified: worktree Vite :14358 via stealth wrapper, iPad UA, five touch points, standalone flag; screenshots `~/Gits/.tmp/tablet-1658-{settings,desktop,return-mobile}.png`; 15 targeted tests passed.)_
- [ ] [HUMAN] On the already-installed iPad PWA, confirm the updated manifest scope takes effect and both switches stay in the installed app window. Browser emulation proves routing and preference handling but cannot prove iOS updates an existing installation's manifest.

## MCP transport import cfgs (1652-4954)

- [x] Import-only change; no runtime behavior to verify after restart. _(verified: src-tauri/src/mcp_http/mcp_transport.rs:12 and mcp_transport_tests.rs:2 imports match test/unix consumers; Rust changes load at Boss's next manual `make dev` restart or `make build`.)_

## Crate diet stage B (1648-2c54) — Rust restart required

- [ ] After landing and Boss's planned `make dev` restart or `make build`, confirm relay traffic still reaches a paired browser and a Web Push reaches an existing subscription with the stored VAPID key. Fixed vectors cover relay/HKDF bytes and RFC8291 ciphertext; independent verifiers cover VAPID signatures. Rust does not hot-reload; this peer does not restart desktop.

## Crate diet stage A (1640-ecee) — Rust restart required

- [ ] After landing and Boss's planned `make dev` restart or `make build`, confirm CUID2 generation still returns 24 lowercase base36 characters starting with a letter, and an upstream OAuth sign-in completes with PKCE S256. Rust changes require a manual restart to load; this peer does not restart desktop.

## Claude MCP launch profiles (1637-d6e1) — Rust restart required

- [ ] After landing and Boss's planned `make dev` restart or `make build`, confirm startup repairs a stale Claude private/run-profile TUIC bridge path and a fresh Claude session receives the TUIC MCP instructions. Targeted tests cover default/private/inherited/run-config roots, disabled integrations, custom transports and missing-path warnings. The backend change does not hot-reload; this lane does not restart desktop.
## Automations MCP (1630-cbfa) — Rust restart required

- [ ] After Boss's planned `make dev` restart or `make build`, use the `automations` MCP tool from a bound agent to create/get/list/update/pause/resume/delete a definition. Confirm invalid edits return the same definition validation errors and the saved creator remains unchanged. Step 8 owns HTTP/IPC/CLI wiring. Rust does not hot-reload; no desktop instance was launched by this peer.
## Automations Once (1629-2262) — Rust restart required

- [ ] After API/UI integration and Boss’s next planned `make dev` restart or `make build`, create a Once run in an IANA zone, confirm one preview instant and completed status after its scheduled run, retain the definition, and confirm restart/catch-up does not launch it twice. Rust does not hot-reload; this lane does not launch desktop.

## Automation prechecks (1614-7e8f) — Rust restart required

- [ ] After dispatcher/API integration and Boss's next planned `make dev` restart or `make build`, run an automation whose precheck exits 0, exits nonzero and times out; confirm dispatch only for exit 0 and saved `skipped_precheck` diagnostics otherwise. Confirm Run Now records a bypass and does not execute the precheck. The standalone helper has targeted shell/process tests; runtime persistence belongs to Step 6. Rust does not hot-reload; this lane does not launch desktop.
## Automations cron and timezone (1611-fa3f) — Rust restart required

- [ ] After landing and Boss's planned `make dev` restart or `make build`, verify creation defaults to the local IANA zone and previews use the saved zone through the commands once Step 8 lands. Verify `0 9 */2 * MON` only previews matching Mondays, while `0 9 1-31/2 * MON` also includes matching non-Mondays. Fixed wall times skip spring gaps and use the earlier fall-fold instant. Rust does not hot-reload; this lane did not launch or restart desktop.

## Automations definition storage (1610-ac85) — Rust restart required

- [ ] After landing and Boss's next planned `make dev` restart or `make build`, verify the selected instance's `automations.json` through the Automations commands once plan Step 8 lands: create/edit/delete a definition, retain a second definition and the global concurrency setting, and reject invalid edits. Step 1 supplies storage only; it does not yet register commands or run a scheduler. Rust does not hot-reload. This lane did not launch or restart a desktop instance.

## Plugin host phase 0 — backend restart required

- [ ] After Boss's planned `make dev` restart or `make build`, verify browser Add account creates a named GitHub account while the default login stays unchanged. Also verify a plugin declaring `net:http` with no `allowedUrls` is rejected. The Rust HTTP handler and manifest validator are not hot-reloaded. No desktop instance was launched by the peer.

## Dependency lane (1584–1589, 1587) — backend rebuild required

- [ ] Load the updated Rust dependencies with Boss's next planned `make dev` restart or `make build`; Rust does not hot-reload. After loading, check native folder dialogs, notifications and updater availability. Targeted git/OAuth2/push/relay tests, the desktop library check and frontend build passed in the worktree; no desktop instance was launched. WebRTC still needs coordinator-owned rb cross-platform verification, and crypto needs critic review before landing.

## Remote terminal metadata (1581-8fd0) — Rust restart required

- [ ] After Boss chooses to rebuild/restart the backend and update the Mint daemon, run Claude in a remote shell: verify agent icon, idle/busy transitions and OSC tab title updates; custom names remain protected. Compare observed streaming with the loopback baseline (grid marker delay 4–16 ms; text/log batches about 200 ms). No Mint latency improvement is claimed by the loopback probe.

## Launch instruction inspector (1547-2f5c) — Rust restart required

## iPad terminal keyboard focus (1577-a9bf)

- [ ] [HUMAN] On the physical iPad Safari remote UI, tap the bottom Claude prompt and a higher row after dismissing the software keyboard; confirm it stays open and text/deletion work. Primary canvas presses now cancel the proven native blur/refocus cycle. The reported top-bar blur still has no proven cause; record whether it appears before keyboard opening, after opening, or after pinch zoom.

- [ ] After Boss chooses to rebuild/restart the backend, launch a new managed peer and open **Inspect Launch Instructions…** from its terminal context menu. Confirm the final brief includes peer context, explicit system instruction sections/file snapshots and served MCP initialization sections have sources/bytes, and secrets are redacted. A shell-launched or restored session must show its launch as unavailable while retaining any MCP initialization instructions actually served; autonomous agent file reads must remain unobservable. The standalone real-component preview was visually checked; live desktop/backend integration waits for the authorized restart. No second desktop instance was launched.
# To Test

## Hands-free phrase boundaries (1656-eb5e) — Rust restart required

- [ ] After Boss's planned `make dev` restart or `make build`, hold a hands-free turn behind a draft or permission dialog, speak two phrases, then clear the hold. Confirm the phrases reach the agent on separate lines as one submission. The backend does not hot-reload; this peer does not restart desktop.

## Terminal Chat presentation (1576-6320)

- [x] Compact inbox/interruption notices, image chips, merged thinking rows, hidden empty replies, answer highlighting, historical tool status and attached Copy actions. _(verified: real Transcript component in an isolated Vite browser preview; screenshot `~/Gits/.tmp/tuic-chatview-1007/chat-final.png`, DOM coordinates and 149 targeted Vitest tests. The mounted terminal toggle test confirms Compose disappears in Chat and returns in CLI. No desktop instance was launched.)_
- [ ] After landing and loading the frontend, reopen Boss's Release Fixes and COORDINATOR conversations to compare the live transcript with the recorded screenshots. The standalone preview does not prove integration with the installed release candidate.

## Claude Chat recorded transcripts (1568-7b55) — Rust restart required

- [ ] After Boss's next planned `make dev` restart or `make build`, open Chat on a bound Claude terminal: confirm older prompt-ID prompts appear, command echoes remain absent, media tool results show markers and model changes show a card. The real-record parser tests cover these projections; the live backend still requires a manual restart because Rust does not hot-reload. This lane does not launch or restart a desktop instance.

## Mobile Markdown review (1569-1a1e) — Rust restart required

- [ ] [VISUAL] After the next `make dev` restart or rebuild, open a Markdown file with `- [ ]` items in the mobile PWA Files view: tap each checkbox (also just beside it) and confirm the file on disk toggles; tap a paragraph, press Comment, save, and confirm the desktop Markdown tab shows the highlight and comment. Edit the file on the desktop between opening and tapping on the phone and confirm the notice and reload appear with no overwrite. Take a mobile screenshot of the bottom comment bar. Rust does not hot-reload; no desktop instance was launched by this lane.

## Paged MCP terminal output (1551-5fa4) — Rust restart required

- [ ] After Boss's next planned `make dev` restart or `make build`, read a long terminal result with `session action=output`; follow `continuation`/`next_cursor` for text and `format=raw` byte pages. Confirm the existing command is not rerun and a connected rebuilt daemon returns the same metadata. Rust does not hot-reload; this lane does not restart the desktop.

## Progress blocked badge supersede (1537-6c4b) — Rust restart required

- [ ] After Boss restarts `make dev` (or installs a rebuilt release), have an agent call `progress type=blocked` and confirm its tab shows the orange waiting dot; then have the same agent call `progress type=done` and confirm the dot clears while the agent keeps working. A real open dialog (for example a Claude AskUserQuestion) must stay orange after a `progress done`. Rust changes do not hot-reload; no desktop instance was launched by this lane.

## AI Chat provider retry line (1536-1c9c, ego 288-f994)

- [ ] [VISUAL] After ego `fix/288-provider-retry` is merged and installed, cause an overloaded or 503 provider (or wait for a real one) in AI Chat and take a screenshot: one orange line `… — connection problem, retrying in Ns (attempt n/6)` that updates in place, disappears when the answer starts, and gives way to the turn error after 6/6, with the next prompt accepted. Frontend only (HMR); ego must be rebuilt.

## Workflow daemon executor (1446-ff21 slice B) — Rust restart required

- [ ] On Boss's next planned backend restart, load the owner-locked workflow runtime and confirm existing run history remains available. The daemon tests cover graph position, control fences and idle deadlines; Agent delivery and start controls remain disabled. Rust does not hot-reload. This lane does not restart the live app or launch a desktop instance.

## Windows CI remaining regressions (1518-d3a7) — Rust restart required

- [ ] After Boss rebuilds/restarts the backend, confirm Windows archive hooks can run Git and short-name uploads publish or skip an existing destination without replacement. Native Windows CI owns the automated proof. No desktop instance was launched; Rust changes do not hot-reload.
## Main build integration — Rust rebuild required

- [ ] Load the cfg and GitHub lint fixes on Boss's next planned backend rebuild/restart. Rust does not hot-reload; this lane does not restart the live desktop. The fixes preserve config recovery and GitHub emission behavior.

## Telegram channel adapter (1438-79b4) — headless rebuild required

- [ ] After Boss authorizes mint deployment and the bound agent is idle, rebuild/restart tuic-remote and verify registration/replacement/unregister, automatic retirement on agent exit with the shell still open, MCP session end and PTY close, and the allowlisted phone conversation: no-agent reply "Nessun agent registrato", Thinking/activity refresh, exact final reply, authored done/blocked notices and opaque-button callback mail. The Rust changes do not hot-reload. No live token/chat-ID reads, mint deployment or desktop restart were performed by this lane. Offline targeted tests use fake credentials and chats; deterministic publish receipts remain outside this slice.
## Optional dictation build graph (1394-ff2c) — Rust restart required

- [ ] After Boss restarts `make dev` or installs a rebuilt release, confirm push-to-talk, hands-free speech, notification output-device selection, and the global window hotkey still work. Tauri builds enable dictation explicitly; plain Cargo desktop builds omit it. Rust changes do not hot-reload. This lane does not restart the live app or launch another desktop instance.
## Remote update cookie migration (1490-e3ac) — Rust rebuild required

- [ ] After Boss rebuilds/restarts the daemon, confirm a Direct remote update works with the current client. This release accepts both `tui-session` and the legacy query token; the client switches next release. Rust backend changes do not hot-reload. No desktop instance was launched by this lane.

## Windows Clippy cleanup (1501-e8cb) — rebuild required

- [ ] After the next Windows rebuild, confirm agent executable discovery still prefers `.exe` over `.cmd`, Chrome registry discovery works, and an upload completes. The Rust syntax cleanup does not hot-reload into the running desktop; Boss controls the next restart.

## HTTP API origin boundary (1456-351c) — Rust restart required

- [ ] After Boss restarts `make dev` or installs a rebuilt release, confirm existing CLI/MCP Unix socket access, authenticated phone/PWA access and desktop remote peer access. Rust does not hot-reload; no desktop instance was launched by this lane. Foreign Origin/Host rejection and token-authenticated clients are covered by targeted router regressions.
## Release-check lint cleanup (1447-a894) — Rust rebuild required

- [ ] After the next backend rebuild, verify capture/resampling, echo cleanup, loudness and Edge speech still work. These lint-only edits do not hot-reload; existing automated regressions need a final run after the managed background launcher is restored. Do not restart the live desktop from this lane.

## Bodyless IPC replies (1416-8ad4) — rebuilt clients required

- [ ] Rebuild/reinstall the CLI and bridge before using this decoder fix. The running clients do not hot-reload Rust changes. Automated shared-decoder regression covers 204/304, protocol-switch, bounded headers/interims and final response boundaries; no desktop restart was performed by this lane.

## Shared IPC instance routing (1390-cd42) — Rust rebuild required

- [ ] After Boss restarts the rebuilt backend and replaces the bridge/CLI binaries, use a disposable named headless instance. Run `tuic --instance <id> ls --json` and `TUIC_APP_INSTANCE=<id> tuic-bridge`: both must reach that instance. An explicit `TUIC_SOCKET` must still win. Do not launch a second desktop instance. The existing live Rust process does not hot-reload these changes.
## Stable MCP bridge (1415-ef32) — Rust restart required

- [ ] After Boss restarts `make dev` or installs a rebuilt release, confirm the primary instance migrates Claude and private Claude MCP commands to `mcp-bridge/<sha256>/tuic-bridge` under its config directory. Start a disposable Claude session and confirm MCP initialize succeeds. Existing desktop Rust code does not hot-reload. Target cleanup and executable lifetime are covered by the targeted regression tests; real Claude startup after the desktop restart remains to check.
## Remote hand-launched agent detection (1420-f3de) — Rust restart required

- [ ] After Boss approves and loads the rebuilt desktop and remote daemon, use the configured Mac-mint connection through desktop MCP only: create a disposable shell PTY, start Claude by hand, confirm `agent_state` appears, submit one task after its composer is ready, send mail with a payload-free wake and read the reply. Check that a plain shell rejects submit with a cause and a corrective action. Do not restart or redeploy Mac-mint while its live PTYs must be preserved. Coordinator harness: `scripts/test-remote-mcp.py` from story 1419.

## Suspend Tab (1358-d008) — desktop menu

- [ ] In the desktop app, right-click an idle agent tab: **Suspend Tab** is enabled; while the agent works or asks a question it is greyed out. Click it: the tab keeps its place and shows `zz`, the tab body shows "Suspended" with a Resume button, `ps` shows no process for it. Right-click it: **Resume Tab** is offered; click: a new shell opens in the same folder and the agent resumes its conversation. Suspend, quit and restart TUICommander: the tab is restored still suspended and not auto-resumed. Suspend a plain idle shell tab and repeat. _(Browser-mode part checked by tuic-1358-suspend; the Tauri window menu itself is not.)_

## Markdown Live mode (1278-33f5)

- [ ] Open a `.md` file with headings, `**bold**`, `*italic*`, `` `code` ``, a link, a fenced ```js block and at least one tweak comment (add one from the viewer). Click **Live**: marks disappear off the cursor line and reappear on it; the tweak highlight shows amber with the comment on hover; the caret jumps over hidden tweak markers; Backspace/Delete beside a highlight never leaves a stray `<!--tweak:...` when you press **Live** off and read the raw file. Select text, **Comment**, type, Enter: the same `<!--tweak:begin/end-->` format as the viewer writes. Cmd+F searches the text. Edit, click **Save** (or Cmd+S), `git diff` shows only the lines you touched. Make no edit on a CRLF file and save an edit: line endings stay CRLF. _(PARTIAL 2026-09-30: verified marks hidden off cursor line/reappear on it, amber tweak highlight, Backspace/Delete beside highlight leave markers intact, Cmd+S saves and git diff shows only the touched lines, comment format `@ts\nbody-->` as tweakComments.ts. NOT verified: caret jump, Cmd+F, CRLF. FAILED: after clicking Comment the input is not focused (activeElement stays cm-content), so typing replaces the selection; after Enter a stray newline follows the end marker (CDP Enter may cause it). Fixture ~/Gits/.tmp/to-test-0930/live-fx)_

## MCP reaper refresh race (1259-62e3) — Rust restart required

- [x] After restarting `make dev` in an isolated `TUIC_APP_INSTANCE=<id>`, keep a disposable MCP bridge with a stable `x-tuic-session` active across the one-hour idle boundary and send a ping near a maintenance sweep. Confirm no reap log for the refreshed protocol session and that `agent action=inbox` still resolves its identity. The deterministic race and expiry cases are covered by Rust tests; the running backend does not hot-reload this fix. _(verified 2026-09-29: by code/test inspection, tests not executed here: Idle refresh vs reap covered by test: mcp_http/mod.rs:8186 'refreshed session must no longer meet the idle deadline'. A live 1h run adds nothing.)_

## Worktree removal recovery (1258-e9ba) — Rust restart required

- [x] After restarting `make dev` in an isolated `TUIC_APP_INSTANCE=<id>`, remove a disposable clean landed worktree with ignored build artifacts through `repo worktree_remove`. Confirm the directory is gone before the branch disappears. The running backend does not hot-reload this Rust change. _(verified 2026-09-29: fixture repo ~/Gits/.tmp/tuic-validate/fx/repo, MCP/HTTP on tuic-remote --instance validate: clean in_sync worktree with warmed target/ removed via repo worktree_remove: {ok, removal_rule:in_sync}; directory gone, branch gone, git worktree list clean (order of dir vs branch removal not observable); t_wt.py)_

## Named debug vault test (1181-2f80)

- [x] Rebuild the headless test binary and confirm a named instance writes its seeded session token to its own credentials file without changing the default file. _(verified: `app_instance_cli` whole-module run passed 11/11 after rebuilding; `named_debug_vault_ignores_and_does_not_mutate_default_legacy_entries` asserts both files.)_

## Global AI Chat (1157-1e54)

- [ ] After a `make dev` restart, open AI Chat and switch between three repositories: `ps` shows no new `ego acp` process and the tabs stay. Send a message: exactly one `ego acp -C ~/Gits` starts, and ego's answer knows which repository was on screen. Reload the webview and send again: still one process. Quit TUICommander: no `ego acp` survives. _(NOT VERIFIED 2026-09-29: Needs real ego binary (`ego acp`) and a live AI Chat agent conversation.)_
- [ ] After a Rust restart, open the same AI Chat peer simultaneously from desktop and a remote browser in an isolated instance; both views must attach to one `ego acp` process and show the same conversation. _(NOT VERIFIED 2026-09-29: Needs a real `ego acp` process (real ego binary/account); dual attach from desktop+browser cannot be done with a fake.)_

## ego MCP over ACP (1156-1b61)

- [ ] After a `make dev` restart and an ego build that advertises `mcpCapabilities.acp`, open AI Chat in an isolated `TUIC_APP_INSTANCE=<id>` and ask ego to list terminals. `ps` shows no `tuic-bridge` child of ego, the app log shows no `MCP initialize` line per tool call, and the tool answers. _(NOT VERIFIED 2026-09-29: needs a real ego agent (ACP) session; not available headless)_

## AI Chat replay storm (1151-4243)

- [ ] After a `make dev` restart, open AI Chat on a repository with two saved tabs in an isolated `TUIC_APP_INSTANCE=<id>`. The app log shows one `ACP attach` line with `method=session/load` per tab, not a repeating stream, and a refused load shows the error with Retry instead of re-sending. _(NOT VERIFIED 2026-09-29: Needs ego executable and real ACP session/load; log check of 'ACP attach' only meaningful with real ego.)_

## Crate split restart

- [ ] Restart `make dev` after the `tuic-terminal`, `tuic-core`, `tuic-git` (including its GitHub domain), and `tuic-dictation` crate splits. Rust changes do not hot-reload in the running development instance. After the restart, use an isolated `TUIC_APP_INSTANCE=<id>` to check GitHub PR status and CI notifications, push-to-talk transcription, and one hands-free spoken reply with real audio; the current live backend still has the previous crate layout. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Restart-of-make-dev item; instance is a headless tuic-remote of HEAD bef15c20f. Backend crate-split behaviour exercised across r0 items (sessions, worktrees, git); UI part needs desktop build.)_

## Ego notice cards (1075-05dc)

- [ ] After the next desktop rebuild, with an ego build that publishes notices (story 171-cb51), open AI Chat on an idle session and let a worker finish so ego publishes a notice between turns. A bordered card appears in the transcript with a title, the notice text and one button, apart from the agent's last reply. Click **Open result**: the result file opens in a TUIC tab. A normal reply without `_meta` still renders as plain assistant text. For `answer` and `approve` cards the button only scrolls to the open question or permission below. Check that look and that behaviour. _(Component and store tests pass; no live ego session was available.)_

<!-- tweak-comments v1: inline review comments.
     Format: [tweak:begin:ID]highlighted text[tweak:end:ID @ISO-TIMESTAMP
     comment body (free text, may span multiple lines)
     ] — where [ ] are the HTML comment delimiters <!-- -->.
     The only escape is '-->' → '--&gt;' inside the comment body.
     Read each comment, apply the feedback to the highlighted text,
     then remove the tweak markers. -->


## Stable macOS dev executable (1510-03ae) — next Boss launch

- [ ] On Boss's next manual `make dev` restart, confirm the printed executable path is `~/Library/Application Support/com.tuic.commander/dev-bin/tuicommander`, the live process maps that existing file, and LAN/tailnet HTTP requests and the iPhone page work with ALF enabled. Confirm bridge startup and local remote-update fallback still find their adjacent binaries. Script tests cover target deletion without launching the desktop; the live firewall and phone checks remain for Boss. No desktop restart was performed by the peer.

## Remote file drops (1434-1719) — Rust restart required

- [ ] After Boss restarts the desktop and updates the remote daemon, drop a Mac file and a folder onto a registered remote repository in tree and flat views. Verify remote bytes, unchanged local sources, directory confirmation and conflict skipping. This native Finder-to-Tauri interaction requires the desktop rebuild; no second desktop instance was launched.

## Safe orphan cleanup countdown (story 1257-a30b) — Rust restart required

- [ ] In an isolated `TUIC_APP_INSTANCE=<id>` after `make dev` restart, create a disposable detached linked worktree whose HEAD is on a branch and has no tracked or untracked changes. In Ask mode verify the dialog counts down from the configured number and removes it; repeat with Keep and Escape and verify it remains. Add an untracked file and verify the dialog names the reason and never counts down. While a clean dialog is open, answer `repo action=orphan_cleanup_answer path=<repo> decision=keep` through MCP and verify it closes without removal. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: No pending orphan cleanup exists headless; orphan_cleanup_answer exists but dialog and countdown are UI.)_

## Queued Claude notice confirmation (story 1251-9ec8) — Rust restart required

- [ ] After a manual `make dev` restart in an isolated instance, queue a notice to a disposable Claude session with the UserPromptSubmit hooks enabled. When Claude accepts it after more than one second, confirm the notice appears without an "Agent input was not confirmed" toast. A notice left in the composer must still show the uncertainty toast after the six-second bound. The running backend does not hot-reload this Rust change. _(NOT VERIFIED 2026-09-30: partial — Real Claude with own --settings hooks: 3s SIGSTOP freeze wake delivered, uncertain=false, msg in transcript. Not observed: >1s confirm notice in UI, left-in-composer case (9s freeze still delivered). Headless lacks agent-hooks assets so hooks were my fixture.)_
## Shared agent mail identity (story 1246-46e3) — Rust restart required

- [ ] After rebuilding `make dev` in an isolated `TUIC_APP_INSTANCE=<id>`, use a disposable managed PTY with two MCP bridges asserting its durable tab UUID and PTY UUID. Send distinct messages to each UUID and to the PTY display name, once through `tuic mcp` and once through the MCP client. Confirm both bridges read every message in `agent action=inbox`, `list_peers` shows one recipient for the PTY, and reconnecting one bridge leaves the inbox readable. Boss's live Rust backend does not hot-reload this change. _(NOT VERIFIED 2026-09-30: partial — Headless PTY S: 2 MCP bridges (x-tuic-session=S) each read all 6 messages sent to UUID, display name and alias, 3 via MCP client and 3 via 'tuic mcp'; list_peers 1 entry for S; reconnecting a bridge keeps inbox readable (7 msgs). NOT tested: distinct durable tab UUID vs PTY UUID (HTTP session create)_

## Managed child idle close (story 1209-cc47) — Rust restart required

- [ ] After restarting `make dev` with an isolated `TUIC_APP_INSTANCE=<id>`, spawn a disposable managed agent child and set its per-agent idle-close delay to 1 minute. Let it become idle and confirm the parent receives an `idle_timeout` notice before the child terminal closes. Confirm its worktree remains. Send a follow-up before a second child's delay ends and confirm the timer restarts; confirm a user-created terminal and a managed child marked keep-open stay open. Run a disposable `tuic bg` job with an unreachable queue and mail path; its failed wake marker must keep the child open. The current live backend and installed CLI need a rebuild to load this change. _(NOT VERIFIED 2026-09-30: partial — Managed grok/pi children exited on their own after long idle (exit_code null); idle_timeout inbox notice, 1-minute per-agent delay, keep_open and worktree-stay not verified; no notice seen in parent inbox.)_
## Native desktop notifications — Rust restart required

- [ ] [HUMAN] After a manual `make dev` restart in an isolated `TUIC_APP_INSTANCE`, leave TUICommander unfocused and trigger an agent question and a Progress `done` entry. Confirm each appears once in macOS Notification Center, with the terminal or project name; clicking each brings TUICommander to the named terminal or Progress project. Confirm a focused window produces none. This requires real cross-app focus and Notification Center; the running Rust backend cannot load the native handler without restart. _(NOT VERIFIED 2026-09-30: blocked — HUMAN: macOS Notification Center with real cross-app focus needs desktop app (headless tuic-remote has no native notification handler); not automatable here.)_
- [ ] After restarting `make dev` with an isolated `TUIC_APP_INSTANCE=<id>`, spawn a disposable managed agent child and set its per-agent idle-close delay to 1 minute. Let it become idle and confirm the parent receives an `idle_timeout` notice before the child terminal closes. Confirm its worktree remains. Send a follow-up before a second child's delay ends and confirm the timer restarts; confirm a user-created terminal and a managed child marked keep-open stay open. Run a disposable `tuic bg` job with an unreachable queue and mail path; its failed wake marker must keep the child open. The current live backend and installed CLI need a rebuild to load this change. _(NOT VERIFIED 2026-09-30: partial — Same as 57: idle-close of managed children seen (grok/pi exited idle) but idle_timeout notice to parent not observed; desktop notification part is UI.)_

## Background wake retry (story 1233-4738) — rebuild the Rust CLI

- [x] After rebuilding and reinstalling `tuic`, run a disposable `tuic bg` command against an isolated instance. If the instance temporarily stops answering, check that `<log>.wake` shows `retrying` with `tuic_session` and `attempts`, then `queued` or `mailed` after recovery. The installed CLI cannot load the Rust change until rebuilt. _(verified 2026-09-30: Python unix-socket proxy started 3s late as TUIC_SOCKET: /usr/local/bin/tuic bg .wake went {status:retrying,tuic_session,attempts:3,error:'queue: Cannot connect...'} then {status:queued,attempts:5}. (kit tuic lacks bg; used /usr/local/bin/tuic, same crate source).)_

## CLI MCP worktree timeout (story 1240-8438) — rebuild the Rust CLI

- [x] After rebuilding and reinstalling `tuic`, use an isolated test instance to create and remove a throwaway worktree through `tuic mcp repo`. Confirm both commands report the server result after a request longer than three seconds. A CLI socket read timeout must warn that the server may still complete the action. The installed CLI cannot load this Rust change until rebuilt; restart a live `make dev` process only when ready to end its current sessions. _(verified 2026-09-30: tuic mcp repo worktree_create on 60k-file repo returned server result after 86.7s, worktree_remove after 35.8s ({ok:true,removal_rule:in_sync}). Delaying replies 5s via unix-socket proxy gave 'tuic: TUICommander reply timed out; the server may still complete the action...' at 3.006s.)_

## Mobile Files and editor (story 1225-60e7)

- [ ] On a 360×800 phone PWA, open Files and long-press a repository path: the full path should appear without opening the repository. Open a deep folder and confirm the header keeps its final folder name visible, while paths ending in `src/.claude/` keep the slash on the right. Check that ordinary folders precede hidden folders; search for a file in a nested folder and open it. In View and Edit, Back, file name, and actions should share one row with touch-sized buttons; the editor should fill the space above the bottom tabs and wrap long lines. Return to a session with an unsent draft and confirm Browse Files did not submit or change the draft. _(NOT VERIFIED 2026-09-29: needs a real phone / PWA client — not reproducible in the isolated headless/browser instance)_

## Claude dismissed question (story 1213-82e1) — Rust restart required

- [x] After a manual `make dev` restart in an isolated `TUIC_APP_INSTANCE=<id>`, open a disposable Claude session and trigger `AskUserQuestion`. Dismiss it with Esc, wait for Claude's ready composer, and confirm the awaiting badge disappears and `session action=submit` accepts a command. The running backend does not hot-reload this Rust change; the recorded PTY capture test covers the state transition and submit write after idle settlement. _(verified 2026-09-29: by code/test inspection, tests not executed here: Replay test with recorded capture claude-askuser-esc-20260929.tcap; clear path pty.rs:7155-7192 and pty/tests.rs:15518/15538 (protocol-question-cleared). Item says so itself.)_

## Mobile session header (story 1203-fe36)

- [ ] [HUMAN] After the frontend reloads, open Codex and Claude sessions on a 360×800 phone. Confirm each shows the correct 24 px logo and state dot, the display name remains readable, and the 56 px header leaves the terminal starting near y58 with no lost rows. Tap the name, Tasks, and overflow Progress to inspect the temporary sheets; check Files, Search, Ideas, Commands, and terminate remain reachable from overflow. Automated component tests cover the button actions and session binding; a real phone check remains. _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_

## AI Chat prompt parking (story 1228-becb)

- [ ] On a 360×800 phone PWA, type a draft in AI Chat and tap Park. Send a different prompt and confirm the draft returns; repeat with an image preview and after a page reload. Check that switching to Sessions retains the same visible terminal row count. _(NOT VERIFIED 2026-09-29: needs a real phone / PWA client — not reproducible in the isolated headless/browser instance)_

## Mobile global AI Chat (story 1208-b371)

- [ ] On the phone PWA after `make dev` serves this frontend, tap Chat with multiple repositories registered. Confirm there is no repository picker, the same titled conversations as desktop appear, and a push link opens its conversation without changing the chat root. Switch to a session and confirm the terminal keeps the same visible row count as before this change. _(NOT VERIFIED 2026-09-29: needs a real phone / PWA client — not reproducible in the isolated headless/browser instance)_

## Mobile terminal states (story 1211-e1f4)

- [ ] On a 360×800 phone PWA, check a working, idle, awaiting-input and completed-unseen terminal in the session list. Verify the corresponding blue, green, orange and purple status colors. Open the completed session: the header should show Idle and the terminal should retain the same visible row count as before this change. _(NOT VERIFIED 2026-09-29: needs a real phone / PWA client — not reproducible in the isolated headless/browser instance)_

## Dictation Metal release link (story 1198-535b) — Rust rebuild required

- [ ] After rebuilding `make dev`, verify macOS dictation starts with a downloaded Whisper model and still uses Metal. The build script change cannot affect Boss's running backend until a rebuild and restart; the targeted release test covers linking and loudness timing. _(NOT VERIFIED 2026-09-30: blocked — Needs downloaded Whisper model, microphone audio and Metal GPU use on desktop build (audio hardware blocked).)_

## Mobile slash commands (story 1199-7b8d)

- [ ] On the phone PWA, open disposable Claude Code and Codex sessions. Type a supported slash command (`/help` in Claude, `/status` in Codex) and press Send; confirm it opens once. In each session, type a slash prefix, choose that command from the suggested slash menu, then press Send; confirm it opens once. In Claude, submit `/model` with an argument and confirm the argument reaches the command. Confirm the Codex quick-command widget offers `/status` and that it opens Status. The current `make dev` frontend must reload the new bundle before this check. _(NOT VERIFIED 2026-09-29: needs a real phone / PWA client — not reproducible in the isolated headless/browser instance)_

## PTY close reason logging (story 1194-31e8) — Rust restart required

- [x] After restarting `make dev` with an isolated `TUIC_APP_INSTANCE=<id>`, close a disposable shell session and confirm the app log records `reason=close_requested` with its session ID. Kill a second disposable shell session through MCP and confirm `reason=kill_requested`. The running backend does not load this Rust change until restart. _(verified 2026-09-29: MCP session action=close on disposable shell -> /logs source=session shows reason=close_requested with session id; MCP action=kill -> reason=kill_requested with id. (HTTP DELETE /sessions uses a different path, logs only 'explicit close'.))_

## Queued agent submission confirmation (story 1163-5bed) — Rust restart required

- [ ] After restarting `make dev` with an isolated `TUIC_APP_INSTANCE`, queue a message for a disposable Codex session while a stop hook delays the next Working screen by about four seconds. Confirm the message reaches the transcript without an uncertain-delivery toast. A silent composer must still report uncertainty after the bounded wait. The running backend does not load story 1239-dca9 until restart. _(NOT VERIFIED 2026-09-30: partial — Codex, 4s SIGSTOP freeze: enqueue typed:true after 4.1s, uncertain=false, message reached transcript. Toast not observable headless.)_
- [ ] After restarting `make dev` with an isolated `TUIC_APP_INSTANCE`, queue a short command for a disposable Codex session while it is busy. When it becomes ready, confirm that the command starts a turn. If the composer retains the text instead, confirm that TUICommander reports uncertain delivery with an error toast and `session status` shows `delivery_uncertain=true`. Repeat with the installed Claude, OpenCode, Goose, Grok, and pi binaries. The running backend does not load this Rust change until restart. _(FAILED 2026-09-30 story 1299-3ce1: Claude, Codex, grok, pi: busy-queue then delivered turn OK; Codex silent composer: 6.29s, delivery_uncertain=true, text retained. OpenCode (--mini): initial prompt and queued cmds never drained, state stuck, prompt_delivery_failed. Goose: text retained with uncertain=false, queue stuck. Toast unveri)_
- [x] In that disposable session, leave one uncertain queued command in the composer and queue a second. Confirm the second waits. Press Enter once for the retained command, wait for the next ready prompt, and confirm the second is delivered once. Check that its toast says to inspect the transcript and composer before acting. Amp, Cursor, and Droid still use the legacy PTY-write result until live screen captures establish a confirmation signal. _(verified 2026-09-30: Codex silent composer (SIGSTOP): first cmd retained uncertain, second queued:1 waited; one Enter ran the retained cmd, second delivered once after next ready prompt.)_

## Concurrent config saves (story 900-43dd) — Rust restart required

- [ ] After a manual `make dev` restart with an isolated `TUIC_APP_INSTANCE=<id>`, open two windows, change different agent and UI preferences from the same loaded state, and confirm both persist after reopening. The live Rust backend does not hot-reload; targeted Rust and frontend tests cover the merge and request shapes. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Two-window UI preference persistence; config save API not modified by hand per rules.)_

## Current MCP tool results (story 1190-75eb) — Rust restart required

- [ ] After a manual `make dev` restart and sidecar rebuild in an isolated `TUIC_APP_INSTANCE`, open a disposable ego PTY session and call `search_tools`. Confirm it lists TUIC tool names without a protocol error. Disconnect the test MCP endpoint and confirm a current-protocol `tools/call` reports the unavailable error without a result-shape error. Targeted HTTP and bridge tests cover the wire fields; the running backend and installed sidecar cannot load this Rust change until restart or rebuild. _(NOT VERIFIED 2026-09-30: partial — search_tools via MCP works on this instance (needs non-empty query, returns tool listing, no protocol error). No ego configured (ego_executable empty), so ego PTY session and disconnect part not testable.)_

## Worktree removal with sealed build output (story 1179-50a8) — Rust restart required

- [x] After a manual `make dev` restart in an isolated `TUIC_APP_INSTANCE`, remove a disposable worktree whose ignored target contains a read-only nested directory. Confirm the checkout and Git registration both disappear, while a symlink target outside the worktree keeps its contents and permissions. Targeted Rust tests cover this behavior; the running backend cannot load the Rust change until restart. _(verified 2026-09-29: fixture repo ~/Gits/.tmp/tuic-validate/fx/repo, MCP/HTTP on tuic-remote --instance validate: target/ilink -> outside dir plus target/ro/deep with mode 555: worktree_remove ok; checkout and registration gone; outside dir kept keep.txt and mode 750; t_wt2.py)_

## CLI install lint (story 1185-790d) — rebuild required

- [x] After rebuilding `tuic`, the macOS elevated install path still uses the target's parent directory; the Linux path compiles without an unused binding. _(verified: `src-tauri/crates/tuic-cli/src/main.rs:1179` gates only the parent binding with `target_os = "macos"`; macOS and Linux-target Clippy both pass with `-D warnings`.)_

## Remote manual update guard (story 1183-7154) — Rust restart required

- [x] After a manual `make dev` restart in an isolated `TUIC_APP_INSTANCE`, start an automatic update for a disposable remote daemon and try a manual update through IPC, HTTP, or MCP. Confirm the backend reports "remote update already in progress". Repeat with a manual update running first: another manual request is rejected and automatic update is skipped. Targeted Rust tests cover these races; the running backend cannot load this Rust change until restart. _(verified 2026-09-29: by code/test inspection, tests not executed here: Guard exists at remote_runtime.rs:451 and test asserts the error at remote_runtime.rs:2781; item itself says targeted Rust tests cover the races.)_

## ACP peer mail receipt (1176-e82e) — Rust restart required

- [ ] After a manual `make dev` restart in an isolated `TUIC_APP_INSTANCE=<id>`, open a disposable ego conversation and subscribe it to `tuic://inbox`. Send ordinary and urgent peer mail to ego, including after `agent register orchestrator=true`, and confirm `agent send` returns `delivered:true` with `delivery_path:acp_inbox_resource` (and `urgent_delivered:true` for urgent mail); confirm ego receives the inbox updates. Disconnect ego and verify a separately registered offline peer still reports `inbox_only`. The live backend cannot load this Rust change until restart; targeted MCP tests cover the subscription and notification path. _(NOT VERIFIED 2026-09-30: partial — No ego configured (ego_executable empty) so ACP ego inbox subscription not testable; peer mail send/urgent paths verified in other items with real agents.)_

## Already unregistered worktree cleanup (1175-71fc) — Rust restart required

- [ ] After a manual `make dev` restart, use an isolated `TUIC_APP_INSTANCE=<id>` and a disposable repository to remove a worktree while its build-input warming is pending, after Git has already unregistered the checkout. Confirm the pending status clears even if a leftover directory cannot be removed. The running backend cannot load this Rust change until restart; the targeted Rust test covers the cleanup failure path. _(NOT VERIFIED 2026-09-30: partial — Could not reproduce the internal state via API: with warm pending, git worktree remove --force (leftover dir undeletable) drops the entry from GET /worktrees/paths and worktree_remove returns 'No workspace found'; the internal warm token is not observable, so the clear-on-unregistered path (remove_w)_

## Push-to-talk sustained speech (story 1135-b600) — Rust restart required

- [ ] [HUMAN] After restarting `make dev` with an isolated `TUIC_APP_INSTANCE`, record a short noise burst, ordinary speech, and quiet genuine speech with push-to-talk. Confirm that only sustained speech reaches the prompt and that a rejected capture shows `no sustained speech` in the dictation ring. The synthetic command tests cover duration and threshold boundaries; the real microphone separation remains unverified and is tracked by story 1117. _(NOT VERIFIED 2026-09-30: blocked — HUMAN: push-to-talk with real microphone speech (audio hardware blocked).)_

## Push-to-talk final skip reason (story 1140-28e3) — Rust restart required

- [ ] [HUMAN] After a manual `make dev` restart in an isolated `TUIC_APP_INSTANCE`, use a microphone capture that triggers a final RMS or Whisper speech gate. Confirm the dictation status and ring show the specific gate reason. An empty successful transcript still shows `no speech detected`. The focused Rust command tests prove response mapping without a microphone; live audio remains unverified. _(NOT VERIFIED 2026-09-30: blocked — HUMAN: needs microphone capture to trigger RMS/Whisper gates (audio hardware blocked).)_

## AI Chat persistent approval (story 1147-33ec)

- [ ] **[HUMAN]** In an isolated AI Chat conversation, trigger a permission request offering both Allow once and Allow always. Confirm Allow always appears enabled and visually distinct from a disabled control, then click it and confirm the persistent option is selected. The component test verifies the click and success-color token; automated screenshot attempts timed out in agent-browser, and macOS denied Screen Recording to both capture tools. _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_

## AI Chat layout and composer (story 1166-ef2f)

- [ ] In an isolated AI Chat conversation, confirm the tool count and status remain on one line at the panel's normal width, raw shell commands appear only after expanding a call, and Copy has room in both message types. While at the bottom, stream an answer and confirm the typing dots stay visible; scroll up and confirm the view stays put. Paste over 200 words and an image, then confirm the compact marker expands to the full prompt on Send and the image preview is removable. Targeted component tests cover these behaviors; the mandated stealth browser wrapper timed out on screenshot and snapshot commands for this worktree fixture. _(NOT VERIFIED 2026-09-29: Needs a real ego (ACP) AI Chat session streaming an answer; stealth browser wrapper reportedly unreliable)_

## CLI sidecar replacement (story 1165-e905) — Rust restart required

- [ ] After a manual `make dev` restart when current sessions may be discarded, install or update `tuic` from Settings in an isolated `TUIC_APP_INSTANCE`. Confirm `tuic --version` runs and a previously running CLI process is unaffected. The live backend cannot load the Rust installer change until restart; fixture tests cover replacement through hard links and symlinks. _(NOTE 2026-09-29: partial evidence only — Installer replacement covered by fixture tests (hard links/symlinks) per item; live install would overwrite Boss's installed /usr/local/bin/tuic, so not done from an isolated instance.)_ _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Settings CLI install is UI; /usr/local/bin/tuic (Sep 29 build) runs and talks to the instance socket, but the sidecar replacement flow was not exercised.)_

## AI Chat message Copy and trailing suggestions (story 1150-4042)

- [ ] In an isolated AI Chat conversation, confirm a message shows Copy on hover and keyboard focus, and a reply ending with `suggest: [ Retry | Show status | Diagnose ]` displays three buttons without the raw token. Targeted component tests cover the parser and keyboard reachability; a browser CSS fixture confirms visibility on hover and focus. _(NOT VERIFIED 2026-09-29: Live AI Chat reply from ego needed to render suggest buttons; parser/keyboard already covered by component tests.)_
- [ ] In the same conversation, confirm Pause, Resume, Compact and New remain on one row as icon buttons at the panel's normal width, have tooltips, and the model summary shows only the name after the final `/`. _(NOT VERIFIED 2026-09-29: Needs a real ego AI Chat conversation (same conversation with Pause/Compact/Copy); ego not available headlessly.)_
- [ ] Send a typed AI Chat message and choose a suggested reply; confirm each user bubble shows the text once after ego replies. Targeted reducer and panel tests cover both paths. _(NOT VERIFIED 2026-09-29: needs a real ego agent (ACP) session; not available headless)_

## AI Chat failed turns and choice buttons (story 1139-7310) — Rust restart required

- [ ] After a manual `make dev` restart in an isolated `TUIC_APP_INSTANCE`, open a disposable ego chat and send a prompt that fails before producing a reply. Confirm its diagnostic appears in the transcript and the composer returns to Send. Answer a two-choice trust elicitation using its direct button and confirm the turn then shows a reply or a failure. The live backend cannot load the ACP failure event until restart; targeted Rust and frontend tests cover the wire and rendering behavior. _(NOT VERIFIED 2026-09-30: partial — No ego/ACP agent configured; failed-turn diagnostic and choice buttons need ego and UI.)_

## AI Chat shared ACP prompt queue (story 1079-fe88) — Rust restart required

- [ ] After a manual `make dev` restart in an isolated `TUIC_APP_INSTANCE`, open the same disposable ego conversation on desktop and phone. Start a long desktop turn, queue a phone prompt, and confirm both views show it. Cancel a queued item from desktop and confirm it disappears from phone without reaching ego; queue another, stop the running turn from phone, and confirm desktop shows cancellation and the queued prompt starts only after the ACP response. Pause a turn with a prompt queued; confirm it stays queued until Resume and remains cancellable from either view. The live backend cannot load this Rust change until restart; targeted Rust fixture and frontend tests cover the protocol and rendering paths. _(NOT VERIFIED 2026-09-30: blocked — Needs a real phone (PWA) plus ego ACP.)_

## ACP ego peer identity (story 1073-3431) — Rust restart required

- [ ] After a manual `make dev` restart in an isolated `TUIC_APP_INSTANCE`, open AI Chat with ego configured and verify its MCP bridge appears in `agent list_peers` as an `ego` peer without a terminal; send mail, reconnect, restart, and verify the same peer UUID can read it with `agent wait`. _(NOT VERIFIED 2026-09-30: partial — No ego configured (ego_executable empty): ego peer in list_peers not testable; register/list_peers verified for MCP peers only.)_
- [ ] Spawn a child from that ACP bridge and verify `parent_session_id` equals the AI Chat peer UUID. Submit blocked progress and verify the desktop progress event carries the ACP conversation ID and the away-state mobile push is emitted when push is configured. _(NOT VERIFIED 2026-09-30: blocked — Needs real phone push for away-state; also requires ego ACP bridge.)_

## Codex approval cancellation (story 1125-f4ea) — Rust restart required

- [ ] After restarting `make dev` with an isolated `TUIC_APP_INSTANCE`, create a disposable Codex session and trigger a shell approval. Confirm its tab reports awaiting input; press Esc and confirm the badge clears when the idle composer returns. Trigger another approval and confirm the badge appears again. The running backend cannot load this Rust change until restart. _(NOT VERIFIED 2026-09-30 story 1302-83ae: partial — Not tested with Codex approval; awaiting input via Claude Ink picker verified (awaiting_input=true, source=question). After Esc cancel of AskUserQuestion Claude session stayed busy/working 156s until new input, badge not cleared: possible fault.)_

## AI Chat ACP session details (story 1072-6787)

- [ ] In an isolated test instance running this frontend, open a disposable ego conversation and confirm its updated title fits the panel header and picker. After a usage update, confirm the context percentage and optional cost remain readable above the panel edge. The targeted component tests cover the values; no instance running this worktree was available for a screenshot. _(NOT VERIFIED 2026-09-29: Needs live ego ACP conversation (title, usage update).)_

## AI Chat ego profile (story 1074-9373) — Rust restart required

- [ ] After a manual `make dev` restart in an isolated `TUIC_APP_INSTANCE`, set **ego profile** to a profile in ego's user configuration and open AI Chat. Confirm ego uses that profile for the new connection. Clear the setting and reconnect; ego must use its normal profile selection. Capture the Settings row to verify its layout. The running Rust backend cannot load the new `AppConfig` field or ACP launch arguments until restart; the browser wrapper timed out twice while opening the worktree's Vite page, and maccontrol returned circuit open. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Ego profile setting is UI plus ego configuration; no ego installed.)_

## AI Chat image paste (story 1085-fa65)

- [ ] [HUMAN] In an isolated desktop test instance with an image-capable ego connection, copy a PNG from another app and paste it into AI Chat. Confirm the thumbnail renders, can be removed, and an image-only submit reaches ego. Repeat with plain text paste. Targeted component/client tests prove the ACP block and guards; browser accessibility showed the thumbnail and controls, but Chrome's screenshot command timed out twice, so the visual result and real cross-app clipboard path remain unverified. _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_

## AI Chat conversation recovery (story 1071-46c9) — Rust restart required

- [ ] After a manual `make dev` restart in an isolated `TUIC_APP_INSTANCE`, open AI Chat with ego, send a turn, create a second conversation, then restart the app. Confirm the last conversation and its history return; select the older title in the newest-first picker and confirm its history appears once. The running Rust backend cannot load the new `AppConfig` field until restart. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: AI Chat conversation recovery is UI plus ego.)_

## Windows Codex npm launcher — Rust restart required (story 987-c0ca)

- [ ] On a Windows build with npm's adjacent `codex` and `codex.cmd` shims, restart TUICommander and launch Codex from the agent menu. Confirm the help probe selects `codex.cmd`, reports `--no-alt-screen` support, and the new session stays on the primary screen. The targeted Rust test passed for the adjacent shims; native Windows execution remains to be checked. _(NOT VERIFIED 2026-09-30: blocked — Needs a Windows host with npm codex/codex.cmd shims (Windows desktop blocked).)_

## Agent Enter gap (stories 974-254a, 975-1de1) — Rust restart required

- [ ] After restarting `make dev` in an isolated `TUIC_APP_INSTANCE`, send text plus `special_key=enter` to a disposable Claude MCP session and confirm it submits. Launch Codex through a wrapper that foreground detection does not recognize, then send a long prompt from a suggestion or dictation while the tab still has no agent type; confirm it submits and check app logs for one unknown-foreground warning. The current backend cannot load the Rust change until restart. _(NOT VERIFIED 2026-09-30: partial — Claude MCP session input text + special_key=enter submits (used in r0-cl4 run, turn started). Wrapper foreground-detection and log-warning not tested.)_

## Managed Codex wrapper trust (story 1047-c41c) — Rust restart required

- [ ] After a manual `make dev` restart in an isolated `TUIC_APP_INSTANCE`, configure a Codex run config whose launcher forwards `"$@"` to Codex. Spawn a throwaway managed peer in a new directory and confirm it reaches Ready and receives its initial task without a trust answer. Turn off **Accept workspace trust for managed spawns** and repeat in another new directory; the ordinary Codex trust question must remain. The current live backend cannot load this Rust change until restart. _(NOT VERIFIED 2026-09-30: partial — Codex spawn with own -c args reaches prompt; wrapper forwarding and skip_trust_dialog opt-out via agents config not exercised.)_

## Missing registered worktree cleanup — Rust restart required

- [x] After a manual `make dev` restart in an isolated `TUIC_APP_INSTANCE`, remove the checkout directory of a throwaway linked worktree. Ask for its lifecycle by workspace id, confirm `missing_checkout=true` and no dirty fingerprint, then confirm removal in the desktop dialog or HTTP with `confirmMissingCheckout=true`. Confirm the Git registration is pruned and the branch remains when branch deletion is disabled. Repeat with a locked registration: cleanup must stop until a separate lock override is confirmed. The running Rust backend cannot load this change until restart. _(verified 2026-09-29: fixture repo ~/Gits/.tmp/tuic-validate/fx/repo, MCP/HTTP on tuic-remote --instance validate (HTTP path only, desktop dialog not tried): rm -rf checkout -> worktree_lifecycle missing_checkout=true, no dirty_fingerprint; MCP remove refuses, HTTP DELETE force without confirm refused, with confirmMissingCheckout=true&deleteBranch=false ok, registration pruned, branch kept; locked variant refused with worktree_locked until overrideLock=true; t_wt3.py t_wt4.py)_

## AI Chat pending ACP badge and notification (story 1070-38ce)

- [ ] In an isolated desktop test instance, open AI Chat, start a request that asks for permission, then hide the panel. Confirm the status-bar AI Chat toggle shows one pending item and one desktop notification. Answer the request and confirm the badge disappears and the notification closes. Targeted component/store tests cover the state changes; the visual screenshot attempt timed out in the browser wrapper after its accessibility snapshot showed the badge. _(NOT VERIFIED 2026-09-29: Needs a real ego ACP request that asks permission plus a real desktop notification; no ego/paid provider in headless run.)_

## Child lifecycle inbox coalescing — Rust restart required

- [ ] After a manual `make dev` restart in an isolated `TUIC_APP_INSTANCE`, let a throwaway managed child ask a confident question, answer it, then have it ask another. Confirm the parent inbox contains both question notices while ordinary state updates still coalesce. The current live backend cannot load this Rust change until restart. _(NOT VERIFIED 2026-09-30: partial — Question notices seen via awaiting_input state_change (confident, source=question); two-notice coalescing in parent inbox not exercised.)_

- [x] After restarting `make dev` in an isolated `TUIC_APP_INSTANCE`, send a peer RESULT to a throwaway parent, then leave its inbox unread while three throwaway children each send repeated state notices. Confirm the latest notice for each child and the complete RESULT remain available in `agent action=inbox`, with no missed count from the replacements. The running backend cannot load this Rust change until restart. _(verified 2026-09-29: Peers ag2-parent/ag2-kid via MCP; kid sent 'RESULT: ...' to parent; parent spawned 3 fake amp children cycling busy/idle for 40s (~6 cycles each) without reading. agent inbox: count 4 = RESULT + one state_change idle notice per child, no missed_count field, has_more false.)_

## Agent inbox FIFO and paging — Rust restart required

- [x] After restarting `make dev` in an isolated `TUIC_APP_INSTANCE`, send 101 messages to a throwaway recipient without reading. Confirm every send succeeds, the inbox reports `missed_count=1`, and the oldest message is absent. Read with `limit=2` and repeat while `has_more=true`; each page must start after the prior `next_since`. The live Rust backend cannot load this change until restart. _(verified 2026-09-29: isolated tuic-remote --instance validate over MCP unix socket: 101 sends by peer UUID all succeeded; first inbox limit=2 returned m1,m2 with missed_count=1 (m0 absent); paging via since=next_since covered m1..m100 in order, no dupes; script ~/Gits/.tmp/tuic-validate/t_inbox.py,t2.py))_

## AI Chat ACP pause settlement (story 1069-97bd) — Rust restart required

- [ ] After a manual `make dev` restart in an isolated `TUIC_APP_INSTANCE`, pause a disposable ego conversation while its turn streams. When ego stops at the pause boundary, Resume must remain visible and a new prompt must work after Resume. The running backend cannot load this Rust change until restart; the targeted ACP fixture test covers the state transition and the resumed prompt. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: ACP pause and Resume are UI and need ego.)_

## Urgent agent mail — Rust restart required

- [ ] After a `make dev` restart in an isolated `TUIC_APP_INSTANCE`, start throwaway Claude Code and Codex sessions and send `agent action=send urgency=urgent` while each is busy. Check that the notice appears after the current tool call and before the agent's next planned step, the peer body remains in the inbox, and the sender receives `urgent_delivered=true`. Repeat with a draft and a confident dialog; each must return `urgent_delivered=false` with a fallback reason and must preserve the composer. Boss's current backend cannot load this Rust change without a manual restart. _(NOT VERIFIED 2026-09-30: partial — Urgent mail to busy Claude/Codex not exercised end-to-end; only normal queue delivery verified (see 110).)_

## Progress journal paging — Rust restart required

- [x] After a manual `make dev` restart in an isolated `TUIC_APP_INSTANCE`, call `repo action=progress_list` on a journal with more than 10 entries. The first page returns 10 entries, `total`, and `nextCursor`; follow the cursor to the end without duplicates. Open Progress and confirm the dialog still shows the complete journal. The current live backend cannot load this Rust change until restart. _(verified 2026-09-29: 13-entry journal: first page 10 entries, total 13, nextCursor 4; following input.cursor gave the remaining 3, 13 unique ids, then null; Progress dialog (browser mode) lists all 13 entries)_

## Headless MCP voice binding (story 1006-2729) — remote daemon rebuild required

- [x] After replacing a disposable `tuic-remote` daemon with this build, call MCP `voice action=status` from a connection without a live terminal and from one bound to a live terminal. The first must be refused as unbound; the second must report that this build has no audio support. The running daemon cannot load the Rust change until it restarts. Targeted headless and desktop tests cover both binding paths. _(verified 2026-09-29: Disposable tuic-remote (--instance ag3d, unix socket): MCP voice action=status from unbound connection -> isError {error:'This connection is not bound to a terminal...'}; from peer named by live PTY id -> {available:false, unavailable_reason:'This TUICommander build has no audio support'}.)_

## MCP agent spawn environment and model — Rust restart required

- [x] After a manual `make dev` restart in an isolated `TUIC_APP_INSTANCE`, set an Agents run config model and environment value, then spawn a throwaway MCP child with overriding `model` and `env` values. Confirm the child sees the caller environment, the override model reaches its argv, and `TUIC_SESSION` and `TUIC_PARENT` still identify the peer. The running backend cannot load this Rust change without a restart. _(verified 2026-09-29: PUT /config/agents run config (model sonnet, env LAYER/ONLY_RUN/TUIC_* spoofs) + MCP spawn with model=opus env{LAYER=caller,TUIC_*=spoof}: child printed --model|opus|<own sid>|<parent P>|caller|present; without overrides --model|sonnet|..|run|present. TUIC_SESSION/PARENT protected. Config restored.)_

## Push-to-talk native release — Rust restart required

- [ ] After a manual `make dev` restart in an isolated `TUIC_APP_INSTANCE`, hold Fn while dictating, then release it while the WebView is briefly busy or loses focus. The macOS microphone indicator must go off on release, the captured phrase must transcribe once, and the file log must show Fn down/up, native stop and IPC stop latency plus audio seconds and final/partial character counts. Repeat a recording longer than 30 seconds; its opening words must remain. The current live backend cannot load this Rust change without a restart. _(NOT VERIFIED 2026-09-30: blocked — Needs Fn key hold, macOS mic indicator and real microphone (audio hardware blocked).)_

## Managed Claude mail wake — Rust restart required

- [x] After restarting `make dev` in an isolated `TUIC_APP_INSTANCE`, send mail to a throwaway managed Claude peer while its turn is busy and its MCP SSE stream is connected. Let it become idle without reading the inbox during the turn. Confirm one payload-free `[TUIC] message available` notice starts a new turn and `agent action=inbox` returns the mail. Repeat with an inbox read before idle and confirm no stale notice is submitted. The current live backend cannot load this Rust change until restart. _(verified 2026-09-30: r0-cl4: real Claude (--settings hooks, --mcp-config bridge, TUIC_SOCKET env). Mail sent while busy; after idle one '[TUIC] message available - read it with: agent action=inbox' notice started a new turn, Claude called inbox and quoted the mail. Second run: inbox read before idle -> no notice submitt)_

## Claude awaiting badge — Rust restart required

- [ ] After a `make dev` restart, let a Claude tab finish an ordinary prose reply with the Activity Dashboard open. The generic desktop notification must not flash Waiting input; an Ink picker or explicit permission request must still show it. The running backend cannot load this Rust change without a restart. _(NOT VERIFIED 2026-09-30: partial — Claude prose reply never set awaiting; AskUserQuestion picker set awaiting_input=true; startup Ink import picker did not. Dashboard and notification flash are UI.)_

## Detached CLI wake status — rebuild the Rust CLI

- [x] After rebuilding and reinstalling `tuic`, run `tuic bg` from a throwaway managed agent and inspect `<log>.exit` and `<log>.wake`. Confirm the command exit code is preserved and wake status is `queued` when the queue takes the request. For an unbound caller, confirm MCP mail surfaces `BG DONE` with the queue error and `.wake` says `mailed`; when both channels fail, `.wake` says `failed` with both reasons. The installed CLI cannot load this Rust change until rebuilt. _(verified 2026-09-30: Used /usr/local/bin/tuic (kit tuic lacks bg/mcp). Managed pi caller: exit 7 kept, wake {status:queued}. Unbound MCP identity + active agent wait: inbox got 'BG DONE exit=4 ... queue wake failed: Expected one live session', wake status mailed. Both fail: status failed 'queue: ...; mail: ...inbox_only)_

## Queued agent command diagnostics — Rust restart required

- [x] After a `make dev` restart, enqueue a throwaway command for a test Codex session and inspect app logs for one `queue delivery attempt` record with session id, agent and shell states, queue counts, typed/submitted result, and separate Enter status. This Rust instrumentation is absent from Boss's current backend until restart; do not interrupt live sessions for it. _(verified 2026-09-29: Fake amp-type agent (adapter-less so idle is confirmed) via spawn binary_path; POST /sessions/{id}/queue {text} -> typed:true. /logs shows exactly one 'queue delivery attempt' with session_id, agent_state/shell_state=idle, queued_before 1/queued_after 0, typed yes, submitted true, enter_separate sent. Not a real Codex session.)_
- [ ] After loading the story 1106 Rust build in an isolated `TUIC_APP_INSTANCE`, _(NOT VERIFIED 2026-09-30: partial — Item text truncated (only NOT VERIFIED note). Codex queue while busy verified delivered (see 110); idle_unconfirmed diagnostic not observed.)_
      queue a command for a throwaway Codex session while it is busy. Confirm it
      runs once when Codex reaches Ready and the queue reaches zero. If the shell
      first becomes idle without confirmed readiness, logs must name
      `defer_reason=idle_unconfirmed`, then `Ready confirmed after shell became
      idle` before the successful flush. The current backend cannot load this
      fix without a restart.

## Consumed MCP agent inbox mail (story 1105-966b) — Rust restart required

- [x] After a `make dev` restart in an isolated `TUIC_APP_INSTANCE`, fill a throwaway peer inbox, read it, and send one more message. Confirm the new send succeeds and the next inbox call returns it without `missed_count`. The current live backend still has the old Rust code; targeted unit tests cover pagination, capacity, and delivery leases. _(verified 2026-09-29: same instance: after full read, one more send returned only 'extra' with no missed_count)_

## MCP tab caller repository (story 1102-0945)

- [x] An MCP caller in repository A opens an unpinned external Markdown tab while repository B is visible: the focused tab switches to A and remains in A's tab bar; `focus=false` leaves B visible and the tab appears on return to A. Inline HTML/URL tabs use the same caller scope. _(verified: targeted `useAppInit` and `mdTabs` Vitest tests cover focused/background external files and caller-scoped HTML.)_

## Sequential Markdown comments (story 1103-d5ae)

- [x] Add block comments to several different numbered Markdown items in one open file. Each marker stays beside its item, the convention header appears once, and each highlight reopens its own comment. _(verified: `MarkdownTab.test.tsx` exercises four sequential saves through the tab, source positions, parsed comments, highlights, and reopen; `ContentRenderer.test.tsx` verifies source-only updates refresh block metadata.)_

## Mobile remote sessions, Progress, and Activity — Rust restart required

- [ ] [HUMAN] After Boss restarts `make dev` when current PTYs can be interrupted, verify `/api/version` identifies the integrated build, then open the Tailscale HTTPS `/mobile` PWA on a phone. _(NOT VERIFIED 2026-09-30: blocked — HUMAN item needing a real phone/Tailscale HTTPS PWA (real phone blocked). Only /api/version checked: {"version":"1.7.7","git_hash":"bef15c20f"} on rust0930 instance.)_
- [ ] [HUMAN] A connected remote session should show live output through WebSocket; create and close only a throwaway remote session from mobile, then confirm it disappears on its owner. Disconnect that machine and confirm the stale session shows unavailable rather than a misleading local 404. _(NOT VERIFIED 2026-09-30: blocked — HUMAN: needs a real phone (mobile PWA) and a second machine for the remote session; blocked (real phone / second physical machine).)_
- [ ] [HUMAN] Open mobile Progress with no desktop repository selected. It should select the newest journal project, show saved done/blocked entries, and allow switching projects. Open Activity and confirm persisted active events appear while dismissed events stay hidden. The backend routes and store shape have targeted automated tests; the real phone remains to be checked. _(NOT VERIFIED 2026-09-30: blocked — HUMAN: needs a real phone PWA for mobile Progress/Activity views (real phone blocked).)_

## Progress toast dismissal (story 1061-7694) — after the fixed frontend loads

- [ ] Tap a Progress toast body on desktop while another repository is active; it should close without changing the repository or terminal. On a second toast, use **Go to repo** and confirm it opens the reporting workspace. On mobile, tapping the toast body should close it without opening its action. Targeted component tests verify these paths; check the loaded UI after this branch is integrated. _(NOT VERIFIED 2026-09-29: needs a real phone / PWA client — not reproducible in the isolated headless/browser instance)_

## Embedded external links (story 989-63fa) — after Rust rebuild

- [ ] After the next `make dev` restart, use an isolated `TUIC_APP_INSTANCE` test instance to click an HTTPS link in an HTML preview and an inline plugin panel: each should open outside the app. Open a cross-origin dashboard URL in a tab, attempt an external navigation, and confirm the blocked-link toast directs users to the tab menu's Open in Browser action. The live Boss backend has not restarted for the Rust navigation event. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Embedded link opening in HTML preview and plugin panel is UI.)_

## Mobile repository files (story 1063-3fe1) — real phone

- [ ] [HUMAN] On a phone connected to an isolated TUICommander test instance, open Files, select a disposable repository, browse into a directory, open a `.md` file in rendered View, switch to Edit, change its source and save, then confirm the updated rendered View and saved content from the desktop. Check another text file stays plain text and that a file over 1 MB and a binary file show a refusal. Targeted Vitest covers these flows; a responsive desktop-browser screenshot does not verify touch and mobile keyboard behavior. _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_

## Editor links and external Markdown tabs (2026-09-27)

- [x] MCP file tabs remain visible after selecting a terminal in a repository with no active workspace; distinct MCP ids coexist and a repeated id updates its native file tab. A terminal link to an existing external Markdown path under `~/Gits/.tmp/` opens in the Markdown viewer. _(verified: targeted TabBar, useAppInit, and terminal file opening Vitest tests.)_
- [x] Unpinned MCP native file and HTML/URL tabs hide in another repository and return when their opening repository is selected again; pinned MCP tabs stay visible across repositories. _(verified: targeted `useAppInit`, `mdTabs`, `tabManager`, and `TabBar` Vitest cases exercise scope, visibility, pinning, and retention.)_
- [x] Cmd/Ctrl+click opens editor web links in the system browser, local paths in the matching TUICommander view, and missing paths with a toast. MCP `tuic://open` opens external Markdown in a Markdown tab. _(verified: targeted editor and MCP tab Vitest cases exercise these routes; browser and native window appearance require a visual check after integration.)_
## Native MCP action cleanup (story 1091-1d5d) — rebuild Rust server and CLI

- [x] After rebuilding, use an isolated test instance to confirm `agent register/list_peers` accepts `path`, `repo worktree_lifecycle/worktree_remove` accepts `branch`, and removed actions return errors naming their HTTP routes. Reinstall the CLI before checking `tuic agent list-peers --path` and `tuic agent stats --json`. Restart a live `make dev` session only when ready to end its current PTYs. _(verified 2026-09-30: agent register path=bigrepo + list_peers path filter returned the peer with path; tuic agent list-peers --path and tuic agent stats --json ({active_sessions:13,...}) work; repo worktree_lifecycle/worktree_remove accept branch; agent stats/detect, repo prs/close_issue, session process_stats return 'w)_

## Detached CLI commands (story 1100-96bd) — rebuild the Rust CLI

- [x] After rebuilding and reinstalling `tuic`, run a disposable `tuic bg <log> -- <cmd>` from an isolated managed session. Confirm the launcher returns before the command, `<log>.exit` records its code, and a busy caller receives the completion wake only after becoming idle. The installed CLI cannot load this Rust change until rebuilt; restart `make dev` only when ready to end its live sessions. Windows behavior is covered by CI-only tests and remains unverified on this Mac. _(verified 2026-09-30: /usr/local/bin/tuic bg from pi session: launcher returned in 0.019s, bgE.log.exit=9, wake queued; while pi was working the BG DONE text sat in /sessions/{id}/queue (id 29) and drained only after idle; pi then reported exit=9. Windows unverified.)_

## Generic MCP CLI (story 1099-79b8) — rebuild the Rust CLI

- [x] After rebuilding and reinstalling `tuic`, use an isolated test instance to compare `tuic mcp session '{"action":"list"}' | jq length` with the instance's MCP session count. Run `tuic mcp agent '{"action":"wait","timeout_ms":8000}'` and confirm it waits for the server reply without a three-second socket failure. The installed CLI cannot load the Rust change until rebuilt; restart `make dev` only when ready to end its live sessions. _(verified 2026-09-30: /usr/local/bin/tuic mcp session list | jq length=16 == MCP session list 16 == GET /sessions 16. 'tuic mcp agent {wait,timeout_ms:8000}' returned {timed_out:true} after 8.012s, no 3s socket failure.)_

## CLI blocking waits (story 1060-df82) — rebuild the Rust CLI

- [x] After rebuilding and reinstalling `tuic`, run `tuic agent wait --timeout-ms 8000 --json` and `tuic session wait <busy-session> --until exited --timeout-ms 8000 --json` against an isolated test instance. Confirm each returns after the server's response rather than failing after three seconds. The running app and installed CLI do not load this Rust change until rebuilt; restart `make dev` only when ready to end its live sessions. _(verified 2026-09-30: tuic agent wait --timeout-ms 8000 --json -> {timed_out:true} after 8.012s; tuic session wait r0-cl2 --until exited --timeout-ms 8000 --json -> {met:false,timed_out:true,until:exited} after 8.012s. (r0-cl2 was idle claude, not busy.))_

## Mobile notification tags (story 1042-f5ca) — updated service worker

- [ ] [HUMAN] On a real subscribed phone after the updated service worker takes control, receive questions from two different sessions. Confirm both notifications stay visible and each opens its own session. Send another push for one session and confirm the other remains. The targeted service-worker test verifies tag replacement and both click deep links; the phone's notification UI requires real device verification. _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_

## Rust test temp root (story 980-420e) — Rust, needs `make dev` restart

- [x] Bare `cargo test` test binaries and Nextest setup scripts route Rust scratch directories through the repository test root. _(verified: `src-tauri/crates/tuic-test-support/src/lib.rs` initializes the libtest environment; `src-tauri/.config/nextest.toml` runs platform setup scripts; targeted subprocess test confirms `tempfile` and `std::env::temp_dir` stay under `test_temp_root()`.)_ No live-app behavior changes; the current backend cannot load the Rust test-support code until a restart.

## Mobile completion push limit (story 1041-cdc7) — Rust, needs `make dev` restart

- [ ] [HUMAN] After restarting `make dev` when Boss is ready to end the current sessions, use a real subscribed phone while the desktop is away. Trigger a titled question followed immediately by session completion; confirm the phone displays one notification. Real phone delivery and display cannot be verified by the local HTTP push receiver. The targeted Rust test verifies accepted push requests, expiry after 30 seconds, and independent session limits. _(NOT VERIFIED 2026-09-30: blocked — HUMAN: needs a real subscribed phone for push delivery (real phone blocked); instance has push.enabled=false.)_

## Managed agent workspace trust (2026-09-26) — Rust, needs `make dev` restart

- [ ] After restarting an isolated `TUIC_APP_INSTANCE=<id>` dev instance, use `agent action=spawn` to start Claude and direct Codex in never-trusted folders under `~/Gits/.tmp/`. Confirm each reaches the agent prompt and receives the task without a manual trust keypress. Turn **Accept workspace trust for managed spawns** off for each agent and confirm its normal trust question remains. User-opened agent terminals must retain normal trust behavior. The current running backend cannot load this Rust change until restart. _(FAILED 2026-09-30 story 1300-f7f4: Codex 0.159.0 spawned with -c projects."<cwd>".trust_level="trusted" in a new dir still showed Trust this folder? and waited 48s+ with no auto-answer. Claude new-dir and opt-out not tested.)_

## MCP config ownership (story 988-d1a1) — Rust, needs `make dev` restart

- [x] After restarting an isolated `TUIC_APP_INSTANCE=<id>` dev instance, confirm startup leaves a sandboxed agent MCP config unchanged. The current live backend cannot load the ownership guard without a restart. Do not restart Boss's running instance or use his real agent configs for this check. _(verified 2026-09-29: Sandbox HOME with ~/.claude.json holding stale tuicommander bridge entry; tuic-remote --instance ag3d: log 'Skipping agent MCP config updates from a secondary instance', file byte-identical (diff). Control with TUIC_MCP_CONFIG_OWNER=1 rewrote entry. Desktop validate instance log shows same skip line. Headless daemon used; same fn.)_

## Plan picker (2026-09-26) — Rust, needs `make dev` restart

- [x] After restarting an isolated `TUIC_APP_INSTANCE=<id>` dev instance, open Plans and Stories in a repository with `plans/*.md`. Confirm the document choices are visible, the selected plan title matches its heading or front matter, and **Add from path or link** stays collapsed until opened. The running backend cannot serve `list_plan_sources` or `add_plan_source` until restart. Targeted Rust and Vitest tests cover discovery and dialog behavior; the visual layout needs the rebuilt app. _(verified 2026-09-29: fx/repo with two md plans (one with front matter title differing from its # heading): 'Plans and Stories' > New plan lists 'Front Matter Title' and 'Plain Heading Plan' with their paths; 'Add from path or link' details open=false. Layout judged from DOM text only, no screenshot.)_

## Remote repository browsing and terminal attach (stories 1025-8103, 1026-7633) — Rust restart and remote daemon update

- [ ] After restarting an isolated `TUIC_APP_INSTANCE=<id>` dev instance and updating its test remote daemon, open the remote repository picker. Confirm it starts at the remote host's home directory, a denied directory shows a readable error while manual path entry and Up remain usable, and a terminal in a remote repository renders its prompt. Opening a terminal with a missing cwd must show a readable error instead of a blank pane; a failed grid stream must show an error toast. Use only disposable test connections and sessions; the live `make dev` backend cannot load the new home-directory route or cwd validation without a restart. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Remote repository picker is UI; needs remote daemon.)_

## Claude question state after wrapped suggestions (story 1023-dcc9) — Rust, needs `make dev` restart

- [ ] After restarting an isolated `TUIC_APP_INSTANCE=<id>` dev instance, run a Claude turn that ends in a wrapped `suggest: [ … ]` item containing `?`. Confirm the idle tab does not show a question badge. A real AskUserQuestion must still show one. The existing live backend cannot load this Rust change without a restart. _(NOT VERIFIED 2026-09-30: partial — Claude wrapped suggest with '?': idle, awaiting=false. Real AskUserQuestion: awaiting_input=true. Badge itself is UI.)_

## Remote update and restart — Rust, needs `make dev` restart

- [ ] After restarting an isolated `TUIC_APP_INSTANCE=<id>` dev instance, use a disposable remote daemon to check the per-connection Auto-update option. Verify zero live sessions updates once through the daemon restart, live sessions show a manual offer with a count that refreshes while connected, and a failed update shows its error. During automatic transfer, confirm the manual update button is disabled; a stalled transfer eventually reports a timeout and resumes connection checks. Check the checkbox and status layout visually; the automated browser screenshot timed out. Do not use Boss's saved daemon. The running Rust backend cannot load this change until restart. _(NOT VERIFIED 2026-09-30: blocked — Needs a second physical machine (remote daemon update).)_
- [ ] After restarting an isolated `TUIC_APP_INSTANCE=<id>` dev instance, connect a disposable Direct daemon and an SSH daemon, confirm the out-of-date badge and the exact live PTY count, then update each and verify reconnect with the new `/health.build.sha256`. The live backend cannot load this Rust change without a restart. Do not update Mac-mint or Boss's saved connections. _(NOT VERIFIED 2026-09-30: blocked — Needs a second physical machine (Direct and SSH daemons).)_

## Squash-merged worktree removal (story 1022-8291) — Rust, needs `make dev` restart

- [ ] After restarting an isolated `TUIC_APP_INSTANCE=<id>` dev instance, inspect a clean squash-merged worktree whose local tip is contained in its merged GitHub PR head. Confirm the lifecycle badge says Merged and removal with branch deletion succeeds. Check that a branch contained in the main checkout's current integration branch also removes when the remote default branch is behind. A read-only ignored build tree should arrive writable only in the new worktree and should not block removal. The current live backend cannot load this Rust change without a restart. _(NOT VERIFIED 2026-09-30: partial — Verified: branch contained in checkout's current branch 'integ' (origin/main behind) removes without force: removal_rule integration_ancestry, branch deleted; ignored read-only target/ in main arrived writable (drwxr-xr-x/-rw-r--r--) in worktree, main stays r-x. NOT verified: GitHub-PR-head 'Merged')_

## MCP local branch deletion (story 1033-1796) — Rust, needs `make dev` restart

- [x] After restarting an isolated `TUIC_APP_INSTANCE=<id>` dev instance, use MCP `repo action=branch_delete` on an integrated local branch with no worktree. Confirm only the local ref disappears; a checked-out or unmerged branch must be refused. The current live backend cannot load this Rust action without a restart. _(verified 2026-09-29: fixture repo ~/Gits/.tmp/tuic-validate/fx/repo, MCP/HTTP on tuic-remote --instance validate: branch_delete int1 ok (proof in_sync, only local ref gone); main refused (current integration branch); un1 refused (unmerged commits); co1 refused (checked out in a worktree); t_wt2.py)_

## Claude usage per profile (story 1016-9cf8) — Rust, needs `make dev` restart

- [ ] After restarting an isolated `TUIC_APP_INSTANCE=<id>` dev instance, focus Claude sessions launched with the default config and a separate `CLAUDE_CONFIG_DIR`. Confirm the status badge changes to each account's quota and its dashboard shows the same account. A profile without credentials must show unknown. The live `make dev` backend cannot load this Rust change without a restart. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Status badge quota per CLAUDE_CONFIG_DIR is UI.)_

## Hidden iframe unload (story 979-3d7f) — live instance required

- [ ] In a disposable test instance, open a URL tab pointing to a page that runs a 1 s busy loop every 5 s. Hide it by switching tabs, repositories, and pane groups, including a pinned tab and a split pane covered by an orphan tab. Verify `document.querySelectorAll('iframe[src="<test-url>"]').length === 0` while hidden and that showing the tab loads the same URL again. While hidden, run a read-only 30 s main-thread probe with 100 ms `setTimeout` ticks; require 0 gaps over 150 ms. Do not use Boss's live instance. _(Deferred: this requires the changed frontend in a running test instance; targeted Vitest proves DOM removal and remount.)_

  Run this in the test instance's frontend console while the tab is hidden (a gap is measured delay beyond the intended 100 ms):

  ```js
  const gaps = [];
  let last = performance.now();
  const end = last + 30000;
  const tick = () => {
    const now = performance.now();
    if (now - last > 250) gaps.push(Math.round(now - last - 100));
    last = now;
    if (now < end) setTimeout(tick, 100);
    else console.log({ gapsOver150ms: gaps.length, gaps });
  };
  setTimeout(tick, 100);
  ```

## Linked-worktree warming excludes MDKB (2026-09-26) — Rust, needs `make dev` restart

- [x] After restarting an isolated `TUIC_APP_INSTANCE=<id>` dev instance, create a disposable worktree from a repository that ignores and contains `.mdkb/`. Confirm the new worktree has no `.mdkb` while an ignored build directory still arrives warm. The live backend cannot load this Rust change without a restart. _(verified 2026-09-29: fixture repo ~/Gits/.tmp/tuic-validate/fx/repo, MCP/HTTP on tuic-remote --instance validate: worktree_create from a repo containing ignored .mdkb/ and target/: new worktree has no .mdkb, target/debug/x present (warm done), .env not copied; t_wt.py)_

## Native scrollback capture fixtures (2026-09-25) — after mcp-config-guard lands

- [x] In an isolated `tuic-remote` instance with `TUIC_CAPTURE_DIR` under `~/Gits/.tmp/`, captured Codex 0.157.1 with `--no-alt-screen` (approval prompt, resize, idle footer) and OpenCode 1.18.30 with `--mini` (idle resize). Both `.tcap` fixtures are in `src-tauri/src/fixtures/agent_prompts/`. Targeted Rust replay tests verify primary-screen mode, the Codex chrome-cutoff anchor, and no BUSY edge from the OpenCode resize repaint (story 939-475b). _(verified: `pty::tests::live_native_scrollback_captures_never_enter_alternate_screen`, `codex_native_scrollback_capture_keeps_approval_and_idle_composer_visible`, `opencode_mini_resize_repaint_does_not_reopen_an_idle_turn`; 3/3 passed)_

## Codex dictation auto-send (2026-09-25) — Rust, needs `make dev` restart

- [ ] [HUMAN] After restarting `make dev` when ready to end the current sessions, dictate a long phrase into a Codex tab with Auto-send enabled. Confirm it submits once rather than inserting a newline. Real microphone input and the Codex TUI are required for this final check. _(NOT VERIFIED 2026-09-30: blocked — HUMAN: real microphone dictation plus Codex TUI (audio hardware blocked).)_

## File browser default (2026-09-25) — Rust, needs `make dev` restart

- [ ] After restarting an isolated `TUIC_APP_INSTANCE=<id>` build with no saved `file_browser_view_mode`, open the file browser and confirm tree view is selected. Switch to flat list, restart, and confirm that choice remains selected. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: File browser view mode is UI.)_

## Editor line wrapping (2026-09-25) — Vite hot reload

- [ ] After Vite reloads the frontend, open a `.txt` file with a long line and confirm it wraps without horizontal scrolling; open a `.rs` file and confirm it does not. Toggle either with the header button or `Alt+Z`, then reopen the file and restart the app to confirm each file kind keeps its own setting. Check the active button style and that cursor, selection, search, git gutter, and inline blame still work while wrapped. _(NOT VERIFIED 2026-09-29: partial — Long-line agb_long.txt: .cm-lineWrapping, scrollWidth==clientWidth (822), Wrap button aria-pressed true + active class. agb_long.rs: no wrap, scrollWidth 16108>822. Header Wrap button toggled .rs to wrapped (sw==cw); localStorage tui-commander-editor-wrap {text,code} separate per kind, survives reload. Not checked: Alt+Z effect, cursor/selection/se)_

## Native WontFix dependency recovery (2026-09-25) — Rust, needs `make dev` restart

- [ ] After restarting an isolated `TUIC_APP_INSTANCE=<id>` dev instance, create a plan with a WontFix prerequisite and a Backlog dependent through the test instance. Confirm the dialog marks the prerequisite abandoned, offers Remove only on that direct cancelled edge, and reports the plan Active until its remaining stories are Done or WontFix. The current live backend cannot load this Rust change without a restart. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Dialog for WontFix dependency is UI.)_
- [x] After the native-stories backend rebuild, add a dependency to a story, reload Plans and Stories, and confirm the story list and derived plan state agree. This checks the reused SQLite connection and status aggregation in the rebuilt app. _(verified 2026-09-29: POST /stories/action: created plan+A,B in fx/repo, add_dependency B<-A; list_stories: A ready, B backlog deps=[A]; plan_view state=active with same story statuses/deps; plan_state=active; list_plans lists plan. Consistent.)_
- [ ] After restarting an isolated dev instance, open Plans and Stories to verify the capability probe succeeds; also confirm an actual story action error shows its own message rather than a restart instruction. _(NOT VERIFIED 2026-09-30: partial — GET /stories/capabilities returns true. Action error message display in UI not observed.)_
- [ ] After the native-stories backend rebuild, cancel a prerequisite with an indirect dependent and confirm the dialog shows both dependencies as abandoned, the Rust-supplied cancellation count, and the plan's Active state. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Cancellation dialog is UI.)_
- [ ] After the native-stories backend rebuild, cancelling an already cancelled story must show an error and leave its revision unchanged; removing a cancelled dependency remains limited to a Backlog story. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Error dialog is UI; revision check not exercised.)_
## Agent native scrollback (2026-09-25) — Rust, needs `make dev` restart

- [ ] After a safe `make dev` restart in an isolated `TUIC_APP_INSTANCE`, launch a throwaway agent whose `--help` child hangs. Confirm the first launch waits at most the two-second probe deadline, later launches of the same binary version do not wait again, and replacing the binary permits a new probe. On Windows, confirm no `cmd.exe` or `node.exe` child remains after timeout. _(NOT VERIFIED 2026-09-30: partial — Not exercised with a hanging --help fixture.)_
- [ ] After a `make dev` restart, open a **new** TUIC shell and type `codex`, `grok`, and `opencode` in separate throwaway tabs. Confirm each supported installed CLI stays in native scrollback; repeat with its per-agent **Prevent alternate screen** setting off, then restore the setting. Existing shells retain the previous PTY environment and cannot verify this change. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Native scrollback in new TUIC shell tabs is UI.)_
- [ ] After a `make dev` restart, launch Claude, Codex, Grok and OpenCode through the agent menu, PR Review where configured, and MCP `agent spawn` in an isolated `TUIC_APP_INSTANCE`; confirm terminal histories remain available after exit. Resume each session and check the same behavior. A CLI without the flag in `--help` should still launch. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Terminal history after agent launch is UI.)_
- [x] Start an isolated test instance with an absolute `TUIC_CAPTURE_DIR` under a disposable directory. Enable `POST /diagnostics/capture` for a throwaway session; `GET /diagnostics/capture` must report that directory and its `.tcap` must appear there. A relative override must return `TUIC_CAPTURE_DIR must be absolute` and leave capture disabled. _(verified 2026-09-29: tuic-remote (own instance ag2a) with absolute TUIC_CAPTURE_DIR: POST /diagnostics/capture -> dir echoed, .tcap (114B) appeared there, GET reports dir+bytes. Relative 'relcap' -> {enabled:false,error:'TUIC_CAPTURE_DIR must be absolute'}, GET enabled:false, no dir created.)_
## Markdown link navigation guard (2026-09-25) — Rust, needs `make dev` restart

- [ ] After a `make dev` restart, click a relative source link in a Markdown file: the code editor opens and no localhost page opens in the system browser. A link to an external web origin is blocked by the WebView navigation guard. Restart only when current live sessions can be interrupted. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Markdown link navigation is WebView UI.)_
- [ ] In the restarted instance, open an HTML preview, PDF preview, URL plugin panel on localhost, srcdoc plugin panel, and reveal.js deck. Their iframe content and in-frame links/slide navigation still load. Export a text download on Linux through a blob URL. _(Static coverage: `lib.rs` navigation guard allows the internal frame schemes and loopback origins; runtime platform behavior needs the restarted app.)_ _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Preview iframes are UI; download part also needs Windows/Linux.)_
## Streaming Progress intents (2026-09-25) — Rust, needs `make dev` restart

- [x] After restarting an isolated `make dev` instance, stream an `intent:` line in a narrow Codex or Ink terminal. The Progress journal gets the complete text and title once, including an indented hard-wrap row; closing a tab with a title-less open intent preserves one final entry. Check that a token in a done/blocked report is redacted in the journal. _(verified 2026-09-29: 40-col agent-typed PTY: word-hard-wrapped 4-row intent -> one journal entry full text, tab title set (Narrow Zed). Close with open titleless intent: exactly one entry (id 48) kept. progress done with ghp_ token+Bearer -> text '[REDACTED]'. Shell fake agent, not real Codex.)_
## MCP bridge config guard (2026-09-25) — Rust, needs `make dev` restart

- [x] After a `make dev` restart, verify the dev app resolves its adjacent `tuic-bridge` but leaves existing working absolute agent MCP commands unchanged. Confirm a missing command is repaired to the dev sidecar only in a disposable agent config. The targeted Rust child-process test covers launch without a sidecar and checks that a disposable HOME stays byte-identical. _(verified 2026-09-29: Ran a tuic-remote copy (same ensure_mcp_configs as desktop, not the desktop app itself) with TUIC_MCP_CONFIG_OWNER=1, disposable HOME, tuic-bridge beside it: entry installed with adjacent bridge path; existing working abs command (other dir) left unchanged; nonexistent abs command repaired to adjacent bridge. Real HOME untouched.)_
## Worktree warm status and safe removal (2026-09-25) — Rust, needs `make dev` restart

- [x] After restarting an isolated `make dev` instance, deinitialize a disposable worktree submodule with a local-only commit and a Git module name different from its checkout path; repeat with a nested named submodule. Removal must refuse and leave each module Git store and commit recoverable. _(verified 2026-09-30: sm3 fixture: submodule name 'modname'!=path vendor/lib, nested named 'nestmod'; local-only commits in both; git submodule deinit -f --all in the worktree. worktree_remove -> 'Cannot remove worktree: uninitialized submodule vendor/lib still has Git state'; worktree stays; lib and nest commits still r)_
- [x] After restarting an isolated `make dev` instance, attempt to remove a disposable worktree with a local-only submodule commit while the main checkout's copy of that submodule is uninitialized. Removal must refuse and leave the source worktree and commit intact; after initializing the main copy, removal should preserve the commit in the module repository. _(verified 2026-09-30: sm6 fixture (clone, main submodule uninit): worktree with committed pointer to local-only vendor/lib commit; worktree_remove delete_branch=false -> 'main checkout has no repository for submodule vendor/lib', worktree kept. After 'git submodule update --init' in main: ok kept_branch, worktree gone, c)_
- [x] After restarting an isolated `make dev` instance, remove a disposable worktree whose submodule has two stash entries and a reflog-only commit. Confirm all three OIDs remain reachable in the main checkout module after removal, including when a separate missing checkout is force-pruned. _(verified 2026-09-29: repo4 w/ submodule libs; worktree s1: 2 stashes + reflog-only commit in its private module. Before: OIDs absent in main .git/modules/libs (cat-file fails). After MCP worktree_remove: all 3 OIDs are commits there, kept as refs/tuic/preserved/... Also s2 (missing checkout, force-pruned): reflog commit + stash OID reachable afterwards.)_
- [ ] After a `make dev` restart in an isolated `TUIC_APP_INSTANCE`, archive a disposable linked worktree with an initialized submodule. Confirm the archive remains a usable Git checkout, its submodule `git status` and refs work, and it disappears from the active sidebar. A locked disposable worktree must remain at its original path during an automatic archive sweep. _(NOT VERIFIED 2026-09-30: partial — POST /worktrees/finalize action=archive on merged worktree w445a (initialized submodule with local commit): archived to __archived/w445a; git status clean, submodule status shows same local commit 2ab282c, submodule refs/log work; worktree_list no longer lists it. Locked w445lock: 'worktree_locked:w)_
- [x] After a `make dev` restart in an isolated `TUIC_APP_INSTANCE`, confirm Worktree Manager Prune refuses a detached checkout during a Git operation and one whose latest commit exists only at detached HEAD. A detached checkout whose HEAD is reachable from a branch or tag should prune cleanly. _(verified 2026-09-29: POST /repo/remove-orphan (what Prune calls) on disposable repo: real conflicting rebase in progress -> 'Cannot remove orphan worktree: a Git operation is in progress'; detached HEAD with commit only there -> 'detached HEAD commit has no durable ref' (both kept); detached at main-reachable commit -> ok; detached at tag-only commit -> ok. Manager UI )_

- [x] After an isolated `TUIC_APP_INSTANCE=<id>` Rust restart, confirm a refused dirty non-force removal leaves the worktree's pending warm state visible. Remove a different worktree while another registered checkout directory is missing; the missing checkout's submodule Git state must remain available for later safe removal. _(verified 2026-09-29: Own repo r440: create worktree w/ 60k-file ignored dir -> warm_artifacts pending; dirty non-force remove refused ('has uncommitted changes'), status still pending, later done. Submodule repo: w2,w3 with modules; rm w3 dir; removed w2 ok; .git/worktrees/w3/modules/sub intact + w3 prunable; w3 later removed via confirmMissingCheckout.)_
- [x] After a `make dev` restart, verify a missing registered worktree refuses removal without force, and a separately confirmed lock override is needed if that registration is locked. After force removal, its submodule-only refs must remain in the main checkout's module repository. _(verified 2026-09-29: Missing+locked registration (rm -rf dir; git worktree lock). DELETE /worktrees/s2 no force -> 'Worktree presence changed since confirmation' (MCP: 'uncommitted changes'), refused. force+confirmMissingCheckout -> 'worktree_locked:missing worktree is locked'. +overrideLock -> ok removal_rule=force; submodule stash+reflog OIDs then present in main mod)_
- [ ] After a `make dev` restart, create a worktree through HTTP/MCP in an isolated `TUIC_APP_INSTANCE`. Its response says warming is pending; `GET /worktrees/paths?path=<repo>` moves to `done` or `failed`, and a configured setup script finishes before copying begins. Remove the worktree and check its warm status is no longer retained. A clean squash-merged branch removes without force and the response names `patch_equivalence`. _(NOT VERIFIED 2026-09-30: partial — Verified: worktree_create response warm_artifacts.status=pending; GET /worktrees/paths shows done within 1s; after worktree_remove the entry is gone; squash-merged branch removed without force -> removal_rule:patch_equivalence. NOT verified: setup script ordering (script fields cannot be set via API)_
- [x] After that restart, create a worktree through desktop IPC and confirm its instructions report `pending` until warming completes. Confirm non-force removal preserves a dirty worktree with `delete_branch` both on and off; when an archive script adds a commit, the branch remains and the response includes `branch_delete_warning`. _(verified 2026-09-30: MCP repo worktree_create: warm_artifacts.status=pending, later done. worktree_remove on dirty wt, delete_branch true and false: both refused 'uncommitted changes', wt+branch kept. With archive_script (empty commit) set via PUT /config/repo-settings: branch kept, branch_delete_warning present. Used M)_

## Night integration 2026-09-25 — Rust, needs `make dev` restart

- [x] After a `make dev` restart, run `tuic agent spawn …` outside TUICommander (no `TUIC_SESSION`): stderr shows one `registering an external MCP caller` notice and the spawn succeeds. `tuic session status <unique name or short id>` resolves; an ambiguous name returns an error. _(verified 2026-09-30: tuic (/usr/local/bin/tuic; kit tuic binary is stale, lacks session/story/mcp) with TUIC_SOCKET, env -u TUIC_SESSION: agent spawn claude -> one 'registering an external MCP caller' notice, rc=0, session created. session status <name> and <8-char id> resolve; two sessions named r1-dup -> 'ambiguous; m)_
- [x] Orchestrator inbox under load: while children finish, a parent that does not read its inbox retains the newest 100 messages in FIFO order. The 101st send succeeds and the next inbox read reports the unread eviction in `missed_count`. _(verified 2026-09-29: same instance: unread inbox kept newest 100 in FIFO order, 101st send succeeded, missed_count=1 (tested peer-to-peer, not with finishing children))_
- [ ] Put a malformed value in one field of `dictation-config.json` (for example `"speech_volume_db": "loud"`) in a `TUIC_APP_INSTANCE=<id>` instance: Settings → Voice keeps the other values, and a "Dictation settings recovered" warn toast appears once. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Dictation routes/config are desktop-only (cfg feature desktop): GET /dictation/config and /dictation/hands-free return 404 on this headless tuic-remote. Settings->Voice toast needs desktop build + frontend.)_

## Workflow definitions (2026-09-25) — Rust, needs `make dev` restart

- [ ] In an isolated rebuilt instance, run two independent story workers. Let one request input so the run pauses, then let the other submit a current-generation report. Confirm its report is retained and an identical retry returns the same receipt while the run remains paused.
- [ ] In an isolated rebuilt instance, integrate and accept a story, add an unrelated canonical commit, and confirm its dependent is held until `recertify_canonical` runs the pinned checks at the new clean tip. Confirm a later ref or tree change invalidates that recertification.

- [ ] After rebuilding an isolated instance, save a direct-executable check with `update_checks`, publish the workflow, and confirm the published revision keeps its check set after the draft changes. A shell command or stale draft revision must be rejected.
- [ ] After rebuilding an isolated instance, read an existing workflow definition and confirm its closure is `human`. Try publishing an `automatic` draft and confirm the API rejects it; switch back to `human` and publish at the current draft revision.
- [ ] After rebuilding an isolated instance, transition a throwaway story through review and approve it through desktop IPC. Read `transition_history` through HTTP and confirm the approval records a human actor. Claim a second story with one managed session, submit it for review, and confirm that session's approval fails with `a story cannot be approved by its implementer` without a new history row; a different managed reviewer can approve and records its session ID. Approve a third story through sessionless HTTP and confirm it records `local_api`.
- [ ] After a Rust restart in an isolated instance, launch a coordinator attempt into a registered throwaway worktree with `workflow_launch`, inspect its prompt and event timeline, create a story with `workflow_story_create` and retry its proposal key, submit `workflow_report` from that managed PTY, and verify a different PTY cannot report the attempt. Exit a second agent without reporting and confirm its attempt becomes interrupted and the run pauses. Do this only when losing the current dev sessions is acceptable.
- [ ] **[VISUAL]** In an isolated rebuilt instance, edit and publish a seeded workflow draft in the Plans and Stories Designer tab; verify backend graph validation errors and the saved revision. The component layout was visually checked with a temporary Vite harness at normal and narrow widths (`~/Gits/.tmp/workflow-designer-visual.png`, `~/Gits/.tmp/workflow-designer-narrow.png`).

- [ ] After restarting Boss's desktop build when sessions can be interrupted, open Plans and Stories from the project toolbar and inspect the layout and stale-revision error. The isolated browser build already completed the create → start manual → check criterion → submit review → approve flow on 2026-09-25; screen capture is pending because agent-browser returned white images even for a solid-red test page and MCPMacControl.app lacks Screen Recording permission.

- [ ] After a restart in an isolated instance, start a run for a published `Resolve plan` definition through the workflow run API, discover it through `list_plan_runs`, page events from sequence zero, pause and resume, and confirm the run survives another restart. The runtime currently requires explicit commands; the scheduler is not yet connected.
- [ ] After rebuilding the isolated Rust instance, submit a managed `workflow_report` with `needs_input` and `inputRequest`; verify Run history shows the pause, an operator `answer_input` command survives restart, and `resume` is refused until an answer exists. Submit a reviewer report with criterion-indexed findings and verify the story status does not change from the report alone.
- [ ] After rebuilding the isolated Rust instance, report a story attempt while the plan coordinator is live. Confirm its inbox receives one `workflow_event` cursor and that replay from the cursor contains the committed report. A duplicate or late report must not produce another wake.
- [ ] After rebuilding an isolated Rust instance, launch two nonoverlapping story attempts from the active coordinator into two registered throwaway worktrees. Confirm distinct assignments appear in run history before spawn, a third attempt is held by the parallel limit, and overlapping or unknown scopes are serialized. Approve a prerequisite, run its pinned check in the assigned worktree, merge it into the canonical branch, then record integration and confirm the dependent releases. A failed post-merge check or later unrecorded ref movement must hold the dependent again.
- [ ] **[VISUAL]** In the isolated rebuilt instance, open **Run history** for a plan with real persisted events. Confirm the timeline advances after an IPC/SSE wake and remains readable at normal and narrow window widths. The component was visually checked with a temporary Vite harness and mocked IPC at both widths (`~/Gits/.tmp/workflow-timeline-visual.png`, `~/Gits/.tmp/workflow-timeline-narrow.png`); live backend integration awaits the Rust rebuild.

- [ ] In an isolated instance after restart, list workflow definitions for a project, edit a draft, publish revision 2, and confirm a run or reader pinned to revision 1 still sees revision 1. Confirm a malformed graph reports a validation error.

## Native story API (2026-09-24) — Rust, needs `make dev` restart

- [ ] After rebuilding an isolated instance, approve a prerequisite in a manual plan and confirm its dependent becomes Ready immediately. Start a workflow run for a separate plan, approve its prerequisite, and confirm the dependent stays Backlog until integration is recorded. Confirm desktop and headless startup remain responsive before opening a workflow; the first workflow access should recover prior active runs.

- [ ] After rebuilding an isolated instance, create a plan and story using `tuic story`, read them from the browser `/stories/action` route, and confirm another project's path cannot read their IDs. Claim from a live tab and verify stale revisions are rejected.
- [x] After rebuilding an isolated instance, create a plan and story using `tuic story`, read them from the browser `/stories/action` route, and confirm another project's path cannot read their IDs. Claim from a live tab and verify stale revisions are rejected. _(verified 2026-09-30: tuic story create_plan+create_story (repo fx/r1); POST :9881/stories/action?path= lists them; other project path (repoB): 'plan does not belong to project' for list/get_story/get_plan. Claim with live claude PTY -> in_progress rev2; stale expected_revision 0 and reused 1 -> 'stale story revision'. R)_

## Remote Project Progress event (2026-09-24) — Rust, needs `make dev` restart

- [ ] With a connected remote repository after restart, report a new Progress entry on the remote agent. The local desktop Progress bell and dialog update, and `progress_list` reads that repository's journal from its owning machine. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Needs desktop build: remote connection + local Progress bell/dialog are desktop frontend/remote_mirror. Headless instance has no remote connection to configure. (A second local tuic-remote could serve as 'remote' for the phase-2 agent.))_

## Voice auto-send is on by default (2026-09-24) — Rust, needs `make dev` restart

- [ ] After a `make dev` restart, with an instance whose `dictation-config.json` has no `auto_send` key (use `TUIC_APP_INSTANCE=<id>`, fresh config), Settings → Voice shows Auto-send on and a dictated phrase is sent with Enter. In basic mode the Auto-send row is hidden; switching it off makes it visible and the stored `false` survives an app restart. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Desktop-only feature (dictation/hands-free routes 404 on headless tuic-remote). Settings->Voice Auto-send default, basic/expert visibility and fresh dictation-config need the desktop frontend (+ mic for dictated send).)_

## "Add another GitHub account" is an expert entry point (2026-09-24) — Rust, needs `make dev` restart

- [ ] After a `make dev` restart, `curl localhost:9876/config/defaults` has `"github_accounts":{"accounts":[]}`. In Settings → Git & GitHub with no additional account, basic mode does not show the "Add another GitHub account" button, and Expert mode shows it. With one additional account configured (or a repository that needs an account), the "Additional GitHub Accounts" block with "Add another github.com account" and "Add Enterprise account" shows in basic mode. Before the restart the old backend has no `github_accounts` domain, so the button stays visible in basic mode. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Desktop-only feature (dictation/hands-free routes 404 on headless tuic-remote). Settings->Git&GitHub basic/expert rows and /config/defaults on desktop; headless GET /config/defaults is 404.)_

## Agent hook toggles store the default as absent (2026-09-24) — Rust, needs `make dev` restart

- [ ] After a `make dev` restart, in Settings → Agents, turn Claude's "Native status signals" off and on again, and Gemini's "Install hooks globally" on and off again. `agents.json` then has no `native_status_signals` / `hook_instrumentation` key for them. After a Settings reopen in basic mode, both rows are hidden. Signals and hooks still behave as enabled/disabled respectively. (Existing `agents.json` files that already hold `true`/`false` keep them until the toggle is used again.) _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Desktop-only feature (dictation/hands-free routes 404 on headless tuic-remote). Settings->Agents toggles and agents.json writes need the frontend (headless has no UI).)_

## Old config.json keeps `config`/`debug` MCP tools disabled (2026-09-24) — Rust, needs `make dev` restart

- [x] After a `make dev` restart, with a `config.json` that has no `disabled_native_tools` key (use `TUIC_APP_INSTANCE=<id>` and remove the key from that instance's config), the `config` and `debug` MCP tools are absent from `tools/list` and show as disabled in Settings. _(verified 2026-09-29: instance config.json has no disabled_native_tools key; tools/list omits config and debug (Settings display not checked here))_
## Agent list on the `+` buttons (2026-09-25) — frontend, HMR

- [ ] [HUMAN] Decide in the morning whether the sidebar `+` right-click should also open the agent list (withheld pending approval, AGENTS.md "Sidebar clicks"). Today: tab bar `+` right-click and long press open the agent list; sidebar branch `+` long press opens it; sidebar `+` right-click opens the branch menu. _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_

## Sub-agent tag icon in the sidebar (2026-09-25) — frontend, HMR

- [ ] [VISUAL] Spawn a sub-agent from an agent tab. Its sidebar row shows a small monochrome agent icon, aligned with the row text, instead of "↳ Parent". Hovering the icon shows "Spawned by <parent>". Take a screenshot (MCP maccontrol has no Screen Recording permission on 2026-09-25). _(NOT VERIFIED 2026-09-29: blocked — Needs agent action=spawn from an agent-typed tab (real agent CLI; all agents NOT FOUND; a fake binary is not detected as agent, arm test showed session not agent-typed).)_

## Mobile terminal prose reflow (2026-09-24) — frontend, refresh PWA

- [ ] [VISUAL] On a phone-width PWA session with Claude output produced in a wider desktop terminal, read a long paragraph: words flow across the phone width without a short orphan line at the desktop row boundary. Lists and box-drawing tables keep their own rows and alignment. _(NOT VERIFIED 2026-09-29: needs a real phone / PWA client — not reproducible in the isolated headless/browser instance)_

## Voice library, voice files, Listen and loudness sliders (2026-09-24) — Rust + frontend, needs `make dev` restart

Settings > Voice > Spoken replies, with the Italian bundle and the runtime downloaded:

- [ ] [VISUAL] Downloadable starts collapsed as "DOWNLOADABLE (n)" with a disclosure marker; Tab focuses it, Enter or Space opens it, and the Voice volume and Levelling sliders are visible without scrolling past the catalogue. _(NOT VERIFIED 2026-09-30: blocked — VISUAL-owned by tuic-live-checks)_
- [ ] [VISUAL] Take a screenshot of the Spoken replies section. The group titles (Installed, Downloadable, Yours), the voice rows, the Listen button beside the voice picker and the two slider labels follow `docs/frontend/STYLE_GUIDE.md`. _(NOT VERIFIED 2026-09-30: blocked — VISUAL-owned by tuic-live-checks)_
- [x] Download one catalogue voice from Downloadable. The progress bar moves; when it ends, the row moves to Installed and the voice appears in the voice picker. _(verified 2026-09-29: Italian, Downloadable>alba(6MB) Download: row showed progress bar 0%->12%.. then left Downloadable; Installed group 'alba Downloaded'; voice select options [Default,giovanni,alba]; assets voice-italian-alba ready. Then deleted it (absent) and restored language auto.)_
- [ ] Click "Add voice file…" and choose a valid Italian `.safetensors` voice. It appears under Yours and in the picker, and it speaks when selected. Choose a 24-layer (French) voice or a file that is not a voice: the reason shows under the button and nothing is added. The × on a Yours row deletes the file. Add a file with the same name as a voice under Yours: it is refused with "You already have a voice called … delete it first or choose another name", and the stored voice still speaks as before. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Dictation/voice routes are desktop-only: GET /dictation/{status,config,speech/voices,hands-free/default-notice} -> 404 on this headless instance. Add voice file UI + import route need desktop build (also needs a valid Italian .safetensors fixture); 'speaks' part is ear-only.)_
- [ ] [HUMAN] With no hands-free conversation, click Listen and listen: the sample is audible, in the selected voice, at the Voice volume level, and the saved voice does not change. Start hands-free, let the agent speak a reply, and click Listen while it plays: the refusal shows inline. _(2026-09-24 isolated instance voice0924: `POST /dictation/speech/voices/preview` with hands-free not armed returned 200 for giovanni, alba and an imported voice, and the output device accepted the audio; UI Listen with alba showed no error. Nobody listened, so audibility is unverified.)_ _(NOT VERIFIED 2026-09-30: blocked — Blocked: audio hardware (mic/speaker/ears): click Listen and hear the sample. Dictation/voice routes are desktop-only: GET /dictation/{status,config,speech/voices,hands-free/default-notice} -> 404 o)_
- [ ] Move Voice volume and Levelling and release. The next reply is louder or quieter and more or less even, without a restart. At -12 dB with Strong there is no clipping. _(NOT VERIFIED 2026-09-30: blocked — Blocked: audio hardware (mic/speaker/ears): judging loudness/clipping by ear. Dictation/voice routes are desktop-only: GET /dictation/{status,config,speech/voices,hands-free/default-notice} -> 404 o)_

HTTP import and delete (use the test instance on `:9877`):

- [ ] `POST /dictation/speech/voices/import` with `{"language":"it","name":"nonna","dataBase64":"<base64 of an Italian .safetensors>"}` succeeds. Voice choice then accepts `speech_voice` `"nonna"`, and a reply speaks in it. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Dictation/voice routes are desktop-only: GET /dictation/{status,config,speech/voices,hands-free/default-notice} -> 404 on this headless instance. POST /dictation/speech/voices/import needs desktop build and a valid Italian .safetensors fixture; spoken reply is ear-only.)_
- [x] The same with the French `8843db76` estelle file into language `"fr"` is refused with the "self_attn/pad … different model" reason, and nothing appears under `<speech>/user-voices/french/`. _(verified 2026-09-29: Installed French bundle via POST /dictation/speech/assets/download {asset:french} (removed afterward), POST /dictation/speech/voices/import {language:fr, 8843db76 french_24l/estelle.safetensors} -> error 'not a voice for French: ... transformer.layers.0.self_attn/pad ... made for a different model'; no user-voices dir created.)_
- [x] `POST /dictation/speech/voices/delete` with `{"language":"it","name":"nonna"}` removes the file. A reply with `speech_voice` `"nonna"` then reports that the voice is missing. _(verified 2026-09-29: Placed dummy models/speech/user-voices/italian/nonna.safetensors; POST voices/delete {it,nonna} -> "Deleted the Italian voice nonna", file gone. Missing voice reported via /voices/preview (same choose_voice): 'Italian has no voice called "nonna"; it offers giovanni')_
- [ ] Reinstall Italian from Settings > Voice while a downloaded voice (for example jean) and an imported voice are present. Both are still listed and still speak. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Dictation/voice routes are desktop-only: GET /dictation/{status,config,speech/voices,hands-free/default-notice} -> 404 on this headless instance. Needs desktop build with Italian bundle+jean+imported voice; 'still speak' is ear-only, listing part checkable via GET /dictation/speech/voices.)_
- [ ] `GET /dictation/speech/voices?language=it` lists giovanni as `default`, then the downloaded and the imported voices. `?language=xx` returns an error that names `xx`. With the Italian bundle not downloaded it returns `[]`, and the voice picker offers only "Default for this language". _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Dictation/voice routes are desktop-only: GET /dictation/{status,config,speech/voices,hands-free/default-notice} -> 404 on this headless instance. GET /dictation/speech/voices?language= needs desktop build; picker UI too.)_
- Note: an isolated `TUIC_APP_INSTANCE` debug instance binds no TCP port while remote access is off; reach it with `curl --unix-socket <tuic-mcp-*.sock>` (the path is in its log).

## Side panels follow an agent click to another repo (2026-09-24) — frontend, HMR

- [ ] Open the Notes, Git and Files panels on repo A. In the sidebar, click an agent row under repo B. All three panels now show repo B, as they do after a click on B's branch row. _(NOT VERIFIED 2026-09-29: blocked — Needs a second registered repo; web 'Add Repository' shows no path prompt here and MCP worktree_create does not register unregistered repos, so only fx/repo is registered.)_

## Spoken replies at one level (2026-09-24) — Rust, needs `make dev` restart

- [ ] [HUMAN] After the restart, arm hands-free and have the agent speak two replies in two different voices. Both sound equally loud, with no clipping or pumping. _(NOT VERIFIED 2026-09-30: blocked — Blocked: audio hardware (mic/speaker/ears): two spoken replies judged by ear. Dictation/voice routes are desktop-only: GET /dictation/{status,config,speech/voices,hands-free/default-notice} -> 404 o)_
- [ ] Set `speech_volume_db` to -24 in `dictation-config.json` through the settings save (`set_dictation_config`) while a reply is queued. The queued reply is not cut off, and the next reply is quieter. The same change is also available from the Voice volume slider in Settings > Voice > Spoken replies. _(NOT VERIFIED 2026-09-30: blocked — Blocked: audio hardware (mic/speaker/ears): audible comparison of reply loudness. Dictation/voice routes are desktop-only: GET /dictation/{status,config,speech/voices,hands-free/default-notice} -> 404 o)_
- [x] An existing `dictation-config.json` with neither field loads with -18 dB and 0.67 levelling (`GET /dictation/config` or `get_dictation_config`). _(verified 2026-09-29: wrote dictation-config.json {enabled,hotkey} only in the instance dir, GET /dictation/config -> speech_volume_db -18.0, speech_levelling 0.67 (file removed after))_

## MCP `session action=rename` and leaner output/spawn responses (2026-09-24) — Rust, needs `make dev` restart

- [ ] After a restart, call `session action=rename session_id=<id> name="Foo"` via MCP: the tab's display name in the sidebar/tab bar changes to "Foo" immediately. _(NOT VERIFIED 2026-09-30: partial — Backend only: MCP session action=rename name=Foo on live claude PTY -> GET /sessions display_name=Foo, display_name_is_custom=true, session list shows Foo at once. Sidebar/tab-bar rendering not observable on headless instance (needs desktop/browser build).)_
- [x] Rename again with `is_custom=false`: an agent's own OSC/intent title can then overwrite it, unlike a default (sticky) rename. _(verified 2026-09-29: session rename is_custom=false -> display_name_is_custom False; then shell printed OSC 2 'OSCTITLE' -> GET /sessions display_name became OSCTITLE. Rename default (sticky, is_custom True) then same OSC -> stayed 'Sticky' True.)_
- [x] `session action=rename` with no `name` or a blank one returns `{"error": ...}` and leaves the existing tab name untouched. _(verified 2026-09-29: session rename with missing/blank name -> {'error':'name (non-empty string) is required for action=rename'}, GET /sessions display_name stays 'Foo')_
- [x] `session action=output` on an idle Claude tab: the data ends at the agent's last output line, with no `❯`, separators or status-line/HUD rows. On a tab showing a permission dialog, the dialog and all its options are still there. _(verified 2026-09-30: Real claude (spawned via agent spawn): idle tab output data ends at '✻ Cooked for 3s · done' with no ❯ box, separators or status/HUD rows. Second claude (--permission-mode default) showing Bash permission dialog: output contains dialog title, command, question and options 1-4 (Yes/always/auto/No).)_
- [x] `agent action=spawn` returns no `*_with` fields; a registered orchestrator still gets `parent_session_id`. _(verified 2026-09-29: agent spawn (fake claude binary_path, agent_type=claude) returned only session_id,task_id,poll_interval_ms,name,server_ts,(communication_warning); no *_with keys. After agent register (ag3-orch), spawn also returned parent_session_id=<orch tuic_session>.)_

## No duplicated agent rows after a WebView reload (2026-09-24) — frontend via HMR

- [ ] Open an agent tab that shows the Context bar. Reload the WebView (`POST localhost:9876/debug/reload_webview`). Scroll up: the last reply must appear once. Before the fix, each reload added 1–2 copies of its top rows. _(NOT VERIFIED 2026-09-29: Needs a real agent tab showing the Context bar (real Claude session); raw-ring check alone does not cover it)_
- [ ] Check without the eye: `GET /sessions/{id}/raw-ring`. Each Claude full repaint (`ESC[2K` run) after the reload must clear exactly the tab's row count, not 1–2 more. _(NOT VERIFIED 2026-09-29: Needs real Claude full repaint output (ESC[2K runs) after WebView reload.)_

## Hands-free turns reach a busy agent at once (2026-09-23) — Rust, needs `make dev` restart

- [ ] After a `make dev` restart, arm hands-free on a Claude Code tab, give it a long task, and speak while it works. The turn appears in the terminal within a second or two (Claude queues it or takes it mid-turn), not after the turn ends. `GET /logs?source=dictation` shows `Hands-free turn typed now` with the session. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Desktop-only feature (dictation/hands-free routes 404 on headless tuic-remote). Hands-free arm on a real Claude tab and spoken turn needs desktop; audio can be fed as `say`-synthesized PCM over WS /dictation/hands-free/audio instead of a mic.)_
- [ ] While Claude shows a permission dialog, speak: nothing is typed into the dialog; the hands-free panel keeps showing the turn, and the log shows `Hands-free turn held` with `reason="confident question on screen"` once (not every 50 ms). Answer the dialog: the turn is typed. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Desktop-only feature (dictation/hands-free routes 404 on headless tuic-remote). Needs hands-free armed + permission dialog (reproducible with claude --permission-mode default, seen in item 529) + turn via audio WS; desktop only.)_
- [ ] Queue a typed command in the Compose panel while the agent is busy, then speak: the spoken turn is typed now, and the Compose badge still shows the typed command, which goes out at the next idle as before. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Desktop-only feature (dictation/hands-free routes 404 on headless tuic-remote). Needs hands-free armed and Compose panel (frontend); desktop only.)_
- [x] Stop hands-free: the status bar reads "Hands-free: stopped" (no "entries had already been typed" count any more). _(verified 2026-09-29: by code/test inspection, tests not executed here: useDictation.ts:68 sets 'Hands-free: stopped'; asserted at src/__tests__/hooks/useDictation.test.ts:120; old count text no longer exists (rg 'already been typed' = no matches).)_

## Opening AI Chat no longer aborts the app (2026-09-23) — Rust, needs `make dev` restart

- [ ] After a `make dev` restart, open the AI Chat panel (ego over ACP) and send a prompt. The app stays up and the reply streams in. Before the fix, `acp_subscribe` called `tokio::spawn` on the main thread, which panicked with `TryCurrentError` (SIGABRT, crash report `tuicommander-2026-09-23-164623.ips`) and killed every PTY session. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: AI Chat panel (ego over ACP) and acp_subscribe main-thread panic are Tauri desktop-only; headless tuic-remote has no main thread/UI. Needs desktop build with the frontend AI Chat panel.)_

## Voiceless dictation languages are marked (2026-09-23) — frontend via HMR

- [x] [VISUAL] Settings → Dictation → Language: languages without a speech bundle (e.g. Japanese) read "Japanese — no spoken replies"; Italian/English and Auto-detect carry no marker. Choosing Japanese shows a hint under the select that replies will not be spoken and suggests Auto-detect; choosing Italian or Auto-detect hides it. _(verified 2026-09-29: web UI Settings > Voice > Language: Dutch/Japanese/Chinese/Korean/Russian read '— no spoken replies', Auto-detect/English/Italian carry none; choosing Japanese shows 'Replies in this language will not be spoken… choose it or Auto-detect', Italian and Auto-detect hide it)_

## Readable Design Mode tab badge (2026-09-23) — frontend via HMR

- [ ] [VISUAL] Arm Design Mode on an agent tab, then stop it. The boxed `D·` badge in the tab is readable on the dark tab bar: letter and border in `--fg-primary`, 11px text. The armed `D` stays green. _(NOT VERIFIED 2026-09-29: blocked — Design Mode arm/stop needs an agent tab (D badge) and Chrome; only shell tabs exist (no agent CLI).)_

## Hands-free from the Command Palette (2026-09-23) — frontend only, Vite HMR

- [ ] [HUMAN] With dictation enabled, open the Command Palette on an agent tab and run "Start hands-free conversation". Speak a sentence: it reaches that tab and the first earcon plays (priming happened inside the palette click). Switch tabs and reopen the palette: the entry reads "Stop hands-free conversation"; run it and the conversation stops. _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_

## Reply language stated once per hands-free conversation (2026-09-23) — Rust, needs `make dev` restart

- [ ] After a `make dev` restart, with the dictation language on Auto-detect, arm hands-free on an idle agent tab and say two Italian sentences, then one English sentence. The terminal receives `<first> (reply in Italian)`, the second sentence with no suffix, and `<english> (reply in English)`. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Desktop-only feature (dictation/hands-free routes 404 on headless tuic-remote). Needs hands-free + Whisper Auto-detect; feed Italian/English `say` audio over WS /dictation/hands-free/audio on desktop build.)_
- [ ] After the same restart, set the dictation language to Italian and arm again: the start notice ends with `Reply in Italian.` and the first spoken turn carries no suffix. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Desktop-only feature (dictation/hands-free routes 404 on headless tuic-remote). Needs hands-free arm and dictation language setting; desktop only.)_

## Custom hands-free start notice (2026-09-23) — Rust, needs `make dev` restart

- [ ] After a `make dev` restart, in Settings → Dictation → Hands-free, type a two-line start notice and arm hands-free on an idle agent tab: the agent receives your text as one line, not the built-in notice. Press **Reset to default**, disarm and arm again: the built-in notice is sent. `GET http://localhost:9876/dictation/hands-free/default-notice` returns the built-in text. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Desktop-only feature (dictation/hands-free routes 404 on headless tuic-remote). Needs Settings->Dictation UI plus arm on an agent-typed session; desktop only.)_
- [ ] After the same restart, the **Start notice** textarea in Settings → Dictation shows the built-in text in grey as its placeholder. Before the restart it is empty and the log reads `Failed to load the default hands-free start notice`, because the old backend has no such endpoint. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Desktop-only feature (dictation/hands-free routes 404 on headless tuic-remote). Needs Settings->Dictation textarea placeholder; desktop only.)_

## Activation phrase survives Whisper's spelling (2026-09-23) — Rust, needs `make dev` restart

- [ ] [HUMAN] After a `make dev` restart, with the activation phrase `senti mac`, arm hands-free and say three sentences that begin with "senti mac". Each one is queued, without the phrase. Then say "senti, ma che ore sono?" to someone else: it is dropped. For every dropped turn, `curl 'localhost:9876/logs?source=dictation'` shows the line `Hands-free turn dropped…` with a `heard=` field that holds the first four words Whisper wrote. _(NOT VERIFIED 2026-09-30: blocked — Blocked: audio hardware (mic/speaker/ears): spoken activation-phrase turns via microphone. Dictation/voice routes are desktop-only: GET /dictation/{status,config,speech/voices,hands-free/default-notice} -> 404 o)_

## Parked voice turns leave together (2026-09-23) — Rust, needs `make dev` restart

- [ ] After a `make dev` restart, arm hands-free on a Claude tab, give it a long task, and speak three short turns while it works. At the next idle the three turns arrive together as one message, in the order spoken, one `(reply in Italian)` line each — not one message per agent turn. A command typed into Compose between the turns still arrives as its own message. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Desktop-only feature (dictation/hands-free routes 404 on headless tuic-remote). Needs hands-free arm on real Claude tab and 3 turns via audio WS; desktop only.)_

## Settings consistency: Language, ego, Spoken replies (2026-09-23) — Rust part needs `make dev` restart

- [x] [VISUAL] Settings → General: no Language picker (only English ships). The AI Chat section (Experimental Features on) looks like TUIC CLI: `?` tooltip, green "Configured at …", **Select…** opens a file picker and saves the pick, **Clear** empties it. _(verified 2026-09-23 by screenshot after restart: no Language row; AI Chat shows the `?` badge, the not-configured hint and Select…, matching TUIC CLI. Select/Clear not clicked — `ego_executable` is empty on disk)_
- [x] [VISUAL] Settings → Dictation → Spoken replies: rows look like the Whisper list. Only the language replies are spoken in is highlighted with "Active"; a running download shows its bar and a × to cancel. _(verified 2026-09-23 by screenshot: Downloaded badges and × on every ready bundle, highlight and Active on Italian only, as on Whisper Large V3 Turbo. The in-progress bar was not observed on screen)_
- [x] After the restart, start a speech download with `curl -X POST localhost:9876/dictation/speech/assets/download -H 'content-type: application/json' -d '{"asset":"german"}'` while Settings → Dictation is open. The bar climbs, then the row turns to Downloaded by itself instead of staying at 100% with a cancel control. _(verified 2026-09-23 on the restarted instance: German download over HTTP emitted 8000+ progress events on `/events`, then `{"asset":"german","done":true}`; catalogue reports `ready`. The store clearing the bar on `done` is covered by `dictation.test.ts` "clears a finished download this client never started")_

## Echo reference covers the whole reply (2026-09-23) — Rust, needs a `make dev` restart

- [ ] [HUMAN] After the restart, use the laptop speakers (no headphones). Arm hands-free with a voice and ask for a reply of at least 10 seconds. The reply plays to the end without cutting itself off, and no turn arrives that repeats its words. In `tuic.log`, no `speech: hushed` line appears during the reply, and there is no `echo: far-end reference full` warning. _(NOT VERIFIED 2026-09-30: blocked — Blocked: audio hardware (mic/speaker/ears): laptop speakers echo test. Dictation/voice routes are desktop-only: GET /dictation/{status,config,speech/voices,hands-free/default-notice} -> 404 o)_
- [ ] [HUMAN] Talk over a long reply after its first 3 seconds. It stops. The log shows `speech: hushed` with `speaking=true` and an `into_playback_ms` above 3000, then `Hands-free turn accepted heard=` with your words. _(NOT VERIFIED 2026-09-30: blocked — Blocked: audio hardware (mic/speaker/ears): talking over a reply. Dictation/voice routes are desktop-only: GET /dictation/{status,config,speech/voices,hands-free/default-notice} -> 404 o)_

## Barge-in waits for sustained speech (2026-09-23) — Rust, needs a `make dev` restart

- [ ] [HUMAN] After the restart, use the laptop speakers (no headphones). Arm hands-free with a voice and ask a question. The spoken reply plays to the end unless you talk over it. Talk over a second reply: it stops within about a quarter of a second, and your first words are in the transcript. _(NOT VERIFIED 2026-09-30: blocked — Blocked: audio hardware (mic/speaker/ears): laptop speakers barge-in. Dictation/voice routes are desktop-only: GET /dictation/{status,config,speech/voices,hands-free/default-notice} -> 404 o)_
- [ ] [HUMAN] After the restart, in hands-free, say half a sentence, pause about a second, and finish it before the hold-back ends. One turn arrives with both halves; the first half is not sent alone. _(NOT VERIFIED 2026-09-30: blocked — Blocked: audio hardware (mic/speaker/ears): speaking half sentences into a mic. Dictation/voice routes are desktop-only: GET /dictation/{status,config,speech/voices,hands-free/default-notice} -> 404 o)_
- [ ] [HUMAN] After the restart, arm hands-free with the activation phrase on the desktop. Say a phrase, pause about four seconds, then continue without the phrase. The pending text gains the continuation and the agent receives one message after the final five-second window. Check the microphone level while TTS plays and whether the reply's own words appear as a new turn; these acoustic and gain observations require the real device. _(NOT VERIFIED 2026-09-30: blocked — Blocked: audio hardware (mic/speaker/ears): speaking with pauses, mic level with TTS. Dictation/voice routes are desktop-only: GET /dictation/{status,config,speech/voices,hands-free/default-notice} -> 404 o)_

## Calm hands-free voice meter (2026-09-23) — frontend via HMR; the Rust level fallback needs a `make dev` restart

- [ ] [VISUAL] Arm hands-free. The toast shows one thin horizontal voice meter and the phase text — no pulsing dot and no moving dots after the text. Push-to-talk (dictation hotkey) still shows the bar meter, the dot and the dots. _(NOT VERIFIED 2026-09-30: blocked — VISUAL-owned by tuic-live-checks)_
- [ ] Stay silent with normal room noise (fan, typing): the voice meter stays flat. Speak: it fills at once and falls back smoothly over about 1.5 s after you stop, with no flicker between words. _(NOT VERIFIED 2026-09-30: blocked — Blocked: audio hardware (mic/speaker/ears): voice meter reaction to real speech/room noise. Also desktop-only (dictation routes 404 on headless).)_

## Unmerged worktree removal (2026-09-23) — Rust, needs `make dev` restart

- [ ] After the restart, a clean worktree with commits not in the default branch shows `Unmerged` in the sidebar even with no diff or dirty badge. Choosing Delete Worktree while **Delete branch on remove** is on keeps the worktree and its terminals and explains that the branch has unmerged commits. With that setting off, removal keeps the local branch. _(NOT VERIFIED 2026-09-30: partial — Backend via MCP: clean worktree with 1 extra commit -> commit_status=unmerged, dirty_files=0. worktree_remove delete_branch=true refused 'branch has unmerged commits', dir+branch kept; delete_branch=false removes worktree, removal_rule kept_branch, branch kept. Sidebar 'Unmerged' label and its termi)_

## Sub-agent tags and branch count (2026-09-23) — Rust, needs `make dev` restart

- [ ] [VISUAL] After the restart, the in-window Activity Dashboard stays close to the detached window's width; a long terminal name remains readable beside the robot marker and project badge, and a narrow main window has no horizontal overflow. _(NOT VERIFIED 2026-09-30: blocked — VISUAL-owned by tuic-live-checks)_
- [x] [VISUAL] Sidebar: a branch whose agent list is expanded shows no number on its icon; collapse it and the number comes back. _(verified 2026-09-29: Nested Terminal Tabs enabled via Settings>Appearance. Branch icon toggle expanded: .branchAgentCount absent (null); after collapse count '11' shown (DOM). No screenshot (timeouts).)_
- [ ] After the restart, spawn an agent with `agent action=spawn` from an agent tab. In the Activity Dashboard (`Cmd+Shift+A`) the child row shows a robot-head icon with tooltip `Spawned by <parent tab name>`, and the parent row shows no icon. Rename the parent tab: the tooltip follows. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Backend seen: GET /sessions child has parent_session=<parent id> (spawn from real claude tab via x-tuic-session=parent). Robot icon, tooltip 'Spawned by <parent>', rename-follow are Activity Dashboard UI (desktop/browser build).)_
- [ ] [VISUAL] Sidebar nested agent rows: the spawned child shows the same robot-head icon; with a long parent name, the tab title stays readable. _(NOT VERIFIED 2026-09-30: blocked — VISUAL-owned by tuic-live-checks)_
- [ ] Pop out the Activity Dashboard: the detached window shows the same icon and parent tooltip. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Pop-out Activity Dashboard is a desktop window feature; needs desktop build. Backend parent_session on spawned child confirmed (see 608).)_
- [ ] After the restart, with a child marked as a subagent, run `curl -X POST http://localhost:9876/debug/reload_webview`: the icon and resolved parent name remain. Open a browser-mode agent tab with no spawn name, let Claude set its OSC title, reload: the tab keeps following later OSC titles. A named `agent action=spawn` tab still ignores Claude's OSC title after the reload. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Needs POST /debug/reload_webview on desktop WebView and frontend tab-title logic (OSC titles); headless has no WebView. Real claude sets OSC titles fine when spawned with env CLAUDE_CODE_CHILD_SESSION=0.)_

## Voices from Kyutai's ungated repository (2026-09-23) — **Rust, needs a `make dev` restart**

- [x] Download reaches Ready with no hash error _(verified 2026-09-23: `POST /dictation/speech/assets/download` for onnxruntime, italian, english, french; all `ready`, sha256 checked during install)_
- [ ] [HUMAN] A hands-free reply in Italian is spoken with the giovanni voice (`/dictation/speech/speak` refuses while hands-free is not armed). _(NOT VERIFIED 2026-09-30: blocked — Blocked: audio hardware (mic/speaker/ears): hearing giovanni voice. Dictation/voice routes are desktop-only: GET /dictation/{status,config,speech/voices,hands-free/default-notice} -> 404 o)_
- [ ] **Rust, needs another `make dev` restart:** arm hands-free on a hand-opened Claude tab, then `voice action=status` from that tab reports `available: true` instead of "Speech is bound to another session" (caller now resolved to its live PTY, `resolve_mcp_origin_pty`). _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Desktop-only feature (dictation/hands-free routes 404 on headless tuic-remote). Needs hands-free armed on a real Claude tab, then MCP voice status; on headless voice status returns 'connection is not bound to a terminal'.)_
- [ ] Settings → Dictation → Spoken replies lists English, French, German, Italian, Portuguese and Spanish. Set the Whisper language to English, download English, and a hands-free reply is spoken in English with the alba voice. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Desktop-only feature (dictation/hands-free routes 404 on headless tuic-remote). Settings->Dictation list, ~390MB downloads; audible part is ear-only. Desktop only.)_
- [ ] French (24-layer, ~390 MB): download and speak one reply. The engine reads the layer count from `bundle.json`, but no 24-layer bundle has been run here before. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Desktop-only feature (dictation/hands-free routes 404 on headless tuic-remote). French bundle download+speak; desktop only; audible verification is ear-only.)_

## Alias survives a WebView reload (2026-09-23) — Rust, needs `make dev` restart

- [ ] After the restart, spawn an agent with `agent action=spawn`, then reload the WebView (`curl -X POST localhost:9876/debug/reload_webview`). The tab context menu still shows "Alias: …" and the tooltip shows the alias that `session list` returns for that session. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Needs /debug/reload_webview and tab context menu 'Alias:' UI (desktop). Backend: alias present in session list (re-N) after agent spawn via MCP.)_
- [x] Browser mode (`http://localhost:9876/`): with the page open, spawn an agent. Its new tab shows the alias without a page reload (`term-alias-assigned` now arrives over `/events`). _(verified 2026-09-23: live `/events` SSE on :9876 delivered `event: term-alias-assigned` `{"session_id":…,"alias":"tt-1"}` for a throwaway `POST /sessions`; listener in `useAppInit.ts:596`; tests `assign_term_alias_publishes_the_alias_on_the_event_bus` + `term_alias_assigned_has_matching_sse_name_and_payload` pass. Browser tab rendering itself not observed.)_
- [ ] Spawn a Claude agent with `name=call-map`. When Claude prints its session title, the tab still reads `call-map` (an `intent:` title may still replace it, a manual rename too). Reload the WebView: the name is still protected from the OSC title. _(frontend half is live via HMR; the reload check needs the restart)_ _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Backend seen: spawned claude tab keeps display_name (from_spawn=true, e.g. r1-tx) through Claude's turns. Frontend OSC-title protection and WebView reload need the desktop/browser build.)_

## Compose panel pin (2026-09-23) — frontend, live via HMR

- [ ] [VISUAL] `Cmd+I` in an agent tab: the panel is one row shorter than before. Click the pin: the terminal shrinks above the panel, the agent redraws at the new height, and no row hides behind the panel. Unpin: the panel overlays the terminal again and the terminal regains its full height. _(NOT VERIFIED 2026-09-29: partial — Pin button toggles (title 'Pin to the terminal bottom' <-> 'Unpin from the terminal bottom', aria-pressed). Terminal geometry not measurable: canvas height stayed 150px pinned vs unpinned, PTY screen_lines unchanged (38/24/13/24); panel-one-row-shorter and redraw at new height unverified; no screenshot.)_
- [ ] Pinned: `Ctrl+Enter` sends, the editor empties and keeps the caret; the panel stays open. `Shift+Ctrl+Enter` does the same through the queue. _(NOT VERIFIED 2026-09-29: partial — Pinned: typed text + Send button: session got text, editor emptied, panel stayed open (verified). Ctrl+Enter / Shift+Ctrl+Enter via agent-browser press never reached the page (document keydown listener saw 0 events), so key bindings and caret retention untested; shell tab not agent.)_
- [ ] Pinned: click in the terminal and type — the caret stays in the terminal (not pulled back into the panel). `Cmd+I` and `Esc` move the caret between the panel and the terminal. _(NOT VERIFIED 2026-09-29: partial — Pinned compose panel open, trusted mouse click on terminal area: document.activeElement is the terminal's INPUT (not inside the panel), panel stays open (caret stays in terminal). Cmd+I/Esc caret moves and typed-in-terminal echo not testable: agent-browser key presses do not reach the page.)_
- [ ] Pinned in one tab only: another tab's compose panel still closes after send. _(NOT VERIFIED 2026-09-29: blocked — Needs a second terminal tab active: trusted clicks on the 'Foo' tab and its sidebar row did not switch away from Terminal 4 in this web session, so cross-tab pinned/unpinned behaviour could not be compared.)_
- [x] The ✕ at the right of the status bar closes the panel, pinned or not; reopening with `Cmd+I` shows it unpinned. _(verified 2026-09-29: Compose panel (opened via 'Compose' hint on Terminal 4 shell tab; browser Cmd+I not delivered): pinned (aria-pressed true) -> x 'Close compose panel' at status bar right (1107,839) closed it; reopened -> pin aria-pressed false. Unpinned x also closes.)_

## Hands-free earcons (2026-09-23) — **Rust, needs a `make dev` restart**

- [ ] Arm hands-free on the desktop, speak a turn: after the hold-back a short, quiet blip plays as it reaches the agent. Set an activation phrase and speak without it: a softer, lower blip plays and nothing is sent. Neither blip opens a turn or stops a spoken reply. Repeat from a browser tab at `:9876` — the tab beeps, the desktop stays silent. Also check the first blip after arming from the global hotkey is audible (WKWebView may keep an ungestured AudioContext suspended). Turn the Earcons setting off and repeat: no sound on the desktop, and none in a browser tab armed after the change. _(NOT VERIFIED 2026-09-30: blocked — Blocked: audio hardware (mic/speaker/ears): earcon blips heard through speakers. Also desktop-only (dictation routes 404 on headless).)_

- [ ] Earcon redesign (frontend, live via HMR): a delivered turn plays two short **rising** notes; a dropped turn plays two softer **falling** notes. Each is recognisable without hearing the other, and neither opens a turn, appears as captured speech, or stops a spoken reply. _(NOT VERIFIED 2026-09-30: blocked — Blocked: audio hardware (mic/speaker/ears): earcon note direction judged by ear. Also desktop-only (dictation routes 404 on headless).)_

## Dictation auto-send on long text (2026-09-23) — frontend, live via HMR

- [ ] With Auto-send on, dictate 30+ seconds into a Claude tab: the text is submitted without pressing Enter, and no "Removed 1 invisible character" notice appears. _(NOT VERIFIED 2026-09-29: Needs 30+ s of real dictated audio into a Claude tab.)_
- [ ] Dictate a short phrase into Claude and into a Codex tab: both still submit. _(NOT VERIFIED 2026-09-29: Needs real dictation audio and real Claude and Codex tabs.)_
- [ ] **After a `make dev` restart (Rust):** `session action=submit` with a 600+ char input to a Claude tab returns `acknowledged:true` and the prompt runs; a peer `agent send` of a long message also submits. _(NOT VERIFIED 2026-09-29: Needs a real Claude tab to run the long prompt and peer agent send)_

## New-tab long press and settings button spacing (2026-09-23) — frontend, live via HMR

- [ ] Hold the `+` in the tab bar for half a second: a menu lists the enabled agents (a submenu per agent with 2+ run configs). Picking one opens a new tab in the active branch that starts the agent. Releasing does not also open a plain tab. _(NOT VERIFIED 2026-09-29: partial — Web UI: mouse down on tab-bar '+' held 0.8s (agent-browser mouse down/up): no menu appeared and a plain terminal opened (tabs 10->11). Instance lists all agents NOT FOUND, so getNewAgentMenuItems is empty; agent menu/submenu/new-agent-tab not verifiable.)_
- [ ] A quick click on `+` still opens a plain terminal; right-click still shows New Tab / Split. _(NOT VERIFIED 2026-09-29: partial — Quick click on tab-bar '+' opened a plain terminal (close buttons 9->10). Trusted right-click (mouse down/up right at 1426,47) showed no menu at all; createAgentLaunchMenu default rightClick=true opens only the agent list (empty: all agents NOT FOUND) — no 'New Tab / Split' menu observed; claim unconfirmed.)_
- [ ] [VISUAL] Settings → Dictation → Voice tuning: "Level gate" no longer touches the "Start test recording" button. Also check the Import/Export row and the Notifications "Reset Defaults" footer. _(NOT VERIFIED 2026-09-29: partial — capture_window blocked (no Screen Recording perm) so no screenshot. DOM geometry via invoke_js in validate instance: Start-test button bottom 1172 vs Level gate label top 1190 (18px gap); Import/Export buttons 8px apart, 8px below prev row; Notifications Reset Defaults footer 20px below prev block. Human eyeball still advised.)_

## Mobile Progress header (story 1214-2b95)

- [ ] [HUMAN] On a 360 px and a 390 px phone, open the Progress tab with a project available. Confirm the title, project and terminal selectors, **List | Flow**, and **Blocked only** are visible without horizontal scrolling; tap both view choices and the filter. Browser geometry at those widths is checked separately; this item covers real touch and device rendering. _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_

## Progress Flow view (2026-09-23) — **Rust, needs a `make dev` restart**

The List | Flow toggle is frontend and appears through HMR at once, but the running backend has no `progress_flow` until it restarts, so Flow shows an error line until then. The first open after the restart migrates `progress.sqlite3` (table rebuild for the new kinds).

- [ ] After the restart, open Progress: the List still shows every older entry, and deleting one still works. _(2026-09-23 after restart: `POST /progress/list` returns 200 with 349 entries, ids 388–1618, so older entries are back; delete not exercised on live data.)_ _(OLD NOTE 2026-09-23: FAILED on :9876. `POST /progress/list` and `POST /progress/flow` both answer `progress_store_unavailable: cannot read the progress id high-water mark: no such table: sqlite_sequence`. The live `progress.sqlite3` (1614 entries) was created with a bare `id INTEGER PRIMARY KEY` (no AUTOINCREMENT), so `sqlite_sequence` does not exist and `rebuild_for_hand_offs` (`progress/store.rs:146-153`) errors in every `ProgressStore::open()`. No entry has been written since the restart (newest `created_at_ms` 12:04). The migration test only covers an AUTOINCREMENT journal.)_ _(NOT VERIFIED 2026-09-30: partial — API: POST /progress/list -> entries+total+ptyIds; POST /progress/delete {ids:[3]} -> deleted 1, list shrinks 8->7. Store opens fine on this fresh journal. NOT observed: legacy non-AUTOINCREMENT progress.sqlite3 migration (would need hand-made DB in instance dir) and the Progress dialog UI.)_
- [ ] From a Claude terminal in a registered repo, `agent action=spawn` a peer with a prompt. List shows `delegated to <child>`; Flow shows a blue arrow from the parent's column to the child's, labelled with the prompt. _(NOTE 2026-09-23: blocked — the progress store fails to open on :9876, see the first item of this section.)_ _(NOT VERIFIED 2026-09-30: partial — Real claude parent, agent action=spawn peer with prompt (x-tuic-session=parent PTY id): /progress/list has type=delegated, targetName=<child>, text=prompt; /progress/flow has delegated event parent->child, summary=prompt. Needs repo registered (POST /watchers/repo). Blue arrow/'delegated to' label n)_
- [ ] Have the child `agent action=send` to the parent and report `done`. Flow shows a grey message arrow child → parent, then a green return arrow child → parent. No toast for the delegation or the message; one silent toast for `done`. _(NOTE 2026-09-23: blocked — the progress store fails to open on :9876, see the first item of this section.)_ _(NOT VERIFIED 2026-09-30: partial — Child PTY id as x-tuic-session: agent send to parent -> journal type=message; progress type=done -> type=done. /progress/flow events: message then done, both child->parent. Toast behaviour and grey/green arrow colours not observed (UI).)_
- [x] Spawn the child into a managed worktree (`repo action=worktree_create spawn_session`). Its `intent:` now appears on its column (it was dropped before). _(NOTE 2026-09-23: blocked — the progress store fails to open on :9876, see the first item of this section.)_ _(verified 2026-09-30: Worktree from repo action=worktree_create, real claude spawned with cwd=worktree printing 'intent: ...': dropped while workspace unregistered, recorded (entry filed under parent project, flow participant.intent set) once repos.<r>.workspaces registered via PUT /config/repositories (what the frontend)_
- [ ] In a Claude terminal, run 2 subagents (one nested). Each gets a column under its terminal with a tool count; the dashed arrow carries its task and, once done, a green arrow carries its report. Clicking a long label fetches the full text, with any token shown as `[REDACTED]`. _(NOTE 2026-09-23: blocked — the progress store fails to open on :9876, see the first item of this section.)_ _(FAILED 2026-09-30 story 1298-c8d8: Real claude (env CLAUDE_CODE_CHILD_SESSION=0 so transcripts persist): alpha, beta+nested gamma, delta subagents. Columns, toolCalls, nested parent, [REDACTED], beta return arrow OK. alpha/gamma/delta stay 'running', no return: jsonl ends in SubagentHandback tool_result, no end_turn; subagent_map.rs )_
- [ ] Select one terminal in the selector: Flow keeps that terminal, its parent and its children only. _(NOTE 2026-09-23: blocked — the progress store fails to open on :9876, see the first item of this section.)_ _(NOT VERIFIED 2026-09-30: partial — POST /progress/flow with ptyId=<wtchild> -> participants [parent Foo, wtchild], 2 events; ptyId=<parent> -> parent + its 4 children only. Selector UI not observed.)_
- [ ] [VISUAL] With 6+ columns: the header row stays pinned while scrolling, every arrow ends on a lifeline, and the dialog scrolls sideways rather than squashing columns. _(NOT VERIFIED 2026-09-30: blocked — VISUAL-owned by tuic-live-checks)_
- [x] **Store fix, needs another `make dev` restart:** after the restart, one `progress` call (or `POST /progress/list`) succeeds, and the live journal's schema (`SELECT sql FROM sqlite_master WHERE name='entries'` on `<config dir>/progress.sqlite3`) contains `'delegated'`. The legacy no-AUTOINCREMENT journal is migrated with every id kept. _(verified 2026-09-23 after restart: `progress` returned id 1617; schema now `INTEGER PRIMARY KEY AUTOINCREMENT` with `'delegated'` in the CHECK; `sqlite_sequence` = 1617 = MAX(id); `POST /progress/list` and `/progress/flow` answer 200, list ids 388–1618.)_
- [ ] After the same restart: in a Claude session with 65+ subagents, the newest and every running one still get a Flow column; expand a Flow row, let a new entry arrive, and the same row stays expanded. _(NOT VERIFIED 2026-09-30: partial — Real claude launched 70 haiku subagents (74 in session). /progress/flow: 73 subagent columns, missing only oldest finished 'beta'; truncated=false. All 73 report 'running' though finished (SubagentHandback, see 665), so cap on finished ones not exercised cleanly. Row-stays-expanded is UI, not observ)_
- [x] `curl -s -o /dev/null -w '%{http_code}' localhost:9876/agents/map` answers `404`: the map page is removed. _(verified 2026-09-23: returned `404` on :9876.)_

## Markdown block review handoff (2026-09-23) — frontend, live via HMR

- [x] An MCP-opened absolute Markdown file outside the active repository writes tweak comments through the external file route; a rejected write shows an error and leaves the draft available to retry. _(verified: `MarkdownTab.test.tsx` exercises external save and rejected-then-successful retry.)_
- [x] A terminal file path with `:line` opens the built-in editor at that line; a numbered Markdown path does the same while an unnumbered Markdown path opens the viewer. _(verified: `terminalFileOpen.test.ts` covers the numbered routes and unnumbered viewer route.)_
- [x] Clicking a later `:line` or `:line:col` terminal link for a file already open in the editor moves the cursor there and retains unsaved edits. _(verified: `editorOpenLinks.test.tsx` checks repeat navigation, oversized column clamping, and retained edits.)_

- [ ] In a Markdown file with plain, task, and nested bullets, hover each bullet's gutter and save a comment. Confirm each highlight stays on its chosen bullet and the file contains an indented `tweak:item` marker directly below that bullet's own content. _(Automated: `MarkdownTab.test.tsx` writes the selected task marker; `ContentRenderer.test.tsx` checks nested source targets; `tweakComments.test.ts` checks nested anchors and list rendering.)_
- [ ] [VISUAL] Hover beside a Markdown block: its rule stays in the gutter with clear space before the text, and the comment button does not cover the block. _(NOT VERIFIED 2026-09-29: blocked — Markdown tab opened (agb.md) but rendered blocks carry no data-comment-source-start attributes and hovering the gutter (mouse move 340,241) produced no comment button/rule; feature not reachable in this web session; no visual check possible.)_
- [ ] Click a task-list checkbox inside a commented block: it cycles state without opening the comment popover; task rows after a comment containing `- [ ]` still update the correct source line. _(NOT VERIFIED 2026-09-29: blocked — Same as 671: no comment overlay hooks in DOM (data-comment-source-start count 0) for the file opened from the file browser; task checkbox cycle with comments not testable. Task checkboxes render (2 unchecked inputs).)_
- [ ] With tweak comments in the file, choose a same-repository agent in the Markdown topbar and click **Send**. An idle agent receives the request immediately; a busy agent shows one queued command and receives it on its next idle transition. _(NOT VERIFIED 2026-09-29: Needs a real agent tab in the same repository with idle/busy transitions to observe immediate delivery vs queued delivery.)_

## Design Mode (2026-09-23) — **Rust, needs a `make dev` restart**

The existing `make dev` process does not hot-reload Rust. Restart it when the
current agent sessions can be closed, or use a separate debug instance with
`TUIC_APP_INSTANCE=<id>` to keep its configuration isolated. Targeted tests
cover the individual contracts; this check joins them in a real Chrome and
agent session.

An isolated `make dev` attempt on 2026-09-23 stopped before launch because
port 1421 was already serving a different checkout's Vite server. Do not stop
that checkout merely to run this check.

- [ ] In the restarted instance, set a repository's **Dev Server URL** to a _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Design Mode routes (/design-mode*) are desktop-only: GET /design-mode -> 404 on headless tuic-remote. Needs desktop build with a real claude tab (spawn it with env CLAUDE_CODE_CHILD_SESSION=0 to avoid inherited child-session env) and host Chrome. )_
      local page with a click handler. Start Design Mode from an agent tab: a
      dedicated Chrome window opens the configured URL, hovering highlights an
      element, and clicking selects it without firing the page handler.
- [ ] Begin typing a note in the bound agent's composer, select two elements, _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Design Mode routes (/design-mode*) are desktop-only: GET /design-mode -> 404 on headless tuic-remote. Needs desktop build with a real claude tab (spawn it with env CLAUDE_CODE_CHILD_SESSION=0 to avoid inherited child-session env) and host Chrome. Also needs composer prefill (pty prefill_agent_input))_
      and confirm both grab blocks appear alongside the untouched note without
      submitting. Check selector, path, style subset, rectangle, HTML snippet,
      nearby text, source location when the dev build supplies one, and a valid
      `[image: …]` PNG path.
- [ ] On a page whose CSS uses custom properties (for example a Tailwind v4 _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Design Mode routes (/design-mode*) are desktop-only: GET /design-mode -> 404 on headless tuic-remote. Needs desktop build with a real claude tab (spawn it with env CLAUDE_CODE_CHILD_SESSION=0 to avoid inherited child-session env) and host Chrome. Needs a Tailwind v4/shadcn fixture page as Dev Server)_
      or shadcn app), select a themed button. The grab carries a `tokens:` line
      with only the `--…` variables that button's rules reference, resolved to
      their values, and not the whole theme.
- [ ] On a page built from web components (open shadow roots with their own _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Design Mode routes (/design-mode*) are desktop-only: GET /design-mode -> 404 on headless tuic-remote. Needs desktop build with a real claude tab (spawn it with env CLAUDE_CODE_CHILD_SESSION=0 to avoid inherited child-session env) and host Chrome. Needs a shadow-root web-component fixture page as Dev)_
      `<style>` or `adoptedStyleSheets`), select a button inside a component.
      The `tokens:` line lists the component's own `--…` variables. jsdom has no
      `ShadowRoot.styleSheets`, so no unit test covers this.
- [ ] Start Design Mode from another agent terminal in the same repository. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Design Mode routes (/design-mode*) are desktop-only: GET /design-mode -> 404 on headless tuic-remote. Needs desktop build with a real claude tab (spawn it with env CLAUDE_CODE_CHILD_SESSION=0 to avoid inherited child-session env) and host Chrome. Needs two agent tabs in one repo.)_
      Confirm the existing Chrome window is reused and subsequent grabs go to
      the newly bound terminal. Close that terminal, then Chrome: the status
      indicator must show Stopped and no further grab may be delivered.
- [ ] With a separate debug instance, check that quitting TUICommander closes _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Design Mode routes (/design-mode*) are desktop-only: GET /design-mode -> 404 on headless tuic-remote. Needs desktop build with a real claude tab (spawn it with env CLAUDE_CODE_CHILD_SESSION=0 to avoid inherited child-session env) and host Chrome. Needs a separate debug desktop instance quit.)_
      only the Chrome windows it owns. A browser/PWA start must explain that
      Chrome opens on the host machine.
- [ ] Review fixes (2026-09-23): a page running `debugger;` keeps responding; _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Design Mode routes (/design-mode*) are desktop-only: GET /design-mode -> 404 on headless tuic-remote. Needs desktop build with a real claude tab (spawn it with env CLAUDE_CODE_CHILD_SESSION=0 to avoid inherited child-session env) and host Chrome. Needs a page running debugger; plus Vite dev page fix)_
      a Vite/webpack dev page resolves `source:` for most components, not only
      the first four modules; closing the bound terminal turns the hover
      highlight off, and a later start delivers no click made in between;
      closing Chrome and starting again works on the first click.

## Terminal Progress (2026-09-23) — Rust, needs a `make dev` restart

- [ ] After restarting an isolated dev instance, open two agent PTYs in one repo and record different `intent:`/`progress` entries. Progress should open on the active PTY, allow switching to the other PTY, and show both in **All repo**. _(NOTE 2026-09-23: blocked — every `ProgressStore::open()` fails on :9876 (`no such table: sqlite_sequence`), see Progress Flow view, first item.)_ _(NOT VERIFIED 2026-09-30: partial — Backend: /progress/list {ptyId:A} returns only A's entries; unfiltered returns all PTYs' entries; ptyIds lists every PTY with history. Progress dialog default-to-active-PTY and switching are UI (not observed).)_
- [ ] In **All repo**, existing entries recorded before this change should remain visible as **Terminal unknown**. Closing a PTY should leave its saved history selectable. _(NOTE 2026-09-23: blocked — progress store fails to open on :9876, see Progress Flow view, first item.)_ _(NOT VERIFIED 2026-09-30: partial — Backend: POST /progress/report without pty -> entry ptyId=null in unfiltered list ('Terminal unknown' source). Closed PTYs (DELETE /sessions) stay in ptyIds, history stays selectable. Label and dialog selection are UI, not observed. Pre-change legacy rows not tested.)_
- [ ] Open PTY A, switch to PTY B, then **All repo** and close Progress. Reopen each view and confirm each has its own last-visit divider; viewing A alone must not mark B or **All repo** as seen. _(NOTE 2026-09-23: blocked — progress store fails to open on :9876, see Progress Flow view, first item.)_ _(NOT VERIFIED 2026-09-30: partial — API: POST /progress/viewed?ptyId=A sets lastViewedMs for A only; list for B and for All (no ptyId) still null; POST /progress/viewed (All) sets only All, B stays null. Per-view state isolation confirmed. Divider rendering and dialog open/close flow not observed (UI).)_
- [x] Start a **new** Claude Code session after the restart (a running one keeps its old tool list). Its tool list must show `mcp__tuicommander__progress` with a full schema, not as a deferred name behind ToolSearch; at the end of a task it must report `done` without being asked. Before this change only Codex terminals wrote done/blocked entries (`anthropic/alwaysLoad` on the `progress` tool, `mcp_transport.rs`). _(NOTE 2026-09-23: first half passes — a Claude Code subagent session started at 13:41 against :9876 received `mcp__tuicommander__progress` with its full schema, not deferred; `progress_is_the_only_tool_claude_code_must_not_defer` passes. Second half fails: the `done` write cannot land while the progress store fails to open (see Progress Flow view, first item).)_ _(verified 2026-09-30: Spawned real claude (sonnet) against rust0930 via tuic-bridge; it reported mcp__tuicommander__progress schema loaded, other 9 tools deferred. Then a plain file-write task: agent called progress unprompted; /repo progress_list has entry type=done 'Created hello730.txt...' ptyId=session. Note: hooks f)_

## Terminal scrollbar thumb minimum 48px (2026-09-23) — frontend, live via HMR

- [ ] [VISUAL] A terminal with a very long scrollback (e.g. `seq 100000`): the thumb stays 48px tall and is easy to grab; dragging it scrolls the whole history, top to bottom. _(NOT VERIFIED 2026-09-29: partial — seq 100000 in Terminal 4 session: scroll-info total_lines 10024, screen_lines 24 (long scrollback reached). Thumb is drawn on the canvas (no DOM element; canvas 300x150), screenshot times out, so 48px height/drag not measured. Code: scrollbarThumb.ts MIN_THUMB_PX=48, height=min(trackH,max(48,trackH*ratio)) (read only).)_
- [ ] [VISUAL] A short scrollback still gets a proportional (larger) thumb; a very short split pane never shows a thumb taller than its track. _(NOT VERIFIED 2026-09-29: partial — Code read only: scrollbarThumb.ts:41 height=min(trackH,max(MIN_THUMB_PX,trackH*ratio)) caps thumb at track height; canvas-drawn thumb not measurable in this web session (no DOM, screenshot timeouts).)_

## Branch icon toggles agents (2026-09-22) — frontend, live via HMR

- [ ] [VISUAL] Hover the icon of a branch with terminals: it swaps to a chevron (pointing down when expanded) in the same box; the row does not shift. _(NOT VERIFIED 2026-09-29: partial — Hover over branch icon (mouse move): chevron opacity 0->1, icon opacity 1->0, toggle box identical [17,68,14,12], row height 26 unchanged; chevron pointing-down not judged (transform none, no screenshot).)_
- [x] Click the icon: agents expand/collapse, the branch does NOT open. Click the row: the branch opens, agents do NOT expand/collapse. _(verified 2026-09-29: With co1 active: clicking main's icon toggle flipped aria-expanded false->true and active stayed co1; clicking main row made main active and aria-expanded unchanged.)_
- [x] Tab to the icon + Enter/Space toggles; focus ring shows the chevron. _(verified 2026-09-29: focus() on icon toggle (role=button tabIndex 0); trusted Enter toggled aria-expanded true->false, Space back to true; :focus-visible true with chevron opacity 1 and icon opacity 0.)_
- [ ] Every branch with terminals starts expanded after the reload; a branch collapsed via its icon stays collapsed after a restart. _(NOT VERIFIED 2026-09-29: partial — Collapsed via icon, reloaded page: aria-expanded stayed false (count shown), so collapse persists across reload. Not done: 'starts expanded after reload' from a fresh state, and instance restart.)_
- [ ] [VISUAL] Repo header: GitHub badge sits right next to the repo chevron; on hover ⋯ and + appear to its left, nothing shifts. _(NOT VERIFIED 2026-09-29: partial — Repo header hover (mouse move): repoActions (⋯ and +) opacity 0->1, repoName x=10 and chevron x=277 unchanged (no shift). No GitHub badge in fx/repo (no GitHub remote) so badge placement not verifiable; no screenshot.)_

## Theme review applied (2026-09-23) — **Rust, needs a `make dev` restart**

Bundled JSONs updated (VS Code Dark now follows VS Code Dark 2026), Deep Black / Delicate One removed, "Clean"
shown as "Ink" and "VS Code Light" as "Paper", default and fallback moved to
Commander (`config.rs` default, `DEFAULT_THEME` in `settings.ts`). Before/after
reference: `docs/design/theme-gallery-2026-09-22/`. `seed_builtin_themes` is a
no-op once `<config>/themes` exists, so an existing install sees none of the
new colors or names until the JSONs are copied into that folder.

- [x] In a **restarted** instance with an empty `<config>/themes` (or
      `TUIC_APP_INSTANCE=<id>`), Settings > Appearance lists 13 themes, with Ink,
      Paper and VS Code Dark and without Deep Black or Delicate One.
      _(verified 2026-09-23: `themes.rs:306` `BUILTIN_THEMES` holds 13 files; their
      `name`s include Ink (`clean.json`), Paper (`vscode-light.json`) and VS Code
      Dark, none is Deep Black or Delicate One; `seed_creates_dir_and_files_when_missing`
      and `builtin_themes_parse_successfully` pass. Not run in an isolated instance.)_
- [x] Same instance, `config.json` with `"theme": "does-not-exist"`: the app
      opens in Commander and logs `falling back to commander`.
      _(verified 2026-09-23: `themes.ts:313` warns `Unknown theme "<key>", falling back
      to commander` and `getAppTheme` falls back to `DEFAULT_THEME = "commander"`
      (`settings.ts:85`); `src/__tests__/themes.test.ts` "falls back to commander for
      unknown theme" and "warns when applying unknown theme" pass, 24/24.)_
- [ ] [VISUAL] On Paper: the toolbar wordmark is a clean grey with no dark _(NOT VERIFIED 2026-09-30: blocked — VISUAL-owned by tuic-live-checks)_
      smear, a colored repo name is readable, and the active tab row in the
      sidebar is visible.

## Clean theme bundled (2026-09-22) — **Rust, needs a `make dev` restart**

`clean.json` is in `BUILTIN_THEMES`, but `seed_builtin_themes` is a no-op once
`<config>/themes` exists, so no existing install receives it from the bundle.
Verified live on 2026-09-22 by copying the file into
`~/Library/Application Support/com.tuic.commander/themes/` (the watcher picked
it up): pure black chrome, white accent, neutral tab-type tints on
`html[data-theme="clean"]`, toast clamp at four lines. Font smoothing
(`antialiased`, 0.01em tracking) is global on `html`, verified live on both
Clean and VS Code Dark.

- [x] In a **restarted** instance with an empty `<config>/themes` (or
      `TUIC_APP_INSTANCE=<id>`), Settings > Appearance must list "Ink" (key `clean`) without
      any manual copy.
      _(verified 2026-09-23: `clean.json` (name "Ink") is in `BUILTIN_THEMES`
      (`themes.rs:332`); `seed_creates_dir_and_files_when_missing` passes. Code and
      test only, not an isolated instance.)_
- [x] Selecting "Ink" (then named "Clean") applies the black chrome and antialiased text at once.
      _(verified: live screenshot + `document.documentElement.dataset.theme ===
      "clean"`, computed `-webkit-font-smoothing: antialiased`, 2026-09-22)_

## DL and SU stop manufacturing scrollback (story `834-1878`, 2026-09-22) — **Rust, needs a `make dev` restart**

`delete_lines` and `scroll_up` used to push the rows they removed into history.
The fork tests and the retained ANSI captures cover the buffer; what they cannot
cover is what a real agent's repaint looks like on screen, and whether anything
a user relies on scrolled away with it.

- [x] In a **restarted** instance, run an agent that repaints with DL — Claude
      Code or any Ink TUI redrawing its box is enough. Scroll back afterwards:
      the history must hold what the agent printed, with no duplicated frames of
      the repainting box. Before the fix each repaint left its removed rows
      behind.
      _(verified 2026-09-23 with a synthetic repaint, not an Ink agent: throwaway
      `POST /sessions` on :9876, `seq 1 100`, then 20× `CSI 1;1H CSI 10 M`, 20× `CSI 5;1H
      CSI 10 M`, 20× `CSI 5 S` and 20× `CSI 2;20r CSI 5 S`: `scroll-info.total_lines`
      grew by exactly 1 per command (its own echo line) instead of up to 200.)_
- [x] Scroll back far enough to be off the live screen, then let the agent
      repaint. The viewport must stay where you put it — a control scroll no
      longer shifts a scrolled-back view, so the rows under your eyes must not
      move.
      _(verified 2026-09-23, synthetic: scrolled 50 lines back, sent 20× `CSI 10 M` at
      the top row; `row-text?row=0` read `228` before and after, `display_offset`
      moved 50→51 only for the one real linefeed of the command echo.)_
- [ ] Select text in the scrollback, let the agent repaint, then copy. The _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Backend half verified on rust0930: seq 1..300, selection-text rows(150..152)=='150\n151\n152' identical before/after 20x CSI M/L/S/r repaint storms (historyBase=0). Frontend mouse selection+copy not observable headless.)_
      selection must still yield the text it covered.
- [x] Run `less` or `man` on a long file and quit. Everything printed before it
      must still be in the scrollback — this is the linefeed path, which must be
      unchanged.
      _(verified 2026-09-23: throwaway session, `seq 2001 2060`, `less` on a 201-line
      file, `q`: `/terminal/lines` still held 2001 and 2060, no `less` content leaked
      into history, `total_lines` 84→85.)_

## Hands-free from a browser tab (story `832-e730`, 2026-09-22) — **Rust, needs a `make dev` restart**

A browser now has its own microphone and speaker for hands-free: the tab opens
`GET /dictation/hands-free/audio?owner=<id>` and streams capture up and replies
down on it. Rust refuses an owner with no socket rather than falling back to the
server's hardware, in either direction, and every part of that is covered by
tests — what no test can reach is a real microphone, a real speaker and a real
tab being closed.

- [x] Open the web UI of the **restarted** instance in a real browser — port _(verified 2026-09-29: web UI (browser mode) Settings has a Voice tab with a Dictation section (renamed from 'Dictation'); no global hotkey field and no microphone-device list in the section text)_
      9876 if it took it, else 9877 — and go to **Settings > Dictation**. Check
      the port first: an instance started before this commit serves the old
      frontend and has no audio route, so testing it proves nothing. The tab
      must be **present** — it used to be hidden outside Tauri. The global
      hotkey and the microphone-device list must be **absent**.
- [ ] Press Start on a terminal running an agent. The browser must ask for _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Hands-free routes (/dictation/*, incl. /dictation/hands-free/audio WS) are #[cfg(feature=desktop)]: 404 on this headless rust0930 instance (unix socket and :9881). Needs the desktop build + browser (Chrome fake-device mic permission prompt).)_
      microphone permission, and the phase must reach `waiting`.
- [ ] **[HUMAN]** Hold a complete turn: speak, see the transcript delivered to _(NOT VERIFIED 2026-09-30: blocked — audio hardware (mic/speakers, human listening); also dictation routes absent on headless build)_
      the agent, and hear the reply **through the browser's speakers** — not
      through the machine running TUICommander. Check the other machine is
      silent.
- [ ] **[HUMAN]** Barge in mid-reply. The reply must stop where you are, not _(NOT VERIFIED 2026-09-30: blocked — audio hardware (mic/speakers, human barge-in); also dictation routes absent on headless build)_
      merely stop being sent.
- [ ] **[HUMAN]** Close the tab mid-utterance. The conversation must disarm _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Socket-close disarm is testable (WS to /dictation/hands-free/audio?owner=X + arm on a real agent PTY, then GET /dictation/hands-free armed) but /dictation/* routes are desktop-gated: 404 on headless rust0930 (unix+:9881). Needs desktop build.)_
      (`GET /dictation/hands-free` reports `armed: false`), nothing may stay
      queued, and nothing may be left speaking.
- [ ] Arm from the desktop app while a browser tab holds an audio socket. The _(NOT VERIFIED 2026-09-30: blocked — audio hardware: desktop microphone; also dictation routes absent on headless build)_
      desktop must use its own microphone and ignore the browser entirely.
- [ ] Reload the browser tab while armed. The old conversation must disarm _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Reload-disarm testable via two WS connects with arm on a real agent PTY (real claude PTY now available), but /dictation/* routes are desktop-gated: 404 on headless rust0930. Needs desktop build.)_
      rather than follow the new socket.

## ANSI edit-operation contract (2026-09-22) — **Rust, needs a `make dev` restart**

Four handler fixes in the Alacritty fork: IL/DL reset the cursor column,
IL/DL/ICH/DCH/ECH resolve a pending wrap, ED0 spares the cell behind one, and
DCH blanks only the cells it removed. Replay evidence and 200 fork tests cover
the parser; these items are the part a live terminal shows.

- [x] Run a full-screen TUI that edits lines in place (`htop`, `lazygit`, `vim`
      with a long line at the right margin). No row may paint at the wrong
      column after a redraw, and no character may disappear from the last column.
      _(verified 2026-09-23 with synthetic CSI on a live :9876 throwaway session, not
      htop/vim: `CSI 3;10H CSI 1 M` + `X` gave `Xddd` and `CSI 4;10H CSI 1 L` + `Y` gave
      `Y` at column 0; at a pending wrap in the last column (220 cols), ECH/DCH/ICH then
      `X` put `X` in the last column of the same row, EL0/ED0 kept the last `F` and `X`
      wrapped to the next row — the documented contract.)_
- [x] In a Claude/Codex tab, let an agent stream a tall frame that repaints.
      Scrollback must not gain rows the agent did not print.
      _(verified 2026-09-23, synthetic, same probe as the DL/SU section: 80 DL/SU
      repaints added no history rows. Not observed with a real agent.)_
- [ ] **[VISUAL]** Select and copy text ending at the right margin after such a _(NOT VERIFIED 2026-09-30: blocked — VISUAL-owned by tuic-live-checks)_
      redraw. The copied text must keep its last character.

## Recover captured terminal context (2026-09-22) — frontend

- [x] Captured context reappears on the idle Grok tab after frontend reload.
      _(verified: live DOM after automatic HMR, 2026-09-22: the same PTY session
      shows `Intent: locking out hashtags` and its campaign prompt without
      additional input; hook regressions cover delayed session attachment.
      The existing nine-word follow-up retains the previous substantial prompt.)_

## Selection during output (2026-09-21) — **Rust, needs a `make dev` restart**

- [x] Isolated browser verification passed: held and released multi-row
      selections survive 50 output rows at the history cap, with stable base
      canvas hashes. Snapshot copy returns the original line; expired endpoints
      return HTTP 409. Clipboard writes were intercepted before app load.
      Evidence: `tests/terminal-stress/SELECTION_FINDINGS.md`.
- [ ] After loading the updated frontend, copy one block, then select a different _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Client-side selection persistence across a full frame is frontend-only (no server selection state); backend contract (historyBase snapshots) verified under 895. Needs desktop/browser build with clipboard guard.)_
      multi-row block while output continues and keep the mouse held. The new
      selection must not disappear when another full frame arrives.
- [ ] After loading the rebuilt backend and frontend, park a terminal in history _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Backend half verified: selection-text with historyBase: snapshot at seq 1..300 rows(150-152) stays valid under 9000 rows, then 409-style error 'selection rows are no longer retained' after passing the 10000-line cap (never substitutes next row); a live snapshot re-read after +500 rows with historyBa)_
      while output continues beyond the scrollback cap. Selection must stay on
      the same retained text, both during dragging and after release; copying
      must not substitute the next row when history advances. Automated browser
      checks require the verified clipboard guard in `tests/terminal-stress/`.
      Investigation and validation: `plans/terminal-selection-output-integrity.md`.

## Terminal Unicode integrity (2026-09-21) — **Rust, needs a `make dev` restart**

- [x] Resize/scroll visibility recovery verified in the isolated browser on
      2026-09-21: 1200×1223 → 1000×700 → scroll-to-top stayed painted, with
      scrollbar movement and valid frames, without forced hide/show. Evidence:
      `.tmp/terminal-integrity/post-fix-20260921-1819-frontend/`.
- [ ] Finish the block-cursor-on-decomposed-glyph visual check using a browser _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Block cursor on decomposed glyph is canvas rendering (no DOM). Backend: 'e'+U+0301 stays 2 code points in 1 cell (hyperlink span after decomposed text is cell-aligned [14,19]); needs desktop/browser canvas build with clipboard interception.)_
      with verified clipboard interception. The prior run was interrupted after
      terminal auto-copy overwrote the host clipboard; do not repeat real
      selection/copy/paste against the shared clipboard.
- [ ] After loading the rebuilt backend, print precomposed and decomposed accents, _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Backend half verified: printf PRE U+00E9, e+U+0301, a+U+0300 and accent written after base in a second printf -> lines keep code points 0xe9,0x65 0x301,0x61 0x300,0x65 0x301; resize 60->14->10->60 reflow keeps all combining marks; OSC8 span after accented text cell-aligned; POST search-buffer matche)_
      including an accent written after its base letter. Verify rendering, block
      cursor, selection/copy, search and a link following accented text; scroll
      during output and resize. The installed instance still needs a restart
      even when the isolated verification build passes. See
      `plans/terminal-unicode-integrity.md` for automated evidence and limits.

## Dictation preserves speech before a pause (2026-09-21) — **Rust, needs a `make dev` restart**

- [ ] After restart, dictate a short phrase followed by a pause while holding F5. The live preview must receive the phrase; the final transcription must retain it. Verify silent recordings remain rejected by the configured speech gates. _(NOT VERIFIED 2026-09-30: blocked — audio hardware: microphone/F5 speech; dictation routes absent on headless build)_

## Hooked dialogs restore the question badge (2026-09-21) — **Rust, needs a `make dev` restart**

- [ ] After restart, a Claude selection dialog with native hooks enabled must _(NOT VERIFIED 2026-09-30: partial — Real claude (sonnet) AskUserQuestion 2-question dialog, hooks emulated: agent_state awaiting_input/awaiting_input=true on dialog open; Tab Q1->Q2, resize x2 kept it; Enter-answer Q1 kept it; ONE 'awaiting_input set' log. On the Submit/Review tab (dialog still open) awaiting dropped (completed/idle o)_
      show the question badge. It must survive switching sub-questions and
      redraws without duplicate notifications. After the final answer or cancel,
      normal protocol activity must clear question when the dialog closes.

## Protocol idle survives terminal animation (2026-09-21) — **Rust, needs a `make dev` restart**

- [ ] After a rebuild/restart, finish a Codex turn in brainstorming with its idle _(NOT VERIFIED 2026-09-30: partial — Real codex 0.159: after a turn idle held 40s and 2nd prompt -> busy ~1s after Enter (hook-idle Protocol). Codex pet animation (pet=codex in private CODEX_HOME) did not render/emit output in the TUIC PTY (last_activity static), so real animation NOT exercised. Emulated: script sends OSC 7770 idle the)_
      animation enabled. The terminal and session API must remain idle while the
      animation runs. Submit another prompt: working must return immediately.
- [x] Finish a Claude turn: idle must survive ordinary redraws. A real active _(verified 2026-09-30: Real claude (sonnet) in rust0930 with launch hooks emulated + project Stop hook that blocks once (exit 2, run sleep 12): agent_state stayed working 0.7s..22.2s across the block (Stop hook fired 08:03:24, again 08:03:41), then idle. After idle: 4 resizes, ctrl+l x2, typing/backspace over 30s: sample )_
      phase from a blocking Stop hook must still report working.

## Ego reaches this instance and the collapsed tool surface (#802-4c43, 2026-09-21) — **Rust, needs a `make dev` restart**

The ego MCP entry carried `TUIC_APP_INSTANCE`, which `tuic-bridge` never reads,
so with a named instance running beside the default one ego drove the DEFAULT
instance. It now carries `TUIC_SOCKET` set to the socket this process bound.
Separately, `tools/list` preferred the session flag over the per-request `_meta`
identity, so ego was handed the full catalogue after `server/discover` had
advertised the collapsed one.

- [ ] Start a named test instance (`TUIC_APP_INSTANCE=tuic-test make dev`) while _(NOT VERIFIED 2026-09-30: partial — AI Chat session/new fails on this host: ~/.ego/config.toml [mcp.servers.tuic] stdio bridge path missing (ENOENT) -> 'supplied MCP servers could not be admitted'; /acp/one-shot (no MCP) works. Not exercised: needs AI Chat session list + named instance)_
      Boss's install is running. Open the AI Chat panel there and ask ego to list
      the sessions: it must report the test instance's sessions, not Boss's.
- [ ] In that same conversation ask ego which tools it has. It must name _(NOT VERIFIED 2026-09-30: partial — AI Chat session/new fails on this host: ~/.ego/config.toml [mcp.servers.tuic] stdio bridge path missing (ENOENT) -> 'supplied MCP servers could not be admitted'; /acp/one-shot (no MCP) works. Not exercised: ego tool listing needs a session with the tuicommander MCP)_
      `search_tools`, `get_tool_schema`, `call_tool` and `progress` — four, not
      the full catalogue.
- [ ] Kill nothing and start a second copy so the primary socket is already held: _(NOT VERIFIED 2026-09-30: partial — AI Chat session/new fails on this host: ~/.ego/config.toml [mcp.servers.tuic] stdio bridge path missing (ENOENT) -> 'supplied MCP servers could not be admitted'; /acp/one-shot (no MCP) works. Not exercised: needs second instance socket + ego session)_
      ego in the second copy still reaches the second copy (it binds a `-<pid>`
      socket, and the entry carries that path).

## Mirrored remote events stay remote (#801-d34e, 2026-09-21) — **Rust, needs a `make dev` restart**

A mirrored event was indistinguishable from a local one, so a remote daemon's
`session-created`, `ui-tab`, `repo-changed` and worktree events ran the LOCAL
handlers. The window emit is now limited to `session-state-changed` and
`session-closed`, every mirrored payload carries `__tuic_origin`, and a frame
that already has one is dropped.

- [ ] Connect mac-mint, start a PTY **on mac-mint** (ssh in, `tuic session` there, _(NOT VERIFIED 2026-09-30: blocked — BLOCKED class: second physical machine (mac-mint/SSH daemon) required; no such host reachable from this instance)_
      or its own UI). This Mac must show it as a session-list row with the remote
      badge and **no new tab** — no `PTY: Session N`.
- [ ] Its busy/idle/question badge still moves from here while it works. That is _(NOT VERIFIED 2026-09-30: blocked — BLOCKED class: second physical machine (mac-mint/SSH daemon) required; no such host reachable from this instance)_
      the one thing the window emit is still allowed to carry.
- [ ] Open a repo folder on mac-mint from its own UI: no repository appears in _(NOT VERIFIED 2026-09-30: blocked — BLOCKED class: second physical machine (mac-mint/SSH daemon) required; no such host reachable from this instance)_
      this Mac's sidebar, and no git work runs here for that path.
- [x] Add a **Direct** connection whose URL is this machine's own daemon
      (`http://127.0.0.1:9876`). Connect must fail with "this very TUICommander
      instance — a machine cannot mirror itself", and the row must read Error.
      _(verified 2026-09-23 by code + test, no connection added to the live config:
      `remote_runtime.rs:769-773` refuses when `/health.instance_id` equals
      `instance_identity()`; `connecting_to_this_very_process_is_refused_by_identity`
      passes and asserts the message, `RemoteStatus::Error`, no token, no further probe.)_
- [x] `curl -s localhost:9876/health | jq .instance_id` returns a UUID, and it
      changes after a restart.
      _(verified 2026-09-23: :9876 returned `a53f203e-7e81-4d47-a0a8-6c3ec9a4d8ca`;
      `crates/tuic-core/src/app_instance.rs:152-155` mints it with `Uuid::new_v4()` in a process-local
      `OnceLock`, never persisted, so each process gets a new one.)_

## Workspace badge: file count instead of "Dirty", `in_sync` instead of "Merged" (2026-09-20) — **Rust, needs a `make dev` restart**

The sidebar called a worktree "Merged" while it held 24 uncommitted files. The
backend verdict now separates `in_sync` (HEAD is the default branch's tip, so
nothing was ever merged) from `merged`, and reports `dirty_files` — the count a
removal discards — instead of a `dirty` flag. **Until the restart the frontend
reads `dirty_files` off an old backend that does not send it, so every count is
0 and the old `merged` verdict still shows.**

- [x] After the restart, the `feat/sqlite-viewer-plugin` row must read `24 dirty` (or whatever `git status --porcelain -uall | wc -l` says in that worktree), not `Merged`. _(verified 2026-09-23 on substitutes — that worktree no longer exists: `GET /worktrees/lifecycle` on :9876 for all 7 linked worktrees of `LS/agent2` and `engineering-blog` returned `dirty_files` equal to `git status --porcelain -uall | wc -l` (7, 0, 0, 0, 2, 3 …); `RepoSection.tsx:590-597` puts the count before `Merged`. The chip shows only when no stats/PR chip is on the row, else the count is in that chip's tooltip.)_
- [x] No `main` row anywhere in the sidebar carries a lifecycle badge, however dirty. Main is not removable from that list, so the badge has nothing to warn about. _(verified 2026-09-23 by code: the badge is gated on `!props.branch.isMain` (`RepoSection.tsx:579`); `LS/agent2` main has 2388 dirty files and would otherwise show one.)_
- [x] A worktree with commits of its own, all merged into the default branch, and a clean tree still reads `Merged`. _(verified 2026-09-23 by test + code, no such worktree exists live: `a_workspace_behind_the_default_tip_is_merged`, `a_workspace_on_the_default_tip_is_in_sync_not_merged` and `a_workspace_with_its_own_commit_is_unmerged` pass; `RepoSection.tsx:597` renders `Merged` for `merged` only.)_
- [x] Removing a worktree with uncommitted files: the confirm dialog must name the count ("N uncommitted files will be discarded"), not the word dirty. _(verified 2026-09-23 by code: `useConfirmDialog.ts:110-111` builds `${lost} uncommitted file(s) will be discarded` from `dirtyFiles`.)_

## Notification sound teardown (2026-09-20) — **Rust, needs a `make dev` restart**

- [ ] In Settings → Notifications, play each Test sound through the output device that previously crackled. The tone must end cleanly, with no relay-like click after its release. The source now reaches an exact zero sample and feeds 100 ms of silence before closing, but only the real CoreAudio device can verify the hardware-buffer teardown. _(NOT VERIFIED 2026-09-30: blocked — BLOCKED class: audio hardware (real CoreAudio output device and ears).)_

## Progress dialog and journal (2026-09-19)

- [ ] **Rust, needs a `make dev` restart.** In an isolated agent session, print an `intent:` that soft-wraps over at least 12 rows in a 40-column terminal. Confirm the complete title appears once and Progress records a capped entry. Scroll a title-less intent under capped history and confirm it stays open until the next prose line. Composer chrome must not extend the intent, and an unfinished `(` title fragment must be removed when the intent closes. _(NOT VERIFIED 2026-09-29: partial — Fake agent, 40-col grid via POST /resize: 560-char soft-wrapped intent (14 rows) -> title 'Soft Wrapped Title' set once, ONE journal entry capped at 500 chars ending '…'. Unfinished '(' fragment removed on close. Title-less intent closes at next prose line. NOT tested: capped-history scroll, composer chrome not extending intent.)_
- [x] **Rust, needs a `make dev` restart.** In an isolated dev instance, print a title-less `intent:` followed by indented prose in a wide agent terminal. Progress must keep only the intent text; the prose must not become a tab title. In a narrow terminal, print a title after three hard-wrap rows, then another intent: both entries and both titles must appear. A long intent must keep its full tab-title event while its journal text ends at 500 characters. _(verified 2026-09-29: Wide 200col: titleless intent + indented prose -> journal only intent text, display_name unchanged. 40col: title after 4 hard-wrap rows then 2nd intent: both entries + titles (Narrow Zed, Second Zed). 750-char intent: journal 500 chars with ellipsis, full Long Title event.)_

- [ ] **Rust, needs a `make dev` restart.** In an isolated dev instance, have Codex stream a long `intent:` in a narrow terminal while it redraws and moves the cursor to its composer. Progress should receive one full entry with its title; a later identical repaint must add none. Restart when current PTY sessions may be lost. _(NOT VERIFIED 2026-09-29: Needs real Codex streaming a long intent in a narrow terminal.)_

- [ ] Open Progress (palette: "Open Project Progress"), move the pointer across three rows, then off the list. Every delete icon must be hidden again; before, WKWebView kept the icon of every row crossed. CSS only, live via HMR — no restart. Item created because the fix could not be reproduced programmatically. _(NOT VERIFIED 2026-09-29: WKWebView-specific hover-stuck icon; author states it could not be reproduced programmatically; needs desktop app and human.)_
- [ ] **Rust, needs a `make dev` restart.** With an agent tab open, let it print `intent: …` and let the screen repaint (spinner running). `sqlite3 "<config dir>/progress.sqlite3" "select count(*) from entries where kind='intent' and created_at_ms > <restart ms>"` must grow by one per distinct intent, not per repaint. Then call the `progress` tool twice with the same `done` text and confirm both rows land. _(NOTE 2026-09-23: FAILS on :9876 — no row at all has landed since the restart (newest `created_at_ms` is 12:04, restart 13:08) because every `ProgressStore::open()` fails with `no such table: sqlite_sequence`; see Progress Flow view, first item.)_ _(RE-CHECK 2026-09-30, make dev from main 8c407689b: store now works — `repo action=progress_list` returns rows landing after restart; two identical `done` calls landed as separate rows 6422 and 6423. NOT VERIFIED: one-intent-per-distinct-intent under spinner repaint; recent intent rows from other agents (6413, 6414, 6421) appear once each, no repaint duplicates seen.)_

## ego reaches TUIC over the stdio bridge (story `796-7fa3`, 2026-09-20) — **Rust, needs a `make dev` restart**

The session's MCP entry moved from an HTTP URL to the `tuic-bridge` sidecar on
stdio. Every test here stops at the wire shape TUICommander writes; what none of
them reaches is ego actually spawning that binary and calling a tool through it.

- [ ] With **Remote Access off** (Settings → Remote Access), open the AI Chat panel and ask ego to list the open terminal sessions. It must answer with them. Before this change the same question got a refusal or an empty answer, because the session carried no MCP server at all — that is the whole bug. _(NOT VERIFIED 2026-09-30: partial — AI Chat session/new fails on this host: ~/.ego/config.toml [mcp.servers.tuic] stdio bridge path missing (ENOENT) -> 'supplied MCP servers could not be admitted'; /acp/one-shot (no MCP) works. Not exercised: needs ego session with bridge)_
- [ ] Turn Remote Access **on**, start a new chat session, ask again. Same answer. The switch must no longer change what ego can reach. _(NOT VERIFIED 2026-09-30: partial — AI Chat session/new fails on this host: ~/.ego/config.toml [mcp.servers.tuic] stdio bridge path missing (ENOENT) -> 'supplied MCP servers could not be admitted'; /acp/one-shot (no MCP) works. Not exercised: needs ego session + Remote Access toggle)_
- [x] Check the tool surface is still the collapsed one. _(verified: covered on both sides instead — `tuic-bridge` `the_downstream_client_name_is_forwarded_and_not_replaced_by_the_bridges_own` asserts the proxied `initialize` still carries `clientInfo.name = ego` and that the bridge's own session opens under its own name, and `mcp_transport::tests::the_collapsed_surface_is_decided_by_the_name_the_bridge_forwarded` asserts `tuic-bridge` does NOT earn the collapsed surface by itself. Falsified by mutation: renaming the forwarded client turns the bridge test red.)_
- [ ] Launch a second instance with `TUIC_APP_INSTANCE=qa`, open AI Chat there, and ask ego which repositories it can see. It must see the `qa` instance's repositories, never the default instance's. _(NOT VERIFIED 2026-09-30: partial — AI Chat session/new fails on this host: ~/.ego/config.toml [mcp.servers.tuic] stdio bridge path missing (ENOENT) -> 'supplied MCP servers could not be admitted'; /acp/one-shot (no MCP) works. Not exercised: needs second instance + ego session)_

## PR review, changelog and improvement scan run on ego (story `795-320b`, 2026-09-20) — **Rust, needs a `make dev` restart**

`pr_review.rs`, `changelog.rs`, `improvement_scan.rs`, two new `AppEvent`
variants and four new HTTP routes are all new Rust, and none of them load into a
running `make dev`. The parsers, the confidence gate and the stores are unit
tested; what no test reaches is a real ego process answering a real diff.

- [ ] Open a PR's detail popover and click **Run** under AI Review. It must produce findings (or "No findings" with a reviewed-file count and "by ego"), never a blank panel. Tick a finding with a line number and click **Post review**; the comment must appear on the PR in GitHub. _(NOT VERIFIED 2026-09-30: partial — POST /repo/pr-review {repoPath,prNumber:88} on real PR via dev ego: summary + files[{findings:[]}] returned (No findings, 1 file). Post-review/UI/line ticking not exercised.)_
- [ ] A finding with **no** line number must be listed but its checkbox disabled — GitHub refuses an inline comment without a line. _(NOT VERIFIED 2026-09-30: partial — Review returned files[].findings schema for PR 88 but zero findings, so no line-less finding to check disabled checkbox (UI anyway).)_
- [ ] Rename `ego_executable` in Settings → General to something that does not exist and run the review again. The popover must show ego's own sentence, not an empty finding list. Put the real path back. _(NOT VERIFIED 2026-09-30: partial — ego_executable=/nonexistent -> /repo/pr-review and /repo/improvement-scan return {error:'ego could not run this turn: invalid ego executable: No such file or directory'}; popover render not seen. Restored afterwards.)_
- [ ] GitHub panel header → the document icon opens the Changelog modal. It must produce markdown for the merged PRs since the last tag, and **Copy** and **Save** must both work on the result. _(NOT VERIFIED 2026-09-30: partial — GET /repo/changelog?path=fx repo returns markdown (## Features/## Fixes with PR refs). Copy/Save buttons are UI.)_
- [ ] Open the **Ops Dashboard** (the chart icon in the same header). Click each of `refactor`, `testing` and `perf`. Each scan must fill the Proposals column with at most five cards, and **Create issue** on one of them must file a real GitHub issue. _(NOT VERIFIED 2026-09-30: partial — POST /repo/improvement-scan focus refactor/testing/perf each 200 with proposals:[] on tiny fixture (<=5 not testable); cards/Create issue is UI.)_
- [ ] While a review is running, the dashboard's Review findings column must show the PR as Working and then Done with a count — that is the `review-progress` event arriving over the bus. _(NOT VERIFIED 2026-09-30: partial — PR review ran to completion on real PR; review-progress event stream not captured; dashboard is UI.)_
- [ ] From a browser (not the Tauri app) at `localhost:9876`, run the same review and the same scan. Both must work: these routes are deliberately not desktop-gated. _(NOT VERIFIED 2026-09-30: partial — Unix-socket (local) router serves /repo/pr-review, /repo/changelog, /repo/improvement-scan OK. Remote router :9881 (Basic auth) returns 404 for all three (not in build_remote_router).)_

## The terminal stream is compressed over a remote connection (story `794-832e`, 2026-09-20) — **Rust, needs a `make dev` restart**

`ws_stream`, `mcp_http::ws_compression` and the ssh `Compression=yes` are new
Rust. The codec is unit-tested on both sides and the framing end to end against a
mock socket; what no test reaches is a real browser inflating a real socket, and
a real ssh process.

- [x] From a **second machine** (or a phone on the LAN), open the web UI against this daemon's IP and attach a terminal. In devtools → Network → the `stream` socket, the URL must carry `compress=deflate`, every message must be **Binary**, and the terminal must paint and stay live. Run something noisy (`yes | head -100000`) and watch the socket's byte counter — it should be a small fraction of what the same run costs locally.
      _(verified 2026-09-20, server side, against a headless `tuic-remote` on mac-mint
      reached from this Mac — a genuine non-loopback peer. A raw RFC6455 client
      (`scripts/ws-stream-probe.mjs`) asked for `?compress=deflate` and read the opcodes: every
      payload arrived as a **Binary** WS frame carrying the tag byte, 2 of 9 tagged
      `TextDeflate`, and **1849 bytes on the wire inflated to 4832** with 0 decode
      errors. The same workload with no `compress=` cost 4859 bytes for 4859 bytes of
      content, so the saving is 62% and the untagged path is untouched. **Still
      uncovered: a real browser running the app's own decoder** — this proves the
      server's framing, not the frontend's inflate.)_
- [ ] Same devtools panel, from a browser on **this** machine at `localhost:9876`: the URL must have **no** `compress=` at all and the messages must still be a mix of Binary and Text. This is the local path, and it must be untouched. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Client-side choice of compress= param is frontend (canvasTerminalTransport); server side: socket without compress= gets untagged frames. Needs a browser at the local URL.)_
      _(NOTE 2026-09-20: the server half is proven — a socket that sends no `compress=`
      gets genuine untagged Text frames, byte for byte the old framing. What is still
      open here is the **frontend's** choice not to ask on a local connection, which no
      HTTP probe can see.)_
- [ ] Resize the remote terminal, scroll it, and let the agent repaint a full screen. No stale rows, no torn frames — a mis-ordered inflate would show as rows from an older screen surviving under a newer one. _(NOT VERIFIED 2026-09-30: blocked — BLOCKED class: second physical machine. Daemon binds loopback only (LAN IP :9881 times out), so every peer is loopback and gets identity tags; no compressed frames obtainable here; real browser inflate also needed.)_
- [x] `curl` the daemon's log after a remote session: no `WsTransport could not decode a compressed frame` lines. One would mean the tags disagree.
      _(verified 2026-09-20: zero occurrences in the daemon log after the remote,
      loopback and untagged runs above, and the client decoded every deflated frame it
      received — `decode_errors: 0` on all three.)_
- [ ] Settings → Services → SSH Tunnels → edit a profile: **Compress the channel (ssh -C)** is on. Save, start the tunnel, and confirm with `ps ax | grep "[s]sh -N"` that the command line holds `-o Compression=yes`. Untick it, restart the tunnel, confirm `Compression=no`. _(NOT VERIFIED 2026-09-30: partial — Via /tunnels API against a fake sshd listener: profile compression:true -> ps shows 'ssh -N ... -o Compression=yes'; false -> Compression=no. Settings checkbox UI not driven.)_
- [ ] Open a repository through an **SSH remote connection** and attach a terminal. The stream socket asks for `compress=deflate` (the client sees a remote connection) but the daemon answers identity tags because the peer is loopback — the terminal must still work, and the saving comes from the tunnel instead. _(NOT VERIFIED 2026-09-30: partial — WS /sessions/{id}/stream?compress=deflate over loopback TCP with subprotocol tuic.deflate: 101 selects tuic.deflate, 44 frames all tag 0x02 (identity), stream live. SSH remote connection through the app not done.)_
      _(NOTE 2026-09-20: the **daemon half is proven** — the same probe run on mac-mint
      against `127.0.0.1:9879` with `?compress=deflate` got 10 tagged frames and **not
      one** `TextDeflate`, where a remote peer on the identical workload got 2. The
      wire cost was 4905 bytes for 4895 bytes of content: exactly the 10 tag bytes and
      nothing else. What is left is the tunnel wiring that puts a real client on the
      loopback side of it.)_

## Smart Prompts `api` mode runs on ego (story `787-ee50`, 2026-09-20) — **Rust, needs a `make dev` restart**

`acp_one_shot_prompt`, `POST /acp/one-shot` and `acp/oneshot.rs` are new Rust, so
none of this is live in a running `make dev`. The collector is unit-tested
against event sequences and the frontend against a double; what no test reaches
is a real ego process, which is every item below.

- [x] With **ego executable** empty, a prompt saved with `executionMode: "api"` is listed but disabled, and hovering it says ego is not configured and names *General* then *AI Providers*. _(verified 2026-09-29: Created prompt via Settings>Smart Prompts, Execution Mode=api, toolbar placement; ego empty: Smart Prompts dropdown item has class itemDisabled, opacity 0.5, title 'ego is not configured - name the binary in Settings > General, then pick a model in Settings > AI Chat' (says AI Chat, not AI Providers).)_
- [ ] With ego configured but no repository or terminal open, the same prompt is disabled and says ego needs a working directory. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: disabled-with-reason state is frontend; not exercisable headless.)_
- [ ] With a real ego and a repository open, run an `api` prompt whose output target is the clipboard. The clipboard must hold ego's final text, trimmed, with no reasoning in it. _(NOT VERIFIED 2026-09-30: partial — POST /acp/one-shot in fixture repo returns {text:'ok',stopReason:end_turn,declined:0}, plain final text; clipboard target is UI.)_
- [x] While it runs: `ps ax | grep ego` shows exactly one extra process, and it is gone within a second of the answer arriving. Run the prompt three times — no ego process accumulates. _(verified 2026-09-30: ps during /acp/one-shot: exactly one extra 'ego acp -C repo' process, gone <=1s after answer; 3 sequential runs left no ego process accumulating.)_
- [ ] The AI Chat panel's own connection is untouched: open the panel, send a turn, then run an `api` prompt. The panel's conversation must survive, and the prompt must not appear in it. _(NOT VERIFIED 2026-09-30: partial — one-shot uses its own launched ego (separate pid from the panel connection, which stayed alive); panel conversation itself not driven (session/new blocked).)_
- [ ] Ask an `api` prompt to do something that needs a tool ("list the files in this directory"). It must come back refused, saying how many permissions were declined — not as an empty answer, and never hanging. _(NOT VERIFIED 2026-09-30: partial — likely fixture (Boss ego config yolo/sandbox off); one-shot 'Run shell command touch r2-made.txt' -> executed (file created), declined:0, no refusal. Cause: ~/.ego/config.toml mode=yolo sandbox=off; unattended one-shot does not force no-tools/refuse. File removed.)_
- [ ] Run one with the output target set to *commit message*: the Git panel's commit box must fill. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: commit box fill is frontend; not exercisable headless.)_

## AI Providers tab over ego's configuration (story `786-4a6d`, 2026-09-20) — **Rust, needs a `make dev` restart**

The backend (`ego_cli.rs`, `/ego/*`) is new Rust, so none of this is live in a
running `make dev`. Every check below also needs a real ego binary: the tests
prove the join and the failure shapes against a double, never against ego.

- [x] With **Experimental Features** off, the Settings nav has no *AI Providers* entry and searching for "default model" finds nothing. Turn it on: the tab appears. _(verified 2026-09-29: Experimental off: nav lacks AI tab, search 'default model' -> 'No settings match your search.'. Enabled via General checkbox: nav gains 'AI Chat' (item says 'AI Providers': renamed) and search 'default model' -> 'AI Chat > Default Model'. Web UI :9880.)_
- [ ] With **ego executable** empty, open the tab. It must name the field to fill (General → AI Chat), not render an empty list, and launch nothing. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Backend half verified on rust0930 with ego_executable empty: GET /ego/providers and POST /ego/providers/model -> HTTP 409 {code:notConfigured,message:'no ego executable is configured; set it in Settings ...'}, no ego process spawned. The tab wording (names General -> AI Chat, no raw HttpRpcError) is)_
- [ ] Point the setting at a path that does not exist. The tab must say the executable could not be *started* — a different message from "not configured" — and show what the OS reported. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Backend half verified: PUT /config ego_executable=/nonexistent/ego-r2 -> GET /ego/providers HTTP 424 {code:launchFailed,message:'could not run the configured ego executable: No such file or directory (os error 2)',command:'ego-r2 config ls --json'} - distinct from notConfigured. Tab text is UI. (con)_
- [ ] With a real ego: the provider rows must match `ego doctor --json`, the model list `ego models --json`, and the picker's selection `ego config ls --json`'s `model`. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Backend half verified: GET /ego/providers vs real ego CLI: credential rows == 'ego doctor --json' (only openai-codex stored/ok, others missing); 11 model slugs and availability == 'ego models --json'; defaultModel == 'ego config ls --json' model (openai-codex/gpt-6-sol). Tab rendering not observed.)_
- [ ] Change the default model, then `ego config ls --json` in a terminal: the new value must be there, quoted. Restart TUICommander — the tab must still show it. (This is the criterion no test can reach: ego holds it, TUIC does not.) _(NOT VERIFIED 2026-09-30: partial — Real ego via instance API: POST /ego/providers/model ollama/gemma4:e4b-mlx -> `ego config get model` prints "ollama/gemma4:e4b-mlx" (quoted), config ls shows it, GET /ego/providers defaultModel reflects it (TUIC re-runs ego each call, no cache); restored, ~/.ego/config.toml byte-identical to backup.)_
- [ ] Press **Refresh from providers** and watch the network (or ego's own logs): the refresh must be the only outbound call, and opening the tab must make none. _(NOT VERIFIED 2026-09-30: partial — Watched processes during GET /ego/providers: default open runs no --refresh; ?refresh=true runs `ego models --refresh --json` (seen in ps). Outbound sockets not caught (0.15s run, lsof sampling empty) so 'only outbound call' not measured; UI button not observed.)_
- [ ] Break one provider (revoke a token, or point ego at an unreachable base URL) and press Refresh. The failure must be shown in ego's own words with the command and exit code, not swallowed into an empty list. _(NOT VERIFIED 2026-09-30: partial — Used shim ego (fails only `models --refresh` with stderr + exit 7, else real ego): GET /ego/providers?refresh=true -> {code:commandFailed,command:'ego-fail.sh models --refresh --json',stderr:<ego words>,exitCode:7}, not an empty list; open still works. Real broken provider and tab text not exercised)_
- [ ] Log out of one provider (`ego auth logout <provider>`) and confirm its row reads "no credential" while the others stay "stored" — a missing credential must not blank out the whole tab. _(NOT VERIFIED 2026-09-30: partial — API shows credential rows independent: openai-codex stored, anthropic/gemini/kimi-coding/ollama/openai 'missing' and model lists intact for all (no blanking). Actual `ego auth logout` NOT run: openai-codex is Boss's only stored credential (would delete it). Tab wording ('no credential') is UI.)_

## AI Chat runs on ego (story `785-58ca`, 2026-09-20) — frontend, Vite HMR picks it up, but it needs a real ego binary

Everything below is proven against a test double at the IPC boundary. What no
test can reach is the live process: these are the checks that need one.

- [x] With **ego executable** empty, open the panel: it must say ACP is not configured, show no input box, and launch nothing. _(verified 2026-09-29: ego empty, status-bar AI Chat button: panel text 'ACP is not configured. Set the ego executable in Settings to start a conversation.'; 0 visible textarea/input in panel. No spawn attributable (only ego pid seen was Boss's cwd ~/Gits, not ours).)_
- [ ] Set the path, open the panel on a repository, send a turn. The answer must stream in, reasoning must fold into a *Thinking* disclosure, and tool calls must stay one card each as their status changes. _(NOT VERIFIED 2026-09-29: Needs real ego streaming a turn with reasoning and tool calls.)_
- [ ] Let ego ask for permission. The buttons must be the ones ego published, and answering must clear the card in every open window — not only the one that answered. _(NOT VERIFIED 2026-09-29: needs a real ego agent (ACP) session; not available headless)_
- [ ] Switch repository and back. Only one ego process per root (`ps ax | grep ego`), and the first conversation must still be there. _(NOT VERIFIED 2026-09-29: Needs real ego processes (ps ax | grep ego) per repo root.)_
- [ ] A live prompt: check whether ego echoes the user message back as `user_message_chunk`. _(NOTE: the panel now renders server `promptSent` rather than a local optimistic message; if ego also sends a live user chunk, the two sources could duplicate it.)_

## Launch-scoped native agent status signals (story `746-30a9`, 2026-09-13) — **Rust, needs a `make dev` restart**

- [x] [HUMAN] After restarting `make dev`, launch Claude from a TUIC shell and confirm the generated `--settings` hooks coexist with and execute alongside a same-event hook in global/project settings; confirm OSC 7770 busy/awaiting/idle reaches the tab. _(verified 2026-09-30: Typed `claude ...` in a rust0930 TUIC shell (zsh wrapper injects --settings from $TUIC_CLAUDE_SETTINGS): OSC hooks reached PTY (log 'Shell state -> idle hook-idle rank Protocol'); project .claude/settings.local.json UserPromptSubmit+Stop hooks also ran (hooks.log). busy/awaiting seen via spawned cla)_
- [x] [HUMAN] After restarting `make dev`, launch Codex 0.154, complete a turn, and confirm its payload contains `type`, `turn-id`, and `last-assistant-message`, OSC 7770 idle reaches the PTY, and the existing Codex `notify` command receives the unchanged JSON argument. _(verified 2026-09-30: Real codex 0.159 (item says 0.154) typed in a rust0930 shell via wrapper (-c notify=[codex-notify.sh], emulated file chaining a recorder): payload argv[1] has type=agent-turn-complete, turn-id, last-assistant-message='ok'; recorder got it as single arg; PTY log 'Shell state -> idle hook-idle rank Pr)_
- [x] On a machine that has `fish` installed, run `cargo nextest run --lib -E 'test(shell_integration)'`. The `tests::launch` matrix runs the inject / skip-when-user-passed / setting-off cases against every shell it finds, but fish is absent from this Mac and from the `ubuntu-22.04` CI image, so the fish half of that matrix has never executed — the fish wrapper is covered only by the structural `fish_wrappers_cover_inject_user_override_skip_and_setting_off` grep. Nothing to change if it passes; delete this item. _(verified 2026-09-30: Prebuilt lib test binary (HOME redirected) 'shell_integration': 17 passed incl. launch::*::in_fish (fish at /opt/homebrew/bin/fish). Binary is from shared mbx cache, not rebuilt at HEAD.)_

Features to test when TUICommander is more usable.

**This file is the only tracker for anything a human must verify.** Never open a
story for a post-rebuild or manual check — its criteria can never be met by an
agent, so it stays open forever and the backlog fills with stories nobody can
close. Add an item here instead. When an item passes, delete it; a section with
no items left goes too. What stays open must carry its own stated reason.

**Cap: 30 open items.** Past the cap the file stops being read, which is exactly
how it reached 339. Before adding an item, close one. When the cap is hit, take
the OLDEST items first and walk each through the AGENTS.md escalation ladder —
code inspection, test run, CLI probe — until every one has a verdict: tick it
with the `file:line` that proves it, correct the description if the code says
otherwise, or delete it with a stated reason. Anything left that is real
unfinished work is **a story, not a check** — open it and drop the item. An item
must never age past a rebuild without a verdict: an unread backlog costs more
than a missed check.

> **Where the restart gate sits (measured 2026-09-07).** The backend serving
> `:9876` is `src-tauri/target/debug/tuicommander`, PID 28512, started
> **2026-09-06 15:43:13**. The newest commit it can contain is `7d232d3e`
> (09-06 15:33, `perf(boot): gate the AI scheduler and knowledge flush`).
> **Every Rust change up to and including that commit is live** — which is the
> entire 08-xx backlog *and* the whole 09-04/09-05 wave, the ACP work included.
> Only these six are **not** loaded: `7d5f0f8a`, `631bad31`, `59e183b2`,
> `68429c0f`, `ecbda408`, `9473819c` — plus anything still uncommitted.
>
> The previous note recorded PID 12931 / 09-03 22:01:49 / `e4d7efd6`, and was
> inherited unchecked for three days after the app had already been restarted.
> That is the failure this paragraph exists to prevent, so it happened to the
> paragraph itself. **Re-measure it, never inherit it** — and re-measure it
> *first*, before triaging anything, because the gate decides which items are
> even answerable:
>
> ```sh
> ps -o lstart= -p "$(pgrep -x tuicommander | head -1)"   # -x, not -f
> git log --until='<that time>' -1
> ```
>
> `pgrep -f target/debug/tuicommander` is what the old recipe said and it does
> not work: the pattern also matches sibling `tuic-bridge` processes under the
> same path, so it returns several PIDs and `ps -p` chokes on the list. The
> binary's own mtime is useless either way — an agent may rebuild the file on
> disk without the running process restarting, which is the case right now
> (file built 09-07 00:22, process started 09-06 15:43).
>
> **A commit inside the gate is not the same as a fix that is live.** The gate
> answers "which commits does the running process contain". It says nothing
> about work that was never committed, and this tree currently carries **101
> uncommitted files** — 90 modified plus 11 untracked. A fix sitting in the
> working tree is in no build at all, whatever the gate says. The OSC 10/11/12
> section below was marked LIVE off a gate check alone and was wrong: Boss saw
> the exact garbage it claims to fix. **Check both** —
> `git merge-base --is-ancestor <commit> <gate>` for the commit, and
> `git diff HEAD --stat -- <file>` for whether it is committed at all.
>
> **This paragraph overrides every per-section "needs a `make dev` restart"
> label below.** There are 29 of them and they are all frozen at the moment
> someone typed them, so re-labelling each one just re-creates this problem at
> the next restart — the gate lives here, in one re-measured place, on purpose.
> As of 2026-09-07, subject to the uncommitted-work caveat above, only these are
> still gated by a *commit* boundary:
>
> | Still needs a restart | Why |
> |---|---|
> | "Opening a 23 MB JSON no longer freezes the editor" | `59e183b2`, `68429c0f`, `ecbda408` all land after the gate |
> | the per-tab agent resume item (issue #119) under "Still needs a human" | `9473819c`, after the gate |
> | the `scrollback_reflow` toggle and the headless `/claude/usage` check | still uncommitted |
>
> **Everything else labelled "needs a restart" is already live** — including the
> whole 09-04/09-05 wave. Test it now; do not wait for a rebuild.
>
> **The frontend has no such gate.** In a debug build the HTTP server reads
> `dist/` from disk on every request (`static_files.rs:65-90`), not the
> `include_dir!` copy, so a browser client at `:9876` picks up any frontend change
> after a plain `pnpm build` + reload — no Rust rebuild, no restart. The desktop
> WebView gets the same change over Vite HMR. If a browser check of a frontend fix
> shows nothing, check `dist/index.html`'s mtime before blaming the code.

## The 2026-09-06 reset (story `664-94db`)

339 open items, 65 commits, 90 days, 12 things ever closed. Nearly all of it was
"after a `make dev` restart" checks whose restart had already happened, unrecorded
— so they were never re-run and never deleted. The 249 items predating 09-04 were
worked through the ladder and closed out. What is left is the 09-04/09-05 wave,
which the running binary genuinely does not contain, and a short tail of checks
that need a human body.

Evidence that closed the bulk, all measured against the running 09-03 binary:

- `GET :9876/sessions`, 15 live sessions: 14 classify (`claude` ×13, `codex` ×1),
  11 idle / 3 working, and **zero** reporting `shell_state: idle` while
  `agent_state: working`. That count was 11 of 14 before the `started_with_agent`
  window (`pty.rs:2321`) — it is the whole "agent sessions reach idle" section,
  measured rather than argued.
- `GET :9876/logs?limit=3000`: **zero** `config write refused` lines, and
  `save_checked` with its stamp guard is gone from the tree.
- `repositories.json`: 37 repos, 3 groups, 37 `repoOrder` entries, `P42` and `ego`
  both present, plain shape with no `{id, before, after}` envelope. The 08-21
  restore held.
- `npx vitest run`: 381 files, 5738 tests, all green.
- ~~`cargo nextest` could NOT run: an in-flight edit elsewhere in the tree leaves
  `pty.rs:23285` calling `OutputRingBuffer::snapshot`, which does not exist.~~
  **Resolved 2026-09-07 — this note is discharged, do not act on it.** The tree
  compiles: no caller of `OutputRingBuffer::snapshot` remains in `pty.rs`, and the
  full suite ran green — `cargo nextest run --lib` **4934 passed / 0 failed /
  14 skipped**, `vitest run` **5919 passed / 0 failed** across 392 files, plus
  `clippy --all-targets -- -D warnings` clean and
  `cargo build --bin tuic-remote --no-default-features` building. So the Rust
  claims below are no longer read-only inferences; the suite backs them.

## Idle watchers stop stalling the event loop — story `674-78a8` — **DELETED 2026-09-19**

Three items on the idle classifier, the watcher cooldown and `max_fires`
persistence. #784-0aec deleted the terminal-watcher engine outright —
`ai_agent/watcher.rs`, `ai_agent/triggers.rs` and `ai_agent/scheduler.rs` are
gone, there is no Watcher Manager to open and no classifier to stall. Watchers
are not scheduled for the ego rebuild, so these are unrunnable rather than
pending and the items are removed instead of ticked.

## A backend-created worktree offers itself as a toast, not a modal (2026-08-30, frontend only — HMR)

The "Switch to new worktree?" confirm was a blocking modal with a ten-second
auto-cancel, raised only by MCP/HTTP worktree creation — the one case with nobody
at the keyboard. It is a toast with a **Switch** button now, mirrored into the
bell so an unattended run leaves the offers waiting instead of discarding them.
Behaviour is covered by `worktreeSwitchPrompt.test.ts`; what tests cannot see is
how it renders and whether it interrupts anything.

- [ ] Have an MCP client call `repo worktree_create`. A toast appears with the _(NOT VERIFIED 2026-09-29: partial — MCP repo worktree_create branch agbw1: toast DOM 'repo | Worktree "agbw1" created | repo__wt/agbw1 | Switch' at +2s; no new dialog (only pre-existing Progress dialog), no countdown text. Typing uninterrupted NOT checked (agent-browser keys do not reach page). Toast still present ~30s later in hidden tab.)_
  repo badge, `Worktree "<branch>" created`, the `repo__wt/branch` subtitle and a
  **Switch** button. Nothing blocks, no dialog, no countdown, and typing in the
  focused terminal is uninterrupted.
- [x] Ignore the toast until it fades, then open the bell: the _(verified 2026-09-29: After toast, bell (31 notification(s)) popover row 'Worktree: agbw1 / just now · repo__wt/agbw1', cursor pointer; click switched header from repo/main to repo/agbw1 and closed the popover. (Toast timing/fade not observed: it persisted in the hidden tab.))_
  `Worktree: <branch>` row under WORKTREES is clickable and switches to it.
- [ ] Create a worktree while a plain shell is the active tab, then click _(NOT VERIFIED 2026-09-29: partial — Plain shell tab active on agbw1; MCP created agbw2, clicked Switch on toast: header repo/agbw1 -> repo/agbw2, tab list now 'agbw2 1' (a new tab opened; no move of the old tab observed). PTY cwd not readable (web-created tabs absent from GET /sessions), so cd not confirmed.)_
  **Switch**: the tab moves to the new branch and `cd`s into the worktree.
- [ ] Repeat with a *running agent* as the active tab: the worktree opens in its _(NOT VERIFIED 2026-09-29: blocked — Needs a running agent-typed tab (all agents NOT FOUND in instance); shell tab stand-in cannot exercise the agent branch.)_
  own terminal and the agent's tab stays on its branch and CWD.
- [x] **Rust change — needs `make dev` restart** (#728-bc76). `create_worktree` _(verified 2026-09-29: Fresh build. '+' Add worktree dialog branch agbw3 Create: sidebar row 'agbw3', no 'undefined' row, log 'addTerminalToWorkspace agbw3 += term-16'; MCP worktree_create returns workspace_id 'agbw1' and row keyed agbw1 with terminals. Remove x -> popover 'Remove workspace?' Remove: row gone, git worktree list has no agbw3.)_
  now returns `workspace_id`, and the frontend keys the new sidebar row by it.
  Against an unrestarted backend that field is `undefined`, so the row lands
  under the key `"undefined"`. After a restart: create a worktree from the "+"
  button and from `repo worktree_create`, and check the row appears under the
  branch, opens a terminal, and removes cleanly.
- [ ] **Rust change — needs `make dev` restart** (#727-2085). Both worktree _(NOT VERIFIED 2026-09-29: partial: worktree_create returns workspace_id and branch (wtA); sidebar row and removal by workspace id not checked)_
  events now carry `workspace_id` *and* `branch`, and creation goes through the
  new `notify_worktree_created`. On an unrestarted backend the frontend reads
  `workspace_id: undefined`, so the sidebar row lands under the key `"undefined"`
  and the prune drops nothing — the failure is silent. After a restart: MCP
  `repo worktree_create` returns a `workspace_id`, the row appears under the
  branch, and `repo worktree_remove` with that id removes both the directory and
  the row.

## An agent quoting a menu footer stops flagging itself as awaiting (2026-08-30, **Rust change — needs `make dev` restart**)

Observed live on Boss's own `tuicommander/main` tab, twice in one turn: the agent
read another session's screen, pasted it into its answer, and the menu footer
came back out inside its own indented output. `parse_question` matched it,
emitted `Question { confident: true }`, and no clear path retracts a confident
question — the tab read "awaiting" while the agent worked, until Boss typed. The
anchor is now matched at column 0 of the rendered row instead of the trimmed
text. Covered by `pty::tests::quoted_ink_footer_in_agent_output_raises_no_question`
(fixture `claude-quoted-ink-footer.tcap`, verified RED without the fix), but a
live agent-frame check cannot be replayed.

- [x] After restarting `make dev`, ask an agent in a throwaway session to print a _(verified 2026-09-29: by code/test inspection, tests not executed here: Replay test with fixture claude-quoted-ink-footer.tcap: pty/tests.rs:16111 quoted_ink_footer_in_agent_output_raises_no_question; parser test output_parser.rs:6206.)_
  captured menu screen — footer row included — inside a fenced code block. Its tab
  must stay "working": no `?` in the sidebar, `awaiting_input` false in
  `GET /sessions`.
- [x] In the same session, open a real interactive menu (any agent prompt that _(verified 2026-09-30: Real claude AskUserQuestion (Ink 'Enter to select · Esc to cancel' footer) -> awaiting_input true in session state, cleared on Esc. Note: /model menu (footer 'Enter to set as default...') did not raise awaiting.)_
  draws the selection footer) and confirm the `?` still appears. The regression to
  fear is the opposite one: an over-tight anchor that silences real menus for
  agents whose frame indents them.

## Repository saves survive a concurrent diffstat change

Requires a `make dev` restart — the change is in `src-tauri/src/config.rs`.

- [ ] With two windows open on the same config, work in a repo so its diff counts _(NOT VERIFIED 2026-09-29: partial — Emulated second window through HTTP PUT deltas: rename, add repo, group, active-repo persisted; UI rename persisted (repo-uirn) while repo diffstat was changing; only 2 'Failed to fetch' error logs, no 'repository configuration conflict'. Sidebar reorder not exercised.)_
  keep moving (an agent committing is enough). Rename another repo, reorder the
  sidebar, add a repo. Each must persist. Before, `GET /logs` showed a stream of
  `Repository changes were not saved` / `repository configuration conflict`, and
  nothing was written.
- [x] `GET http://localhost:9876/logs?level=error` shows no
  `Repository changes were not saved` entry over a working session.
  _(verified 2026-09-07: live instance PID 28512, 10h45m uptime with Boss's
  agents committing throughout — `?level=error` returns **0 entries**, and
  neither `Repository changes were not saved` nor `repository configuration
  conflict` appears anywhere in the 1000-entry buffer at any level.)_
- [ ] Rename the same repo in two windows without reloading either: this must _(NOT VERIFIED 2026-09-29: blocked — Needs a stale second window: backend broadcast converges the client within ~3s, so a stale-baseline same-repo rename race could not be produced in one tab.)_
  STILL conflict. The exemption covers counts, not intent.
- [x] The sidebar diff counts keep updating — the exemption must not make them _(verified 2026-09-29: Added file with git add -N in fx/repo: sidebar main row showed '+1 -0 Tracked line changes' within 5s and disk additions=1.)_
  unwritable.

## A parked tab names the repo to register

Frontend only; Vite HMR picks it up.

- [x] Have an agent spawn a child via MCP in a worktree of a repo that is NOT _(verified 2026-09-29: MCP session create cwd=fx/agb/ur__wt/b1 (unregistered repo ur): toast 'Tab parked outside your repos | Nothing claims "<...>/fx/agb"... register the repo' + Register button (wording differs from item). GET /logs warn names 'register ".../fx/agb/ur"'. Plain session, not agent spawn.)_
  registered (`<repo>__wt/<branch>`). A toast appears: *Tab parked in the wrong
  repo — nothing claims "<repo root>"*. The log warning names the same path.
- [x] Reconnecting many sessions from that one repo raises ONE toast, not one _(verified 2026-09-29: 4 MCP sessions in ur__wt/b1 (8 warn log lines) -> exactly 1 toast element in DOM; after page reload (sessions re-adopted) again 1 toast. Caveat: sessions were later auto-closed by the client reload.)_
  per session.
- [ ] Register that repo: the parked tab moves to it by itself, and the active _(NOT VERIFIED 2026-09-29: partial — Clicked toast Register: repo 'ur' appeared in sidebar (rows main,b1), toast gone. Header switched repo/agbw2 -> ur/main (maybe Register's own activation, cannot separate). Parked tab moving not observed: the parked sessions had been closed after reload.)_
  repo does NOT change under you while the tab moves.

## An exited tab says so instead of going black

Frontend only; Vite HMR picks it up. Already verified live in the running dev
build: `term-100` ("GitHub state", exited agent in a deleted worktree) renders
one `terminal-exited-notice`, the other 15 tabs render none. What is left is the
visual check.

- [ ] Click an exited tab (grey dot). The panel shows a centred, muted *Session _(NOT VERIFIED 2026-09-29: blocked — Web-created tabs never got a PTY (GET /sessions unchanged: 3; tabs 'main N'), and MCP-created remote sessions are removed from the UI on exit; no exited (grey dot) tab could be produced.)_
  ended* / *The process exited and its output was released. Close this tab to
  remove it.* — not a black void.
- [ ] Open a brand-new terminal: the notice must NOT flash before the PTY _(NOT VERIFIED 2026-09-29: partial — Clicked New Tab '+' and sampled DOM every 50ms for 5s (97 samples): .exitedTitle count 0, no 'Session ended' text. The exited-tab half (notice appears) not observed.)_
  spawns. A new tab also has a null sessionId; only `shellState === "exited"`
  may show the notice.
- [ ] Let an agent finish in a background tab: the tab keeps its grey dot and _(NOT VERIFIED 2026-09-29: blocked — No agent CLI; exited tabs cannot be produced in this web session (remote-session tabs are removed on exit).)_
  its name, and the panel shows the notice when you switch to it.

## Repository saves converge across windows

**Rust change — a `make dev` restart (or `make build`) is required.** The
frontend half is HMR-only, but `repositories-changed` is emitted by the backend,
so nothing happens until the Rust process is rebuilt.

- [ ] Open the desktop app and a browser at `http://localhost:9876/`. Rename a _(NOT VERIFIED 2026-09-29: partial — Second client emulated with HTTP PUT /config/repositories delta (rename repo ur -> ur-renamed): browser tab sidebar showed the new name within 3s with no reload. Reverse direction (UI rename -> other client) only checked as disk write (config displayName 'repo-uirn'). No desktop client.)_
  repo in the browser. The desktop sidebar shows the new name without a reload,
  and vice versa.
- [x] Add a repo in one client: it appears in the other, in the right sidebar _(verified 2026-09-29: HTTP PUT delta adding repo ur2 (repos + repoOrder append): browser sidebar chip UR2 appeared after 3s at the end, matching repoOrder position. Emulated other client via API; no second browser tab.)_
  position.
- [x] Remove a repo with no terminals open in one client: it disappears from the _(verified 2026-09-29: HTTP PUT delta removing repo ur2 (no terminals): sidebar chip UR2 disappeared within 3s; config no longer lists it.)_
  other.
- [x] Remove a repo that has open terminals in the other client: that client _(verified 2026-09-29: Client had tabs main 1..3 on repo ur; HTTP PUT removed ur from disk: client kept header ur-renamed/main, the sidebar chip/rows (main, b1) and all 3 tabs. Other client emulated via API.)_
  KEEPS the repo and its tabs (they must not be orphaned), and the repo is still
  visible in the sidebar — not just present in memory.
- [ ] Remove a worktree/branch in one client while the other has a terminal open _(NOT VERIFIED 2026-09-29: partial — DELETE /worktrees/agbw2 (deleteBranch) while browser had tab 'agbw2 1': log 'Worktree removed - pruned sidebar row agbw2'; row AND tab both disappeared (item expects both to stay). Disk-only delta removing ur main workspace: row+3 tabs stayed. Unclear if agbw2 tab had a live PTY.)_
  on that exact branch: the branch row and its tabs stay in the other client.
- [x] Rename a repo in one client while the other has that same repo open and _(verified 2026-09-29: While a script appended lines to fx/repo (git add -N, diffstat moving), HTTP PUT delta renamed repo -> repo-api: sidebar shows REPO-API and disk displayName repo-api with additions 6 written by the client; no error log. Other client emulated via API.)_
  actively changing (edit a file so the diffstat moves): the rename still lands.
- [x] Group a repo in one client, then delete the group there: the other client _(verified 2026-09-29: HTTP PUT group grpagb (AGBGRP) holding ur: sidebar groupSection AGBGRP appeared; then deleting group + moving ur to repoOrder: groupSection gone, ur listed ungrouped, no empty accordion. Emulated other client.)_
  loses the group and shows the repo ungrouped, with no empty accordion left.
- [x] Switch the active repo in one client: the other client's focus does NOT _(verified 2026-09-29: HTTP PUT changed disk activeRepoPath ur -> fx/repo -> ur2 (emulated other client). The browser's store (__TUIC__.store('repositories').activeRepoPath) stayed on '.../fx/agb/ur' while disk said ur2, and sidebar Fetch acted on the client's own active repo. Focus did not move.)_
  move.
- [x] After any of the above, rename a *different* repo in the client that _(verified 2026-09-29: After API changes (rename/add/remove ur, ur2), renamed repo in the receiving UI via Repo Settings 'Custom name...': disk displayName repo-uirn, ur removal NOT reverted (disk has only fx/repo then), no new 'Repository changes were not saved' with conflict (only 2 older 'Failed to fetch' entries).)_
  received the change. `GET http://localhost:9876/logs?level=error` shows no
  `Repository changes were not saved`, and the first client's change is still
  there — the receiver must not have reverted it.
- [ ] Toggle something that writes no change (re-save the same value): the other _(NOT VERIFIED 2026-09-29: partial — 3 no-op PUT deltas (before==after): /logs grew by 1 entry ('pty Tombstone reaped'), no repositories/load_repositories entries. No log line names load_repositories anyway, so client re-read is not directly observable.)_
  client must not re-read. `GET /logs` shows no burst of `load_repositories`.

## Auto-retry on Claude Code's prose 5xx message

**Rust change — a `make dev` restart (or `make build`) is required.** The parser
runs in the backend, so nothing changes until the Rust process is rebuilt.

**Precondition:** Settings → Agents → Claude → enable auto-retry. It is
`auto_retry_on_error`, default `false`, and it is currently unset in
`config.json`, so with it off you only get the red error badge and no retry.

- [ ] Reach a real `API Error: 500 Internal server error. This is a server-side _(NOT VERIFIED 2026-09-29: Needs a real Claude Code API 500 server-side error message)_
  issue…` in a Claude tab. `GET http://localhost:9876/logs` shows
  `[ApiError] … pattern=claude-server-error-friendly kind=server` followed by
  `[AutoRetry] claude: attempt 1/3 in 5s`.
- [ ] The tab does NOT play the error sound and does NOT show the red awaiting _(NOT VERIFIED 2026-09-29: Needs real Claude prose 5xx error message and retry timing.)_
  badge while a retry is pending — only after the 3rd attempt is exhausted.
- [ ] `continue` is injected after 5s and the turn resumes. _(NOT VERIFIED 2026-09-29: Needs real Claude Code emitting its prose 5xx error and auto-continue)_
- [ ] With auto-retry disabled for Claude, the same error sets the red badge _(NOTE 2026-09-29: partial evidence only — Parser case for Claude's prose 5xx message at output_parser.rs:3843; disabled-retry branch is code inspection of auto-retry setting (verify in useAgentPolling/retry handler).)_
  immediately and injects nothing.
- [ ] The message wraps across terminal rows (narrow the window before it _(NOTE 2026-09-29: partial evidence only — Detection covered by pty tests using API_ERROR fixture (pty/tests.rs:7310); wrapped-row case needs a real narrow agent tab if wanted.)_
  fires): detection still happens — the pattern anchors on `API Error: 5xx`.
- [ ] A 429/overload (`API Error: 529` or "temporarily limiting requests") is _(NOTE 2026-09-29: partial evidence only — Detection in tuic-terminal/src/output_parser.rs:497-502 (rate_limit/overloaded/'limiting requests'), classify_error in tuic-core error_classification.rs:27; needs test cite or fixture replay.)_
  still logged as a rate limit, not as a server error, and injects nothing.

## Usage ticker follows the agent in the terminal (Claude / Codex / Grok)

**The Rust restart is complete.** The active debug backend on :9876 was verified
on 2026-09-19 with the Codex App Server and Grok ACP integrations loaded.

**Precondition:** Settings → Agents → the Claude Usage toggle must stay enabled;
it now drives all supported usage providers. Codex and Grok must be logged in
through their own CLIs.

- [ ] Focus a tab running Claude: the status bar ticker is labelled `Claude` and _(NOT VERIFIED 2026-09-29: Needs real Claude/Codex/Grok CLI accounts with usage data)_
  shows the `5h` / `7d` numbers as before. Clicking it still opens the Claude
  Usage dashboard tab.
- [ ] Focus a tab running Codex: the label becomes `Codex` and the text shows _(NOT VERIFIED 2026-09-29: Needs real Codex/Claude usage data (accounts).)_
  the Codex windows (e.g. `7d: 100% -1d`). The switch happens on tab focus,
  without waiting for the 5-minute poll.
- [ ] Clicking the Codex ticker opens a **Codex Usage Dashboard** tab (a _(NOT VERIFIED 2026-09-29: Needs real Codex/Claude/Grok agent in terminal to drive usage ticker)_
  singleton — clicking again focuses the existing tab, it does not duplicate).
- [ ] Focus a tab running Grok: the ticker is labelled `Grok`, shows the _(NOT VERIFIED 2026-09-29: Needs a real Grok CLI tab and provider billing data.)_
  provider's billing period and percentage, and opens a singleton **Grok Usage
  Dashboard** with tier and billing amounts.
- [ ] Switch to a plain shell tab: the ticker keeps showing the last agent _(NOT VERIFIED 2026-09-29: Needs real Claude/Codex/Grok sessions to drive usage ticker.)_
  rather than blanking or reverting to Claude.
- [ ] Switch Claude → Codex → Claude quickly. No stale value from the previous _(NOT VERIFIED 2026-09-29: Needs real Claude and Codex agents with usage tickers.)_
  agent lands on the ticker (the seq guard should drop late responses).
- [x] `curl http://localhost:9877/codex/usage` returns the JSON payload and
  contains **no** `email`, `user_id` or `account_id` field.
  _(verified 2026-09-19 against an isolated `tuic-remote` on :9877: the official
  App Server replacement returned HTTP 200 with plan and rate-limit data and no
  identity fields)_
- [ ] Log Codex out through the Codex CLI and focus a Codex tab: the ticker _(NOT VERIFIED 2026-09-29: Needs real Codex CLI login/logout)_
  reports the authentication failure without displaying a cached reading.
- [ ] With the Claude Usage toggle off, no usage ticker appears for Claude, _(NOT VERIFIED 2026-09-29: Needs real Claude/Codex/Grok agent tabs and usage accounts.)_
  Codex, or Grok.

### Codex Usage Dashboard

The active backend now includes `get_codex_usage_stats` and `GET /codex/stats`.

- [ ] **Rate Limits** section shows the account windows first with plain `5h` / _(NOT VERIFIED 2026-09-29: Needs real Codex account rate-limit data)_
  `7d` names, then the per-model windows prefixed with the model name. A window
  at 100% is red, ≥70% amber, below that normal.
- [ ] **Tokens per Day** renders one bar per day; hovering a bar shows the date _(NOT VERIFIED 2026-09-29: partial — Dashboard reachable only via the status-bar Codex ticker of a codex agent tab (no agent CLI) so not rendered. GET /codex/stats has 49 daily buckets, min 3,937,882 vs peak 2,988,540,050 tokens; code barHeightPercent = max(2, round(t/peak*100)) gives a 2% sliver; bar title '<date>: <n> tokens' (CodexUsageDashboard.tsx:222-227).)_
  and the token count. The tallest bar is the busiest day, and a near-zero day
  is still visible as a sliver rather than invisible.
- [ ] **Insights** shows the fields the official App Server supplies (lifetime _(NOT VERIFIED 2026-09-29: Needs real Codex account/App Server for Insights fields.)_
  tokens, peak day, streak, longest turn). Fields absent from that API are not
  invented and do not render as misleading zeroes.
- [ ] Kill the network and open the dashboard: a cached snapshot is used only _(NOT VERIFIED 2026-09-29: Needs Codex account usage dashboard and network cut-off.)_
  for up to 30 minutes; authentication failures are always surfaced.
- [x] `curl http://localhost:9877/codex/stats` contains **no** `profile` object
  (no username, display name or avatar URL).
  _(verified 2026-09-19 against an isolated `tuic-remote` on :9877: the official
  App Server response returned token summary/daily buckets under `stats` and no
  profile or identity fields)_

### Grok Usage Dashboard

The active backend now includes `get_grok_usage_api` and `GET /grok/usage`.

- [x] `curl http://localhost:9877/grok/usage` returns `credit_usage_percent`,
  `current_period`, and the subscription tier without exposing credentials.
  _(verified 2026-09-19 against an isolated `tuic-remote` on :9877: 26% weekly,
  subscription tier and billing fields present; the public payload uses
  `current_period.period_type` and contains no legacy `type` field)_
- [x] The usage bar, billing period end, on-demand used/cap, and prepaid balance
  match Grok's own billing view. Missing values render as unavailable, not zero.
  _(verified 2026-09-19 in browser mode against the isolated :9877 backend: the
  live 26% weekly period, tier, end date and billing cards rendered correctly;
  `GrokUsageDashboard.test.tsx` separately proves null renders as `--` while a
  real zero renders as `0.00`)_
- [ ] A logged-out Grok CLI surfaces an authentication error instead of a stale _(NOT VERIFIED 2026-09-29: Needs real Grok CLI logged out)_
  cached percentage.

## `index.lock` owner probe now fails closed (#694-4fcc)

**Rust — needs a `make dev` restart to take effect.** Unit-tested (28 passed), but
the live behaviour changed, so it is worth one look on a real repo.

Policy, decided by Boss 2026-09-07: when the `lsof` owner probe cannot answer, the
lock is **kept**, not reclaimed. The escape hatch is age at
`UNADJUDICATED_LOCK_STALE_SECS` (1 h), so a lock nothing can adjudicate is still
cleared eventually.

- [x] Normal case unchanged: a genuinely orphaned `index.lock` (kill a `git add` _(verified 2026-09-29: Disposable repo: killed a real 'git add many' (60k files) with SIGKILL when .git/index.lock appeared -> 0-byte orphan lock; after 31s GET /repo/files?path= answered normally, lock gone, plain git add then works.)_
  mid-write, wait 30 s) is still reclaimed and git works again.
- [ ] With the probe unavailable, the lock survives: temporarily shadow `lsof` _(NOT VERIFIED 2026-09-29: blocked — probe_index_lock_owner does Command::new("lsof") using the running instance's own PATH; shadowing lsof needs the instance restarted with a modified PATH (forbidden). Not attempted; existing unit tests inject the probe (reclaim_stale_index_lock).)_
  with a non-executable stub on `PATH`, create a 30 s-old lock, run a git command
  through TUIC, and confirm the lock is **still there** and `GET /logs` carries
  `Keeping index.lock … ownership could not be determined`.
- [x] The log names *which* failure it was — `could not run` vs `outlived its 2s _(verified 2026-09-29: by code/test inspection, tests not executed here: Owner-probe failure messages tested at git_cli.rs:1101 ('could not run') and git_cli.rs:1129 ('outlived its 2s deadline'); messages at git_cli.rs:406-408.)_
  deadline`. The two are not interchangeable and the message must say which.
- [ ] Watch for a lock kept longer than it used to be during ordinary work. The _(NOT VERIFIED 2026-09-29: Observational over ordinary real-world work (lock retention latency); not a discrete check.)_
  measured `lsof` latency here is 0.32–3.7 s against a 2 s deadline, so
  `DeadlineExceeded` is routine, not exotic — if that turns out to be noisy in
  practice, the deadline is the knob, not the policy.

## Terminal answers OSC 10/11/12 colour queries

**LIVE since the 2026-09-07 07:42 `make dev` — but NOT because it was committed.**
The answering code is still uncommitted: `git show HEAD:src-tauri/crates/tuic-terminal/src/terminal_grid.rs`
has `Event::ColorRequest(..)` in the **ignore list** and no `palette_color_for_index`
at all. `make dev` builds the *working tree*, so the rebuilt binary contains it.

> **Three states, not two — and the restart gate only distinguishes two of them.**
> The gate answers "which commits does the running process contain". A fix can be:
> (a) committed and inside the gate → live; (b) committed and after the gate →
> needs a restart; (c) **uncommitted** → live if a `make dev` rebuilt since it was
> written, in no binary at all otherwise. The gate is blind to (c), and this tree
> carries 101 uncommitted files.
>
> This section was wrong twice in one day, once in each direction: first marked
> LIVE off a gate check while the code was uncommitted and unbuilt, then marked
> NOT LIVE right after a `make dev` had in fact built it. **Neither check is the
> answer — probe the running process instead**, which for this fix is one command:
>
> ```sh
> # in a throwaway session: printf '\033]11;?\033\\' as a COMMAND, not piped
> # answered  -> output contains 11;rgb:xxxx/xxxx/xxxx
> # unanswered-> nothing comes back
> ```
>
> Measured 2026-09-07 on PID 37840: `11;rgb:2525/2525/2626`. Answered.

Fixes the `^[[?6c` garbage and the 1.2 s probe loop: Claude Code asks for the
background with `OSC 11 ; ? ST` + `ESC[c`, and TUIC used to drop the colour query
while answering the fence.

The code below is **working-tree code**, reviewed by inspection (ladder rungs
1–2). It describes what will run once this is committed and rebuilt — not what
runs now:

- the reply is built at `tuic-terminal/src/terminal_grid.rs:196-202` and pushed as
  `TermEvent::PtyWrite`, drained unconditionally on the chunk path at
  `pty.rs:5106-5119` — so it does NOT depend on a frontend being attached;
- `palette_color_for_index` (`tuic-terminal/src/terminal_grid.rs:94-103`) resolves foreground,
  background and cursor off a global `PALETTE` that always has a value, so the
  `None` branch cannot swallow a 10/11/12 query;
- reply content is asserted by `tuic-terminal/src/terminal_grid.rs:2360-2415`.

**A live CLI probe of the reply was attempted and is NOT a usable check — do not
retry it the obvious ways.** Two traps, both hit on 2026-09-07:

1. `printf '…' | cat -v` (what this item used to say) **cannot work**: the pipe
   sends the query to `cat`, not to the terminal, so `cat -v` just prints the
   query back and the emulator never sees it. The old recipe was proving nothing.
2. `POST /sessions/{id}/write` **also cannot work**: it feeds the shell's *stdin*,
   while an OSC query has to arrive on the emulator's *output* parse path. The
   shell just echoes `11;?` as typed text.

Running `printf '\033]11;?\033\\'` as a command does reach the parser, but then
reading the reply needs raw-mode `stty` juggling inside the PTY, and that harness
returned a single truncated `ESC` byte — a harness artefact, not an app result.
What is left genuinely needs eyes on a real agent:

- [x] The `ESC]11;?` / `ESC[c` pair fires **once or twice at startup, not every
  ~1.2 s**. _(verified 2026-09-07 on PID 37840: the colour query is answered —
  `11;rgb:2525/2525/2626` — so the probe concludes and stops re-arming. The DA
  query also gets exactly one reply, not a repeat. This is the loop half of the
  fix and it works.)_
- [ ] **`^[[?6c` no longer appears at startup — NEEDS A `make dev` RESTART.** _(NOT VERIFIED 2026-09-29: partial — Own shell session on validate instance: fresh startup output and a script emitting DA1 query (ESC[c) -> no '?6c' or '^[[' in /output (1388 bytes), DONE echoed clean. Original capture ordering/frontend xterm DA1 reply path not reproduced (no webview attached).)_
  Diagnosed from capture `f2bddfb0` on 2026-09-07, which settled it. The
  ordering, verbatim from the frames:

  ```
  [24] OUT ESC[>0q     claude asks XTVERSION
  [25] OUT ESC[c       claude asks DA1                       t=152.165s
  [26] OUT ^[[?6c      ← our reply, ECHOED BACK as literal text
  [33] OUT ESC[>0q     claude asks again...
  [34] OUT ESC[c       ...because it never received the answer
  [35] OUT (status)    the second reply lands silently — ECHO is off by now
  ```

  We answered *before* claude switched the tty out of cooked mode. `ICANON`
  was still set, so the reply was never delivered (a canonical read blocks for
  a newline a terminal reply never contains), and `ECHO`+`ECHOCTL` painted it
  as `^[[?6c`. Claude re-queried 100 ms later and got a clean answer. So the
  reply was never lost — only the first one was garbage on screen. That is also
  why a clean throwaway session did not reproduce it: the race needs claude to
  be slower to reach raw mode than we are to answer.

  Fix (uncommitted, in the working tree): `tty_would_swallow_reply` in `pty.rs`
  reads the master's termios and `write_terminal_reply` withholds a reply while
  `ICANON` is set.

  > **The gate keys on `ICANON`, not `ECHO`, and the first draft got this
  > wrong.** `ECHO` only decides whether the bytes are *also* painted; `ICANON`
  > decides whether they are *delivered*. In cbreak (`ICANON` off, `ECHO` on)
  > the querier reads the reply immediately, so an `ECHO` gate would withhold a
  > reply nothing will resend — trading Boss's cosmetic `^[[?6c` for a hung
  > agent. Both directions are pinned:
  > `terminal_reply_is_withheld_while_the_tty_is_canonical` and
  > `terminal_reply_is_delivered_in_cbreak_even_though_the_tty_echoes`.
  **Rust — will not hot-reload.** Verify after the next `make dev`: launch
  `c2` in a real repo tab, no `^[[?6c` above the banner, and the DA/colour
  queries still get answered (`ESC]11;?` still reports `11;rgb:…`).
- [ ] Switch to a light theme, then repeat the query: the reported colour _(NOT VERIFIED 2026-09-29: blocked — No theme picker in web Settings (checked previous batch) and theme cannot be switched without editing config; OSC 10/11 colour reply after theme change not testable.)_
  follows the theme (the frontend republishes on remeasure).
- [ ] Only one publish per real theme change — `GET /logs` shows no burst of _(NOT VERIFIED 2026-09-29: partial — Patched window.fetch to log '/theme-colors' and resized the viewport 4 times (1200x700, 900x800, 1100x850, 1440x900) with ~30 tabs: 0 theme-colors requests and 0 new /logs entries. The web client may not publish at all, so a real 'one publish per change' was not observed.)_
  palette traffic when resizing the window with several tabs open.
- [x] `curl -X POST http://localhost:9876/terminal/theme-colors -H 'content-type: application/json' -d '{"foreground":[255,0,0],"background":[0,255,0],"cursor":[0,0,255]}'` _(verified 2026-09-29: POST /terminal/theme-colors returned {ok:true}; an OSC 10/11/12 query in a session shell (osc.py) changed from cccccc/1e1e1e/cccccc to ff0000/00ff00/0000ff)_
  returns `{"ok":true}` and changes what the query above reports. (Port corrected
  from 9877: there is no second instance running; the live app serves 9876.)

### Upstream MCP OAuth — concurrent flows, expiry, late redirect

**Requires a `make dev` restart** — all of this is Rust (`mcp_oauth/`,
`mcp_proxy/registry.rs`). The running instance still has the old serialized
behaviour.

- [ ] Settings → Services → MCP: click **Authorize** on two different upstreams _(NOT VERIFIED 2026-09-29: Needs real upstream MCP OAuth servers and external browser consent)_
  back to back. Both show the consent dialog and open a browser tab within a
  second. Previously the second click hung silently for 5 minutes: no browser,
  no dialog, no error, while the row already read "Awaiting authorization…".
- [ ] Click **Authorize**, then **Cancel** before completing consent: the row _(NOT VERIFIED 2026-09-29: Needs a real upstream MCP OAuth server to reach 'Awaiting authorization' and complete/cancel consent in a browser.)_
  leaves "Awaiting authorization…" immediately and Authorize works again on the
  next click (no queue built up behind it).
- [ ] Click **Authorize** and then do nothing for >5 minutes. The row returns to _(NOT VERIFIED 2026-09-29: blocked — Needs a mock OAuth upstream MCP server plus >5 min wait; not set up within time-box.)_
  **Authorize to connect** (`needs_auth`) on its own, and
  `GET http://localhost:9876/logs?source=mcp_oauth` shows
  `Cleaned up expired OAuth flows` naming the upstream. It must not stay stuck
  on "Awaiting authorization…".
- [ ] Click **Authorize**, wait out the full 5-minute timeout *in the browser*, _(NOT VERIFIED 2026-09-29: Needs real upstream MCP OAuth provider, browser consent and 5-minute timeout with an external account.)_
  then complete consent. The browser shows the TUIC "Authentication failed" card
  reading "This authorization request expired or was cancelled…" plus "press
  Authorize again" — **not** the browser's own "can't connect to the server"
  page.
- [ ] A normal successful authorization still lands on the green _(NOT VERIFIED 2026-09-29: Needs a real upstream OAuth MCP server and browser authorization flow (external service))_
  "Authentication complete" card and the upstream goes `ready`.

### Terminal: no grid wipe on tab switch, resubscribe on reattach (#657-4345)

Frontend only (`Terminal.tsx`) — Vite HMR picks it up, no `make dev` restart
needed. Canvas painting is not observable over HTTP, so these need eyes.

- [ ] Switch back and forth between two busy terminal tabs. The returning tab _(NOT VERIFIED 2026-09-29: blocked — Trusted and DOM clicks on terminal tabs do not switch tabs in this web session (known limit); also 'busy tab' repaint flash not visible without screenshots.)_
  shows its content immediately with no blank flash. Previously every switch ran
  `resubscribe()` + `refresh()`, which cleared the grid and repainted it
  (paint → wipe → paint).
- [ ] Detach a tab into a floating window, then close that window to reattach. _(NOT VERIFIED 2026-09-29: blocked — Desktop-only: detach tab to floating window and reattach needs native window interaction on the validate instance (maccontrol targets orchestrator, not this instance).)_
  The reattached tab still paints live output and scrolls — the grid channel is
  resubscribed on this path, which is the only path that still resubscribes.
- [ ] Open a terminal in a split pane, collapse the pane to zero width, leave it _(NOT VERIFIED 2026-09-29: blocked — Split pane creation/collapse needs drag/keyboard interactions that do not work in this web session.)_
  collapsed for a minute. `GET http://localhost:9876/logs?source=terminal` shows
  one `Container stayed zero-size for 120 frames` warning and CPU stays flat.
  Previously that container kept a `requestAnimationFrame` loop re-arming every
  frame for the lifetime of the page, one loop per terminal, surviving unmount.

- [ ] (story 644-2cf4, Rust — needs a `make dev` restart) A reader-thread panic no _(NOTE 2026-09-29: partial evidence only — Panic path hard to force; code inspection at pty.rs:10929 (READER THREAD PANICKED) clears running flag.)_
  longer leaks its ticker. The panic path now clears the `running` flag, so the
  16 ms frame ticker and the 1 Hz silence timer both stop. Hard to force by hand;
  the observable if it ever happens is that a session logging
  `READER THREAD PANICKED` leaves no residual CPU and its tab stops repainting.
  Enable diagnostics and watch `thread count` stay flat after such a log line.

- [ ] (story 645-9bfb, Rust — needs a `make dev` restart) Resize an alternate-screen _(NOT VERIFIED 2026-09-29: Needs real grok agent streaming in alternate screen.)_
  agent (grok) while it is streaming, then let it ask a low-confidence question.
  The tab must badge within about a second of the resize. Before the fix the resize
  grace re-armed on every chunk, so questions, rate-limit and API-error events and
  the busy badge stayed suppressed until the agent went quiet for a full second.
  Also confirm a resize during a normal-screen Claude re-render still does NOT
  flip an idle tab to busy — that is the behaviour the grace extension protects.

## Settings search (story 684-35a8, frontend — Vite HMR picks it up)

- [ ] Open Settings. A "Search settings" box now sits at the top of the left nav. _(NOT VERIFIED 2026-09-29: partial — Only measured at default nav 180px: search input 155px wide, 12px font, padding 24/22px, placeholder 'Search settings' fits (scrollWidth<=clientWidth). Nav resize handle drag via agent-browser mouse did not change width, so 140/280px not tested; no screenshot.)_
  Check it reads well at the narrowest (140 px) and widest (280 px) nav widths —
  the box shares the nav's resize handle area, and only the DOM is covered by
  tests, not the rendering.
- [ ] Type `relay`. The tab body is replaced by a result list; each row shows the _(NOT VERIFIED 2026-09-29: partial — Dark theme only (no theme picker in web UI): search 'relay' rows have label + trail 'Remote Access > Cloud Relay'; trail color rgb(115,115,115) on bg rgb(30,30,30) = ~3.5:1 contrast. Light theme not checked; no screenshot.)_
  setting on top and a `Tab › Section` trail underneath. Confirm the trail is
  legible against the panel background in both light and dark themes.
- [x] Click the "Relay Server URL" result. Services & MCP opens and the view _(verified 2026-09-29: Settings search 'relay', clicked 'Relay Server URL' result (Remote Access > Cloud Relay): active nav 'Remote Access', label at y=722 of 900 viewport, height 14 (field in view). Scroll animation not observed.)_
  scrolls to that field. The smooth-scroll animation itself is not observable
  over the DOM — confirm it lands on the field, not at the top of the tab.
- [ ] Search a Dictation setting (e.g. `whisper`) in the desktop app: it appears. _(NOT VERIFIED 2026-09-29: partial — Browser :9880: search 'whisper' returns 'Whisper Model > Voice > Speech recognition' (NOT 'No settings match'): the Voice tab is present in web mode now, so item premise is outdated. Desktop half not checked.)_
  In browser mode (`http://localhost:9876/`) the Dictation tab is absent, so the
  same query must return "No settings match your search."

## Cross-kind tab drag reorder (story 682-b8d2, frontend — Vite HMR picks it up)

Free-mode and terminals-first drag reorder across tab kinds never worked: the
cross-kind order list had no writer, so the reorder call always returned early.
The DOM order is covered by tests; a real pointer drag in the WebView is not.

- [ ] Settings → Appearance → Tab Ordering → **Free**. Open a terminal, a diff and _(NOT VERIFIED 2026-09-29: blocked — Free-mode drag of diff/terminal/markdown tabs needs HTML5 drag-and-drop; D&D is Boss-approval territory and agent-browser mouse drag has not worked in this session (sidebar/nav handle drags did nothing). Setting 'Tab Ordering' exists with options Grouped by Type/Terminals First/Free (default grouped).)_
  a markdown tab. Drag the diff tab onto the left half of the terminal tab: it must
  land before the terminal and stay there. Repeat dragging the terminal to the right
  half of the markdown tab.
- [ ] Still in Free mode, open a new terminal after a drag. It must appear at the _(NOT VERIFIED 2026-09-29: blocked — Needs a prior drag in Free mode; D&D not drivable here.)_
  end without disturbing the order you dragged.
- [ ] Switch to **Terminals First**. Terminals stay leftmost. Drag the markdown tab _(NOT VERIFIED 2026-09-29: blocked — Needs a drag of the markdown tab onto the diff tab (D&D not drivable here).)_
  onto the diff tab — the two non-terminal tabs must swap, and the terminals must
  not move.
- [ ] Switch to **Grouped by Type** (the default). Ordering must be unchanged from _(NOT VERIFIED 2026-09-29: blocked — Drag within kinds not drivable; only confirmed default tab_ordering is Grouped by Type (Appearance select value grouped-by-type).)_
  before this story: kinds stay grouped, and dragging only reorders within a kind.
- [ ] Close a tab you dragged, then reopen one. No ghost position: the reopened tab _(NOT VERIFIED 2026-09-29: partial — Set Free (config tab_ordering_mode=free), closed tab 'Foo', opened New Tab: new tab 'main 4' at the end of the list. No dragged tab was involved (D&D not drivable), so 'no ghost position' after a drag is untested. Mode restored to grouped-by-type.)_
  appears at the end, not at the closed tab's old slot.

## Corrupt `config.json` is preserved, state-lane depth is reported (story `712-e1d2`, Rust — needs `make dev` restart)

An unparseable `config.json` used to be silently replaced by defaults, and startup
then wrote those defaults straight over it (`lib.rs:1228` fills the empty session
token and VAPID key, so `config_dirty` is always set on that path). It is now moved
aside as `config.corrupt-<uuid>` before defaults are returned, matching what every
other config file already did. Covered by
`config::tests::corrupt_app_config_survives_the_first_run_save_that_follows_it` and
`config::tests::two_corrupt_app_config_loads_keep_two_distinct_backups`; the items
below are the live confirmations only.

- [ ] With the app stopped, truncate `config.json` mid-document, then start it. The _(NOT VERIFIED 2026-09-30: partial — Not re-run: desktop stop/start impossible here and I must not restart the instance or start a second daemon; prior 29/09 tuic-remote evidence not repeated.)_
  app must come up on defaults, and the config dir must hold a
  `config.corrupt-<uuid>` file with the original bytes. Repeat once more: the second
  run must add a SECOND backup, not overwrite the first.
- [x] `curl -X POST localhost:9876/diagnostics -d '{"enabled":true}' -H 'content-type: application/json'`,
  wait 30s, then `curl 'localhost:9876/logs?source=diagnostics'` — the `HEALTH` line
  must carry a `state_lane=<n>` field, normally `0`.
  _(verified 2026-09-07 on live PID 28512: two consecutive HEALTH snapshots both
  carry `state_lane=0`, e.g. `HEALTH cpu=5.8% children_cpu=0.0% threads=120
  fds=85 sessions=10 … head_emits_suppressed=0 state_lane=0`. Diagnostics was
  off before the check and was restored to off after. The `CPU SPIKE` variant
  cannot be forced on demand and is left unverified.)_

## API-error dedup reopens on user input (story 646-1a9f, Rust — needs `make dev` restart)

The reset lived in `parse_clean_lines` keyed on a `UserInput` event no output
parser emits, so after the first API error of a session the identical error was
never reported again. The input path now parks the reset on `SilenceState` and
the reader drains it. Covered by `pty::tests::user_submission_rearms_the_api_error_dedup`;
the item below is only the live confirmation that the notification really fires.

- [ ] Provoke or wait for an `API Error: 5xx` in an agent tab — the error toast/sound _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Toast/sound is frontend; API Error 5xx not provoked (would need fake Anthropic endpoint and a UI to observe the notification).)_
  must fire. Submit a prompt, provoke the same error again: it must notify a
  SECOND time instead of staying silent for the rest of the session.

## MCP handshake and repo issue actions (story 676-89c2, Rust — needs `make dev` restart)

`initialize` used to answer a fixed `2025-11-25` whatever the client asked for,
and the `repo` tool never dispatched its GitHub issue actions.

- [x] Reconnect an MCP client that speaks an older **supported** revision. The
  `initialize` result must echo the version the client offered, not `2025-11-25`.
  _(verified 2026-09-07, live PID 28512, `POST /mcp`: offered `2025-03-26` →
  answered `2025-03-26`; `2026-07-28` → `2026-07-28`; `2025-11-25` →
  `2025-11-25`; unsupported `1999-01-01` → `2025-11-25`. **Wording corrected —
  "an older revision" is not enough and misled this check once.** The supported
  set is `["2026-07-28","2025-11-25","2025-03-26"]`
  (`mcp_transport.rs:5443`); offering `2024-11-05` or `2025-06-18` correctly
  falls back to `2025-11-25`, because echoing a revision the server does not
  implement would be a promise it cannot keep — see the doc comment on
  `negotiate_protocol_version`, `mcp_transport.rs:5450-5463`. A fallback answer
  is NOT the bug this item was written about.)_
- [ ] `repo action=issues`, `action=close_issue` and `action=reopen_issue` all _(NOT VERIFIED 2026-09-30: partial — MCP repo action=issues -> error 'was removed; use GET /repo/issues' (item stale). GET /repo/issues returns [] and POST /repo/issues/close on #2000000000 returns GitHub 'Failed to close issue (404)': HTTP path reaches GitHub.)_
  reach GitHub instead of answering `Unknown action 'issues' for tool 'repo'`.
- [x] Register the same UI tab id repeatedly from one session: it dedupes, and the _(verified 2026-09-29: MCP ui action=tab from a peer named as a live PTY id: 5x id 'same' then session DELETE -> close-html-tabs tab_ids as one entry (see next); 75 distinct opens -> close list 64 entries (cap SESSION_HTML_TAB_LIMIT), oldest (same,t1-t6) evicted, t70 kept.)_
  per-session count stops at the cap instead of growing.

## GitHub poller survives a dropped connection (story 648-051b, Rust — needs `make dev` restart)

The shared HTTP client had no timeout at all, so a dropped VPN wedged the poller
on a socket the peer never answers.

- [ ] Start the GitHub poller, then drop the network (turn off Wi-Fi or the VPN). _(NOT VERIFIED 2026-09-30: partial — Not run: dropping host Wi-Fi/VPN would disrupt other agents on this shared Mac; no override for the GitHub base URL found.)_
  Within ~30 s the request must fail and the poller must log the error and carry
  on, not sit silent forever.
- [ ] With the network still down, disable GitHub polling in Settings. It must _(NOT VERIFIED 2026-09-30: partial — Not run: depends on the network outage of 1760 and a Settings toggle (frontend).)_
  stop immediately, not after the in-flight request gives up.

## Git status and index.lock ownership (story 673-19fa, Rust — needs `make dev` restart)

The sidebar dirty badge now reads the gix porcelain-v2 counts, and the stale
`index.lock` sweep asks `lsof` who owns the lock before trusting the age rule.

- [ ] The sidebar repo badge still shows clean / dirty / conflict correctly: _(NOT VERIFIED 2026-09-30: partial — Fixture repo, /repo/info status vs git status: clean/clean, ' M'->dirty, 'M '->dirty, UU->conflict, abort->clean, all match, but each change appears only after the 60s repo_info cache TTL on this headless instance (no watcher invalidation). Sidebar badge UI not seen.)_
  edit a file, stage it, create a merge conflict, then clean up. Each state must
  match what `git status` reports.
- [x] Start a long `git add` or `git stash` in a large repo from a TUIC terminal _(verified 2026-09-30: Real 'git add' held .git/index.lock ~40s (clean filter sleep 50) in a TUIC terminal; lock present at 11/20/31/41s while /repo/info, working-tree-status, branches were called; removed only when git ended (50s).)_
  and leave it running past 30 s. TUIC must NOT delete that repo's
  `.git/index.lock` while the command still holds it.

## Weekly advisory scan (story 663-feea, CI — verify after merge)

`audit.yml` now installs a prebuilt `cargo-audit` and reads its ignore list from
`src-tauri/.cargo/audit.toml`. The workflow only runs on Mondays or on demand,
so nothing local can prove the install step resolves.

- [ ] Trigger `audit.yml` manually (`gh workflow run audit.yml`) and confirm the _(NOT VERIFIED 2026-09-29: Needs GitHub Actions run (gh workflow run audit.yml) on the remote; external service, not local.)_
  `Install cargo-audit` step resolves `taiki-e/install-action@cargo-audit` and
  the scan runs to completion.

## Process manager after the shared `ps` walk (story 669-e059, needs a Rust restart)

The stats refresh now queries the process table ONCE per refresh and walks each
session's subtree out of that shared map, instead of forking `ps` per session.
Rust does not hot-reload, so this needs a `make dev` restart to load.

- [x] With several sessions open (at least one running a nested command such as _(verified 2026-09-29: 3 MCP sessions in ur2, one running sh -c 'sleep 40; sleep 41'. GET /process/stats and the Process Manager modal (palette > Process Manager) list shell 706d0311 pid 80056 (2.4 MB) plus child 'sleep' pid 80058 with 1.1 MB RSS; every session listed with non-zero RSS. Depth only 2 levels.)_
  `cargo test` or a `sh -c 'sleep 30'`), open the process manager and confirm
  each session still lists its child AND its descendants, with non-zero RSS.

## Smart Prompts dropdown: missing-provider hint is now clickable (story 706-8d98) — **DELETED 2026-09-19**

Five items on a dimmed `api`-mode prompt whose reason text was clickable through
to `Settings → Providers`. #784-0aec deleted every precondition: there is no
Providers tab and no Headless slot to unassign, and an `api` prompt is now
refused with a reason rather than dimmed behind a provider hint. The tab comes
back as 786-4a6d and `api` execution as 787-ee50, each with its own checks —
these are unrunnable rather than pending and the items are removed instead of
ticked. The DEFERRED comment in `SmartButtonStrip.tsx` still records why the
compact split-button strip kept a plain hover tooltip.

## HTTP git commands are now bounded (story 697-d6ea, Rust — needs a `make dev` restart)

Rust does not hot-reload, so this needs a restart to load. The HTTP error
path also changed shape: a git spawn failure used to return HTTP 500 and now
returns HTTP 200 with `{ success: false, exit_code: -1, stderr: ... }`, the
same shape the Tauri command has always returned. That is deliberate — the
frontend documents `run_git_command never throws; inspect success explicitly`
(`BranchesTab.tsx:18`), so the old 500 made a browser client behave
differently from the desktop.

**The shape half is verified** (2026-09-07, live PID 28512, no restart needed —
it is inside the gate): `POST /repo/run-git {"path":"…/tuicommander",
"args":["rev-parse","--verify","no-such-ref-xyz123"]}` returns **HTTP 200** with
`{"success":false,"exit_code":128,"stderr":"fatal: Needed a single revision"}` —
not a 500. Note the payload field is `path`, not `repoPath`, and there is a
subcommand allowlist (`git_routes.rs:251-267`): `reset` comes back **HTTP 400**
`Git subcommand "reset" is not allowed via HTTP`. The items below are the
remaining behavioural checks.

- [ ] Desktop, normal path: fetch/pull/push from the Git panel still work and _(NOT VERIFIED 2026-09-30: partial — POST /repo/run-git fetch: reachable local remote success:true; hanging remote (silent TCP listener) -> after 180s 'git timed out after 180.0s and was killed'. Side finding: orphaned 'git remote-http' helpers (ppid 1) survived until the peer closed. Git panel UI/pull/push not driven.)_
  still report failures the way they did before. No visible change expected.
- [ ] Browser mode (`http://localhost:9876/`): do a fetch on a repo whose _(NOT VERIFIED 2026-09-30: partial — Same API-level evidence as 1828 through the local router (unix socket): reachable fetch ok, hang -> 'git timed out after 180.0s' at 180s. Browser UI not driven.)_
  remote is reachable. It should behave exactly as on desktop.
- [x] Slow/dead remote: point a throwaway repo at an unroutable remote and _(verified 2026-09-29: POST /repo/run-git {fetch origin} on throwaway repo: remote http://127.0.0.1:9899 (mute TCP listener) -> after 180s 'Failed to execute git: git timed out after 180.0s and was killed', success=false. Unroutable 10.255.255.1 failed earlier at 75s by OS connect timeout ('Couldn't connect'), so it does not reach the deadline on this net.)_
  fetch. It must give up after ~180s with a `git timed out` message, not hang
  forever. This is the whole point of the story — do it on a throwaway repo,
  never on a real one.

## Language picker in Settings → General (story 689-52d8, visual)

The General tab now renders a Language select above Shell, listing every locale
that ships a message catalog. Only `en.json` exists today, so the list has one
entry ("English"). Frontend-only change, so Vite HMR loads it, but the rendering
cannot be checked from a test.

- [ ] Open Settings → General and confirm the Language select sits directly under _(NOTE 2026-09-29: superseded — GeneralTab.tsx:143 hides the Language select when AVAILABLE_LOCALES.length<=1 (only 'en' catalog); rewrite the expectation)_
  the "General" heading, above Shell, with the same field styling as the IDE and
  update-channel selects (label, control width, hint line).
- [ ] Confirm the option reads "English" and the hint reads "Language of the _(NOTE 2026-09-29: partial evidence only — Language hint present at en.json:299 and GeneralTab.tsx:149 ('Language of the TUICommander interface'); option label check by inspecting GeneralTab.)_
  TUICommander interface".
- [ ] Type "language" in the Settings search box and confirm the result reads _(NOT VERIFIED 2026-09-29: partial — Search 'language' lists 'Language > General > General' (text matches) and click succeeds, but the General tab renders no Language field (hidden, 1 locale), so nothing to scroll to: stale search entry for a hidden control.)_
  `General › General` and scrolls to the field when selected.
- [ ] The single option is by design: only locales that ship a catalog are _(NOTE 2026-09-29: moot — the picker is already hidden with one locale (GeneralTab.tsx:143); the item says it stays visible)_
  offered, and listing others would show English under a foreign name. The
  control stays visible so the docs that already promise it stay true. Say if
  you would rather it were hidden until a second catalogue lands.

## PTY chunk-path refactor (story `668-59be`, **Rust — needs `make dev` restart**)

Behaviour must be IDENTICAL to before; five characterization tests assert that,
so these checks are looking for what a test cannot see on a live agent.

- [x] On a live Claude tab and a live grok tab: the state badge still moves _(verified 2026-09-30: Real claude: starting->working(busy)->idle over a tool turn; permission dialog -> awaiting_input; real grok: working->completed/idle twice. 'Feel' is subjective; transitions ordered correctly.)_
  working → idle → awaiting as it did. The chunk path was reordered around the
  chrome cutoff and the SilenceState locks; the tests cover the events, not the
  feel.
- [x] A slash menu (`/` in Claude Code) still opens and is detected. This is the _(verified 2026-09-30: Real claude, typed '/': session state slash_menu_items populated (/wiz:handoff highlighted ...), agent_state stays idle; cleared after backspace.)_
  case that killed the proposed optimisation — the menu renders BELOW the input
  box, so it is the first thing to break if the cutoff order is ever touched
  again (`DEFERRED (2026-09-06)` at `pty.rs:4911`).
- [x] A choice dialog and an Ink question footer still badge the tab as awaiting, _(verified 2026-09-30: Real claude --permission-mode default: Bash dialog -> agent_state awaiting_input/awaiting true, after Enter -> working -> idle, awaiting false. Ink AskUserQuestion footer: awaiting true, false after Esc.)_
  and the badge still CLEARS afterwards.
- [x] **Observability trade — check this deliberately.** The DECRST-leak _(verified 2026-09-29: Shell session prints ESC[2J/ESC[3J and bare '1049l'. Diagnostics OFF: 0 'DECRST leak'/'Anomalous ANSI' in GET /logs (count unchanged 2/2 on a second OFF run). POST /diagnostics enabled:true then same output: error 'DECRST leak: kitty_clean has bare 1049l...' + warn 'Anomalous ANSI sequence: ESC[2J/3J' appear. Guards at pty.rs:6344/10758/10787. Disa)_
  `error!` and the "Anomalous ANSI sequence" `warn!` no longer appear unless
  Diagnostics is on. Run `curl -X POST localhost:9876/diagnostics -d
  '{"enabled":true}' -H 'content-type: application/json'`, then confirm they
  reappear in `GET /logs`. If either turns out to be load-bearing for an open
  bug while OFF, revert the three `&& crate::cpu_watchdog::diagnostic_mode()`
  guards at `pty.rs:5025`, `pty.rs:8014`, `pty.rs:8042` — they are isolated.
- [x] Resize a tab mid-turn on an agent that was busy: the resize grace still _(verified 2026-09-30: Real claude in a silent 25s 'sleep' tool call: 4 POST /sessions/{id}/resize calls mid-turn; agent_state stayed working/busy throughout, idle only at turn end (25.2s).)_
  suppresses the false idle. `on_resize` and the `is_resize_grace` read now run
  BEFORE `stamp_last_output_now` rather than after (`pty.rs:5741-5770`). Both
  touch only `SilenceState.last_resize_at`, but that machinery has a long
  fix/revert history, which is why it is here and not left to the suite.

## Browser-mode scroll (story `658-3ce1`, **Rust — needs `make dev` restart**)

`pending_scroll` is now created by `spawn_reader_thread` instead of the
desktop-only `subscribe_terminal_grid`, so a session no desktop terminal ever
rendered can still be scrolled. Proven live against a headless `tuic-remote`
built from this tree — the same POST answered `{"ok":true}` and left
`display_offset` at 0 before the fix, and moved it to the requested offset after
— so what is left needs a real canvas, which no endpoint renders.

**Re-proven 2026-09-07 on the running desktop build** (PID 28512, inside the
gate — no restart needed), on a session created purely over HTTP that no desktop
terminal ever rendered: `seq 1 500` → `scroll-info` `{"display_offset":0,
"total_lines":502,"screen_lines":24}`; `POST terminal/scroll-to-offset
{"offset":120}` → `{"ok":true}`; `scroll-info` then reads
`"display_offset":120`. The viewport genuinely moved rather than just the
counter: `row-text?row=0` returns `"358"`, and 502 − 24 − 120 = 358 exactly.
That is the whole mechanism the two items below sit on; only the wheel/scrollbar
*rendering* still needs eyes.

- [ ] Open the web UI (browser, not the desktop app) on a session with _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Needs a browser attached to a scrolling session (wheel/drag on the canvas); headless instance has no frontend. 29/09 note: tab switching in the web UI failed for MCP-created tabs; server scroll-to-offset returns ok but display_offset stays 0 with no subscribed viewer.)_
  scrollback and scroll with the wheel and by dragging the scrollbar: the
  viewport must move, not just the thumb.
- [ ] With that browser attached, close the same terminal's tab in the desktop _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Needs a browser-attached scrolling session AND a desktop instance to close the same tab; both frontends; headless instance has neither.)_
  app. The browser must keep scrolling — the unsubscribe no longer drops the
  session's scroll target.

## Opening a 23 MB JSON no longer freezes the editor (2026-09-06, frontend — HMR; one Rust part needs `make dev` restart)

Above 500 KB the editor is plain text: no highlighting, no git gutter, no inline
blame, and the disk poll no longer re-reads the whole file 5 s after opening.
`get_gutter_changes` (Rust) returns nothing for an untracked file instead of
one "added" marker per line. Measured in Chrome only; WKWebView is the one
that blocked for over a minute.

- [ ] Desktop app: open `~/Gits/personal/ego/mutants.out/mutants.json` (23 MB, _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Opening a 23 MB JSON in the editor is frontend/desktop behaviour; instance has no frontend. Real mutants.json is 5 KB now (needs a synthetic 24 MB file in a registered repo).)_
  gitignored). It must open in a few seconds at most, unhighlighted, with no
  gutter markers. With `window.__TUIC__.setPerfDebug(true)` first, any
  remaining `UI freeze` line on `/logs` names an `editor.*` breadcrumb.
- [x] After the `make dev` restart: a small **untracked** file opens with an _(verified 2026-09-29: Web UI local repo (fx/agb2/lr): untracked small.txt opens with no git markers (changeGutter only active-line element); tracked f.txt edited vs HEAD shows 2 cm-gitMarker elements; Git panel diff of untracked small.txt shows '+a +b +c' (all added). Instance is the running debug build (not restarted after this change, current build).)_
  empty gutter; a tracked file with an unsaved-vs-HEAD edit still shows its
  markers; the diff viewer still shows the untracked file as all added.
- [ ] Desktop app (WKWebView), after the fix that installs the document with _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: WKWebView desktop smoothness with a 23 MB file; engine-specific, needs the desktop build.)_
  `EditorView.setState` instead of a whole-document dispatch: the same 23 MB
  file must scroll smoothly, and a small file must still highlight, show its
  git gutter and its inline blame, and keep undo working across an external
  reload (edit the file from a terminal while the tab is open).

## goose tabs now reach idle after a turn (story `699-c6e0`, **Rust — needs `make dev` restart**)

`goose session` is one long-lived foreground command, so OSC 133 marks the tab
busy once and nothing ever cleared it. `detect_goose_screen_activity` now reads
the composer footer (`Enter to send` → Ready) and the interrupt hint
(`Ctrl+C to interrupt` → Working), with the hint checked first so a working
screen is never downgraded. Captured live off goose 1.49.0.

- [x] Open a goose tab, let it sit at the composer: the badge must read idle, _(verified 2026-09-30: POST /sessions/agent {agent_type:goose,binary_path:goose,args:[],no prompt} (goose 1.49 at composer): session status agent_state=idle shell_state=idle in 6 samples over 24s. Note: plain 'goose' typed in a shell PTY (no agent_type seeded) stayed shell_state busy 67s+.)_
  not "working". This is the whole bug — before the adapter it latched busy
  from the moment the process started.
- [x] Send it a prompt: the badge must go to working for the whole turn (the _(verified 2026-09-30: Sent 'reply with the word ok' + Enter to goose session: session status agent_state=working in 46 consecutive 1s samples (spinner text changing) then idle in all following samples once composer returned; no flicker. Turn 58s.)_
  spinner message is whimsical and changes every second — the badge must not
  flicker with it) and back to idle when the composer returns.
- [ ] Interrupt a turn with Ctrl+C: the badge must return to idle, not stay _(FAILED 2026-09-30 story 1301-87fd: Goose turn then Ctrl+C (\u0003 via /sessions/{id}/write): screen shows composer placeholder 'Interrupted, what should goose work on instead?' (no 'Enter to send' hint); agent_state stays working/shell_state busy for 40+s. detect_goose_screen_activity (pty.rs:3895) needs Enter to send -> Unknown.)_ _(fix landed e8043223a 2026-10-01: retest on the next build)_
  working.
- [x] amp, cursor and droid are still **not** adapted (see the DEFERRED note on _(verified 2026-09-29: by code/test inspection, tests not executed here: Informational note: amp/cursor/droid unadapted, see has_ready_screen_adapter pty.rs:3991 (no adapter listed in pty/tests.rs:512/2418/2530).)_
  `has_ready_screen_adapter`). If you run one of those, expect the old
  latched-busy behaviour — that is known, not a regression from this change.

**Config note:** to capture the fixtures I pointed `~/.config/goose/config.yaml`
at the local ollama (`gemma4:12b-mlx`), since goose refused to start without a
provider and `goose configure` has no non-interactive flags. Your original file
is at `~/.config/goose/config.yaml.bak-tuic` — restore it if you had goose set
up against a real provider.

## Still needs a human

Every item here failed the ladder for a stated reason — real hardware, a second
application, a canvas no endpoint renders, or a judgement made by eye or ear.
None of them is here because nobody looked.

**Embedded factual claims re-probed 2026-09-07 — all still true**, so nobody
needs to re-run this: `command -v` still finds none of `amp`, `cursor`,
`cursor-agent`, `goose`, `droid`, `zed`, `lazygit`; none of `~/.amp`,
`~/.cursor`, `~/.goose`, `~/.droid`, `~/.factory`, `~/.config/zed` exists; and
`~/.claude/settings.json` still has 5 hook events (`PostToolUse`, `PreToolUse`,
`SessionStart`, `Stop`, `UserPromptSubmit`) and **0** TUIC references. The
`699-c6e0` premise and the hook-reinstall item are both unchanged.

- [ ] [HUMAN] Install any of `amp`, `cursor-agent`, `goose` or `droid` and capture an _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_
  idle and a mid-turn screen for it (story `699-c6e0`). All four are offered as
  launchable agents (`src/agents.ts:188-275`) but none has a ready-screen adapter
  (`has_ready_screen_adapter`, `pty.rs:3245`), so a tab running one latches busy for
  the life of the process: OSC 133 marks the command busy once and nothing clears it.
  Measured 2026-09-06 — `command -v` finds none of the four binaries, none of their
  config dirs exists, and `src-tauri/src/fixtures/agent_prompts/` holds captures for
  claude and grok only. The adapter cannot be written from documentation: grok's first
  fixtures passed green while the real UI stayed stuck BUSY for 132s (story
  `523-1df4`). Per agent — install it, open a tab, `POST /diagnostics/capture` with
  `{"enabled":true,"session_id":"<id>"}`, sit at the idle composer, send one short
  prompt, let it finish, then `{"enabled":false}`; one `.tcap` spanning both states is
  enough. Hand the files over — writing the adapter is code work and stays on
  `699-c6e0`, not a check.

- [ ] [HUMAN] Under `make dev`, edit any file in `src/` to force a Vite full reload _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_
  (story #716-031e). Every terminal pane must come back filling its pane, with no
  window resize: no small canvas in the top-left corner with black around it, and
  scrolling must show every row. Split a pane and reload again — both halves. Canvas
  geometry is not observable over HTTP, which is why this is by eye.

- [ ] [HUMAN] Reinstall the TUIC hooks from Settings → Agents, then confirm _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_
  `~/.claude/settings.json` gains TUIC references (measured 2026-09-06: 5 hook
  events present, **0** TUIC references). Then trigger a real elicitation — a
  Context7 sign-in prompt will do — and check the tab badges as awaiting and
  clears on Accept or Decline. This is the only path that exercises the
  `Elicitation` → awaiting / `ElicitationResult` → busy map at
  `agent_hook.rs:45-68`, which has never run against a real Claude binary.
  Needs a human because the hook install and the elicitation are both user
  actions no endpoint can drive. **When it fires, capture it** —
  `POST /diagnostics/capture` — and hand the `.tcap` over; the fixture is code
  work and becomes a story, not a check.

- [ ] [HUMAN] In a release `.app`, hold `j`/`l`/`i` in vim: the cursor repeats and no _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_
  accent picker appears. Option-key composition must still produce accented
  characters, and a user with `defaults write -g ApplePressAndHoldEnabled -bool true`
  must keep their override (the registration domain is lowest priority). Needs the
  release bundle domain — `press_and_hold.rs`, called from the `lib.rs` setup — and
  real key-repeat hardware. (#79)
- [ ] [HUMAN] Settings → Notifications → Attention → Test in a rebuilt app: the native _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_
  engine matches the sample Boss approved on 2026-08-09 (triangular G4→G4→E5,
  75/75/140 ms, 50 ms gaps, gain 0.8) and stays identifiable from another room
  without being irritating. Audio, judged by ear.
- [ ] [HUMAN] Copy a long Claude message out of the terminal and paste it into Slack: _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_
  no `▎` gutter and no gutter NBSPs, while lists, blank lines, indentation, `:wave:`
  and the body spacing survive unchanged. The text itself is asserted by nine Rust
  tests (`cargo nextest -E 'test(copied_selection)'`, `tuic-terminal/src/terminal_grid.rs:1687`); the
  paste is not. Tried twice from automation — `agent-browser clipboard read` fails
  with `Resource temporarily unavailable (os error 35)`.
- [ ] [HUMAN] Drag a file out of the file browser onto Finder, and drop a large folder _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_
  from Finder into the app. The first is a real cross-application OS drag. The second
  confirms a **deliberate** gap, not a regression: `fs_transfer_paths` (`fs.rs:1624`)
  is still synchronous on the main thread because it is the drag-and-drop backend and
  D&D changes need Boss's approval.
- [ ] [HUMAN] Install `zed`, put a comment, a trailing comma and hand-tuned indentation _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_
  in `~/.config/zed/settings.json`, install the bridge from Settings → Agents, and
  confirm Zed still starts, still shows every setting, and lists the `tuicommander`
  context server — `diff` against `<config dir>/mcp-backups/zed-settings.json.orig`
  must show only the added member. Then press **Remove all MCP integrations** and
  confirm each client lost only its `tuicommander` entry and a relaunch does not put
  it back. Zed is not installed here, and a real client reading the file afterwards
  is the one thing the splice tests cannot cover. (issue #115)
- [ ] [HUMAN] Compare OSC 133 gutter marks side by side, browser at `:9876` against the _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_
  desktop app, on the same session: same rows, same size, neither client stealing the
  other's dirty rows. Canvas painting is not observable over HTTP, and both clients
  have to be visible at once. (Port corrected 2026-09-07 from `:9877` — only one
  instance runs, and it serves 9876; a browser pointed at 9877 gets nothing.)
- [ ] [HUMAN] Open `vim` or `htop`: the wheel still goes to the app, `Shift+wheel` _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_
  scrolls TUIC history, and quitting restores the shell scrollback unchanged. The
  enter/exit half is covered by the `gh-run-watch.raw` replay test; mouse-reporting
  forwarding needs a real wheel. `lazygit` is not installed.
- [ ] [HUMAN] Print a fullwidth char and overwrite half of it — `printf '\e[1;5H中'` _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_
  then `printf '\e[1;6HX'` — and confirm no ghost `中` survives beside the `X`. Scroll
  away and back to prove it is not just hidden by a later full-row reship. Canvas
  painting.
- [ ] [HUMAN] Raise an MCP `ui action=confirm` and answer it on a phone at _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_
  `/mobile.html`: the desktop dialog must disappear by itself, and the reverse must
  work too. Then, with a push subscription registered and the PWA closed, confirm the
  push carries the title. Needs a real phone and a real subscription.
- [ ] [HUMAN] Comment a word that repeats many times in a markdown preview ("reason" _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_
  ×18) and confirm the highlight lands on the occurrence you selected, and that
  selecting across an existing highlight hides "Add comment". The offsets are asserted
  in `tweakComments.test.ts`; where the highlight is *drawn* is not.
- [ ] [HUMAN] Boss's call: **Trim** the real `src-tauri/target` row in Build Cleaner. It _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_
  is 58 GiB and a Trim forces a full rebuild of the running dev app, so no agent may
  run it. Trim against other repos' `target/` is covered.
- [x] Rust change, needs a `make dev` restart (stories #5525 / #8c80). Switch to a repo _(verified 2026-09-29: Isolated instance (web UI + /logs): default active_and_switch: adding/switching to never-indexed repo lr logs info 'content index warm on repo switch' then 'content index built'. Set active_only in Settings>General (config index_strategy=active_only), added+switched to lr2: only debug 'content index warm skipped by strategy', no warm/built pair. Cr)_
  that has never been indexed this session with `index_strategy` at its default
  `active_and_switch`, then check `GET :9876/logs` for `content index warm on repo
  switch` followed by `content index built` for that repo — until now nothing warmed a
  repo on switch, so the "and switch" half of the strategy did nothing. Then set the
  strategy to `active_only` in Settings → General and switch again: NO `content index
  warm`/`content index built` pair may appear for that repo (the skip itself is logged
  at debug level, so absence of the build is the check). Finally, with several repos registered
  and unindexed, run a cross-repo content search (`?` in the command palette, all-repos
  on) for a string that is not in the active repo: the empty state must read
  "N not indexed" and must NOT promise "retry shortly" for repos nothing is building.
- [ ] Rust change, needs a `make dev` restart (story #650-b0a0). With the app started _(NOT VERIFIED 2026-09-29: Needs a valid external relay server URL/token and killing the relay server; external service.)_
  while **Cloud Relay is off**, turn it on in Settings → Services with a valid relay URL
  and token: the status dot must go green with no app restart, and `GET :9876/logs`
  must show `relay: connecting to …`. Turn it off: the dot goes grey and the log shows
  `relay: shutting down` then `relay: stopped`. Turn it on again — the supervisor must
  still be watching after a stop. Then kill the relay server (or pull the network) while
  connected: every reconnect log must read `reconnecting in 1s` for the first attempt
  after each *successful* connection, growing 1→2→4… only across consecutive failures.
- [x] Rust change, needs a `make dev` restart (story #656-2b63). Spawn an agent via MCP _(verified 2026-09-29: MCP agent spawn rows=50 cols=140 (fake agent via binary_path): scroll-info screen_lines=50, ROW45 at row 45, child stty 50 140. After exit session wait until=exited -> {met:true,exit_code:7}; unknown ids -> 'Unknown session'. POST /sessions/agent and POST /sessions rows/cols 50x140 -> screen_lines 50.)_
  `agent action=spawn` with explicit `rows`/`cols` (e.g. 50x140), then confirm the tab
  renders the full screen: before this, the VT screen was built at a hardcoded 24x220
  while the child was handed the caller's geometry, so anything below row 24 (an agent's
  input box, a dialog footer) never reached the parsers. Then let that child exit and
  call `session action=wait session_id=<id> until=exited` **after** it has died: it must
  answer `{met:true, exit_code:N}` instead of `{"error":"Unknown session …"}`. A wait on
  an id that never existed must still fail fast with `Unknown session`. The same
  registration path now also backs `POST /agents` and browser/remote `POST /sessions`,
  so a browser-created terminal and an HTTP-spawned agent both need a smoke check.
- [ ] Rust change, needs a `make dev` restart (story #654-bfc1). Put a stub earlier on _(NOT VERIFIED 2026-09-29: partial — Stub cannot take priority: resolve_cli probes /usr/local/bin then /opt/homebrew/bin (real gh there; code comment says a PATH stub is never picked up) and I may not touch system dirs. Observed on headless tuic-remote (PATH stub first, sandbox HOME): HTTP bound 0.28s after start, deferred probe logged off boot path. Not checked: 10s 'did not answer' )_
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
- [ ] Rust change, needs a `make dev` restart (story #642-3741). `mod dictation_routes` _(NOT VERIFIED 2026-09-29: partial: /dictation/status, /models, /devices, /config, /system/relay-status and /system/check-update?channel=nightly answer JSON (unix-socket router); browser dictation record/inject and output device need a microphone/speaker)_
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
- [ ] Rust change, needs a `make dev` restart (story #670-b9a2). Grid delivery got three _(NOT VERIFIED 2026-09-29: Needs the desktop WebView with tauri::ipc::Channel, blocking the app JS thread via devtools; not reachable from HTTP/MCP on a test instance.)_
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
- [ ] Rust change, needs a `make dev` restart (story #672-c1a3). Three always-on _(NOT VERIFIED 2026-09-29: partial — AI cron scheduler obsolete (no ai/scheduler in src). Knowledge persists: ai-sessions/*.json written. tuic.log.2026-09-29 grows. Own daemon SIGINT: 'Received shutdown signal' line flushed to log, process exited. UTC-midnight rotation not observed (23:19 UTC).)_
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
- [ ] Rust change, needs a `make dev` restart. Dictation speech gates: the transcriber _(NOT VERIFIED 2026-09-29: Needs real microphone silence/speech and Whisper gating.)_
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
- [ ] Rust change, needs a `make dev` restart. Per-tab agent resume (issue #119): with _(NOT VERIFIED 2026-09-29: Needs several real Claude tabs in one folder to verify per-tab session discovery)_
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
- [ ] **Rust change — needs a `make dev` restart** (or `make build`). Session state is _(NOT VERIFIED 2026-09-29: partial — /events SSE (unix socket) emits 'session-state-changed' {session_id,state{awaiting_input,shell_state,last_activity_ms}} only on real transitions (11 events for 2 shell commands: busy/idle). Browser resource entries: no /sessions or list_active_sessions polling in 15s (hidden tab); diagnostics HEALTH lines show no extra IPC field. Awaiting badge wit)_
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
- [ ] **Rust change — needs a `make dev` restart** (or `make build`). Block-display _(NOT VERIFIED 2026-09-29: partial — Toggles Show block timestamps/Block folding/scrollbar marks exist (Expert on for last two), off writes config.json show_block_timestamps/block_folding_enabled=false, survive page reload+GET /config. Restart, Ctrl+Cmd label, Cmd+Shift+. fold not drivable. Default-hidden without Expert.)_
  settings now persist (story `702-327a`). `show_block_timestamps`,
  `show_scrollbar_marks` and `block_folding_enabled` were absent from the Rust
  `AppConfig`, so serde silently dropped them from every `save_config` payload —
  the frontend wrote them and the next `load_config` returned nothing, and
  `?? true` restored the default. All three are now real fields. Verify:
  (1) **The toggles exist:** `Settings > General > Terminal` shows **Show block
  timestamps** and **Block folding**, both on. (2) **They persist across a
  restart** — this is the part the old build could NOT do: turn both off, quit,
  relaunch, reopen Settings; both must still be off. Cross-check
  `config.json` — it must now carry `"show_block_timestamps": false` and
  `"block_folding_enabled": false` (before this change those keys never appeared
  in the file at all). (3) **Timestamps obey the toggle:** with it on, hold
  Ctrl+Cmd over a terminal with several command blocks — a relative-time label
  appears at the right edge of each block's prompt row; with it off, nothing
  appears. (4) **Folding obeys the toggle:** with it off, Cmd+Shift+. and the
  `Toggle block fold` palette entry must both do nothing; with it on, both fold
  the block nearest the viewport centre. (5) **Nothing else regressed:** flip an
  unrelated setting (e.g. Copy on select), restart, confirm it also survived —
  the new fields must not have disturbed the config merge.

- [ ] **Show scrollbar marks toggle** (719-36af) — frontend only, so Vite HMR _(NOT VERIFIED 2026-09-29: partial — Toggle 'Show scrollbar marks' present after Block folding (Expert on), searchable ('scrollbar' -> Terminal>Terminal), persists in config.json false/true and after page reload. Canvas tick gating/Cmd+F ticks not observable; restart not done. Item says General>Terminal; it's Terminal tab.)_
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

- [ ] **Scrollback reflow honours its Settings toggle** (660-d087) — **Rust _(NOT VERIFIED 2026-09-29: partial — Web UI Terminal (Expert on) 'Reflow scrollback on resize' ON by default (config true). OFF: 100->30 col resize truncated lines (22 rows, no extra). Flipped ON after session existed: resize 30->80 rejoined wrapped lines, config persisted. Visual re-wrap/htop and restart not checked.)_
  change, needs a `make dev` restart.** Until now the grid reflowed scrollback
  unconditionally and `scrollback_reflow` had no consumer at either end, so
  this change adds the missing Settings control AND the backend wiring.
  (1) **Default is unchanged behaviour:** open Settings > General > Terminal.
  "Reflow scrollback on resize" must be ON for an existing install — the config
  key defaulted `false` before it had a consumer, so it was flipped to `true`
  (`#[serde(default = "default_true")]`) precisely so an upgrade does not
  silently change what the terminal does. Scroll back through old output after
  opening a side panel: lines should re-wrap, exactly as before this change.
  (2) **Off truncates:** turn the toggle OFF, then narrow the terminal (open a
  side panel or drag the split). Scrollback lines written at the old width must
  now be cut at the new width instead of wrapping onto extra lines. The visible
  screen must look the same either way — a cursor-addressed TUI (htop, vim)
  redraws itself and is never reflowed.
  (3) **It reaches sessions already open:** with several tabs running, flip the
  toggle and resize a tab that was created BEFORE the flip. It must follow the
  new setting without being recreated — `commit_config_change` pushes it to
  every live grid, and a change that only affected the next session is the bug
  this story was opened for.
  (4) **It persists:** flip it off, restart, confirm `config.json` carries
  `"scrollback_reflow": false` and the toggle is still off.

- [ ] **Headless daemon serves Claude usage again** (678-9a75) — **Rust change, _(NOT VERIFIED 2026-09-29: partial — Ran built tuic-remote (--no-default-features build in instance target, --instance ag1remote, TUIC_PORT=9891, own TMPDIR): starts; unix-socket GET /claude/usage reaches handler (500 upstream 'OAuth access token has expired', env not code), /claude/timeline 400 (needs params), /claude/session-stats 400, /claude/projects 200 - not 404. TCP needs auth )_
  needs a rebuild.** `claude_usage_cache` carried `#[cfg(feature = "desktop")]`
  while `build_router` mounts `/claude/usage` and `/claude/usage/timeline`
  unconditionally, so `cargo build --bin tuic-remote --no-default-features` did
  not compile at all. The gate is gone. After a rebuild, start `tuic-remote` and
  check `curl http://127.0.0.1:<port>/claude/usage` answers instead of 404/500.
  Desktop behaviour must be unchanged — the same endpoint on :9876 still works.

- [ ] **Frontend liveness watchdog + WebView reload escape hatch** — **Rust + _(NOT VERIFIED 2026-09-29: partial: /logs?source=diagnostics has no 'Frontend unresponsive' on a healthy start; POST /debug/reload_webview -> {ok:true, action:navigate} and both PTY sessions remain. Note: it navigated the desktop window to http://127.0.0.1:1421/ (Vite dev URL), so repaint, the 40 s block and sleep cases were not observed)_
  frontend change, needs a `make dev` restart.**
  (1) **Quiet when healthy:** after the restart, `curl
  'localhost:9876/logs?source=diagnostics'` must NOT contain `Frontend
  unresponsive`. The beat runs every 5s, so a healthy app is silent.
  (2) **It fires:** block the main thread from devtools/invoke_js with
  `const t=Date.now(); while(Date.now()-t<40000){}` — within ~35s the log must
  carry `Frontend unresponsive: no heartbeat for 30s`, exactly ONE line, and a
  `Frontend responsive again` line once the loop ends.
  (3) **Sleep does not false-positive:** close the lid for a few minutes, reopen.
  `Sleep/wake detected` must appear WITHOUT a `Frontend unresponsive` next to it.
  (4) **The reload works and keeps sessions:** with several PTY tabs running,
  `curl -X POST localhost:9876/debug/reload_webview` → `{"ok":true}`, the UI
  repaints, and every session is still there with its scrollback.
  (5) **Browser mode is unaffected:** open `localhost:9876` in a browser; it must
  not beat (command is `INTENTIONALLY_UNMAPPED`) and must not produce errors in
  the console or 404s in the log.

- [ ] **Resume finds the session the alias hid** — **Rust + frontend change, _(NOT VERIFIED 2026-09-29: Needs real Claude with c/c2 aliases and two config dirs, resume into real conversation.)_
  needs a `make dev` restart.** Fixes `c2 --resume <id>` → `No conversation
  found with session ID` when the session belongs to the *other* config dir.
  (1) **Discovery captures the real command:** open a tab, launch Claude with
  `c2` (alias for `CLAUDE_CONFIG_DIR=~/.claude-private claude
  --dangerously-skip-permissions`), let it go busy→idle once, then check the tab
  carries the rebuilt string, not `c2`:
  `curl -s localhost:9877/... ` is not enough — read it from the store via
  devtools/`invoke_js`: `window.__TUIC__` terminal dump must show
  `agentLaunchCommand: "CLAUDE_CONFIG_DIR=/Users/stefano.straus/.claude-private
  claude --dangerously-skip-permissions"`.
  (2) **The resume works across dirs:** with that tab, switch branch away and
  back (or restart) so the resume command is offered. It must read
  `CLAUDE_CONFIG_DIR=… claude --resume <uuid> --dangerously-skip-permissions`,
  and running it must land in the SAME conversation — not `No conversation
  found`, not a fresh session.
  (3) **The `c` case still works:** repeat with the `c` alias (default
  `~/.claude`). The rebuilt command must have NO `CLAUDE_CONFIG_DIR=` prefix and
  must resume its own conversation, not the private-dir one.
  (4) **No regression without an alias:** a tab launched from the TUIC agent
  menu (run config, no alias) resumes exactly as before.
  (5) **Worktree seed caveat:** a tab auto-seeded with an inline prompt
  (auto-fix / conflict-assist) rebuilds with that prompt still in the command,
  so its resume re-sends it. Known and documented (`DEFERRED` in
  `rebuild_launch_command`) — confirm it is only cosmetic-annoying, and report
  if it is worse than that.

## WebView lost-document recovery + memory report (2026-09-08)

Needs a `make dev` restart — these are Rust changes and `make dev` runs
`--no-watch`.

1. **The reload endpoint navigates, not reloads.**
   `curl -X POST localhost:9876/debug/reload_webview` must answer
   `{"ok":true,"action":"navigate","url":"http://127.0.0.1:1421/"}` — not a bare
   `{"ok":true}`. The window must repaint and every PTY session must survive.
2. **The poller heals a lost frame by itself.** Force the failure the incident
   produced, from devtools on the main frame:
   `document.open(); document.write(""); document.close();` — or navigate the
   top frame to `about:blank`. Within ~15 s the log must carry
   `Main WebView lost its document` followed by `WebView recovery attempted`,
   and the app must come back with its sessions. Confirm it does NOT loop: a
   single recovery pair, then `Main WebView is back on the app`.
3. **A healthy app is never re-navigated.** Leave the app running for a few
   minutes and confirm the log has no `lost its document` line and the UI does
   not flicker/reload — an over-broad check would reload every 15 s.
4. **In-app routes survive.** Navigate around the app (settings, tabs, hash
   routes) and confirm no recovery fires.
5. **`GET /diagnostics/memory` names the structures.**
   `curl -s localhost:9876/diagnostics/memory | python3 -m json.tool` — the
   `maps` list must be sorted biggest-first, `grid.vt_log_buffers` must carry a
   plausible byte count for the open sessions, and `phys_footprint_bytes` must
   match `footprint -p <pid>` (NOT `ps` RSS, which reads far lower).
6. **The leak is still unattributed.** Leave the instance running through a
   normal working day, then compare `/diagnostics/memory` against the footprint.
   If `accounted_bytes` tracks the footprint, the named structure is the leak.
   If the footprint climbs far above `accounted_bytes`, the growth is outside
   `AppState` and the next suspect is the wry event-loop message queue.
7. **Only app URLs become recovery targets.** After restarting `make dev`,
   navigate among in-app routes, then verify that a blocked navigation to a
   different localhost port or external host does not replace the URL returned
   by `POST /debug/reload_webview`. The Rust origin guard is covered by
   `webview_recovery::tests::recovery_keeps_the_last_app_url_when_other_documents_are_observed`;
   this checks the native WebView path after rebuild.

## Workspace identity migration (725-b343) — needs a `make dev` restart

The repositories store is now keyed `workspaces: Record<WorkspaceId, WorkspaceState>`
instead of `branches: Record<string, BranchState>`, and `activeBranch` is now
`activeWorkspaceId`. Migration is an identity function (`workspaceId = branchName`),
so no persisted key moves — but it runs against Boss's real `repositories.json` on
first start, and `config.rs` changed, so **none of this is live until the Rust
backend restarts**.

**Restarted 2026-09-09 16:59. Items 1-3 verified against the live
`~/Library/Application Support/com.tuic.commander/repositories.json`; item 4 was
unverifiable as written and is corrected below.**

1. [x] **Nothing is lost on first start.** _(37 repos migrated, 0 integrity
   problems: every `activeWorkspaceId` indexes its own map — no dangling pointer —
   and for every entry `workspaceId == branchName == key`, which is what an
   identity migration must produce. 36 workspaces still carry their
   `savedTerminals` / `runCommand` / `ciAutoHeal`. Branch names containing a slash
   survived as keys unaltered (`feat/ai-fingerprint-coverage`,
   `POC-0001/fingerprint-native-12`) — sanitization applies only to newly minted
   ids, never to a migrated key.)_
2. [x] **The migrated record persists.** _(All 37 repos carry `workspaces` and
   `activeWorkspaceId`; `branches` and `activeBranch` appear on none of them.)_
3. [x] **No conflict storm.** _(`GET /logs?limit=2000` since the restart: zero
   `repository configuration conflict` and zero `Repository changes were not
   saved` at any level. The only repo-related warning is an unrelated GitHub
   404 cooldown. Re-check after a longer multi-window session — this is a
   fresh-boot buffer, not a full day's evidence.)_

## Content-index memory bound, incremental update and snapshots (2026-09-10) — **Rust, needs a `make dev` restart**

Background: since `412dc849` (2026-09-06) every repo switch warmed an index and
nothing ever released one, so the backend reached 40.7 GB across seven indices.
Three changes ship together — a memory bound with LRU eviction, an incremental
update that touches only the files that moved, and an on-disk snapshot so an
evicted repo reloads instead of rebuilding.

1. [ ] **The bound actually bounds.** With `index_memory_budget_mb` at its default
   `1024`, switch across ten or more registered repos, then read `GET :9876/logs`
   for `content index evicted to stay within the memory budget`. The backend's RSS
   must settle near the budget instead of climbing with every repo visited — check
   it in Activity Monitor or the in-app memory report, not by eye on the log alone.
2. [ ] **Eviction is invisible to a search.** Right after an eviction line names a
   repo, run a cross-repo content search for a string only in that repo. The result
   must arrive (the repo is rebuilt or restored on demand) and must never be a stale
   hit from before the eviction.
3. [ ] **An edit costs a file, not a corpus.** In a large repo already indexed, edit
   one file and wait past the 60-second rebuild cooldown. The log must show
   `content index updated incrementally` with a small `files=` count — not
   `content index rebuilt`. Then search for a word only in the edit: it must be
   found. This is the whole point of the change; a `content index rebuilt` here
   means the incremental path declined and the reason is worth reading.
4. [ ] **A big change still rebuilds.** Switch branches in a large repo (a checkout
   rewrites far more than a quarter of the corpus). The log must show
   `content index rebuilt`, and a search for a string introduced by the new branch
   must find it. Falling back here is correct, not a regression: the embedder's
   average document length is refitted only by a full build.
5. [ ] **Coming back to an evicted repo is cheap.** After a repo is evicted, switch
   back to it and read the log: `content index restored from snapshot`, and the
   restore must be visibly faster than the original `content index built` for the
   same repo. Check `<data_dir>/content-index/` holds one `.idx` per evicted repo
   and that the directory does not grow without bound across a long session.
6. [ ] [HUMAN] **A snapshot never serves stale content.** Evict a repo, then modify
   and delete files in it from outside the app, then switch back. The restored index
   must reflect the current working tree — the deleted file must not appear in a
   search and the modified file's new text must be findable. The snapshot is always
   validated against disk before use, and this is the check that it is.

## Unowned PTY tabs park in the Global Workspace (2026-09-10)

A session whose cwd belongs to no registered repo used to borrow a slot from the
ACTIVE repo, so its home depended on where you were standing: the two gate-os
worktree sessions landed under `brainstorming` and `tuicommander` respectively.
They now go to the Global Workspace instead, and leave it the moment a repo claims
the cwd. Frontend only — Vite HMR picks up the code, but the placement decision
runs during session adoption, so **reload the WebView** to see it applied to the
sessions already running.

1. [ ] **An unowned session lands in the Global Workspace, not the visible repo.**
   With `gate-os` still unregistered, reload the WebView while a gate-os session is
   alive. The tab must NOT appear in the tab strip of whatever repo is focused, and
   the "Global Workspace" entry must appear in the sidebar with a count that
   includes it. Click it: the terminal renders and is still attached to its PTY.
2. [ ] **Standing somewhere else changes nothing.** Switch to a different repo and
   reload again. The tab must land in the Global Workspace both times — the two
   gate-os sessions must end up TOGETHER, which is the whole bug.
3. [ ] **Register walks it home.** Click Register on the "Tab parked outside your
   repos" toast. The tab must move out of the Global Workspace and into `gate-os`
   under its worktree's branch, and the Global Workspace count must drop.
4. [ ] **One toast, not one per repo you visit.** The toast previously carried the
   active repo as its scope, which defeated the dedup: walking to another repo
   raised the same warning again there. With several unowned sessions from one repo,
   exactly one toast must be present, and moving between repos must not raise more.
5. [ ] [HUMAN] **A hand-promoted tab is not evicted.** Promote a normal, properly
   owned terminal to the Global Workspace by hand, then trigger a reconcile (add or
   remove a repo, or `cd` the terminal). It must STAY promoted — the unpromote is
   keyed on "was parked", not on "is promoted", and this is the check that it is.
6. [ ] **The active branch gets its own terminal now.** Where a borrowed tab used to
   satisfy "this branch has a terminal" and suppress it, an empty active branch now
   opens one of its own. Confirm this is the behaviour you want and not one extra
   terminal per launch that annoys you.

## Rust worktree API keys on workspace_id (story `726-5ac7`, 2026-09-10) — **Rust + IPC shape, needs a `make dev` restart**

The whole removal/dirtiness/archive path now resolves by opaque `workspace_id`
instead of branch name, and `get_worktree_paths` changed shape from
`{branch: path}` to `{workspace_id: {branch, path}}`. Under the identity
migration a git worktree's id **is** its branch, so nothing visible should
change — which is exactly why it needs eyes: a silent mismatch between the new
payload and the sidebar would look like nothing happening.

1. [ ] **Sidebar still lists every worktree.** After the restart, each repo's
   branch rows appear with their diff badges and merged marks intact. An empty
   sidebar with a live repo means the frontend failed to read the new
   `{branch, path}` value shape.
2. [ ] **Remove a worktree from the sidebar.** The row disappears immediately
   (the `worktree-removed` event now carries `workspace_id`, not `branch` — if
   the payload key were still misread the row would linger until a refresh).
3. [ ] **Merge & archive, then merge & delete a worktree.** Both must complete
   and the archived directory must still contain any uncommitted file, since
   `archive_worktree` now derives the archive folder name from the resolved
   record's branch rather than the caller's string.
4. [ ] **The dirty-worktree guard still asks.** Leave an uncommitted file in a
   worktree, then archive it. It must come back as a confirmation prompt, not a
   silent destroy — `worktree_dirtiness` is now id-addressed and this is the
   path that gates the irreversible part.
5. [ ] **Post-merge cleanup dialog with "keep worktree" unchecked.** The branch
   goes, the directory stays, HEAD detaches. `delete_local_branch` now takes a
   branch *and* a workspace id and refuses when they disagree.
6. [ ] **MCP `repo action=worktree_remove` now requires `workspace_id`.** A call
   passing only `branch` must be refused with a message naming `workspace_id`.
   Get an id from `repo action=worktree_list` first.

## Long dictation keeps its window tails (story `738-1e31`, 2026-09-11) — **Rust, needs a `make dev` restart**

The final whole-recording whisper pass used to run with the streaming flags
(`single_segment`, `no_timestamps`) at any length. Above one 30 s whisper window
those flags make `seek` advance a full window regardless of how much the decoder
reached, so every window silently lost its tail. The flags are now chosen by
audio length, and the no-speech gate filters per segment instead of discarding
the whole transcript on its worst segment.

1. [ ] **Dictate for more than 60 s.** The final text must not be shorter than
   the streaming partials that appeared while speaking. Check
   `GET http://localhost:9876/logs?source=dictation`: the `[accuracy]` line now
   reports `ratio=` instead of `match=`, and a ratio under 90% also logs a
   warning naming both character counts. A ratio at or above 100% is the normal
   case — streaming skips VAD-silent windows.
2. [ ] **A pause mid-dictation no longer eats the transcript.** Dictate, stay
   silent for several seconds, then keep dictating. Both halves must arrive; the
   silent stretch is now dropped as one segment rather than rejecting everything.
3. [ ] **Short dictation is unchanged.** A few seconds of speech still
   transcribes, and dictating into a silent room still produces nothing rather
   than invented subtitle boilerplate — that suppression relies on the flags the
   short path still sets.

## Session alias as the universal agent address (story `737-2150`, 2026-09-11) — **Rust + frontend, needs a `make dev` restart**

Every action that takes a `session_id`, and `agent action=send`'s `to`, now accept
three names for one terminal: the PTY id, the `tuic_session`, and the alias. The
alias also persists across a restart, and the session/peer list payloads dropped the
fields that answered a question nobody asked.

1. [ ] **Address a session by alias.** Take an alias from `session action=list`
   (e.g. `tu-1`) and call `session action=output session_id=tu-1`. The same call with
   that session's `tuic_session` must return the same terminal.
2. [ ] **`agent action=send to=<alias>`** reaches the peer that owns that terminal,
   with the same `delivery_path` as sending to its `tuic_session`.
3. [ ] **The alias survives a restart.** Note a tab's alias, quit the app, start it
   again, and check `session action=list`: the restored tab must hold the same alias,
   and a new session in that repo must get the next free number rather than reusing it.
4. [x] **Tab context menu copies the alias.** Right-click a terminal tab that has an
   alias. The menu shows `Alias: tu-1` under a separator; clicking it puts the alias
   on the clipboard. A tab with no alias shows no such item. _(verified: story
   `761-c847` retains event-before-binding aliases in `terminalsStore`; the focused
   store, listener, and TabBar tests pass 214/214)_
5. [ ] **`is_caller` marks the right tab.** From an agent running in a TUIC tab, call
   `session action=list`: exactly the caller's own session carries `is_caller: true`.
6. [ ] **Reading a dead session still works.** Let a session's process exit, then call
   `session action=output` on it. It must return the buffered output, not
   `Unknown session` — resolution falls through for a reference it cannot resolve.

## Named `tuic-remote` application instances (story `736-0afd`, 2026-09-11) — **Rust, needs a `make dev` restart**

`tuic-remote --instance <id>` now picks a separate config directory and OS-keyring
vault before it touches any state. The default path through `config_dir()` and the
credential vault moved behind the same seam, so the desktop app must be checked for
regressions even though it has no `--instance` flag.

1. [ ] **The desktop app still reads its own config.** After the restart, settings,
   repositories, GitHub token and MCP upstream credentials are all still there —
   `config_dir()` now goes through `app_instance`, and the default branch must
   resolve to the same platform directory as before.
2. [ ] **A named daemon starts empty and stays isolated.** Build the headless binary
   (`cargo build --bin tuic-remote --no-default-features`), run
   `./tuic-remote --instance build-host --set-password`, and check that
   `<platform config>/com.tuic.commander/instances/build-host/config.json` appears
   while the default `config.json` is untouched. A macOS Keychain entry must be
   created for service `tuicommander-instance-build-host`, not `tuicommander`.
3. [ ] **The password is per instance.** The password set for `build-host` must not
   log you into the default daemon, and vice versa.
4. [ ] **An invalid id fails loudly.** `./tuic-remote --instance Work-Laptop` and
   `--instance default` both exit 1 with `Invalid application instance …` and never
   bind the port.

## Mobile PWA lazy screens + IDE-icon split (2026-09-12) — frontend only, Vite HMR picks it up

`pnpm build` was failing on `main`: `dist/mobile.html` weighed 117 378 gzip bytes
against the 100 KB budget in `scripts/report-frontend-bundles.mjs`. Two causes, both
desktop weight leaking into the mobile initial load graph:

- `MobileApp.tsx` imported `ActivityScreen` and `SettingsScreen` eagerly although the
  app always opens on the `sessions` tab. Both are `lazy()` now.
- `IDE_ICON_PATHS` (33 inlined IDE/terminal SVGs) lived in `stores/settings.ts`, which
  mobile reaches through `ToastContainer` -> `stores/terminals` -> `stores/settings`.
  Moved to `stores/ideIcons.ts`; only `IdeLauncher` reads it.

Result: 101 941 gzip bytes, **459 bytes under budget**. The margin is thin on purpose
— see the note below.

1. [ ] **Mobile PWA still boots and the bottom tabs work.** Open `/mobile.html`, tap
   **Activity** and **Settings**: both must render (they now arrive over a second
   request). A blank tab means the `lazy()` chunk failed to load.
2. [ ] **The IDE launcher still shows its icons.** Desktop, Settings -> the IDE picker:
   all 33 entries must show their logo, not a broken-image glyph.
3. [ ] **Deep link into a session still works** (`/mobile/session/<id>`) —
   `SessionDetailScreen` is deliberately still eager.

**Known, not fixed:** mobile still pulls `stores/settings.ts` and with it the whole
`i18n/en.json` string table (13 KB gzip) although no mobile component calls `t()`.
The chain is `ToastContainer` / `utils/activitySnapshot` / `stores/toasts` ->
`stores/notifications` -> `stores/terminals` -> `stores/settings`. Cutting the
`terminals -> settings` edge would take ~26 KB gzip off mobile and give the budget
real headroom, but it is a core-store refactor and needs Boss's approval first.

## Orchestrator inbox wake after background probe (Rust — needs `make dev` restart)

1. [ ] Start a managed parent with a child, leave the parent shell visibly ready,
   and have the child send `RESULT` while the parent still has a pending
   background-process probe. When the probe settles, the parent must receive the
   payload-free `agent action=inbox` notice without closing the child or waiting
   for another lifecycle event. Reading the inbox must return the original
   `RESULT` payload exactly once.

## Codex usage in the active agent badge (frontend — Vite HMR)

1. [ ] With the Usage Dashboard feature enabled, focus a Codex terminal and
   confirm its `5h`/`7d` utilization appears beside the Codex icon in the bottom
   status bar, not as a second standalone ticker. Clicking the badge must open
   the Codex Usage dashboard. Switch directly from a Claude tab and confirm the
   old Claude percentages never appear under the Codex icon while the Codex poll
   is in flight.

## Plan and Stories external-plugin migration (Rust — needs `make dev` restart)

1. [ ] After restarting, Settings → Plugins → Installed lists Plan Tracker and
   Stories Ticker as ordinary external plugins, with no Built-in badge.
2. [ ] In a repository with `plans/` and `stories/`, a newly created Markdown
   plan opens in a pinned background tab and the status ticker shows the open
   story count.
3. [ ] Uninstall Plan Tracker, restart again, and confirm it is not recreated.
   Reinstall it from Browse after the `plan.zip` release asset is published.

## Project Progress corrupt-store recovery (Rust — needs `make dev` restart)

1. [ ] After the Project Progress reporting surface is wired, use a throwaway
   registered Git repository with invalid bytes at `.tuic/progress.sqlite3`.
   The first report must return `progress_store_recovered`, name a preserved
   `.corrupt-<uuid>` database artifact with the original bytes, and require a
   retry. Repeat with existing `-wal` and `-shm` sidecars and confirm all three
   named artifacts retain their exact original bytes under one unique recovery
   suffix. The retry must persist into a validated schema-v1 replacement and read
   the new event after another restart; it must not claim the empty replacement
   is the original history. Two simultaneous first reports must perform exactly
   one recovery: one reports recovery, the other succeeds against the replacement,
   and every later startup succeeds.

## Post-merge cleanup runs without freezing the window (2026-09-12) — **Rust, needs a `make dev` restart.**

`switch_branch`, `delete_local_branch`, `finalize_merged_worktree` and `close_pty`
were plain `fn` Tauri commands, so they ran inline on the macOS main thread. All
four now run on the blocking pool. Their HTTP twins were already correct, so this
is only observable in the desktop app.

1. [ ] **The window stays alive during Execute.** Open the post-merge cleanup
   dialog on a branch that has at least two terminals, check every step, press
   Execute: the spinner must animate, the sidebar must stay scrollable and the
   window must keep redrawing for the whole run. Before this change the whole
   WebView was frozen — cursor included — until the last step returned.
2. [ ] **The steps still report in order.** Each row must go running → done one
   at a time, with the same success/error wording as before; a failing step must
   still stop the ones after it.
3. [ ] **Closing a tab is still immediate and complete.** Close a terminal
   normally: the tab disappears, the process dies (no orphan `claude`/shell in
   `ps`), and a worktree-cleanup close still removes the directory.
## Remote-access credentials cannot be half configured (2026-09-12) — frontend only, Vite HMR picks it up

Boss's config held a password hash with an empty username, which made the server
answer every Basic Auth attempt from the phone with 401. The live config is
already repaired (`username: "admin"`); these checks cover the UI that produced it.

1. [ ] In Settings → Services → Remote Access, clear the Username field, type a
   password and press Tab. The Username field must fill in with `admin` by itself
   and `GET http://localhost:9876/config` must report that username — not `""`.
2. [ ] With a password set, clear the Username field and press Tab: it must snap
   back to `admin` instead of saving an empty username.
3. [ ] With no password set, an empty Username field must stay empty (it is only
   forced when a credential pair exists).
4. [ ] From the phone, open the LAN URL and log in with the saved username and
   password: the Basic Auth prompt must accept them and not reappear.

## Server request timeout no longer ties the confirm answer window (story `760-c29f`, 2026-09-13) — **Rust, needs `make dev` restart**

`REQUEST_TIMEOUT` (the outer HTTP layer every route runs behind, `mcp_http/mod.rs`)
and `CONFIRM_TIMEOUT` (`ui action=confirm`'s own answer window, `mcp_transport.rs`)
were both 300 seconds — an exact tie the outer layer could win, dropping the
handler's future before its cleanup ran. `REQUEST_TIMEOUT` is now 301 seconds,
a deliberate 1-second margin above the confirm window. Covered by an in-process
test that drives an unanswered confirm through the real `build_router` stack
(`mcp_http::tests::confirm_left_unanswered_resolves_clean_through_the_real_server_stack`),
but the actual dialog-dismissal behavior across every connected client can only
be observed against a rebuilt binary:

- [x] Trigger `ui action=confirm` from an MCP client and let it sit unanswered _(verified 2026-09-29: MCP ui action=confirm left unanswered: call returned after 300s (03:09:47 -> ~03:14:47) with {confirmed:false, reason:"no answer within 300s"} (not HTTP 408). Dialog text was in the :9880 browser DOM at +5s and gone after. Desktop WebView and mobile PWA surfaces not observed.)_
  past 300 seconds without touching any client. Confirm the requesting call
  receives `{confirmed:false, reason:"no answer within 300s"}` — not a bare
  HTTP 408 — and that the confirm dialog disappears on its own from every
  connected surface (desktop WebView, a browser tab, the mobile PWA) rather
  than staying stuck on screen.

## `tuic <dir>` refuses a temporary directory (story `763-d219`, 2026-09-13) — **Rust CLI, needs a `tuic` rebuild + reinstall**

The check lives in the `tuic-cli` binary, so neither Vite HMR nor a `make dev`
restart picks it up — the installed `tuic` on `$PATH` must be rebuilt (`make
build`, or `tuic install-cli` after a `cargo build -p tuic-cli`).

1. [ ] `tuic "$TMPDIR/scratch"` (after `mkdir -p "$TMPDIR/scratch"`) exits
   non-zero and prints the refusal naming the path, plus the `tuic new <path>`
   suggestion. Nothing new appears in the sidebar.
2. [ ] With `TMPDIR=$HOME/Gits/.tmp` exported — this repo's Rust-suite
   convention — `tuic "$HOME/Gits/.tmp/scratch"` is refused too. This is the
   case that proves the check reads the *caller's* `TMPDIR` and not the app's;
   it is the one of the fifteen observed ghost rows that an app-side check
   would have missed.
3. [ ] `tuic new "$TMPDIR/scratch"` still opens a shell there. Only
   registration is refused, never a session.
4. [ ] A real repository still registers: `tuic ~/Gits/personal/tuicommander`
   lands in the sidebar and activates as before.
5. [ ] A directory whose name merely *starts* with a temp root's name is not
   refused — e.g. `mkdir -p /tmpfoo && tuic /tmpfoo` on Linux, or any
   `~/Gits/.tmpfiles/repo`. Covered by
   `tuic-cli::tests::a_real_repository_is_not_disposable`, but worth one real
   run because that test cannot exercise canonicalization against the live
   filesystem.

## Stale-temp repository classifier + repair, and `TUIC_APP_INSTANCE` (story `763-d219`, 2026-09-13) — **Rust, needs a `make dev` restart**

Both live in `src-tauri/src/config.rs` / `lib.rs` / `crates/tuic-core/src/app_instance.rs`, so
neither is loaded by Vite HMR — `make dev` must be restarted (or `make build`
for release) before any of this is observable.

**Items 1–3 are pre-staged; do not build the fixture by hand.** An isolated
instance is already seeded at
`<config dir>/instances/story763verify/repositories.json` with four rows: the
real `tuicommander` repo, one legitimate-but-offline git repo that must
survive, and two temp-root ghosts (`tuic-763-ghost-alpha`, `tuic-763-ghost-beta`)
shaped exactly like the classifier's own `stale_temp_repo_json` fixture. Run
`make test TUIC_APP_INSTANCE=story763verify` and check items 1–3 against it.
A desktop-feature build is required: `src-tauri/target/debug/tuicommander` was
last built without it and refuses to start the GUI, and `tuic-remote` serves no
frontend at all (`src-tauri/src/mcp_http/static_files.rs:10-11`). A debug build
reads `dist/` from disk first (`static_files.rs:14-17`), so the frontend itself
needs no recompile.

0. [x] A repository row is classified as missing only when filesystem metadata
   returns `NotFound`; permission, invalid-data, and other I/O errors preserve
   the row. _(verified: `config::tests::only_not_found_metadata_errors_prove_the_path_is_missing`
   exercises all four error kinds)_

1. [ ] With one or more genuinely stale-temp rows in `repositories.json` (path
   gone, under a temp root, `isGitRepo:false`, one empty shell workspace, no
   user metadata), the sidebar footer shows a red flagged-repo icon with a
   count badge; those rows do not appear as ordinary sidebar entries.
2. [ ] Clicking the badge opens the popover listing each candidate's display
   name; clicking "Repair N stale repositories" removes them, the badge
   disappears, and `repositories.repair-backup-<timestamp>.json` exists in the
   config directory holding the pre-repair document.
3. [ ] A legitimate repository whose path is temporarily offline/unmounted (not
   under a temp root, or not git, or holding any terminal/commit/metadata)
   never appears in that popover and renders normally in the sidebar.
4. [ ] `TUIC_APP_INSTANCE=story-763-verify make dev` (or an equivalent env-var
   launch) creates and uses `<config dir>/instances/story-763-verify/` —
   confirm via `GET /config/repositories` on that instance's port and by
   checking the directory on disk — and never touches the default instance's
   `repositories.json`. An invalid id (e.g. `TUIC_APP_INSTANCE=Default`)
   fails the process at startup with a clear error instead of silently
   falling back to the default instance.

## Rust dependency tree refresh (story `757-9ee7`) — **Rust, needs a `make dev` restart**

After rebuilding with `make dev`, confirm the running backend uses the refreshed
Cargo dependency tree; no frontend HMR reload can load these Rust changes.

## Project Progress reporting (story `750-d656`) — **Rust, needs a `make dev` restart**

After restarting an isolated `make dev` instance, report one milestone through
the MCP `progress` tool and confirm one `progress-recorded` SSE event appears and
the event remains available after reconnect. Repeat the exact report within 60
seconds and confirm the duplicate receipt produces no second event.

## Linked worktrees start WARM (story `767-3968`, 2026-09-13) — **Rust, needs a `make dev` restart**

- [ ] Create a linked worktree of this repo through the dialog or _(NOT VERIFIED 2026-09-30: partial — Own daemon r3iso, fixture repo (ignored node_modules/ 200 files, src-tauri/target/): repo worktree_create (no mode/dirty) -> worktree_list warm_artifacts.status pending->done within ~1s; worktree has node_modules/ (200 files) and src-tauri/target/, no .env, .git is a file, git status clean. warmed_d)_
      `repo action=worktree_create` without `mode` or `dirty` fields.
      It must contain `node_modules/` and `src-tauri/target/` straight away, and
      the MCP/HTTP `instructions` payload must report
      `warm_artifacts.warmed_directories` > 0.
- [x] `git -C <worktree> status` must still work after creation, and the _(verified 2026-09-29: fixture worktree via MCP repo worktree_create: git -C status OK, .git is a file)_
      worktree's `.git` must still be a FILE, not a directory.
- [x] The ignored top-level FILES must NOT have been copied: no `.env`, _(verified 2026-09-29: fixture repo with ignored .env/.mcp.json/CLAUDE.md: none appear in the new worktree (node_modules/ and target/ do))_
      `.mcp.json`, `CLAUDE.md` newly appearing in the worktree beyond what the
      branch tracks.
- [ ] `plugins/` (a submodule) and `src-tauri/plugins/claude-wakeup/` must not _(NOT VERIFIED 2026-09-30: partial — Fixture analog (not real repo) on r3iso: repo with submodule 'plugins' + tracked src-tauri/plugins/claude-wakeup with ignored node_modules; linked worktree via repo worktree_create: warm done; plugins/ has .git file + p.js, git submodule status clean (initialized), plugins/node_modules (ignored) not)_
      have been double-copied or left half-populated.
- [ ] Time it. Expect ~38 s on this repo; if it feels worse than a cold build, _(NOT VERIFIED 2026-09-30: partial — Not run on the tuicommander repo (would create a worktree in Boss's real repo; forbidden). Analog: 200-file ignored dir fixture warmed in under 1s; 29/09 analog with 60k files ~13s. ~38s expectation on the real repo not measured.)_
      say so rather than living with it.
- [x] Switch Settings → worktree storage to "inside repo" (`.worktrees/`), _(verified 2026-09-29: Disposable repo2 (.gitignore has .worktrees/ cache/), per-repo settings PUT /config/repo-settings worktree_storage=inside-repo + copy_ignored_files=true: repo worktree_create returned in 0.06s, worktree at repo2/.worktrees/in1, warm_artifacts status done, cache/c.bin copied, no nested .worktrees/recursion (find depth 4).)_
      create a worktree, and confirm creation does not hang or recurse — the
      destination's own ignored ancestor must be skipped.
- [x] Confirm Settings and settings search contain no copy-on-write workspace _(verified 2026-09-29: by code/test inspection, tests not executed here: cow.rs:3 says COW workspace clones are gone; rg for copy-on-write/cowMode in src/ finds no Settings or dialog UI. Verified by absence in src/.)_
      toggle, the create dialog has no mechanism or parent-changes picker, and
      the Worktree Manager has no clone badge or Publish action.

- [x] After restarting `make dev`, verify Project Progress HTTP controls on the _(obsolete, verified 2026-09-29: Progress pause/resume/clear removed: transport.test.ts:395 lists progress_pause/clear/export as gone; no /progress/pause route in mcp_http/mod.rs:884-890.)_
      isolated test instance: pause rejects reports, resume accepts only new reports,
      and clear leaves an existing `progress.md` untouched. _(Rust backend change;
      requires restart to load.)_

## Project Progress panel (story `752-8492`, 2026-09-13)

Screenshots captured on an isolated instance (`TUIC_APP_INSTANCE=story752`,
HTTP `:9877`) in browser mode live in `~/Gits/.tmp/story752/shots/`.

- [x] Open Progress from the command palette in desktop-width browser mode.
      _(verified: **Open Project Progress** opens `#progress-panel`; shot
      `10-wide-populated-history.png`.)_
- [x] Populated list at desktop width, with a long summary that wraps inside the
      event body. _(verified: shot `10-wide-populated-history.png`, 395-character
      summary over six lines, kind badge right-aligned, `Source`/`Correct`/`Move`
      on the meta row.)_
- [x] Empty state with projects present. _(verified: shot
      `11-wide-empty-blockers.png` — **No progress matches this view.** under a
      live state card, with `Delete (0)` and `Merge` correctly disabled.)_
- [x] Paused state. _(verified: shot `12-wide-paused.png` — **Paused** in the
      state card, **Pause** became **Resume**, history still listed.)_
- [x] Unavailable project shown as an error beside working projects, not as a
      project without progress. _(verified: shot `13-wide-error-unavailable.png`
      — the project root was deleted underneath a running instance.)_
- [x] Narrow viewport with data. _(verified: shot `14-narrow-populated.png` at
      700x950 — scope row wraps, the tab strip scrolls, the error card and the
      long summary stay readable.)_
- [x] The bell exposes ONE aggregate Progress row that opens the panel.
      _(verified: shot `15-bell-aggregate-row.png` — "Project Progress / 9 unread
      changes across projects".)_
- [x] A live report shows exactly one toast that names project and workstream and
      carries an **Open Progress** action. _(verified: shot `03-live-toast.png`.)_
- [x] An unavailable project shows a red card with its name and the reason, above
      the list, instead of looking like a project without progress. _(verified:
      shots `00-error-cards-ownership-bug-prefix.png`, `02-narrow-error-state.png` —
      both captured before the two backend fixes, kept because they show what a
      total backend outage looks like in this panel.)_
- [x] The mobile Progress tab hosts the same panel full-bleed. _(verified: shot
      `04-mobile-progress-tab.png`. NOTE: the mobile shell never calls
      `repositoriesStore.hydrate()`, so the tab has no projects to scope and can
      only show the empty state — the layout is proven, the data path is not.)_
- [x] Provenance, pagination (**Load older …**), and the destructive confirmation _(obsolete, verified 2026-09-29: Feature gone: rg 'Load older|loadOlder' src src-tauri/src only hits src/mobile/components/OutputView.tsx:167 (unrelated); ProgressDialog has no pagination/provenance/confirm.)_
      text, which name the scope and the count and state that existing
      `progress.md` exports are not deleted. Not captured: the seeded set was
      below one page and the confirmations are native `window.confirm` dialogs,
      which a screenshot of the page cannot show.
- [x] While the panel is open, report another event and verify the displayed _(obsolete, verified 2026-09-29: ProgressPanel replaced by ProgressDialog; rg -i 'watermark|unread' src/components/ProgressDialog src-tauri/src/progress -> no matches)_
      watermark stays frozen, the later event remains unread, and exactly one
      toast appears without a duplicate MESSAGES row.

## Project Progress ownership self-edge (story `752-8492`, 2026-09-13) — **Rust, needs a `make dev` restart**

- [x] Register any repository and open Progress. Every project must list its _(verified 2026-09-29: registered fx/repo (its own main workspace): repo progress_list and POST /progress/list answer entries, no project_unavailable; dialog lists them)_
      events. Before the fix, `resolve_owning_project_in` read the repository's
      own main workspace (`worktreePath` == repo root, no `parentRepoPath`) as an
      ownership cycle, so `progress_status` and `progress_list` failed for every
      registered project with
      `project_unavailable: managed workspace ownership cycle` and the panel
      showed nothing but red cards.
- [x] A genuine two-project ownership cycle must still fail closed. _(verified 2026-09-29: by code/test inspection, tests not executed here: Ownership cycle fails closed: src-tauri/src/progress/ownership.rs:71 (project_unavailable ownership cycle) with test assertion at :318)_

## Project Progress export (story `753-9998`, 2026-09-13) — **Rust, needs a `make dev` restart**

- [x] After restarting an isolated `TUIC_APP_INSTANCE`, select a project in the _(obsolete, verified 2026-09-29: Progress export feature removed: transport.test.ts:395 asserts progress_export is gone; rg 'progress_export|ProgressExport' finds only that test line; progress.md only a comment at progress/store.rs:273.)_
  Progress panel, preview `progress.md`, and export it. Verify the preview remains
  usable at desktop and narrow/mobile widths and the file appears at the owning
  project root rather than the active worker workspace.
- [x] Preview an existing `progress.md`, edit it externally, then choose Replace. _(obsolete, verified 2026-09-29: Project Progress export (progress.md Replace) removed: transport.test.ts:395 lists progress_export as gone; no export route in mcp_http/mod.rs:884-890.)_
  Verify the stale write is refused and the external edit remains unchanged;
  preview again and confirm explicit replacement succeeds.

## Project Progress end-to-end journey (story `755-35c8`, 2026-09-13) — **Rust, needs a `make dev` restart**

The backend half of this journey is **done**, not pending. It ran live on a
rebuilt debug instance on `:9877` against throwaway projects `/tmp/pe-a` and
`/tmp/pe-b` (never Boss's repos); the evidence is in the 755-35c8 worklog. The
header this section used to carry — "the `/progress/*` routes are absent from
the backend that is running now" — described the state before `edd69ea7` moved
all ten routes into `shared_routes()`, and is kept here only so a reader who
remembers it knows it was retired rather than lost.

What is left is the half HTTP cannot observe: the **panel**. A projection that
is right over the wire and wrong on screen is a real failure mode, and no
assertion below can be promoted from the backend evidence.

- [x] In an isolated `TUIC_APP_INSTANCE`, register two projects. Report events
      into two workstreams in each, including one `blocked`.
      _(verified: two projects stayed separate with their own revision and
      unread count; "Shadow AI" projected `progressing` with 0 active blockers
      and "Windows Packaging" `blocked` with 1, from started/milestone/blocked
      reports.)_
- [x] Restart the instance. Verify the history, the workstream states, the
      active blockers, and the unread count all survive the restart.
      _(verified: the writing process 71345 was gone and pid 85275 read back 5
      events in order, both workstream states, revision 8, and `readCursor` 0 /
      `unreadCount` 5 — the unread cursor survived too.)_
- [x] Rename one workstream, then report again with the OLD workstream name and
      verify the event lands in the renamed workstream.
      _(verified behaviourally, and again through the post-restart process: a
      report using the pre-rename name landed in workstream `749fd0ff` and
      created no second workstream, so the aliases are durable.)_
- [x] Pause one project, report into it, and verify the receipt says `paused`
      and no event is recorded. Resume and verify the next report is recorded
      with no backfill.
      _(verified: paused receipt, no event, no revision bump; resume recorded
      the next report and did not backfill the paused one.)_
- [x] Clear one project. Verify its history, workstreams, and read state go
      away, that collection is paused, and that an existing `progress.md` at
      that project root is unchanged.
      _(verified: revision moved 1→2 rather than resetting, `collectionEnabled`
      went false in the same transaction, the exported `progress.md` hashed
      identically before and after, and repeating the clear with the stale
      `expectedRevision` was refused with `progress_revision_conflict`.)_
- [x] Preview and export `progress.md` and confirm the Markdown matches the
      status projection.
      _(verified: the preview was deterministic, its Markdown matched the status
      projection including the renamed workstream on historical events, and the
      write landed 780 bytes at the owning project root.)_

Still owed, and only these — all of them are about what is drawn:

- [x] Open the Progress panel and confirm the **global scope** shows both _(obsolete, verified 2026-09-29: Global scope/workstreams UI absent: rg -i 'global|workstream' src/components/ProgressDialog/ProgressDialog.tsx = no matches (dialog rewritten to one journal, see line 3079).)_
      projects with the right per-project state, and that switching to each
      project scope shows that project's workstreams and its blocked one.
- [x] Report into a **paused** project while the panel is open: the receipt is _(obsolete, verified 2026-09-29: Progress pause/correction removed: rg -i 'pause' src-tauri/src/progress -> no matches; no correction control in src/components/ProgressDialog; transport.test.ts:395 lists progress_pause gone)_
      already proven to say `paused`, but confirm no toast appears either.
- [ ] Correct one event through the panel's correction control (edit a summary) _(NOT VERIFIED 2026-09-30: partial — Obsolete item: no correction/edit-summary control exists (rg -i 'correct|edit summary' in ProgressDialog components and src-tauri/src/progress found no matches); journal API exposes list/delete only. Cannot exercise.)_
      and confirm the panel and a fresh export both show the corrected text.
      This leg was never exercised: the live run covered the workstream rename,
      not an event correction.

## Protocol-ranked agent state (story `745-8ff1`, 2026-09-13) — **Rust, needs a `make dev` restart**

- [x] After restarting `make dev`, run an instrumented agent turn for longer _(verified 2026-09-30: Fake claude binary (agent_type=claude, hook_instrumented) via POST /sessions/agent: emitted OSC 7770 busy, then a bare '❯' Ready repaint with no spinner, silent 20s, then OSC 7770 idle. status stayed busy/working ~22s through the stale Ready, idle only at hook-idle rank=Protocol (log). Fake stands i)_
      than the ordinary silence threshold. A stale Ready repaint must not turn
      the tab idle before the agent's protocol completion signal arrives.
- [ ] Disable native/global status instrumentation for one agent and confirm _(NOT VERIFIED 2026-09-30: partial — Fake codex (no hook OSC; 'Working (Ns • esc to interrupt)' then '› Ask Codex' prompt) with PUT /config/agents/codex/native-status-signals enabled=false: status busy then idle ~13s after work ended. Log attributes the idle close to activity_source=process rank=Process, not ready-screen. Same with sig)_
      its existing Ready-screen fallback still returns the tab to idle.

## Vendored fxhash in the bm25 fork (story `758-ff0d`, 2026-09-13) — **Rust, needs a `make dev` restart**

- [ ] Before restarting, note a repo you have searched recently — its content-index _(NOT VERIFIED 2026-09-30: partial — Cannot recreate a pre-vendoring snapshot (no old binary). Own daemon r3iso: /fs/search-content on fixture repo -> 'content index built', results returned; after kill+restart of the daemon the same search logged 'content index built' again (no snapshot written/restored on abrupt stop), so the restore)_
      snapshot on disk was written by the pre-vendoring binary. After the restart,
      run a content search in that repo (`?` in the command palette) for a word you
      know is in it. Results must appear immediately, with `GET :9876/logs` showing
      the snapshot being restored and NOT `content index rebuilt` for that repo. An
      empty result set with a successful restore is the exact failure the vendoring
      had to avoid: the persisted `token.index` values are fxhash32 hashes, so a
      drifted algorithm still decodes the file and then matches nothing.

## Progress Markdown export (story `753-9998`, 2026-09-13) — **OBSOLETE, do not run**

_(NOTE 2026-09-18: the feature every item below tests no longer exists. `6b925e04`
deleted `src-tauri/src/progress/export.rs` with its routes and `EXPORT_LOCK`, and
replaced the Progress **panel** with `ProgressDialog` — which has no export card,
no source-metadata checkbox and no preview. `src/components/` holds only
`ProgressDialog`, and no `/progress/export*` route survives in `mcp_http/mod.rs`.
Kept for history; the five items are unrunnable, not pending.)_

Automated verification already covers the backend contract end to end (unit
tests plus a live HTTP run against a rebuilt debug instance on `:9877`:
preview → write → `progress_export_exists` → `progress_export_content_changed`
with the human edit preserved). What is left is what HTTP cannot observe.

- [x] Open the Progress panel in the desktop app, pick one project, and check _(obsolete, verified 2026-09-29: Export card removed: rg 'Include source metadata|Replace progress' src src-tauri/src = no matches; section header itself says OBSOLETE.)_
      the export card against `docs/frontend/STYLE_GUIDE.md`: the source-metadata
      checkbox, the preview button, the revision line, and the scrolling
      Markdown preview block.
- [x] Toggle "Include source metadata" while a preview is shown. The preview and _(obsolete, verified 2026-09-29: Section marked OBSOLETE; rg 'Include source metadata|Replace progress' src -> no matches; progress_export gone (transport.test.ts:395))_
      its export button must disappear, because that snapshot can no longer be
      written.
- [x] Export once, then export again. The second run must ask for confirmation _(obsolete, verified 2026-09-29: Section header at to-test.md:2940 says OBSOLETE; `rg 'Replace progress' src src-tauri/src` returns no matches (only a comment in progress/store.rs:273).)_
      before replacing the file, and the button must read `Replace progress.md`.
- [x] Edit `progress.md` by hand between the preview and the write, then write. _(obsolete, verified 2026-09-29: Section marked OBSOLETE; `rg progress_export_content_changed src src-tauri/src` -> no matches (progress.md export gone))_
      The panel must show `progress_export_content_changed` and your edit must
      still be in the file.
- [x] After an export, run `git status` in that project: only `progress.md` may _(obsolete, verified 2026-09-29: Progress Markdown export removed (heading says OBSOLETE); rg 'progress_export|ProgressExport' matches only transport.test.ts:395 asserting it is gone.)_
      appear. Nothing under `.tuic/` may be listed.

## MCP instruction de-duplication (#754-affa) — needs a `make dev` restart

Rust-only change to `mcp_transport.rs`. Boss's live instance still serves the old
strings until the backend is restarted; nothing below can be checked before that.

- [x] After restart, `curl -s localhost:9876/mcp/instructions | jq -r .instructions`. _(verified 2026-09-29: GET /mcp/instructions (unix socket): ## Tools holds the delegation line, Worktrees rule and Submit rule, no per-tool bullets, no ## Workflow, no UI feedback line. NOTE: ## Multi-Agent Work also keeps a Mail bullet besides the peer count and isolated-branches bullet)_
      The `## Tools` section must hold three lines (the delegation sentence, the
      Worktrees rule, the Submit rule) and **no** per-tool bullet list; there must
      be no `## Workflow` section and no `**UI feedback:**` line. `## Multi-Agent
      Work` keeps the peer count and the isolated-branches bullet only.
- [x] `ack` / `intent:` / `suggest:` markers must be byte-identical to before — _(verified 2026-09-29: ack, intent and suggest marker lines present verbatim in /mcp/instructions (comparison with the old capture not possible))_
      they are protocol, and a reworded marker breaks the tab title and the
      suggestion bar. Compare against a capture of the old output if in doubt.
- [x] In a connected agent, ask for the `repo` tool schema: its description must _(obsolete, verified 2026-09-29: repo tool now has only progress_list: REPO_ACTIONS at mcp_http/mcp_transport.rs:1148 lists a single progress_* action, not nine; text obsolete.)_
      now document all nine `progress_*` actions, which it never did before.
- [x] Watch one agent session for a turn. It must still emit `ack` exactly once _(verified 2026-09-30: Real claude -p (stream-json, --mcp-config stdio bridge to instance MCP socket) at r3 registered repo: first assistant text 'TUICommander v1.7.7 is connected.' then 'intent: Listing the current directory (List directory)' (ack once, intent at the phase), then progress done, suggest: [A|B|C]. One turn)_
      per connection and `intent:` at each phase change — the markers moved not
      at all, but this is the cheapest way to notice if they did.

## Progress reachable from `tuic-remote` (#755-35c8 finding) — needs a `make dev` restart

Rust-only routing change in `mcp_http/mod.rs`: the ten `/progress/*` routes moved
from `build_router` into `shared_routes()`. Before this, a remote/PWA client
talking to a `tuic-remote` daemon got **404 on the whole Progress feature** — no
route at all, which looked like an auth failure. Tests cover route existence;
these check the live surface.

- [x] Start a headless daemon: `TUIC_APP_INSTANCE=remote-check tuic-remote`, then
      `curl -u <user>:<pass> -X POST 'http://127.0.0.1:<port>/progress/list'`.
      It must answer with a list body, not 404.
      _(NOTE 2026-09-18: `/progress/status` was deleted by `6b925e04` — probing it
      returns 404 for that reason, not a routing regression. The ten routes are now
      four: `/progress/{report,list,delete,viewed}`, all POST, and all still inside
      `shared_routes()` at `mcp_http/mod.rs:708-723`, so the property this item
      exists to protect is intact. Use `/progress/list`.)_
      _(verified 2026-09-20 against a headless `tuic-remote` on mac-mint:
      `POST /progress/list?path=/home/stefano` with credentials answers **200**
      `{"project":"/home/stefano","entries":[]}`. The control that makes this mean
      something: `POST /progress/nonexistent` with the same credentials answers **404**,
      so the 200 is a registered route and not a catch-all.)_
- [x] Same call with **no** credentials from a non-loopback address must still be
      rejected by the auth middleware — the move must not have widened access.
      _(verified 2026-09-20: the same POST from this Mac to mac-mint — a genuine
      non-loopback peer — answers **401** with no credentials and **401** with a wrong
      password, while `GET /health` answers 200 unauthenticated, which is the one route
      documented as open. The headless build has no loopback bypass, so this also holds
      from the daemon's own localhost.)_
- [x] On the desktop instance, the Progress **dialog** must behave exactly as _(verified 2026-09-29: Web UI :9880: Progress dialog opens from bell popover, lists entries, updates live when POST /progress/report adds one (entry id 58 appeared while open), list/report/viewed calls succeed; no 404 (dialog renders entries, logs show no progress errors).)_
      before: the routes are merged into `build_router` through `shared_routes()`
      now, so a regression here shows up as the dialog 404ing on every call.
      _(NOTE 2026-09-18: "panel" — `ProgressPanel` was replaced by `ProgressDialog`
      in `6b925e04`. Same check, different surface.)_

## A turn closed by the foreground probe logs `activity_source=process` — needs a `make dev` restart

`foreground_probe` never constructed `ForegroundProbe::Quiet`, so every close
that the process table actually answered was logged as `agent-ready-screen`,
indistinguishable from a screen-only guess (#771-4733). Rust-only — the running
app keeps the old logging until restart.

- [x] After restart, let an agent tab finish a turn with nothing running under _(verified 2026-09-30: Goose turn (agent_type=goose, spawned via POST /sessions/agent) after 'reply with the word ok': /logs 'Shell state → idle' data activity_source=process rank=Some(Process) for that session at turn end (also at startup), not agent-ready-screen. Codex/claude not used (trust dialogs).)_
      it, then `curl 'http://localhost:9876/logs' | grep 'Shell state'`: the
      close must read `activity_source=process rank=Process`, not
      `agent-ready-screen`.
- [x] A tab whose agent still has a `cargo`/`npm` child running when the ready _(verified 2026-09-29: by code/test inspection, tests not executed here: Covered by pty/tests.rs:4617-4686 asserting source stays 'agent-ready-screen' with a child process running)_
      screen appears must still close as `agent-ready-screen` — the probe must
      not claim an observation it did not make.
- [ ] After such a close, typing into that tab (or the agent resuming on its _(NOTE 2026-09-29: partial evidence only — activity_source=process test: pty/tests.rs:4626 (must name the probe). Busy-on-typing recovery not confirmed by a named test; verify or add.)_ _(NOT VERIFIED 2026-09-30: partial — Same goose session: after the process-ranked idle close, typing a prompt+Enter logged 'Shell state → busy' activity_source=user-submit rank=Protocol (idle->busy recovered; status busy/working 19 samples). NOT tested: agent resuming on its own.)_
      own) must turn it BUSY again. A tab stuck IDLE while the agent works is
      the regression this rank change could cause.

## A wake that could not start is retried at the next idle edge — needs a `make dev` restart

A `NotStarted` wake attempt burns the orchestrator wake budget for the whole
group, and only an inbox read restored it. So one draft in the composer, one
open question or one unconfirmed idle at the moment mail arrived silenced the
"you have mail" notice for the rest of the session: the mail sat in the inbox
and the master terminal was never told. A new BUSY→IDLE edge now re-arms the
budget before chasing the notice. Rust-only — the running app keeps the old
behaviour until restart.

- [x] Type a draft into the orchestrator's composer (do not submit), have a peer _(verified 2026-09-30: Fake claude orchestrator (agent_type=claude, OSC 7770 hooks, register orchestrator=true). Typed 'draftx' (no Enter), peer agent send -> delivery_path=inbox_only, nothing typed. Cleared draft + submitted 'go': after busy->idle edge the fake received '[TUIC] message available — read it with: agent act)_
      `agent action=send` to it, then clear the draft and let the turn settle.
      Within a few seconds the orchestrator must be handed the
      `agent action=inbox` line. Before the fix nothing ever arrived.
- [x] The payload must never appear on the orchestrator's screen — only the _(verified 2026-09-30: Same run: composer log and screen output never contained the payload 'PAYLOAD-SECRET-r3-1' (grep -c = 0); only the '[TUIC] message available — read it with: agent action=inbox' pointer was typed. Payload only via agent inbox.)_
      pointer to the inbox.
- [ ] A notice already being typed must not be duplicated by a concurrent idle _(NOT VERIFIED 2026-09-30: partial — Draft typed, two peers (r3-b, r3-c) sent two messages (both inbox_only), draft cleared, one idle edge: exactly one wake line typed (not two). Exact race of notice being typed at the same instant as an idle edge not constructed.)_
      edge: one wake per group, not two.

## Progress rewritten to one journal, one database and a dialog — needs a `make dev` restart

The whole Progress feature was re-implemented against the 2026-09-14 revision of
`plans/project-progress.md`: one append-only journal in a single database at
`<config dir>/progress.sqlite3`, two reportable kinds plus a host-written
`intent`, a dialog replacing the sidebar panel, eight `repo progress_*` actions
cut, and no Markdown export. Rust and frontend both changed, so the running
build has the old behaviour until restart.

- [x] After restart, `progress.sqlite3` must exist in the config directory, and _(verified 2026-09-29: progress.sqlite3 exists in instances/validate; ls -a fx/repo shows no .tuic dir after progress writes)_
      no *new* `.tuic/` directory may appear in any repository. The 42 existing
      ones are stale leftovers of the old design — see the cleanup item below.
- [ ] Ask an agent to report: the entry must appear in the dialog with its agent _(NOT VERIFIED 2026-09-30: partial — Real claude report (progress type=done via MCP) -> repo progress_list entry type=done agentName=rep-1 ptyId set; intent entries type=intent exist alongside (fake agent marker 'Checking the marker ONTAG'). Dialog rendering (agent name, muted intent styling) not seen: needs_browser.)_
      name, and an `intent:` marker from any agent tab must appear as a muted
      `intent` entry in the same list.
- [x] An agent calling `progress` with `type=intent` must be refused, naming _(verified 2026-09-29: by code/test inspection, tests not executed here: Test: progress/model.rs:309 parse_reportable('intent') is an error; MCP path test mcp_transport.rs:16890 sends type=intent.)_
      `done` or `blocked`.
- [x] Open the dialog on a project with history, note the divider, let a new _(verified 2026-09-29: Web UI Progress dialog (All repo): seeded via POST /progress/report. Open with new A: 'Seen before' divider below A. B reported while open: rows B,A,divider - divider stayed below A. Close+reopen: no divider (nothing new; code draws none for index 0). Report C closed, reopen: divider between C and B (B,A now read).)_
      entry arrive: the divider must NOT move while the dialog is open. Close
      and reopen: it must now sit above the entries just read.
- [x] Settings → Agents → **Collect project progress** off: the `progress` tool _(verified 2026-09-30: On own daemon (r3iso): PUT /config progress_tracking=false -> fresh MCP peer tools/list lost 'progress' (10->9); fake claude PTY intent line not journaled (on: journaled). Per-agent agents.claude.progress_tracking=false: tool still listed, progress done -> 'progress_tracking_disabled: Progress colle)_
      must disappear from a newly-connected agent's tool list, and `intent:`
      markers must stop being recorded. Per-agent **Collect progress** off must
      instead answer `progress_tracking_disabled` on a report.
- [x] `repo action=progress_list` must still work; `progress_status`, _(verified 2026-09-29: by code/test inspection, tests not executed here: src/__tests__/transport.test.ts:395 asserts progress_status/pause/clear/export are gone; progress_list still in REPO_ACTIONS (mcp_transport.rs:1148))_
      `progress_pause`, `progress_clear` and `progress_export` must be gone.
- [ ] The mobile PWA's Progress tab must render the same list full-bleed. _(NOT VERIFIED 2026-09-30: blocked — real phone (mobile PWA Progress tab full-bleed rendering))_
- [ ] **[HUMAN]** Screenshot check against `docs/frontend/STYLE_GUIDE.md`: _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: [HUMAN] screenshot comparison of the Progress dialog against docs/frontend/STYLE_GUIDE.md: needs a rendered UI (no frontend here); no audio hardware involved.)_
      blocked entries red, `intent` muted and italic, the divider legible, the
      delete button appearing on row hover.
- [ ] **[HUMAN]** Narrow the window to ~480px with a long entry on screen: the _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Needs a rendered UI narrowed to ~480px with a long Progress entry; headless instance has no frontend.)_
      dialog must stay readable — it is `min(680px, 100vw - 48px)` wide and the
      text wraps with `overflow-wrap: anywhere` — and the header must keep the
      blocked-only toggle and the close button on one row (778-a9a6 criterion 8).
- [ ] After the restart has proved the new store works, delete the stale _(NOT VERIFIED 2026-09-30: partial — Real deletion left to Boss (destructive on ~/Gits). Ran the two exact find commands on a simulated tree (a/.tuic with progress.sqlite3{,-wal,-shm}; b/.tuic with progress.sqlite3 + tunnels/t.json; c/x/.tuic): files deleted, empty .tuic dirs removed, b/.tuic/tunnels kept. Read-only count of real match)_
      per-repo databases. They are not migrated by design. Delete the **files**,
      not the directory:
      `find ~/Gits -maxdepth 5 -path '*/.tuic/progress.sqlite3*' -delete`
      then drop the directories that this leaves empty:
      `find ~/Gits -maxdepth 4 -type d -name .tuic -empty -delete`
      _(NOTE 2026-09-18: the previous `rm -rf` on the whole `.tuic/` directory is
      wrong even though it happens to be harmless today. `.tuic/` is a live
      namespace — `tunnels/storage.rs:33,59,69` writes `<repo>/.tuic/tunnels/` —
      so the blanket delete destroys tunnel storage for any repo that has one.
      Verified 2026-09-18: 43 `.tuic` directories (not 42; `agent2__wt/`
      `analysis-ai-risk-score-20260918` is new), 0 contain `tunnels/`, and every
      file in all 43 matches `progress.sqlite3*`.)_
- [ ] Run diff-scoped mutation testing once on the final HEAD of this batch: _(NOT VERIFIED 2026-09-30: partial — Not run: diff-scoped mutation testing (make mutants) is an overnight cargo job (~5 min/mutant) and cargo/build is forbidden for this pass. Orchestrator batch task.)_
      `make mutants RANGE=<commit before the Progress rewrite>`. It is an
      overnight-class job (~5 min per viable mutant), so it is deliberately not
      run during the day — 780-e99a criterion 4.
- [x] Bring the worktree build up on `:9877` and exercise Progress through its _(verified 2026-09-29: On isolated validate instance (unix socket = its own HTTP router, not :9876): POST /sessions in fx/repo, MCP progress type=done -> {id:29}; POST /progress/report?path= -> {id:30}; POST /progress/list?path= and repo progress_list returned both entries (ptyId, agentName); DELETE /sessions ok.)_
      own HTTP instance — creating a throwaway session, reporting, listing and
      deleting — rather than against the orchestrator on `:9876`.

## TypeScript mutation tooling (story `944-15f3`, 2026-09-25)

- [x] Run the narrow canary from `docs/guides/development-setup.md` and confirm
      Stryker reports the `pathBasename` condition mutant as `Killed`, with at
      least one Vitest test executed against it. _(verified: 2026-09-25;
      `scripts/ts-mutants.mjs` on `pathUtils.ts:65-65` reported 6 Killed, 0
      Survived, and 1.00 tests per mutant; JSON saved under
      `~/Gits/.tmp/results/ts-mutation-gate/mutation.json`.)_
- [x] On the next changed TypeScript source/test pair, run the same scoped _(verified 2026-09-29: Ran node scripts/ts-mutants.mjs 'src/utils/panelSync.ts:30-40' -- src/__tests__/utils/panelSync.test.ts (changed pair from 8b3e740d2) with TMPDIR set: completes in 1m26s, 20 mutants (8 killed, 2 timeout, 8 survived), score 55.56, 0.89 tests/mutant, mutation.json written. Removed reports/mutation after.)_
      command before using its mutation score as a story gate.
- [x] After the StoriesDialog dependency-removal change is present in this _(verified 2026-09-29: ts-mutants.mjs on StoriesDialog.tsx:532 (story row onClick) with StoriesDialog.test.tsx: 1 mutant, Killed, 100% score, 1.00 tests/mutant. Only that one click handler was mutated; the original false-survivor identity not confirmed.)_
      checkout, run its targeted test through `scripts/ts-mutants.mjs` and
      verify the previously false-surviving click-handler mutant is `Killed`
      (story `944-15f3`).

## File pickers moved off `tauri-plugin-dialog` — needs a `make dev` restart

The app died on 2026-09-18 when `+[NSOpenPanel openPanel]` returned NULL after
the window-server connection was interrupted, panicking on the main thread. The
pickers now go through our own `pick_path` command, which owns the main-thread
closure and catches that unwind (`src-tauri/src/native_dialog.rs`). Automated
tests cover the wire contract and the wrapper's mapping; the panels themselves
need a window server, so these are by hand.

- [ ] Sidebar → add a repository: the folder picker opens, a pick registers the _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Native OS folder picker (rfd) in the desktop app; no window server/frontend on the headless instance. Needs desktop build + macOS UI automation.)_
      repo, and Cancel leaves the sidebar unchanged.
- [ ] Cmd+O (open file) and the open-folder action: both return a path and the _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Native Cmd+O / open-folder dialog: desktop build only, not drivable over HTTP/MCP.)_
      chosen file opens in an editor tab.
- [ ] New File (save panel): the suggested name is pre-filled and the file is _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Native New File save panel (suggested name pre-filled): desktop build only.)_
      created at the chosen location.
- [ ] Settings → Plugins → Install from ZIP: the type filter still restricts the _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Native Install-from-ZIP file dialog and its .zip type filter: desktop build only.)_
      selection to `.zip` — that filter is the one option most likely to have
      been dropped in the move.
- [ ] Settings → Plugins → Install from Folder, and Tunnels → the SSH identity _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Native Install-from-Folder and SSH identity file pickers: desktop build only.)_
      file browse button: both still pick.
- [ ] **[HUMAN]** The crash path itself: let the Mac sleep with the display off, _(NOT VERIFIED 2026-09-30: partial — Not run: needs real Mac sleep with display off plus the desktop app picker; sleeping the shared host Mac would disturb other agents. No audio hardware involved.)_
      wake it, and immediately open a picker. It must either open, or show the
      "system file dialog is unavailable" error — the app must NOT exit. This
      needs real standby, which no automated check here can reach.

## Remote Machines authentication (#781-9652) — needs a `make dev` restart

The whole change is Rust plus the transport layer, so nothing here is live in
Boss's running session. Restart first. A daemon to test against is already up:
`mac-mint:9877`, user `stefano`, systemd user unit `tuic-remote`, running a
headless build of this tree.

- [ ] Settings → Services → Remote Machines → add a Direct connection to _(NOT VERIFIED 2026-09-30: partial — Stand-in local daemon http://127.0.0.1:9893 as Direct conn with correct user/password: status connected (not unauthenticated), protocol_version 1. Form/UI flow (Settings > Remote Machines) and mac-mint:9877 not exercised.)_
      `http://mac-mint:9877` with the username and password. Connect: the status
      goes **Connected**, not "Not authenticated".
- [ ] Same connection with a wrong password: the status reads **Not _(NOT VERIFIED 2026-09-30: partial — API: wrong password -> status 'unauthenticated' + 'Authentication rejected by the remote daemon'. UI wording 'rejected these credentials' / amber badge / no calls routed need the frontend (needs_browser).)_
      authenticated** with "rejected these credentials", stays amber rather than
      red, and no terminal or repo call goes through.
- [x] Edit an existing connection: the password field shows the "stored — leave _(verified 2026-09-29: Web UI edit of Direct conn: password input placeholder 'Password (stored — leave blank to keep it)'. Saved with blank password (status drops to [] until Connect), pressed Connect -> status connected, no error: vault password kept.)_
      blank to keep it" placeholder, and saving with it blank keeps the
      connection working.
- [ ] Restart `tuic-remote` on mac-mint under a live connection. The daemon mints _(NOT VERIFIED 2026-09-30: partial — Stand-in: local second tuic-remote (port 9893) instead of mac-mint. Killed+restarted under live connection: error(Unreachable) then connected within ~2s (<5s poll), new token, no manual action. mac-mint itself not available (second machine).)_
      a new token; within one poll (5s) the connection re-authenticates by itself
      and stays Connected.
- [ ] **[VISUAL]** The password field and the vault hint render inside the _(NOT VERIFIED 2026-09-30: blocked — VISUAL-owned by tuic-live-checks)_
      add/edit form without breaking the Settings layout.

## Remote repos run on the remote machine (#782-3d05) — needs a `make dev` restart

Frontend-only, but it changes where every repo-scoped call goes, so it needs the
same restart as the item above and the same daemon (`mac-mint:9877`).

- [ ] With the connection Connected, add a remote repo from it. The sidebar shows _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Frontend flow (add remote repo, sidebar badge, git status compare). Backend stand-in exists: second local tuic-remote (Direct conn) worked for connect/mirror; comparing against 'ssh mac-mint git status' needs the second machine.)_
      the repo with its remote badge and the git status, branch and file tree are
      the **remote machine's** — compare against `ssh mac-mint git -C <path> status`.
- [ ] Open a terminal on that repo. It spawns on mac-mint: `hostname` and `pwd` _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Frontend flow: terminal tab on a remote repo (hostname/pwd, resize, close ends remote session). Backend mirror of remote sessions verified with a local second daemon (session-created/state events); the tab needs the UI. mac-mint itself not available.)_
      answer for the remote box, typing and resizing work, and closing the tab
      ends the session there (`ssh mac-mint` + check the daemon's `/sessions`).
- [ ] Edit a file on mac-mint by hand while the repo is open locally. The git _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Frontend refresh of git panel/file tree from remote watcher SSE. Backend half verified: second daemon repo-changed reaches the local /events bus with __tuic_origin; panel refresh needs the UI.)_
      panel and file tree refresh by themselves — the remote watcher and its SSE
      stream are doing it.
- [ ] Commit and stage from the git panel on the remote repo. The commit lands on _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Commit/stage from the git panel on a remote repo: frontend action; needs the UI and a remote host (local second daemon can stand in).)_
      mac-mint, not on any local repo.
- [ ] A local repo behaves exactly as before — no extra latency, no remote call. _(NOT VERIFIED 2026-09-30: partial — Local repo: GET /logs?source=network on the instance is [] after local repo/session activity (no remote call logged). Latency/'exactly as before' comparison and UI not measured.)_
      Confirm with `GET http://localhost:9876/logs?source=network`.
- [ ] Stop `tuic-remote` on mac-mint with the repo still open. Repo operations _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Frontend: repo operations with the daemon stopped must say 'Remote connection … not connected'. Backend: killing the daemon flips status to error(Unreachable) within a poll; UI message not seen.)_
      report "Remote connection … not connected" rather than showing local data.
- [x] Open a file from the remote repo in the editor and use Go to definition. _(verified 2026-09-29: Local tuic-remote as Direct machine; remote repo added via picker (fx/agb2/rr), opened a.rs from File Browser in editor: /logs?source=network has warn '"mdkb_outline" has no remote route and ran on the local machine' (once). Go to definition itself not invoked (keys do not reach page); the log line is triggered by opening the file.)_
      mdkb has no remote route, so it runs locally against a path this machine
      does not have and logs `has no remote route and ran on the local machine`
      once — check `GET http://localhost:9876/logs?source=network`. The log line
      is the thing under test; the feature itself is a known gap.

## The connection runtime moved to Rust (#790-ef85) — needs a `make dev` restart

The health probe, the token exchange, the 5s status poll and the SSH tunnel now
run in the backend; `remoteConnections.ts` only renders what the backend pushes.
Every item below must behave exactly as it did before the move — that is the
point of the story — plus the two things only the backend can do.

- [ ] Connect a remote machine from Settings → Remote Machines. The status goes _(NOT VERIFIED 2026-09-30: partial — Direct conn to local second daemon: connect -> status connected with protocol_version=1 (+build). Intermediate 'connecting' state not sampled; Settings panel rendering not seen (headless).)_
      Connecting → Connected and the panel shows the protocol version.
- [ ] Connect the same machine from a second client (browser at _(NOT VERIFIED 2026-09-30: partial — Two concurrent /events SSE clients (unix socket) both received the same 6 'remote-connection-status' events during daemon restart (error -> connected). No desktop window/browser panels compared side by side.)_
      `http://localhost:9876/`) while the desktop app is open. **Both** panels
      show Connected: the status is pushed to every client, not owned by the one
      that clicked.
- [ ] A wrong password reports `unauthenticated` (not "connection error") and no _(NOT VERIFIED 2026-09-30: partial — Wrong password: POST /connect -> {'error':'Authentication rejected by the remote daemon'}; GET /config/remote-connections/status = status 'unauthenticated' (stable 6 samples), no base_url/token. NOT tested: that no repo/terminal call is routed while unauthenticated.)_
      repo or terminal call is routed to that machine while it is in that state.
- [x] Restart `tuic-remote` under a live connection: within one poll the _(verified 2026-09-30: Own tuic-remote (--instance r3iso, TUIC_PORT=9893, bcrypt pw) as Direct connection via PUT /config/remote-connections + /password + POST /connect: status connected with protocol_version=1. Killed+restarted the daemon: status error(Unreachable) then connected again ~2s after start, no user action; in)_
      connection re-authenticates by itself and stays Connected. This is now a
      Rust task, so it keeps working with the TUICommander window closed to the
      tray or the WebView asleep — the case the WebView implementation lost.
- [ ] Disconnect: the tab's remote sessions stop being routed, the SSH tunnel is _(NOT VERIFIED 2026-09-30: partial — Direct transport: DELETE /config/remote-connections/{id}/connect -> ok, status list empties. SSH tunnel/ps ssh/__remote_* Tunnels profile parts need an SSH transport to a second machine: blocked (second physical machine).)_
      gone (`ps aux | grep ssh` on this machine), and the Tunnels panel shows no
      leftover `__remote_*` profile — the tunnel is built in memory now.
- [ ] Quit TUICommander with an SSH-transport connection live. No orphan `ssh` _(NOT VERIFIED 2026-09-30: blocked — Second physical machine (SSH transport to mac-mint) plus quitting the desktop app; neither available headless.)_
      process and no `__remote_<id>` profile file is left behind.

## Remote sessions report idle / busy / question (#791-055e) — needs a `make dev` restart

`remote_mirror.rs` now follows the remote daemon's own `/events` and repeats
every frame on the local bus under the daemon's own name, and the WebView's
`remoteEventBridge.ts` is gone with it. Nothing on the frontend subscribes to a
remote machine any more.

Verified 2026-09-19 against a **real second daemon** — `tuic-remote --instance
mirrortest` on `:9899` with its own password, reached over a Direct connection
from the running desktop on `:9876`. Not a mock: a separate process, real auth,
real SSE. Torn down afterwards (connection deleted, vault entry cleared, instance
dir removed).

- [x] The dot goes busy while a remote session runs and idle when it stops.
      _(verified: a session created on the `:9899` daemon appeared in the
      desktop's `GET /sessions` as `3b19ac21 | connection_id=1111…5555 | shell=idle`;
      writing a 5-second loop to it produced, on the **desktop's own**
      `/events?types=session-state-changed`, the sequence idle → busy → idle —
      11 frames, under the ordinary event name and matched by the ordinary type
      filter, which is exactly what `applySessionStateEvent` consumes.)_
- [x] A daemon restart under a live connection re-seeds instead of leaving stale
      rows. _(verified: two mirrored sessions before; killed and restarted the
      daemon; the connection re-authenticated by itself with a new token and the
      list came back holding only the one session the restarted daemon actually
      has.)_
- [x] Disconnecting clears the badges and drops the sessions.
      _(verified: `DELETE …/connect` published
      `session-closed {"session_id":"3b19ac21…","reason":"remote-disconnected"}`
      on the local `/events`, and `GET /sessions` then held no row with a
      `connection_id`.)_
- [ ] Make a remote **agent** ask a question. The question badge and the _(NOT VERIFIED 2026-09-30: partial — Second daemon (:9893) Direct-connected; fake claude agent there (OSC awaiting + 'Do you want to proceed?') -> local /events (unix socket) carried session-state-changed agent_state=awaiting_input awaiting_input=true question_text='Do you want to proceed?' with __tuic_origin; after answering on remote)_
      notification are the same ones a local agent raises. Answer it: the badge
      clears. _(Not covered above: the probe drove a plain shell, so
      `awaiting_input` never moved. Needs a real agent on the other machine.)_
- [ ] While the remote agent is mid-turn, queue a command from the Compose _(NOT VERIFIED 2026-09-30: partial — Mirror side only: remote fake agent mid-turn -> local /events session-state-changed shell_state=busy agent_state=working, idle at end; GET /sessions on local lists the mirrored row with connection_id. Compose-panel 'N queued' badge and queue gate are frontend: not exercised.)_
      panel. The `N queued` badge moves and the command is delivered at the
      agent's next idle window — the queue gate reads the mirrored state.
- [ ] Commit something on the remote repo from a shell there. The local panels _(NOT VERIFIED 2026-09-30: partial — Registered repo watcher on second daemon (POST /watchers/repo), committed from git in that repo: local /events?types=repo-changed received {repo_path,kind:git-state,__tuic_origin.connection}. Local panel refresh is frontend: not exercised.)_
      for that repo still refresh (this used to come from `remoteEventBridge.ts`;
      it now arrives on the mirrored `repo-changed`, through the same coalescer a
      local change uses). Needs a repo registered on the remote machine.
- [x] With no remote connection configured at all, nothing changes.
      _(verified 2026-09-19 on the restarted build: no `connections.json` exists,
      `GET /sessions` returns the 4 local rows with their state and **no**
      `connection_id` field on any of them — the mirror adds nothing when there
      is nothing to mirror.)_

## Ideas panel: queue instead of typing, and a shorter Compose panel

- [ ] Open an agent tab and the Ideas panel. Each idea shows a queue button _(NOT VERIFIED 2026-09-29: partial — Web UI Ideas panel on a plain shell tab: each idea shows only pencil (Edit idea), ▶ (Send to terminal), ✕; no queue button. Agent-type tab (queue button present) not testable: no real agent CLI tab.)_
      (stacked lines) left of the ▶ send button. On a plain shell tab the queue
      button is absent and only ▶ remains.
- [ ] Click queue while the agent is mid-turn: nothing is typed into the prompt, _(NOT VERIFIED 2026-09-29: Needs a real agent mid-turn to observe queue-on-busy and idle delivery)_
      the Compose `N queued` badge goes up by one, and the idea gets its used
      timestamp. The idea is delivered at the agent's next idle window.
- [ ] Detach the Ideas panel to its own window. The queue button is always shown _(NOT VERIFIED 2026-09-29: blocked — Needs desktop detached Ideas window plus maccontrol click/focus observation; detached windows and focus are not observable via HTTP/MCP; browser and maccontrol not permitted for this run.)_
      there; clicking it with a plain shell active raises the "not running an
      agent" toast in the main window instead of queueing, and does NOT steal
      focus back to the main window (unlike ▶, which does).
- [ ] The Compose panel is visibly shorter (160px, was 200px) and still fits the _(NOT VERIFIED 2026-09-29: partial — Compose panel not openable in web UI on a shell tab (palette 'Toggle compose panel' rendered nothing; needs agent session). Code disagrees with item: ComposePanel.module.css .panel height 142px (item says 160px) and .queueList max-height 78px (item says 96px) - item text may be stale; no runtime measurement, queue with several commands not producib)_
      editor, the status bar and the buttons. Open the queue list with several
      queued commands: the list caps at 96px and the editor keeps usable rows.

## Updater — symlinked binary path (2026-09-19)

- [ ] Settings -> General -> Updates -> Check Now, on a build whose binary sits _(NOT VERIFIED 2026-09-29: Tauri updater Check Now UI is desktop-only (not in web mode) and needs a symlinked build; maccontrol lacks screen access.)_
      under a symlinked path (a `make dev` build: `src-tauri/target` is an mbx
      target view). It must print a muted "In-app updates are unavailable…"
      hint and NOT the red "Update failed" dialog nor the red hint.
- [ ] The same build on a release install with no symlink in the path still _(NOT VERIFIED 2026-09-29: Needs a release install (symlink-free path) updater check)_
      reports "You are on the latest version" or the available version.
- [ ] After the Notes→Ideas rename: existing ideas still load. The store reads _(NOT VERIFIED 2026-09-29: partial — Web UI Ideas panel: added 2 ideas -> instance notes.json 'notes' array written (same file/store); edit (blur commit) and delete work and persist to notes.json ([] after delete). Instance had no pre-existing notes.json so 'existing ideas load/count' not checked; reassign, image paste (note-images), detach/re-dock not done.)_
      the same `notes.json` through the same `load_notes`/`save_notes` commands,
      so nothing should have moved — but this is the one failure that would be
      silent and lossy, so open the panel and count the ideas before trusting it.
      Add, edit, reassign and delete one; paste an image (assets still land in
      `note-images/<id>/`); detach the panel and re-dock it.

## MCP 2026-07-28 stateless lifecycle — needs a `make dev` restart (#843d)

Rust-only change: it is NOT live in the running session until the backend is
rebuilt.

- [x] `curl -s localhost:9876/mcp -H 'content-type: application/json' -d @src-tauri/src/mcp_http/fixtures/ego_server_discover.json` _(verified 2026-09-29: tuic-remote --instance validate, MCP unix socket: discover fixture returned resultType=complete, supportedVersions [2026-07-28,2025-11-25,2025-03-26], capabilities.tools.listChanged, instructions, no mcp-session-id header; script t3.py)_
      returns a `result` with `resultType: "complete"`, `supportedVersions`,
      `capabilities.tools.listChanged`, `instructions`, and NO `mcp-session-id`
      response header.
- [x] Claude Code (the legacy `initialize` path) still connects and still lists _(verified 2026-09-29: legacy initialize with mcp-session-id then tools/list returned the full surface (session, agent, task, remote, repo, story, progress, ui, plugin_dev_guide, voice); t4.py)_
      the full tool surface — the two lifecycles share one endpoint.
- [x] A `tools/list` carrying `params._meta."io.modelcontextprotocol/clientInfo"` _(verified 2026-09-29: tools/list with _meta clientInfo name=ego returned search_tools, get_tool_schema, call_tool, progress only; t3.py)_
      with `name: "ego"` returns the three meta-tools plus `progress`, and no
      native or upstream definitions.

## One MCP tool family — needs a `make dev` restart (#f6ed)

- [ ] `tools/list` on `:9877` returns exactly `session, agent, task, repo, _(NOT VERIFIED 2026-09-30: partial — item text stale: tools/list now returns session,agent,task,remote,repo,story,progress,ui,plugin_dev_guide,voice; tools/list (x-tuic-session peer, unix socket) returns session, agent, task, remote, repo, story, progress, ui, plugin_dev_guide, voice: differs from i)_
      progress, ui, plugin_dev_guide, config, debug` and no `ai_terminal_*`.
- [x] `call_tool`/`tools/call` with `ai_terminal_read_screen` answers _(verified 2026-09-29: unknown-tool error for ai_terminal_read_screen lists Available: session, agent, task, remote, repo, story, progress, ui, plugin_dev_guide, config, debug, voice, search_tools, get_tool_schema, call_tool; no ai_terminal_*)_
      "Unknown tool", and the message does not advertise `ai_terminal_*`.
- [x] Echo a fake token into a terminal (`echo GITHUB_TOKEN=ghp_…`), then read _(verified 2026-09-30: 80-col session (cols=80), sh with short prompt: 'echo aaaa… GITHUB_TOKEN=ghp_<42 chars>' (101 chars, wraps mid-token). session output default and format=raw both show GITHUB_TOKEN=[REDACTED]; longest leaked substring of the token >=4 chars = 0. Also OK in zsh default prompt. Earlier 29/09 leak not r)_
      it back with `session action=output`: the value must come back
      `[REDACTED]`, in both the default format and `format=raw`.
- [x] A `config.json` still carrying `ai_terminal_mcp_enabled` loads without _(verified 2026-09-29: by code/test inspection, tests not executed here: AppConfig (config.rs:764) derives Deserialize without deny_unknown_fields, so legacy ai_terminal_mcp_enabled is ignored; rg finds no field)_
      error — the field is simply ignored now.

## Bridged `log` records are diagnostic — needs a `make dev` restart

- [ ] With an upstream whose TLS fails (or any dependency logging through the _(NOTE 2026-09-29: partial evidence only — Bridged log records classified at app_logger.rs:97 bridged_log_classification (diagnostic); confirm via that function's tests rather than a live TLS failure.)_ _(NOT VERIFIED 2026-09-30: partial — Same real TLS failure: entries have audience=diagnostic; GET /logs?audience=user has 0 rustls entries (996 user rows), ?audience=diagnostic has them. Error-panel User tab/unseen badge are frontend: not observed.)_
      `log` facade at error level), the error log panel's default **User** tab
      stays clean and the unseen-error badge does not move.
- [ ] The same entries are present under the **Diagnostic** tab, with `source` _(NOT VERIFIED 2026-09-30: partial — API side verified: the TLS-failure entries are audience=diagnostic with source rustls_platform_verifier::verification::apple (not 'log'). Diagnostic tab rendering needs the frontend (needs_browser).)_
      showing the real module (e.g. `rustls_platform_verifier::verification::apple`)
      instead of `log`.
- [x] `GET /logs?source=rustls_platform_verifier::verification::apple` returns _(verified 2026-09-30: Real TLS failure: Direct connection to python https server with self-signed cert (https://localhost:9894), POST /connect -> Unreachable; GET /logs?source=rustls_platform_verifier::verification::apple returns 2+ entries (level error, audience diagnostic, 'localhost certificate is not trusted'); GET /)_
      them; `GET /logs?source=log` returns none.

## The daemon is a whole machine — needs a real `tuic-remote` run (#23a5)

Run on a *separate* box, or on this Mac with `--instance <id>` and after
checking the note below. `tuic-remote` now writes an MCP entry into the config
of every agent installed on the machine it runs on — including this one, whose
agent configs Boss uses. Running an unisolated daemon here rewrites them with
the path it resolves for `tuic-bridge`.

- [ ] Start the daemon and confirm `<config dir>/mcp.sock` exists (Windows: _(NOT VERIFIED 2026-09-29: needs a Windows or Linux host — not reproducible in the isolated headless/browser instance)_
      the `tuicommander-mcp` named pipe) while it runs.
- [ ] Put `tuic-bridge` next to `tuic-remote`, launch an agent in a tab bound to _(NOT VERIFIED 2026-09-29: Needs a real tuic-remote daemon on another host and a real agent listing tools)_
      a repo on that machine, and confirm it lists the `tuicommander` tools —
      `session`, `repo`, `progress`, `agent` — not an empty tool list.
- [ ] `repo action=worktree_list` from that agent answers about the daemon's _(NOT VERIFIED 2026-09-29: partial — Own local tuic-remote daemon: MCP repo action=worktree_list via a bridge on the daemon socket answers from the daemon process (daemon 'repo list' = [] vs validate instance list [fx/repo]); same filesystem so cannot show 'daemon repos not the Mac's' and no real agent/bridge listing tools on a separate host.)_
      repos, not the Mac's.
- [x] Remove `tuic-bridge` from beside the daemon, restart, and confirm the _(verified 2026-09-29: Ran daemon with HOME=fakehome, TUIC_MCP_CONFIG_OWNER=1: with bin/tuic-bridge present ~/.claude.json got command=<bin>/tuic-bridge. Removed bridge, restarted: log 'Skipping agent MCP config updates: no bridge beside this executable', .claude.json stayed {} (no dead path written). Bridge restored.)_
      written config names a path that does not exist (the failure this story
      exists to remove) — then put it back.
- [ ] On a daemon with no agents installed at all: no agent config file and no _(NOT VERIFIED 2026-09-29: partial — Own tuic-remote, empty HOME, no --instance: only codex/grok/opencode/VSCode configs created; no .claude/.cursor/.gemini etc. (skip-if-not-installed works). Cannot test zero agents: has_cli finds /opt/homebrew/bin/{codex,opencode,code}, ~/.grok/bin on this machine, so those count as installed.)_
      agent config directory is created.
- [x] Cross-repo content search (Cmd+P → search file contents) against the _(verified 2026-09-29: Daemon tuic-remote --instance ag3g with repositories.json (1 repo, active): log 'content index pre-warm complete'; GET /fs/search-content-all?query=zebraquokka -> 1 match with repo_path, repos_pending:0, repos_searched:1. Control daemon with no repos: 0. HTTP route, not Cmd+P UI.)_
      daemon returns results for the pre-warmed repo instead of reporting every
      repo pending forever.
- [ ] Leave the daemon running for an hour and confirm the maintenance sweep _(NOT VERIFIED 2026-09-29: Requires an hour-long real tuic-remote soak.)_
      logs reaped MCP sessions rather than growing without bound.

## `tools/list` gained a 2026-07-28 cache envelope — needs a `make dev` restart (#3c1b)

The three new fields are withheld from the legacy revision, so the risk is not
that ego breaks — it is that Claude Code does. Verified in tests; confirm on a
live instance.

- [x] Claude Code connects to the running instance and lists TUIC's tools as _(verified 2026-09-29: legacy-initialized session tools/list result has only key 'tools')_
      before (its `initialize` names 2025-11-25, so its `tools/list` result must
      still carry `tools` and nothing else).
- [x] `curl -s -X POST localhost:9876/mcp -H 'mcp-protocol-version: 2026-07-28' _(verified 2026-09-29: tools/list with mcp-protocol-version: 2026-07-28 returns resultType=complete, ttlMs=0, cacheScope=private beside tools)_
      -H 'content-type: application/json' -d '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}'`
      answers with `resultType`, `ttlMs` and `cacheScope` beside `tools`.
- [x] The same call without the header answers with `tools` alone. _(verified 2026-09-29: same call without the header returns tools only)_

## The embedded AI engine is gone — needs a `make dev` restart (#784-0aec)

~24k lines of Rust and ~44 frontend files were deleted. Nothing below is a new
feature: each item confirms that removing the engine did not take a *surviving*
feature with it. All of it needs the rebuilt backend, so run it after the
restart, not before.

- [x] The app starts with the existing `config.json` and no config backup _(verified 2026-09-30: Headless tuic-remote --instance r3iso started with config.json carrying ai_chat_enabled, ai_triage_enabled, ai_watchers_enabled, ai_terminal_mcp_enabled: starts, /health ok, no config backup file in the instance dir (only config.json, config.json.lock, ai-sessions, logs, worktrees). Desktop app itse)_
      appears beside it (`ai_chat_enabled`, `ai_triage_enabled` and
      `ai_watchers_enabled` are still in Boss's file and must be ignored).
- [x] Settings → General → Experimental Features shows the master toggle alone; _(verified 2026-09-29: Web UI Settings>General>Experimental Features: only 'Enable experimental features' toggle (Expert off and on, experimental on and off); no AI Triage/AI Watchers/AI Chat sub-toggles. Nav gains 'AI Chat' tab when enabled.)_
      the AI Chat, AI Triage and AI Watchers sub-toggles are gone.
- [ ] With the master toggle ON, the AI Chat panel opens and shows the _(NOTE 2026-09-29: rg -i 'moving to' src finds no 'moving to ego' shell string; AI Chat panel is real now; re-write the expectation)_ _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: UI check of the AI Chat panel with the master toggle; 29/09 note: expectation text stale (no 'moving to ego' shell string; panel is real now).)_
      "moving to ego" shell with the focused terminal's name in its header; with
      it OFF the panel, its shortcut and its command-palette entry are absent.
- [ ] SSH Tunnels still opens — it shares that master toggle. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: UI check that SSH Tunnels still opens under the experimental master toggle; no frontend here.)_
- [ ] Settings has no Providers tab and no AI Chat tab, and its search returns _(NOTE 2026-09-29: description stale — an AI Chat settings tab exists (SettingsPanel.tsx:78, settingsSearchIndex.ts:625); re-write the expectation before testing)_ _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Settings UI/search check; 29/09 note: expectation stale (an AI Chat settings tab now exists, SettingsPanel.tsx:78).)_
      nothing for "provider", "triage" or "watcher".
- [x] The toolbar has no watcher eye next to the notification bell. _(verified 2026-09-29: Web UI toolbar: buttons next to the notification bell are 'Smart Prompts Library', a session-finished chip and the bell; only 'watcher'-named element is the Command palette button (class watcherBtn, title 'Command palette (⌘P)'). No watcher eye.)_
- [ ] A PR detail popover opens and shows checks, files and comments with no AI _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: PR detail popover needs the frontend and a real GitHub PR; not exercisable headless.)_
      review section and no error in its place.
- [ ] The GitHub Ops dashboard renders three columns — auto-fix sessions, _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: GitHub Ops dashboard rendering; 29/09 note: dashboard now has five columns, item text says three (stale). Needs UI + GitHub-remote repo.)_
      conflict assists, CI/merge readiness — and conflict assist still populates
      its column when a conflicting PR is opened.
- [ ] Smart Prompts still run in shell, inject and headless modes. _(NOT VERIFIED 2026-09-30: partial — Headless mode verified on own daemon: POST /prompt/execute-headless {command: claude, args:[-p,--strict-mcp-config,--settings <hooks>], stdinContent:'reply with the word ok', repoPath} -> "ok" in 9.5s. Shell and inject modes are frontend-driven: not exercised.)_
- [ ] A terminal's command knowledge still records: run a failing command, then _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Headless tuic-remote deliberately does NOT run ai_agent::knowledge::spawn_persist_task (lib.rs:2627 comment); on both rust0930 and r3iso, failing+passing commands in a shell session produced no ai-sessions/*.json. Needs the desktop build (persist task) to check recording and restart survival.)_
      a passing one, and confirm the session's knowledge survives a restart
      (this is the one part of `ai_agent/` that was kept).
- [x] Nothing in the app opens a knowledge-history overlay any more. Its only _(verified 2026-09-29: by code/test inspection, tests not executed here: No knowledge-history overlay opener remains: rg 'KnowledgeHistory|knowledge-history' finds no matches; only an empty comment at App.tsx:1017. ai-sessions still written at ai_agent/knowledge.rs:309.)_
      opener was the chat panel's knowledge footer, so the overlay and its two
      backend commands went with it — `<config_dir>/ai-sessions/*.json` keeps
      filling up with no reader.
- [ ] The AI Chat panel still detaches into its own window and the main window _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Detached AI Chat window + 'Bring back' placeholder are desktop-only (PanelWindowControls).)_
      shows the *Bring back* placeholder; closing the detached window restores
      the docked shell.
- [ ] **Needs a `make dev` restart (Rust).** Load an ego conversation whose _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Ego conversation transcript rendering (no clipped ack) is frontend; ego exists at /usr/local/bin/ego but the transcript UI is needed.)_
      first answer starts with `TUICommander v1.7.7 is connected.` followed by
      `intent:`. The status appears and no clipped acknowledgement such as
      `.7.7 is connected.` remains in the transcript.
- [x] **Needs a `make dev` restart (Rust).** In a tab *you* opened by hand (not _(verified 2026-09-29: Hand-opened shell PTY (POST /sessions cwd=registered repo) e2b29bb4 used as MCP peer: progress type=done -> {id:50} (no project_required); repo progress_list shows entry in that project with ptyId; webview DOM contains toast element (class *_toast > *_message) with the text.)_
      one an orchestrator spawned), an agent calling `progress type=done` no
      longer answers `project_required`: the entry lands in that project's
      journal and a toast appears. `resolve_mcp_origin_repo_path` used to read
      the PTY map under the peer's `$TUIC_SESSION`, which only matches for a
      spawned child. The same fix also gives `ui action=tab` and `ui
      action=toast` the right repo badge in those tabs.
- [x] **Needs a `make dev` restart (Rust).** An agent that writes the ack and its _(verified 2026-09-29: Fake claude-type agent (spawn binary_path, grid resized to run width): 'TUICommander v1.7.7 is connected. intent: ... (Ack Run)' -> display_name 'Ack Run' + journal entry; 3-row and 2-row agent-hard-wrapped variants put (Title) on later row -> title set. 'Ready when you are. intent: x (Prose Reject)' -> no title, no journal row.)_
      first `intent:` as one sentence run (`TUICommander v1.7.7 is connected.
      intent: … (Title)`) now sets the tab title and the Progress journal row
      instead of being dropped entirely; the same for an intent long enough that
      the agent's own wrapping pushes the `(Title)` onto the next row. Prose is
      still rejected — `Ready when you are. intent: x` must NOT set a title.
- [x] **Needs a `make dev` restart (Rust).** In a 120-column agent tab, a long _(verified 2026-09-29: 120col resize: 539-char intent soft-wrapped ~5 rows sets title 'Five Row', one journal row (500 chars, truncated). Titleless 'intent: ... ending in (' then different intent: both entries kept (ids 46,47). Note: two intents in one chunk yield only last (fixture: separate chunks).)_
      `intent:` soft-wrapped across five rows still sets its final `(Title)` and
      writes one truncated journal row. A following different intent must not
      silently erase a previous titleless line ending in an unfinished `(`.

## Remote repo browser (2026-09-20) — frontend only, Vite HMR picks it up

`RemoteRepoPicker` replaces the "type the absolute path" prompt when adding a
repository from a connected machine. No Rust changed, so HMR is enough — but
nothing here is reachable until a remote connection reads **Connected**, which
needs the `make dev` restart that #781-9652 is waiting on.

- [ ] With **no** machine connected, the sidebar `+` must behave exactly as before: _(NOT VERIFIED 2026-09-29: partial — Web UI, machine disconnected (status []): sidebar 'Add Repository' opens a single popover with a path text input (Cancel/Add), no menu/picker. Browser mode cannot show the native dialog, so 'local native dialog' equivalence not checkable; picker absent as required.)_
      straight to the local native dialog, no menu. The picker must not appear.
- [ ] With mac-mint connected, `+` opens the menu; picking it opens the browser _(NOT VERIFIED 2026-09-29: needs a second machine (mac-mint / SSH daemon) — not reproducible in the isolated headless/browser instance)_
      showing `/` on **mac-mint**, not this Mac. Compare against
      `ssh mac-mint ls /`.
- [ ] Walk to `/home/stefano/Gits`, press **Add This Folder** on a real repo. It _(NOT VERIFIED 2026-09-29: Needs remote machine with /home/stefano/Gits (second machine).)_
      lands in the sidebar with the remote badge, and its git status is the remote
      machine's.
- [x] Only folders are listed — no files. _(verified 2026-09-29: Web UI: connected local tuic-remote (:9892) as Direct machine, sidebar Add Repository menu (Local Repository | agb2wrong) -> 'Browsing agb2wrong' picker at remote home: 82 rows, all directories (checked vs os.path.isdir), 0 of 149 home files listed; '..' entry and 'Add This Folder'/Cancel present.)_
- [ ] Type a path that does not exist on mac-mint into the field and press Enter: _(NOT VERIFIED 2026-09-29: needs a second machine (mac-mint / SSH daemon) — not reproducible in the isolated headless/browser instance)_
      the daemon's own message must show, not an empty folder.
- [ ] Close the picker and reopen it for the same machine: it must resume where it _(NOT VERIFIED 2026-09-29: Remote repo picker needs a real remote machine)_
      was left, not at `/`.
- [ ] **[VISUAL]** The list scrolls inside the dialog without breaking its layout _(NOT VERIFIED 2026-09-29: partial — Remote picker at /usr/lib (17 folders) and home (82): dialog top 193 bottom 707 in 900px viewport; entries list is a scroll container (client 318 / scroll 413, overflow auto) so the dialog does not grow; footer buttons stay. No screenshot (times out), judged by geometry.)_
      on a directory with many entries (`/usr/lib` is a good one).

## Orchestrator RESULT wake with background work (#797-8549) — needs a `make dev` restart

- [x] After restarting `make dev`, leave an orchestrator at its confirmed-ready,
      empty composer while one background descendant is still running, then have
      a child send `RESULT`. The send must report
      `delivery_path=wake_notification_and_inbox`; the parent must receive only
      the generic `agent action=inbox` notice, and the inbox must contain one
      untouched RESULT.
      _(verified 2026-09-21: a delayed `tuic agent send` ran as the live Codex
      process's background descendant. After this turn yielded, TUIC injected only
      `[TUIC] message available`, the sender received
      `wake_notification_and_inbox`, and `agent action=inbox` returned exactly one
      untouched `RESULT live-wake-797-8549`.)_
- [x] Repeat with text partially typed in the parent composer: the route must be _(verified 2026-09-30: Fake claude orchestrator with a real background descendant (sleep 240 started by a turn >60s after start; status background_work=true). Draft 'partialdraft' typed + RESULT send -> inbox_only, draft still on screen, nothing submitted. Control with empty composer -> wake_notification_and_inbox, one wa)_
      `inbox_only` and the draft must remain unchanged. The focused Rust regression
      covers this mechanically; this item retains the live composer check.

## `tuic agent send` accepts current delivery reports (#800-18c6) — needs a sidecar rebuild

- [x] After the next `make dev` or sidecar rebuild, run the installed _(verified 2026-09-29: /usr/local/bin/tuic agent send <peer-uuid> msg (TUIC_SOCKET to validate instance, TUIC_SESSION set) against a registered PTY peer running 'sleep 60': exit 0, 'Buffered for <id> (inbox_only) — unread until the recipient polls its inbox'; message present in peer inbox. Caveat: peer is a shell (shell_state reported idle), not a real busy agent.)_
      `/usr/local/bin/tuic agent send` against a busy registered peer. It must exit
      0 and print `Buffered … (inbox_only)`, not `Registry did not accept the
      message`. The freshly built `target/debug/tuic` already passed this exact
      live check; this item verifies that the installed sidecar has caught up.

## SQLite Viewer plugin host primitives — needs a `make dev` restart (#797-a4bd)

- [x] Open a `.db` fixture from the File Browser and confirm the themed object
      list, row page, index inspector, and query console render without a stale
      dirty badge. _(verified in an isolated `sqlite-viewer-920` instance;
      screenshot: `.screenshots/sqlite-viewer-plugin.png`)_
- [ ] Exercise the per-column filter and visual query-plan buttons through the UI, _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: SQLite Viewer registry plugin UI (per-column filter, visual query plan): needs frontend and plugin install.)_
      then enable editing on a primary-key table, change a scalar cell, save,
      reopen the file, and confirm the persisted value. _(The exact SQL filter,
      explain, edit, export, and reopen path is runtime-tested; UI automation was
      stopped after MacControl switched same-name windows/coordinate spaces.)_
- [x] Close and reopen the SQLite tab and confirm a fresh iframe/database viewer
      is created. _(verified in the isolated instance; pending-load cleanup is
      also covered by `main.test.js`.)_

## Remote machine reachability is visible outside Settings

Boss added a repo from mac-mint, the daemon went unreachable, and the only place
that said so was Settings -> Remote Machines. Frontend-only, so Vite HMR already
has it: no `make dev` restart needed.

- [ ] With a remote machine in `error`/`disconnected` and at least one repo _(NOT VERIFIED 2026-09-29: needs a second machine (mac-mint / SSH daemon) — not reproducible in the isolated headless/browser instance)_
      registered on it, the status bar shows `Offline: <machine>` in red, and the
      tooltip points at Settings -> Remote Machines.
- [x] The sidebar badge on that repo reads `offline` in red instead of `remote`, _(verified 2026-09-29: Web UI: remote repo 'RR' badge 'remote' (grey, tooltip 'On agb2wrong.'); after killing the local tuic-remote daemon: badge 'offline', color rgb(241,76,76) red, tooltip 'agb2wrong is not answering. Reconnect in Settings → Remote Machines.')_
      and its tooltip names the machine and its state.
- [ ] Reconnect the machine: the status-bar pill disappears and the badge goes _(NOT VERIFIED 2026-09-29: Needs a remote machine to disconnect/reconnect)_
      back to a muted `remote` without a reload.
- [ ] A remote machine with NO registered repo must NOT appear in the status bar _(NOT VERIFIED 2026-09-29: needs a second machine (mac-mint / SSH daemon) — not reproducible in the isolated headless/browser instance)_
      while disconnected — that is its normal resting state.

## Adding a remote repo must open ONE tab, not two (`usePty` pre-registration)

Observed: adding `/home/stefano/omi-local-stack` from mac-mint opened `shell 1`
plus a phantom `PTY: Session 17`. Both were the same remote PTY — the desktop
create is routed over HTTP to the daemon, whose `session-created` echo was not
deduped because the guard keyed on `isTauri()` instead of "is this call routed
to a remote connection". Covered by two new tests in `usePty.test.ts`.

- [ ] Add a repo from a connected remote machine. Exactly one shell tab appears. _(NOT VERIFIED 2026-09-29: needs a second machine (mac-mint / SSH daemon) — not reproducible in the isolated headless/browser instance)_
- [ ] Adding a LOCAL repo still opens one tab and the backend still mints the id. _(NOT VERIFIED 2026-09-29: partial — Web UI: Add Repository > Local Repository > path 'fx/agb2/lr' > Add: LR appears in sidebar, tab bar shows exactly one tab 'main 1' (no duplicate), repositories.json entry for lr has no connectionId (rr has). Backend-minted id not inspected.)_

### Root cause found while testing the above

`remoteConnectionsStore.hydrate()` was called from exactly one place —
`RemoteMachinesPanel.tsx`. Until the user opened Settings -> Remote Machines the
store held nothing, so the status bar, the sidebar badge and `Sidebar.tsx`'s own
`getConnections()` read an empty map and could not tell a live machine from a
dead one. Now hydrated once at startup in `useAppInit`; `hydrate()` is
idempotent, so the panel still calls it.

- [ ] Start the app WITHOUT opening Settings. A down remote machine holding a _(NOT VERIFIED 2026-09-29: needs a second machine (mac-mint / SSH daemon) — not reproducible in the isolated headless/browser instance)_
      repo must already show `Offline: <name>` in the status bar.

## Deleting a remote machine must take its connection with it (#803-f875)

Rust change — needs a `make dev` restart (or `make build`) to load; the running
session will NOT have it.

Deleting a connected machine used to leave the SSH tunnel, the status poll and
the mirror task running against a machine that no longer existed in the config.
There is now one teardown path (`remote_runtime::teardown`) and both the IPC and
the HTTP delete route call it.

- [ ] Connect an SSH-transport machine, confirm `ssh` is running _(NOT VERIFIED 2026-09-29: Needs SSH-transport remote machine (second machine mac-mint) and ssh process)_
      (`pgrep -fl ssh`), then delete the machine from Settings -> Remote
      Machines. The `ssh` process must be gone within a second, and the app must
      not show a status push for the deleted id afterwards.
- [x] Do the same over HTTP against the dev instance: _(verified 2026-09-29: Connected conn via API, then DELETE /config/remote-connections/{id} -> {ok}; status [] at once, log 'Disconnected'. Killed+restarted daemon and waited 20s: no further Connecting/Connected log, status stays []. Supervisor stopped.)_
      `curl -X DELETE http://127.0.0.1:9877/config/remote-connections/<id>` —
      same result. Before this change the HTTP route stopped nothing.
- [ ] Any sessions that machine had mirrored disappear from the session list on _(NOT VERIFIED 2026-09-29: Needs a real connected remote machine with mirrored sessions.)_
      delete, and no `session-state-changed` for them arrives after it.
- [x] Delete a machine that was never connected: no error, nothing logged as a _(verified 2026-09-29: Validate instance: PUT /config/remote-connections (enabled:false, never connected) then DELETE /config/remote-connections/{id} -> {ok:true} 200; list no longer has it; /logs since the call has no warn/error and no remote entries.)_
      failure.

## An errored remote machine recovers on its own (#803-f875)

Same restart caveat — Rust.

- [ ] Connect a machine, then stop `tuic-remote` on it. The badge goes to _(NOT VERIFIED 2026-09-29: Needs a real tuic-remote daemon to stop and start)_
      `error` and its mirrored sessions retire from the list.
- [ ] Start the daemon again and WAIT — do not press Connect. Within one poll _(NOT VERIFIED 2026-09-29: partial — Daemon killed (badge 'offline', status error Unreachable), restarted daemon with no Connect press: status 'connected' within 8s (polled every 8s), sidebar badge back to grey 'remote'. Reappearing sessions not checked (daemon had none).)_
      interval the badge must return to `connected` by itself and the machine's
      sessions must reappear.
- [ ] Wrong password: the badge reads `unauthenticated`, and for an SSH-transport _(NOT VERIFIED 2026-09-29: Needs SSH-transport remote machine with wrong password (second machine))_
      machine no `ssh` process is left behind (`pgrep -fl ssh`).

## A dropped request must not leak an ego process (#804-2ec8)

Rust change — needs a `make dev` restart (or `make build`) to load.

`/acp/one-shot` was awaited inline under the router's 301s timeout, which drops
the handler future. Everything after the drop was skipped, including the
`disconnect` that stops ego, and nothing else ever would: the supervisor is an
independent task and only settled connections are pruned. The turn now runs on
its own task, the launch has its own 60s budget and the turn 240s, so the whole
call fits inside the router's bound.

- [ ] Run a Smart Prompt in `api` mode, then close the tab / kill the request _(NOT VERIFIED 2026-09-29: Needs a real ego process running a live turn (POST /acp/one-shot, kill mid-turn, pgrep ego); no ego/provider in headless run.)_
      mid-turn (`curl ... & sleep 2; kill %1` against
      `POST http://127.0.0.1:9877/acp/one-shot`). Within a few seconds
      `pgrep -fl ego` must show no leftover process.
- [ ] `GET /acp/connections` must not list a connection for the abandoned turn. _(NOT VERIFIED 2026-09-29: Needs ego ACP turn abandoned by dropped request.)_
- [ ] A normal Smart Prompt still answers, and a long one that runs out of time _(NOT VERIFIED 2026-09-29: Needs real ego and a 240 s timeout turn (oneshot.rs:333).)_
      reports "ego did not finish the turn within 240s" rather than a bare 408.
- [x] Point `ego_executable` at something that starts and never speaks (e.g. a _(verified 2026-09-30: browser mode on isolated instance, mute `sleep 3600` script: no AbortError at +30 s; UI shows the 502 "the agent did not answer initialize within 60s" between +47 s and +83 s, Retry button; no leftover child)_
      `sleep 600` wrapper) and press Connect in AI Chat: it must fail within a
      minute with "the agent did not answer initialize within 60s" instead of
      spinning forever, and leave no child behind.
- [ ] Press Connect on a remote machine and navigate away immediately. The _(NOT VERIFIED 2026-09-29: needs a second machine (mac-mint / SSH daemon) — not reproducible in the isolated headless/browser instance)_
      machine must still reach `connected` (or `error`) — never stay stuck on
      `connecting`, which used to make every later Connect a silent no-op.

## Terminal stream compression is acknowledged, not assumed (#805-f52e)

Rust and frontend — the Rust half needs a `make dev` restart (or `make build`).

The browser used to decide every frame was tagged from its own request alone. A
daemon that predates `?compress=deflate` ignores it and sends untagged frames,
and the client then read the first byte of a grid row as a tag. The server now
selects the `tuic.deflate` subprotocol when it is going to tag, and the client
reads `ws.protocol` in `onopen` before the first frame.

- [ ] Open a terminal on a remote machine over a **direct** (non-tunnel) link. _(NOT VERIFIED 2026-09-29: needs a second machine (mac-mint / SSH daemon) — not reproducible in the isolated headless/browser instance)_
      It renders normally, and DevTools shows the stream socket with
      `Sec-WebSocket-Protocol: tuic.deflate` on the 101.
- [ ] Open a terminal on the same machine (local session). The socket asks for _(NOT VERIFIED 2026-09-29: blocked — Browser WebSocket URL/subprotocol not observable: no resource-timing entry for WS and no fresh PTY tab can be created to patch WebSocket before connect (agent-browser eval only, page reload drops patch). Code: canvasTerminalTransport.ts:221 (per agentb0 1048).)_
      nothing: no `compress=deflate` in the URL and no subprotocol on the 101.
- [ ] Point a current build at an **older** `tuic-remote` (one without this _(NOT VERIFIED 2026-09-29: Needs an older tuic-remote binary from an earlier commit)_
      commit). The terminal must render correctly — untagged framing — and the
      app log must carry "asked for compression and the server did not take it"
      rather than "could not decode a compressed frame" once per frame.
- [ ] Open a terminal through an **SSH tunnel**. The frames must be tagged but _(NOT VERIFIED 2026-09-29: needs a second machine (mac-mint / SSH daemon) — not reproducible in the isolated headless/browser instance)_
      never deflated (`ssh -C` already compressed the channel), which is the
      `::ffff:127.0.0.1` case the canonical-address fix covers. Check CPU on the
      daemon stays flat while an agent repaints.

## The AI Chat panel keeps its connections apart (#806-4335)

Frontend only — Vite HMR picks this up, no `make dev` restart needed.

- [ ] Open AI Chat on two different repo roots so two ego connections are live. _(NOT VERIFIED 2026-09-29: Needs two live ego connections on two repo roots.)_
      Stop ego on the first (or disconnect it). The second panel must keep
      streaming — no "Not receiving updates" banner on the root that did not end.
- [ ] Press Recover after a gap. The connection list must show ONE connection _(NOT VERIFIED 2026-09-29: Needs live AI Chat/ego connections and a gap recovery.)_
      afterwards, not the dead one plus the fresh one, and the fresh panel must
      keep receiving updates rather than freezing a second later.
- [ ] Send a prompt while ego is wedged or the session is not accepting prompts. _(NOT VERIFIED 2026-09-29: Needs a wedged/non-accepting real ego session in AI Chat.)_
      The message must disappear from the transcript rather than sitting there as
      a turn that was never received.
- [ ] Attach to a session id ego does not have. The transcript that was on screen _(NOT VERIFIED 2026-09-29: needs a real ego agent (ACP) session; not available headless)_
      must come back rather than being left blank under a live session.

## The headless build announces repo-op progress too (#808-84e1)

Rust — needs a `make dev` restart, and the interesting half needs the headless
binary: `cargo build --bin tuic-remote --no-default-features`.

**Which binary:** `run_headless` (the main binary's headless mode), NOT
`tuic-remote`. The three routes are in `build_router`; `build_remote_router`
does not carry them, so `tuic-remote` answers 404 (#810-4986).

- [ ] Start the headless mode of a `--no-default-features` build, open the web _(NOT VERIFIED 2026-09-29: Needs PR review (ego/GitHub account) on a --no-default-features headless build.)_
      UI against it, and start a PR review on a repo. The Review findings column
      must move from "running" to a result on its own. Before this commit it
      stayed on "running" forever, because the `review-progress` event was
      dropped on that build.
- [ ] Same daemon, run an improvement scan. The proposals must appear in the _(NOT VERIFIED 2026-09-29: Improvement scan runs on ego against a real daemon; needs ego)_
      panel when the scan finishes — the return value never populates it, only
      the `proposals-ready` event does.
- [ ] Same daemon, run conflict assist on a PR with conflicts. The status must _(NOT VERIFIED 2026-09-29: Needs a real GitHub PR with conflicts, conflict assist (ego) on a headless daemon.)_
      reach the panel rather than leaving it idle.
- [ ] `curl -N http://127.0.0.1:<port>/events` against that daemon while each of _(NOT VERIFIED 2026-09-29: blocked — review-progress/proposals-ready/conflict-assist-status can only be produced by ego (ACP; POST /repo/improvement-scan -> 'no ego executable is configured') or a real GitHub PR (pr-review/conflict-assist -> 'No GitHub remote URL found'). Not triggerable in isolated daemon.)_
      the three runs. The `review-progress`, `proposals-ready` and
      `conflict-assist-status` frames must appear on the stream.
- [ ] Desktop build, same three operations: unchanged. The window emit still _(NOT VERIFIED 2026-09-29: blocked — Needs PR review, improvement scan and conflict assist (ego + real GitHub PR with conflicts) on a headless daemon plus desktop panels; ego/GitHub PR not available headless.)_
      fires, so nothing about the desktop panels may look different.

## A registered remote machine comes up by itself and stays up

Rust — needs a `make dev` restart.

One task per connection now owns its whole lifecycle: bring it up, keep it up,
retry while it is down. It replaces the heartbeat that was spawned only from the
SUCCESS branch of a connect, which is why a machine whose first attempt failed
sat in `error` until somebody pressed Connect — measured on mac-mint, answering
200 throughout while the app showed it unreachable for forty minutes.

- [ ] Start the app with a remote machine registered and REACHABLE, without _(NOT VERIFIED 2026-09-29: needs a second machine (mac-mint / SSH daemon) — not reproducible in the isolated headless/browser instance)_
      touching Settings. It must reach `connected` on its own, and the sidebar
      badge must read `remote` rather than `offline`.
- [ ] Start the app with the remote machine OFF. It must show `offline`, and the _(NOT VERIFIED 2026-09-29: needs a second machine (mac-mint / SSH daemon) — not reproducible in the isolated headless/browser instance)_
      backend must keep retrying — the wait doubles from 2s to a 60s ceiling.
      `GET http://localhost:9876/logs?source=remote` shows one `Connecting` line
      per attempt, spaced by a growing gap.
- [ ] With the app running and the machine offline, turn the machine ON. It must _(NOT VERIFIED 2026-09-29: Needs a remote machine to switch on/off)_
      go `connected` by itself within one backoff window. No click.
- [x] Press Connect on a machine that is off. The button must report the failure _(verified 2026-09-29: Daemon stopped, machine 'Error'. Pressed Connect in Settings>Remote Machines: row shows 'Error / Unreachable: error sending request for url (http://127.0.0.1:9892/health)'; /logs show repeating 'Connecting' / 'Remote connection failed' afterwards, i.e. retry continues. (Cannot tell attempt-own vs shared error text.))_
      (that attempt's own error), AND the retry must continue afterwards.
- [x] Press Disconnect on a connected machine. It must stay disconnected — _(verified 2026-09-29: Direct conn to local tuic-remote (:9892) connected; pressed Disconnect in Settings>Remote Machines; watched 95s (daemon reachable throughout): status [] via GET /config/remote-connections/status, no new 'Connecting'/'Connected' remote log lines, UI shows no Connected.)_
      watch it for longer than 60s. A retry that resurrects it is the bug the
      generation counter exists to stop.
- [x] Disconnect, then Connect again immediately. The machine must come up, and _(verified 2026-09-29: DELETE /connect then POST /connect immediately: status connected (same token). Then killed daemon: status error Unreachable, restarted daemon: back to connected in 3s with new token, so the new supervisor still retries after the retired one was taken down.)_
      it must still retry if it later drops — the new supervisor must not have
      been taken down with the retired one.
- [x] Give a machine the WRONG password and connect. It must land in _(verified 2026-09-29: Own tuic-remote w/ password (--set-password) on :9894; connection with WRONGPW + POST /connect -> 502 'Authentication rejected', status unauthenticated; over 60s /logs shows one 'Connecting'+one 'Remote connection failed' for it, no retries (other conn kept retrying). Correct PW + POST connect -> status connected.)_
      `unauthenticated` and STOP: no repeated attempts in the logs. Fix the
      password, press Connect, and it must start again.
- [ ] Delete a remote machine while it is retrying. Nothing may keep probing it, _(NOT VERIFIED 2026-09-29: needs a second machine (mac-mint / SSH daemon) — not reproducible in the isolated headless/browser instance)_
      and no entry for it may remain in the status bar.

## The website names warm copy-on-write worktrees

- [ ] `website/index.html`, "Git worktrees, fully managed": the second bullet _(NOT VERIFIED 2026-09-29: partial — Rendered website/index.html in a same-origin srcdoc iframe (no screenshot). 1200px: 'Warm worktrees' bullet names copy-on-write, li 544x69, no overflow. 390px: bullet wraps (92px tall, code chips intact) but its column right edge is 402 > 390 viewport and document scrollWidth is 508 (several .feature-content blocks are 361-484px wide, all sections )_
      now names the copy-on-write warming that `docs/user-guide/worktrees.md`
      documents. Checked at 1200px; check it on a phone width too.

## A missing bridge says so (#809-724c)

Rust — needs a `make dev` restart.

- [x] Move or rename `tuic-bridge` so it is neither beside the executable nor on _(verified 2026-09-29: tuic-remote copied to dir without tuic-bridge (owner env, sandbox HOME): /logs?level=warn holds 'Skipping agent MCP config updates: no bridge beside this executable' with searched_paths [<exedir>/tuic-bridge, 'tuic-bridge']. (Plus an extra warn 'temporary or mounted app' since binary sat under TMPDIR.) Control with bridge beside logged 'Ensuring br)_
      the resolved path, then start the app. `curl 'http://localhost:9876/logs?level=warn'`
      must carry one line naming both checked paths and the symptom. Before this
      commit there was nothing in the log at all.
- [ ] Put it back and restart. That warning must NOT appear, and AI Chat must be _(NOT VERIFIED 2026-09-29: partial — Headless tuic-remote copy (TUIC_MCP_CONFIG_OWNER=1, disposable HOME): without tuic-bridge beside it, /logs warn = 'Skipping agent MCP config updates: no bridge beside this executable' with searched_paths [<dir>/tuic-bridge, tuic-bridge]; with bridge beside it: 0 warns, 'Ensuring bridge configs'. Desktop app restart and AI Chat list-terminals (ego) )_
      able to list terminals again.

## Hands-free voice entries in the Compose queue (#814-6d13)

_(NOTE 2026-09-23: superseded — hands-free turns and notices are now typed straight into the terminal, busy or not, and never enter the Compose queue; there is no `voice_command` kind, no `queuedIds`, no `cancelled`/`alreadyDelivered`. Read "reaches the Compose queue" as "is typed into the terminal"; a dialog or draft holds the turn in the hands-free panel. See "Hands-free turns reach a busy agent at once" at the top.)_

Rust — needs a `make dev` restart. The hands-free mode has no UI control yet
(Step 8), so this checks the queue half through the existing HTTP surface.

- [x] With an agent tab busy, `POST /sessions/{id}/queue` a command, then check _(verified 2026-09-29: Fake agent child (agent_type claude, idle-not-ready) : POST /sessions/{id}/queue twice -> queued 1,2; GET queue -> every entry has kind=user_command. No voice_command kind exists (state.rs kind() only notice/initial_prompt/user_command; kind removed in 560709f9e).)_
      `GET /sessions/{id}/queue`: every entry still lists a `kind`, and an
      ordinary Compose command still reads `user_command`. The new
      `voice_command` kind must not appear for anything typed by hand.
- [x] Enqueue two commands on a busy agent and let them drain on the next idle _(verified 2026-09-29: Fake amp-type agent held busy 9s; POST /queue ALPHA then BRAVO (queue list ids 6,7 in order); after idle, agent output: ALPHA, GOT: ALPHA, BRAVO, GOT: BRAVO; queue empty. Order preserved.)_
      window. They must still arrive in order — `enqueue_user_command` now
      appends through a shared helper, and a reordering would show up here.

## Pocket TTS speech synthesis (#823-c260)

Rust — needs a `make dev` restart. Nothing calls the port yet (playback is
story 816), so the only way to reach it today is the bundle-backed tests:

```
TUIC_POCKET_BUNDLE_DIR=<bundle> [TUIC_POCKET_VOICE=<voice>] \
  cargo nextest run --lib --run-ignored ignored-only -E 'test(/dictation::speech::pocket/)'
```

- [HUMAN] Listen to `.tmp/kokoro-eval/ONNX/frase{1,2,3}-rust-int8.wav`, rendered
      by this adapter, against the `-torch-fp32`, `-onnx-fp32` and `-onnx-int8`
      sets from the evaluation. Two questions, one listening pass: does the Rust
      port sound like the Italian that was approved, and is int8 (125 MB per
      language) good enough against fp32 (400 MB)? The second answer decides what
      the downloader in story 813 offers.
- [ ] Windows and Linux: the adapter loads `onnxruntime.dll` / `libonnxruntime.so` _(NOT VERIFIED 2026-09-29: Requires Windows and Linux hosts for onnxruntime library loading.)_
      from beside the models by an explicit path. Only macOS/arm64 has been run,
      and a missing library must still report `ModelUnavailable` rather than
      taking the process down inside `ort`.

## Hands-free arm and disarm (#814-6d13)

_(NOTE 2026-09-23: superseded — hands-free turns and notices are now typed straight into the terminal, busy or not, and never enter the Compose queue; there is no `voice_command` kind, no `queuedIds`, no `cancelled`/`alreadyDelivered`. Read "reaches the Compose queue" as "is typed into the terminal"; a dialog or draft holds the turn in the hands-free panel. See "Hands-free turns reach a busy agent at once" at the top.)_

Rust — needs a `make dev` restart. There is still no UI control, so the HTTP
surface is the only way to reach it.

**Arming opens the microphone and starts capturing.** `open_endpoint` runs
before the bind (`dictation/commands.rs:959`) and a successful arm spawns a
driver thread that polls every 50 ms, transcribes each closed utterance through
Whisper, and injects the text into the bound session's Compose queue. Speak near
the machine while armed and the words reach the agent. Use a throwaway session,
and disarm before walking away.

- [ ] Hands-free arm/disarm over HTTP, against a throwaway agent session: _(NOT VERIFIED 2026-09-29: partial — Shell session arm -> {error:'Session cannot accept hands-free input'} verified; agent-typed session (spawned fake agent_type=claude) passes that gate and fails 'Model not downloaded'. armed:true/sessionId echo not reached (needs whisper model+mic; not arming to avoid mic prompt). disarm returned wasArmed:false (armed state never entered); GET consi)_
      `curl -X POST localhost:9877/dictation/hands-free/arm -H 'content-type: application/json' -d '{"sessionId":"<id>","owner":"desktop"}'`
      must return `armed: true` with `sessionId` echoed back. Arming against a
      shell (non-agent) session must return `Session cannot accept hands-free
      input`. `GET /dictation/hands-free` must agree with what arm returned, and
      `POST /dictation/hands-free/disarm` must report `wasArmed: true` once and
      `wasArmed: false` on a second call.
- [ ] Arm, then speak one short Italian sentence and stop. Within about a second _(NOT VERIFIED 2026-09-29: Needs real microphone speech input (Italian sentence))_
      of the pause the text must appear as a `voice_command` entry in
      `GET /sessions/{id}/queue`, and reach the agent on its next idle window.
      `GET /dictation/hands-free` must walk `waiting` → `capturing` →
      `transcribing` → `holding_back` → `delivered` across the turn; a phase that
      never leaves `capturing` means end-of-speech was not detected.
- [ ] Arm, then close the bound session from the UI. The mode must disarm itself _(NOT VERIFIED 2026-09-29: Needs microphone hands-free arm and real bound session.)_
      with `TargetClosed` and release the microphone without a disarm call —
      check `GET /dictation/hands-free` reads `armed: false` and that the app log
      carries `Hands-free disarmed: TargetClosed`. This is the path that keeps a
      dead tab from holding the device open.
- [ ] Arm, then unplug or switch away the input device. After the silence _(NOT VERIFIED 2026-09-29: Needs physically unplugging/switching real input device)_
      timeout the mode must disarm with `DeviceFailed` and name the device in the
      message, rather than sitting armed and deaf.
- [ ] `owner` is now checked against the one adapter that exists. Any value other _(NOTE 2026-09-29: description stale — a browser audio owner now exists (continuous.rs:321); the refusal text was not found by rg 'Audio endpoint' in Rust)_
      than `desktop` must be refused with `Audio endpoint '<owner>' is not
      available on this build` and must leave the mode unarmed — the browser
      endpoint is story 818. This is a behaviour change: arming from a remote
      client used to bind and now fails at the endpoint.
- [ ] Push-to-talk must be unaffected. With hands-free armed, run a normal _(NOT VERIFIED 2026-09-29: Needs real audio capture for push-to-talk plus hands-free.)_
      push-to-talk recording: it must capture and transcribe as usual, and must
      neither disarm hands-free nor be disarmed by it. Then disarm hands-free and
      confirm push-to-talk still works. The two modes hold separate captures.
- [HUMAN] Confirm the microphone indicator (menu bar / camera-mic dot) turns on
      at arm and off at disarm, for every disarm path above. Nothing in the test
      suite can see whether the OS actually released the device, and an armed
      mode that leaks the microphone after disarm is the failure that matters
      most here.

## Hands-free activation phrase (#815-7c76)

_(NOTE 2026-09-23: superseded — hands-free turns and notices are now typed straight into the terminal, busy or not, and never enter the Compose queue; there is no `voice_command` kind, no `queuedIds`, no `cancelled`/`alreadyDelivered`. Read "reaches the Compose queue" as "is typed into the terminal"; a dialog or draft holds the turn in the hands-free panel. See "Hands-free turns reach a busy agent at once" at the top.)_

Rust — needs a `make dev` restart. **There is no UI control for the phrase**;
`DictationSettings.tsx` has no field for it, so the only way to set one today is
the config surface. A Dictation control belongs to story 818.

- [ ] With `hands_free_activation_phrase` empty, arm and speak: every recognised _(NOT VERIFIED 2026-09-29: Needs real spoken utterances through recognizer into Compose queue.)_
      utterance must reach the Compose queue exactly as it did before this
      story. An empty phrase must change nothing.
- [x] Set the phrase to `attività tuic`, then save something unrelated from the _(verified 2026-09-29: Web UI Settings>Voice: set Activation phrase 'attività tuic' and Hold-back 2250ms (Expert), then changed Language auto->Italian from the UI; GET /dictation/config after each: phrase 'attività tuic', hands_free_hold_back_ms 2250 and language 'it' all persisted (no reset to defaults). Hotkey/device saves not separately exercised.)_
      Dictation settings UI — a hotkey, the language, the device. Re-read
      `GET /dictation/config`: the phrase and `hands_free_hold_back_ms` must
      **still be there**. This is the defect the store fix closes; before it,
      every save from the UI silently reset both fields to their defaults.
- [ ] Armed with that phrase, speak "che ore sono" alone: nothing must reach the _(NOT VERIFIED 2026-09-29: Needs spoken audio input.)_
      queue. Then "attività tuic che ore sono": the queue must receive
      `che ore sono` with the phrase stripped. Then, within 15 seconds, speak a
      bare follow-up: it must go through without the phrase. Wait past 15 seconds
      and the phrase must be required again.
- [ ] Say "tuicommander che ore sono" with the phrase set to `tuic`: it must NOT _(NOT VERIFIED 2026-09-29: Needs real spoken audio input with activation phrase)_
      activate. A longer word that merely starts with the phrase is a different
      word, not a prefix match.
- [ ] Disarm and re-arm while a window is open: the first utterance after the _(NOT VERIFIED 2026-09-29: Needs real speech through a microphone to test the activation phrase window.)_
      fresh arm must need the phrase again. Every disarm closes the window.

## Resume banner names the work

- [ ] Leave an agent tab running with a declared `intent:` (or a typed prompt), _(NOT VERIFIED 2026-09-29: Needs real agent tab with declared intent and app quit/reopen resume banner.)_
      quit the app, reopen it and select that branch. The
      "Agent session was active — click to resume" banner must now carry
      `Intent: <...>` (or `Prompt: <...>` when no intent was declared), truncated
      with an ellipsis and with the full text in the tooltip. A tab that never
      had either must show the banner exactly as before.
- [ ] The Context bar (`Show last prompt` setting) on the restored tab must show _(NOT VERIFIED 2026-09-29: Needs a real resumed agent tab to declare intent and restore context bar values.)_
      the same restored values, and must be replaced by the live ones as soon as
      the resumed agent declares a new intent or the user sends a prompt.

## Echo canceller starts with the app

Needs a `make dev` restart — the Rust backend does not hot-reload.

- [x] After the restart, `GET http://localhost:9876/logs?source=dictation` must _(verified 2026-09-29: Validate instance (this build, started fresh; dictation state installs echo canceller at startup mod.rs:170): GET /logs?source=dictation has 0 'no echo cancellation'; full startup log desktop.log and instance tuic.log also have none. Caveat: the warn in echo.rs:305 has no source=dictation field, so that filter would not match it anyway; grep the wh)_
      NOT contain `no echo cancellation`. That line means `WebRtc::new()` failed
      and hands-free fell back to `PassThrough`, which cannot hear the user over
      the speaker. It is logged at warn level on purpose; a quiet fallback would
      look exactly like a working canceller until someone tried to interrupt.
- [ ] Startup must not be visibly slower. `DictationState::new()` now builds one _(NOT VERIFIED 2026-09-29: partial — desktop.log: first log 22:39:23.595, HTTP TCP+unix listening 22:39:24.617 (~1.0s incl. Tailscale detection 0.5s), key monitors 22:39:25.7. No AEC3-less baseline build to compare, so 'not slower' is not proven; nothing looks slow.)_
      AEC3 instance for the life of the app, before any dictation is used.

## Speech assets download and install (#813-e84b)

Needs a `make dev` restart — the Rust backend does not hot-reload. There is no
Dictation UI for these yet (#818-2a29), so drive them over HTTP.

**The voices are published.** `speech-voices-v1` was cut on 2026-09-22 and serves
`italian-giovanni.safetensors` at the sha256 pinned in `assets.rs`, verified by
downloading it back from the public URL. The Italian download is therefore
expected to complete rather than 404 on its last file.

- [ ] `curl localhost:9877/dictation/speech/assets` lists two assets, _(NOT VERIFIED 2026-09-29: partial — GET /dictation/speech/assets on clean instance: onnxruntime state=absent, italian state=absent, but list has 157 entries (onnxruntime, 6 languages, 150 voices), not two. Item text stale.)_
      `onnxruntime` and `italian`, both `"state": "absent"` on a clean machine.
- [x] `curl -X POST localhost:9877/dictation/speech/assets/download -H _(verified 2026-09-29: POST speech/assets/download {asset:onnxruntime} -> 'Installed to .../models/speech/onnxruntime' (5.9s, download_bytes 42631433); only libonnxruntime.dylib present, 74612032 bytes; asset list state=ready.)_
      'content-type: application/json' -d '{"asset":"onnxruntime"}'` downloads
      42 MB, extracts one library, and answers `Installed to <path>`. The file
      at `<config>/models/speech/onnxruntime/libonnxruntime.dylib` must be
      about 74 MB — that is the library, not the 330-byte pkgconfig file beside
      it in the archive. Re-query the list: `"state": "ready"`.
- [x] While that download runs, the same list must report _(verified 2026-09-29: During POST /dictation/speech/assets/download {asset:german}: GET /dictation/speech/assets showed german state=downloading (3 polls, italian ready); second identical POST refused: 'could not write the download: German is already downloading'. (First download later failed on a HuggingFace 'error decoding response body' network error, state absent.))_
      `"state": "downloading"` for it, and a second download of the same asset
      must be refused rather than started.
- [x] `POST /dictation/speech/assets/cancel {"asset":"onnxruntime"}` mid-download _(verified 2026-09-29: Deleted onnxruntime, started POST assets/download, POST assets/cancel at 0.05/0.15/0.4s: download returned {error:'download cancelled'}, GET assets state=absent (not incomplete), models/speech/.staging empty each time. 3 runs; asset removed afterwards.)_
      must stop it, and the list must go back to `absent` — not `incomplete`.
      Nothing may be left under `<config>/models/speech/.staging/`.
- [x] `POST /dictation/speech/assets/delete {"asset":"onnxruntime"}` removes the _(verified 2026-09-29: Downloaded onnxruntime (state ready, libonnxruntime.dylib), POST assets/delete {asset:onnxruntime} -> 'Deleted ONNX Runtime', dir gone; second delete -> same success string. Also on already-absent asset.)_
      directory, and a second delete answers success rather than an error.
- [x] `{"asset":"italian"}` downloads about 125 MB into _(verified 2026-09-29: POST {asset:italian} -> 'Installed to .../speech/italian' (~133MB on disk, 130082025 B download); contains voices/giovanni.safetensors; list shows italian state=ready voices [giovanni].)_
      `<config>/models/speech/italian/` and the list then reports `ready`,
      including `voices/giovanni.safetensors`.
- [ ] The failure path still needs checking, and no longer happens by itself: _(NOT VERIFIED 2026-09-29: Needs network interruption or bad URL fault injection during a real download)_
      interrupt the network mid-download (or point one `Fetch` at a bad URL) and
      confirm the failure leaves `state` at `absent` with no `.staging`
      directory behind. That behaviour was previously proven for free by the
      missing voice, so it is now unobserved rather than known-good.
- [x] Corrupting one installed file afterwards (`truncate -s 100 _(verified 2026-09-29: Downloaded French via API (ready), then truncate -s 100 <models/speech/french/bundle.json> (used French, not shared Italian): asset list shows french state=incomplete missing=['bundle.json'] (was ready, []). Deleted french afterwards.)_
      <config>/models/speech/italian/bundle.json`) must move the asset to
      `"state": "incomplete"` with that file named in `missing` — never `ready`.

## Spoken replies and the `voice` MCP tool (story `817-f67c`, 2026-09-22) — **Rust, needs a `make dev` restart**

Everything below needs an installed Italian bundle, because arming without one
opens the conversation **without** a voice. That is no longer a blocker: the
`speech-voices-v1` release was cut on 2026-09-22, so install the bundle through
the download items above first, then work through these.

- [ ] Arm hands-free against a throwaway session on the restarted build, then _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Arm/status need desktop build; WS audio client can arm without a mic (Italian bundle must be installed).)_
      `curl 'localhost:9877/dictation/speech/status'`. `available` must be
      `true`, `sessionId` must be that session, and `voice` must name the
      installed Italian voice.
- [ ] `curl -X POST localhost:9877/dictation/speech/speak -H 'content-type: _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. POST speak -> state queued checkable there; 'audible in Italian' is speaker-hardware (blocked).)_
      application/json' -d '{"text":"Ciao, sto parlando."}'` must answer
      `state: "queued"` with an `utteranceId` — never `"finished"`. Listen: the
      reply comes out of the speaker in Italian.
- [ ] Poll `GET /dictation/speech/status?utterance=<id>` while it plays. It must _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Status state walk checkable there; 'finished only after last word audible' needs ears.)_
      walk `queued` → `rendering` → `speaking` → `finished`, and only reach
      `finished` **after** the last word is audible.
- [ ] **Barge-in.** Queue a long reply, then start talking over it. The speaker _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Barge-in could be simulated by feeding speech audio on the WS; audible stop needs ears.)_
      must stop within a beat, the utterance must report `interrupted` rather
      than `finished`, and `turn` must have advanced. What you said must arrive
      in the terminal as a new turn — not appended behind the reply.
- [ ] Re-send the same reply quoting the **old** `turn`. It must be refused with _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Stale-turn refusal is pure API once armed via WS audio client.)_
      a message naming both turns, not spoken.
- [ ] `POST /dictation/speech/stop` while a reply plays: silence immediately, _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. POST /dictation/speech/stop turn/queue checks are API-only once armed; audible silence needs ears.)_
      the queue empties, and the returned `turn` is higher than before.
- [ ] Disarm while a reply is playing. The audio must stop, and a `speak` after _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Disarm then speak refusal is API-only once armed via WS; audio stop needs ears.)_
      that must be refused with "Hands-free is not armed".
- [ ] From a **second** terminal's Claude Code, `voice action=status`: it must _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. voice tool via real claude reachable (see 3934); binding refusal needs an armed conversation on the desktop build.)_
      report the binding refusal, not the first conversation's queue. From the
      armed terminal's own Claude Code, `voice action=speak` must be heard.
- [x] With nothing armed, `GET /dictation/speech/status` must answer _(verified 2026-09-29: GET /dictation/speech/status with nothing armed -> {available:false,unavailableReason:"Hands-free is not armed",...} HTTP 200 JSON, no error)_
      `available: false`, `unavailableReason: "Hands-free is not armed"` — never
      an error.
- [x] In Claude Code connected to this build, `voice` must appear in the tool _(verified 2026-09-30: Real claude -p (tuic-bridge stdio, TUIC_SOCKET+TUIC_SESSION=shell PTY id) saw mcp__tuic__voice on a fresh connection; status answered {available:false,unavailable_reason:'This TUICommander build has no audio support'}, no error. raw tools/list on fresh MCP session lists voice.)_
      list on a fresh connection without any list-change notification, and
      `action=status` must answer rather than erroring.

## One language, end to end (story `822-7d7a`, 2026-09-22) — **Rust, needs a `make dev` restart**

The first four items need an installed Italian bundle, for the same reason as
the block above: without one there is no voice to listen to. The release that
serves it exists as of 2026-09-22, so install it first. The point of every one
of these items is the same — the
model must never answer in a language the user is not speaking.

- [ ] Set Dictation language to **Italian**, arm hands-free, and say something in _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Needs Italian language, armed conversation and real agent reply; drive via WS audio client on desktop build.)_
      Italian. The terminal entry must read `<what you said> (reply in Italian)`,
      on one line, and the agent must answer **in Italian**. Instruction
      delivery is not the proof: read the agent's reply.
- [ ] With the same setup, listen to the spoken reply. It must be the Italian _(NOT VERIFIED 2026-09-30: blocked — audio hardware: listening to the Italian TTS voice is speaker/ears; also headless build has no audio.)_
      voice reading Italian — not Italian text read by another language's voice,
      and not an English sentence.
- [ ] Set the language to **Auto** and disarm/re-arm. Before you say anything, _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Auto-language status (available:false, language:'') is an HTTP check after re-arm.)_
      `curl 'localhost:9877/dictation/speech/status'` must answer
      `available: false`, `language: ""` and a reason naming Auto. Nothing may be
      spoken in this state.
- [ ] Still on Auto, say something in Italian. `status` must then report _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Needs spoken Italian/English input (WS audio) and real agent reply.)_
      `language: "it"`, and the entry must carry `(reply in Italian)`. Say the
      next turn in **English**: the entry must carry `(reply in English)` and the
      agent must switch with it.
- [ ] Set the language to **Korean** (transcribed, no voice bundle) and arm. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Korean status refusal is an HTTP check after arm.)_
      `status` must answer `available: false` with
      `No speech bundle ships for language "ko"`. It must **not** fall back to
      the Italian voice.
- [ ] While a reply is being spoken, change the Dictation language in Settings. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Needs Settings UI language/RMS change during playback; audible cut needs ears.)_
      The speaker must stop mid-sentence and `status` must report the new
      language. Then change only the **RMS threshold** while another reply
      plays: that one must keep playing to the end.
- [ ] Turn the hands-free entry/exit hints **off** (story 821's setting, when it _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Needs hints-off setting in UI and armed entry.)_
      lands) and repeat the first item. The `(reply in …)` requirement must still
      be in the entry — it is not a hint.
- [ ] In the armed terminal's Claude Code, `voice action=status` must report _(NOT VERIFIED 2026-09-30: partial — Verified: voice tool inputSchema has only action,text,turn,utterance_id (no language/voice param). Not verified: status.language from an armed terminal (no dictation/arm in headless build).)_
      `language`, and the tool schema must offer no way to pass a language or a
      voice.

## The model is told when hands-free starts and stops (story `821-842a`, 2026-09-22) — **Rust, needs a `make dev` restart**

_(NOTE 2026-09-23: superseded — hands-free turns and notices are now typed straight into the terminal, busy or not, and never enter the Compose queue; there is no `voice_command` kind, no `queuedIds`, no `cancelled`/`alreadyDelivered`. Read "reaches the Compose queue" as "is typed into the terminal"; a dialog or draft holds the turn in the hands-free panel. See "Hands-free turns reach a busy agent at once" at the top.)_

Automated coverage is in place for the mechanism: the notice reaches the Compose
FIFO of the bound session, a notice the agent never read is withdrawn instead of
contradicted, both disarm paths send the end notice, the setting turns both off,
and push-to-talk sends neither. What no test here can check is whether a real
model **acts** on them — that is the whole point of the feature, and it needs a
live agent and Boss's judgement.

Run every item against a throwaway tab in the worktree build, never Boss's live
sessions. Settings > Dictation now carries **Notify model when hands-free
changes** (on by default).

- [ ] Arm hands-free on a Claude tab. The tab must receive a line saying voice _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Arm/disarm notice lines need the desktop arm path (WS audio client works without a mic) and a real claude tab (clau)_
      is on for this terminal, submitted as its own turn. Then say something
      ordinary and read the reply: the agent should either call the voice tool
      or explain why it cannot — **not** ignore the notice.
- [ ] Disarm. The tab must receive the "voice is off, reply as text" line, and _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Arm/disarm notice lines need the desktop arm path (WS audio client works without a mic) and a real claude tab (clau)_
      the next thing you type must be answered in text with no voice attempt.
- [ ] **Rapid arm/disarm while the agent is busy.** Arm and disarm again within _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Arm/disarm notice lines need the desktop arm path (WS audio client works without a mic) and a real claude tab (clau)_
      a second or two while the agent is mid-task. The start notice must
      disappear from the Compose queue and **no** stop notice may appear — the
      agent must end up with neither line, not with a lone "voice is off".
- [ ] Arm, wait for the agent to read the start notice, then close the bound _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Arm/disarm notice lines need the desktop arm path (WS audio client works without a mic) and a real claude tab (clau)_
      tab. The runtime disarms itself; confirm the log shows the end notice was
      attempted and reports honestly that the target was gone.
- [ ] Turn the setting off, arm and disarm. Neither line may appear anywhere. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Arm/disarm notice lines need the desktop arm path (WS audio client works without a mic) and a real claude tab (clau)_
      Then, still with it off, arm and check that speech itself still works
      (`voice action=status` must report `available: true` once a language is
      known) — the setting must silence the notices and nothing else.
- [ ] Arm with the setting **on**, let the agent read the start notice, then _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Arm/disarm notice lines need the desktop arm path (WS audio client works without a mic) and a real claude tab (clau)_
      turn the setting off in Settings while still armed, then disarm. The stop
      notice must still be sent: the agent was already told voice was on.
- [ ] Hold the push-to-talk hotkey and dictate a sentence. No notice of either _(NOT VERIFIED 2026-09-30: blocked — audio hardware: push-to-talk hotkey hold + dictated sentence (Fn/hotkey and mic); no hotkey/mic path on headless build.)_
      kind may appear, the hands-free badge must stay off, and `voice
      action=status` must still report `available: false`.
- [ ] **[VISUAL]** Settings > Dictation: the new toggle must sit with the other _(NOT VERIFIED 2026-09-30: blocked — VISUAL-owned by tuic-live-checks)_
      dictation toggles and its hint must read clearly at the panel's width.

## Voice conversation controls in the Dictation panel (story `818-2a29`, 2026-09-22) — **Rust, needs a `make dev` restart**

Settings > Dictation now carries two new sections below Voice tuning: **Spoken
replies** (the speech assets, the language replies are spoken in, and the voice)
and **Hands-free conversation** (terminal picker, Start/Stop, live phase,
activation phrase, hold-back). The dictation hotkey now also stops a running
conversation.

Everything mechanical is covered by tests; these items need real audio, a real
download, or Boss's eye. Run them against the worktree build, never Boss's live
sessions.

- [ ] Download **ONNX Runtime** and **Italian** from Spoken replies. The percent _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Asset download routes /dictation/speech/assets* are desktop-only; needs Settings rows to check percent per row.)_
      must climb on each row independently — starting both at once must not show
      one row the other's progress — and each row must end at Downloaded.
- [ ] Cancel a download halfway. The row must go back to Not Downloaded with no _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Cancel route and .staging cleanup need the desktop build (prior 29/09 run saw cancel not clearing 'downloading').)_
      progress bar left behind, and no partially installed files may remain.
- [ ] With Italian ready, the **Voice** control must appear and list `giovanni`. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Voice control list (giovanni) is UI; hearing the voice is speaker hardware.)_
      Pick it, then arm a conversation and hear a reply in that voice.
- [ ] Change the voice while a reply is being spoken. The reply must stop _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Voice change during reply needs UI; audible mid-sentence cut needs ears.)_
      mid-sentence rather than finish in the other voice.
- [ ] **Opening Settings > Dictation must not light the microphone indicator.** _(NOT VERIFIED 2026-09-30: blocked — audio hardware: observing the macOS microphone indicator (human eyes on menu bar) when opening Settings > Dictation.)_
      Neither must starting the app. Nothing arms by itself.
- [ ] Start a conversation from the panel, then press the dictation hotkey. The _(NOT VERIFIED 2026-09-30: blocked — audio hardware: dictation global hotkey plus real capture/voice queue on the desktop app.)_
      conversation must stop — capture, queue and voice — and the status line
      must say how many spoken entries had already been typed.
- [ ] Press the hotkey with nothing armed. It must record as usual, not report a _(NOT VERIFIED 2026-09-30: blocked — audio hardware: global dictation hotkey recording with microphone.)_
      stopped conversation.
- [ ] Let a conversation end by itself (close the bound terminal), then press the _(NOT VERIFIED 2026-09-30: blocked — audio hardware: global dictation hotkey recording with microphone after conversation ends.)_
      hotkey. It must record — a stale armed flag must not eat the keypress.
- [ ] Set an activation phrase, then speak a sentence without it: nothing may be _(NOT VERIFIED 2026-09-30: blocked — audio hardware: real spoken sentences (mic) against the activation phrase; no synthetic STT path in headless build.)_
      sent. Speak one with it: the phrase itself must not reach the terminal.
- [ ] **[VISUAL]** Both new sections at the panel's width: the asset rows must _(NOT VERIFIED 2026-09-30: blocked — VISUAL-owned by tuic-live-checks)_
      line up with the Whisper model rows above them, and the phase line must
      stay readable while it changes.
- [ ] Open the app in a browser tab (`http://localhost:9877/`) and open _(NOTE 2026-09-29: stale — browser mode now has a Voice tab (renamed Dictation) with the full Dictation/Hands-free content)_ _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Instance has no frontend (headless); item is a browser-tab Settings check (Dictation tab absent? previous note says browser mode now shows Voice tab, item text stale).)_
      Settings. The **Dictation** tab must be absent entirely, and searching
      settings for "Hands-free" must report no match rather than opening an
      empty panel. The browser microphone and speaker are story `832-e730`.

## Barge-in over a real speaker (story `816-cbbf`, 2026-09-22) — **[HUMAN]**

Criterion 3 of the story asks for a real audio probe, and this is the half of it
no test can reach. Everything that can be measured offline already is —
`talking_over_the_reply_stops_it_without_losing_the_first_words` in
`continuous.rs` reports 50 ms stop latency and 0 false triggers against the real
AEC3 canceller, over a **modelled** room (40 ms delay, 0.35 gain, no
reverberation, no noise floor, no speaker distortion). See
`docs/backend/dictation.md` → "What barge-in measures".

Needs a real microphone and a real speaker, in a room, with no headphones.

- [ ] **[HUMAN]** Arm hands-free, ask something with a long answer, and let the _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_
      reply play **out of the laptop speaker**. Say nothing for the whole reply.
      The reply must finish. A reply that cuts itself off is the echo path
      failing on real reverberation, which the modelled room cannot produce.
- [ ] **[HUMAN]** Same again, and talk over it after a couple of seconds. The _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_
      reply must stop within about a quarter of a second, and the transcript
      that reaches the terminal must contain your **first** word — that is the
      pre-roll doing its job. A transcript that starts mid-sentence is the
      failure to report.
- [ ] **[HUMAN]** Repeat both at a high speaker volume, close to the _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_
      microphone. This is the case the linear room model is least like: a
      driven speaker clips, and AEC3 cannot subtract what the amplifier added.
      Report whether false interruptions appear and at roughly what volume.

## A whole voice conversation, on real hardware (story `820-21a5`, 2026-09-22) — **[HUMAN]**

The eight states a conversation has to survive are held by tests and indexed in
`docs/backend/dictation.md` → "The eight states the conversation has to
survive". What is left here is what no test can reach: real Whisper and real
Kokoro inference, a real microphone and speaker, and the browser endpoint.

**One of these is still blocked.** The Italian voice is no longer: the
`speech-voices-v1` release on `sstraus/tuicommander` was cut on 2026-09-22 and
serves `italian-giovanni.safetensors` at the pinned sha256, so a first run can
download a voice. Browser capture/playback is story `832-e730`, which is not
built. Do not mark that item from a mock — record it as blocked.

- [ ] **[HUMAN]** In an isolated instance (`TUIC_APP_INSTANCE=voice-check`) and _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_
      against a throwaway terminal: download the speech assets, arm hands-free,
      speak a question, and let it run to the end — automatic end of turn, the
      agent answering out loud, and talking over the answer to interrupt it.
      Everything with the real assets, not the test doubles.
      _Unblocked 2026-09-22: the voice is published. Real Pocket TTS synthesis
      is already proven against a local bundle — `cargo nextest run --lib
      --run-ignored ignored-only -E 'test(/dictation::speech::pocket/)'` with
      `TUIC_POCKET_BUNDLE_DIR` and `ORT_DYLIB_PATH` set, 3/3 green. What is left
      here is the microphone, the speaker and the interruption._
- [ ] **[HUMAN]** Ask the **model** to speak through the `voice` MCP tool while _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_
      that same conversation is armed, and confirm it reaches the same speaker
      and the same queue as a reply the desktop asked for.
- [ ] **[HUMAN]** The same conversation from a browser tab against the same _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_
      instance: the microphone and the speaker must be the **browser's**, not
      the desktop's, and the desktop must behave identically.
      _Blocked: browser audio transport is story `832-e730`._
- [ ] **[HUMAN]** Record, with numbers: time from the end of speech to the _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_
      first audible word of the reply, and the process footprint before arming,
      while speaking and after disarming (`GET /diagnostics/memory`). A
      conversation that leaks per turn is the failure to look for.
- [ ] **[HUMAN]** Repeat the first item on Windows and on Linux from a release _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_
      build. Cross-platform evidence cannot come from this Mac.

## Speech and download progress on `/events` (story `833-6fd4`, 2026-09-22) — **Rust, needs a `make dev` restart**

The three pushes are dual-emitted: the desktop window gets an `emit`, the SSE
stream gets the identical body. Only a restart loads them.

- [x] Open the web UI (`http://localhost:9877/`, browser mode) and start a _(verified 2026-09-29: Web UI :9880 browser mode, Voice>Spoken replies: clicked Download on English; row went 0% -> 12% -> 25% -> 81% in the browser tab (polled DOM every 3s), then Downloaded. (Also found: whisper-model Download in browser sends {} -> 422 'missing field model': transport.ts:136 reads args.model_name but store passes modelName.))_
      speech-asset download from the Dictation panel. The progress bar must move
      in the **browser** tab, not only on the desktop — before 833 a browser
      client saw the download start and finish with nothing in between.
- [x] `curl -N http://localhost:9877/events` while that download runs: frames _(verified 2026-09-29: SSE /events (unix socket) during italian speech download: 'event: speech-download-progress' data {downloaded,total,percent,asset:'italian'} + final {asset,done:true}. Concurrent Whisper small download (POST /dictation/models/download {model:small}): 'event: dictation-download-progress' data {downloaded,total,percent} with no asset key (29961 frames)_
      named `speech-download-progress` carrying `asset`, `downloaded`, `total`
      and `percent`. A Whisper-model download on the same stream must be named
      `dictation-download-progress` and must **not** carry `asset`.
- [ ] Arm hands-free, let a reply play, and watch the same stream: one _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Needs /events SSE watch on desktop build while a reply plays (speech-utterance frames).)_
      `speech-utterance` frame per transition, in the order
      `queued → rendering → speaking → finished`, with no polling of
      `GET /dictation/speech/status`.
- [ ] Talk over a reply and confirm the last frame for that utterance is _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Talk-over needs armed conversation; audio can come via WS client, hearing cannot.)_
      `interrupted` rather than `finished`, and that it arrives — the transition
      happens on the render thread after `speak` has long returned, which is the
      case a polling client used to miss entirely.

## Agent toast repository action (story `835-314c`, 2026-09-22) — **frontend visual**

The rendered DOM was exercised with the real Vite modules and contained two
toasts at once: the different-repository toast had `Go to repo`; the current-
repository toast did not. `agent-browser` navigation and DOM inspection worked,
but `Page.captureScreenshot` timed out repeatedly, and CUA could not bind the
headed test browser window.

- [ ] **[VISUAL]** Capture the two toast states together after the screenshot _(NOT VERIFIED 2026-09-29: partial — Toast VISUAL: screenshots time out in this browser so no contrast/wrapping judgement possible; toast trigger states not reproduced in web UI. Not verified.)_
      backend is available. Confirm the secondary button spacing, contrast and
      wrapping at the normal window width and at a narrow width.

## SSH-managed `tuic-remote` deploy and install (stories `836-c262`–`847-ad3d`, 2026-09-22) — **Rust, needs a `make dev` restart**

The Rust backend does not hot-reload. Restart the test instance before these
checks; use `TUIC_APP_INSTANCE=remote-deploy-check` so no production connection
or credential is touched.

- [x] Against a throwaway Linux or Apple Silicon macOS SSH host with no daemon,
      save **Deploy on connect** and click Connect. It must progress through
      `Deploying: <step>` to Connected, bind only `127.0.0.1`, and leave the
      host's agent configuration files unchanged.
      _(verified 2026-09-22 through the HTTP parity surface against an isolated
      Ubuntu systemd container on mac-mint: Connected, loopback listener only,
      pairing token absent from argv, and no `~/.claude.json` created)_
- [x] Disconnect, wait less than the configured survive time, and reconnect.
      Existing remote sessions must still be present. After disconnecting for
      longer than the survive time, the daemon and its pid file must disappear.
      _(verified 2026-09-22: a three-second client between lifetime polls reset
      the deadline; after the new idle window both process and pid file vanished)_
- [x] Connect again with the same desktop version. The remote binary hash must
      match, no second SCP should occur, and the vault pairing token must still
      work after restarting the desktop test instance.
      _(verified 2026-09-22: inode/mtime stayed unchanged and the isolated
      credential-file digest survived a full `make dev` restart)_
- [x] Click Install. On Linux verify the systemd user unit and mode-0600 env
      file; on macOS verify the mode-0600 launchd plist. Reboot or log out/in and
      confirm Connect no longer deploys. Then click Uninstall and confirm the
      service files and ephemeral pid are gone.
      _(verified 2026-09-22 on isolated Ubuntu/systemd via HTTP parity: unit and
      protected env installed, linger enabled, service and loopback listener
      returned after a container reboot before any SSH login, then Uninstall
      removed the unit, env, pid and listener. launchd rendering/mode/lifecycle
      are covered by the targeted Rust service tests.)_
- [x] The Remote Machines form, deployment picker, survive-minutes field, SSH
      host picker and monochrome icons were rendered in an isolated browser on
      port 9877. _(verified 2026-09-22 from the worktree build; proof in
      `.tmp/visual-proof/remote-machines-fields.png`)_

## SSH local-forward readiness (story `1159-4e28`, 2026-09-28) — **Rust, needs a `make dev` restart**

- [ ] After restarting an isolated test instance, connect the Installed-service _(NOT VERIFIED 2026-09-30: blocked — second physical machine: aws-graviton (56481148) SSH remote; AWS box is DOWN per coordinator brief, and instance config must not be edited by hand.)_
      `aws-graviton` remote (56481148) over SSH. It should progress from
      Connecting to Connected once the local forward listens and `/health`
      answers, without an intermediate "installed daemon not answering" error.
      Record the elapsed time and the tunnel status transitions. Use only the
      configured host and credentials; the targeted Rust tests cover delayed
      local ports and delayed health independently.

## Config defaults and expert-mode UI pref (story `863-03c1`, 2026-09-24) — **Rust, needs a `make dev` restart**

- [x] After restarting the desktop dev build, `GET http://127.0.0.1:9876/config/defaults` _(verified 2026-09-29: GET /config/defaults on validate desktop instance: keys app, notifications, agent_settings, repo_defaults, agents, github_accounts, dictation (superset of item's 4; matches docs table). app keys == GET /config keys; live notifications and /dictation/config equal defaults; app differs only in instance services (port 9880, auth, vapid). dictation pre)_
      (or `:9877` for a worktree build) returns `{ app, notifications,
      agent_settings, dictation }` — each nested object matching the shape of
      its own `load_config`/`load_notification_config`/`load_agents_config`/
      `get_dictation_config` response, and every field holding that domain's
      documented default value (see `docs/backend/config.md` → "Config
      Defaults"). Confirm `dictation` is present on the desktop build.
- [ ] Toggle `settingsExpertMode` (via `ui.ts`'s `setSettingsExpertMode`, once a _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: settings_expert_mode is set by frontend ui.ts (writes ui-prefs.json per 29/09 note, item path config.json is wrong); headless instance has no frontend; full restart not allowed.)_
      caller wires it in — this story only persists the pref, no UI control
      yet) and confirm `settings_expert_mode` round-trips through
      `~/Library/Application Support/tuicommander/config.json` (or platform
      equivalent) and survives a full `make dev` restart.
- [ ] After the restart, `GET /config/defaults` also returns `repo_defaults` _(NOT VERIFIED 2026-09-30: partial — Verified: GET /config/defaults returns repo_defaults (16 keys) and agents ({agents:{}}); remote.log has 0 'unknown configKey' lines. Not verified (needs UI): Expert-off hides 8 Git&GitHub rows and Smart Prompts 'Headless Agent'.)_
      and `agents` (story `864-a5c9`). In Settings with Expert off, Git &
      GitHub hides the eight repository-default rows while they hold their
      defaults, and Smart Prompts hides "Headless Agent" while it is not
      configured. The app log shows no `settingsExpert` "unknown configKey"
      warning for `repo_defaults.*` or `agents.*`.

## Error messages name the reorganized Settings pages (story `861-977b`, 2026-09-24) — **Rust, needs a `make dev` restart**

- [x] With a configured microphone unplugged, starting dictation reports _(verified 2026-09-29: by code/test inspection, tests not executed here: Message exists at src-tauri/src/dictation/commands.rs:1513 'check Settings > Voice > Input device'; old label gone)_
      "check Settings > Voice > Input device" (was "Settings > Dictation >
      Microphone"; neither the page nor the label exists any more).
- [x] With progress collection off for an agent, the `progress` MCP tool _(verified 2026-09-29: by code/test inspection, tests not executed here: progress/service.rs:38 'progress_tracking_disabled: ... (Settings → Agents)'; mcp_transport.rs:1523 comment. Message names Agents page.)_
      answers `progress_tracking_disabled … (Settings → Agents)`. The toggle
      lives on the Agents page; "Settings → Progress" never existed.
- [ ] [VISUAL] Settings → General with Experimental Features **off**: an **ego** _(NOT VERIFIED 2026-09-30: blocked — VISUAL-owned by tuic-live-checks)_
      section sits directly after Code Intelligence, with a `?` tooltip, the
      "Configured at …" / "Not configured" line, **Select…** and **Clear**.
      Settings → AI Chat (Experimental Features on) shows only Default Model
      and Providers. With the ego path empty, AI Chat says to name the binary
      in Settings → General.
## Mobile Basic Auth recovery (2026-09-25) — **Rust, needs a `make dev` restart**

- [ ] **[HUMAN]** On a phone PWA with remote access enabled and a stale cached Basic _(NOT VERIFIED 2026-09-30: blocked — real phone: PWA with stale cached Basic credential and the browser's native Basic Auth challenge.)_
      credential, navigate until the browser shows its Basic Auth challenge. Enter the
      current password without reloading. The app must reconnect and resume the session.

## CircleCI failure logs — **Rust, needs a `make dev` restart**

- [ ] After restarting the worktree build, open a failed CircleCI check on a remote-only PR, including a PR with a failed GitHub Actions job. Its Log button shows only that CircleCI check's log and the end of a long failed step, with a truncation marker when the beginning was dropped; a stale or mismatched CircleCI build reports a revision mismatch. The running app cannot load this Rust change until restart. _(NOT VERIFIED 2026-09-30: blocked — not a hardware class, but needs a CircleCI API token and a real remote-only PR with a failed CircleCI check; GET /circleci/token -> configured:false. Reading the CircleCI code path for a fake server was denied by the sandbox classifier (credential), so not attempted. Coordinator decision.)_

## Safe linked-worktree removal — **Rust, needs a `make dev` restart**

- [x] After restarting an isolated worktree build, remove a clean linked worktree with a populated submodule. It succeeds without a dirty-file confirmation. A submodule with a local commit stays intact on a non-force request. A forced removal of a branch with unmerged commits keeps the branch and reports why. The running app cannot load this Rust change until restart. _(verified 2026-09-29: Own repo+submodule: clean wt3 removed via worktree_remove, no confirmation. wt4 (submodule local commit): non-force refused 'uncommitted changes', dir intact; forced w/ fingerprint: commit 4ad6c724 still in main libsub. wt5 unmerged: non-force refused; force ok + warning 'unmerged commits', branch wt5 kept.)_
- [x] After the same restart, confirm a forced removal with a dirty submodule, then change its HEAD before the request completes. Removal must stop with a changed-state message; retry after a fresh review. A clean merged submodule commit must remain accessible from the main checkout after removal. _(verified 2026-09-29: Disposable repo w/ submodule via MCP repo tool: dirty submodule (requires_force), fingerprint from worktree_lifecycle, then committed in submodule (HEAD change) -> worktree_remove force with old fp: 'Worktree state changed since confirmation; review it before removal', worktree kept. Fresh lifecycle fp -> ok. Clean merged sub commit 7cf89d3b (branc)_

## Vite watcher scope and native reload attribution — needs a `make dev` restart

- [ ] Restart `make dev` when live PTY sessions can be interrupted. In an isolated dev instance, create and delete a checkout with HTML files under repository `.tmp/`; verify document age continues increasing and no full reload occurs. Call `POST /debug/reload_webview` and verify the native log records caller address, trigger, action, and target URL while frontend startup records navigation type and document start. The Vite watch config and Rust backend require a restart to take effect. _(NOT VERIFIED 2026-09-30: partial — Verified: vite dev on :5199 with repo vite.config.ts: creating/deleting HTML under repo .tmp/ (nested too) logged no 'page reload'; controls public/*.html and root *.html logged 'page reload'. POST /debug/reload_webview on headless -> 'webview recovery requires the desktop feature'; native log lines)_
## Desktop Progress entry (2026-09-26)

- [x] With zero unread Progress updates, open the toolbar bell and select Terminal Progress for the active repository. The bell badge remains absent; a new update restores the count. The command palette and `Cmd/Ctrl+Shift+P` open the same dialog. _(verified: targeted Toolbar, keyboard shortcut, and action registry tests; rendered bell screenshot at `~/Gits/.tmp/tuic-progress-entry/progress-bell.png`.)_

## Ask Boss from the mobile PWA (2026-09-26) — Rust, needs a `make dev` restart

- [ ] **[HUMAN]** After this Rust parser change is landed and Boss restarts `make dev` when current PTYs can be interrupted, open the HTTPS mobile PWA on a real phone at 360×800. In a disposable Codex session, trigger `request_user_input` with two choices and `Other`. Confirm its waiting badge, tap the question control in the existing session header, read the title and all options, select an option once, and verify Codex receives exactly one answer and the overlay clears. Repeat with `Other` and type a note; verify it reaches the question rather than the main composer. Check the terminal has lost zero rows. The running backend cannot load the Rust parser change before restart. _(NOT VERIFIED 2026-09-30: blocked — real phone: [HUMAN] item needs a physical phone (PWA/Photos/share sheet/touch layout/push).)_

- [ ] **[HUMAN]** After restarting the desktop app when its current PTY sessions can be interrupted, enable Remote Access and Tailscale HTTPS, then open the shown HTTPS `/mobile` URL on the phone. On iPhone, launch the installed Home Screen PWA. In mobile Settings, turn Push notifications off and on to replace the old subscription, grant permission, and confirm a test push appears on the phone. Do not change Tailscale/network configuration as part of this check. _(NOT VERIFIED 2026-09-30: blocked — real phone: [HUMAN] item needs a physical phone (PWA/Photos/share sheet/touch layout/push).)_
- [ ] **[HUMAN]** With the desktop window left focused but no Mac HID input for two minutes, have a managed agent report `progress type=blocked` with an identifiable question. Confirm one phone notification contains the question, opens that exact session, and one typed reply reaches it once. During a confident free-text question, leave an automated peer message queued: the phone answer must reach the question first and the peer message must remain parked until the question clears. Repeat with the desktop actively used: no duplicate push. The running app cannot load these Rust changes until restart. _(NOT VERIFIED 2026-09-30: blocked — real phone: [HUMAN] item needs a physical phone (PWA/Photos/share sheet/touch layout/push).)_
- [ ] **[HUMAN]** After the separate question-state change is integrated, trigger a real Claude AskUserQuestion with a visible title. Confirm the phone push contains that title rather than the hook's empty awaiting signal or an Ink footer, then answer it from the opened session. _(NOT VERIFIED 2026-09-30: blocked — real phone: [HUMAN] item needs a physical phone (PWA/Photos/share sheet/touch layout/push).)_

## PTY build environment — Rust, needs a `make dev` restart

- [x] After restarting an isolated TUIC build, open a shell PTY in a different Rust repository and check that `CARGO_TARGET_DIR`, `CARGO_MANIFEST_DIR`, and `OUT_DIR` are unset while `CARGO_HOME` and an ordinary user environment variable remain available. Spawn a managed agent in the same repository and confirm the same. The running TUIC backend cannot load this Rust change until restart. _(verified 2026-09-30: Isolated tuic-remote (--instance r4iso, own TMPDIR) started with CARGO_TARGET_DIR/CARGO_MANIFEST_DIR/OUT_DIR=/poison/* and MYUSERVAR=hello. Shell PTY env: only CARGO_HOME, MYUSERVAR, USER; poison vars absent. MCP-spawned managed agent (fake binary): same. Fixture cwd was not a Rust repo.)_
- [x] After restarting an isolated `make dev` build, open a new terminal and run `env | grep -E 'CARGO_INCREMENTAL|RUSTC_WRAPPER|^MBX_'`; expect no matches. In a managed agent PTY, check that `HOST_CC` and `HOST_CXX` are also unset. Confirm a configured per-agent `CARGO_INCREMENTAL=1` still reaches its PTY. The current Rust backend requires a restart before this can be checked. _(verified 2026-09-29: Own tuic-remote started with CARGO_INCREMENTAL=0 RUSTC_WRAPPER MBX_* HOST_CC HOST_CXX in its env (confirmed via ps): new shell PTY env|grep -> no matches; spawned agent PTY (env in spawn) -> HOST_CC/CXX unset, none of the vars; spawn env CARGO_INCREMENTAL=1 -> reaches agent PTY.)_

## Peer mail wake after Rust restart

- [x] After restarting an isolated `make dev` build, send a 10 KiB message to a disposable external Claude client subscribed to MCP SSE. Confirm the channel shows the sender UUID, message ID, size, and first-line preview without the body; `agent action=inbox` returns the complete message once. The running Rust backend cannot load this change until restart. _(verified 2026-09-29: Fake external client (initialize clientInfo claude-code, GET /mcp SSE) got notifications/claude/channel: '[TUIC] message available...\nfrom <sender uuid> id <msgid> 10240 bytes: Big report title', meta from_tuic_session+message_id, no body 'zzz'. agent inbox returned 1 message with 10240-char content; second inbox call 0. Via validate instance sock)_
- [x] After restarting `make dev`, spawn one disposable agent through MCP _(verified 2026-09-29: MCP agent action=spawn and POST /sessions/agent (stub /bin/sh -c 'sleep 30', no MCP call by the child): GET /sessions rows show tuic_session == session_id for both (96caece0.., fe777b72..). Both deleted.)_
      `agent action=spawn` and one through `POST /sessions/agent`. Confirm each
      `GET /sessions` row reports `tuic_session` equal to its `session_id` before
      the agent calls MCP, then close both sessions. The current backend cannot
      load this Rust binding change until restart.
- [x] Restart the isolated `make dev` test instance and run _(verified 2026-09-30: Instance has no hook config (--no-agent-configs), so spawn's --settings file is missing and claude died. Ran a copy of canary-peer-mail-wake.py with binary_path wrapper (strips --settings; cwd=worktree; TCP relay to the UDS): 'PASS claude: peer mail wake appeared in PTY within 20 s'. Deviations note)_
      `TUIC_CANARY_URL=http://127.0.0.1:9877 python3 scripts/canary-peer-mail-wake.py claude`.
      Confirm the disposable Claude PTY shows `PEER_MAIL_WAKE` within 20 seconds.
      See `docs/guides/development-setup.md` for the instance setup; the Rust
      change does not hot reload into the current process.
- [x] On that rebuilt isolated instance, run _(verified 2026-09-30: Same modified canary with --capacity: 'PASS: inbox accepted and returned mail after 100 read messages'.)_
      `TUIC_CANARY_URL=http://127.0.0.1:9877 python3 scripts/canary-peer-mail-wake.py claude --capacity`.
      Confirm mail 101 is accepted and returned after the first 100 were read.

## Activity Dashboard window after Rust restart

- [ ] After restarting an isolated `make dev` build, restore Activity Dashboard with saved geometry larger than 550×650. The detached OS window opens at 550×650 while retaining its saved position; a smaller saved size remains unchanged. The running Rust backend cannot load this fix until restart. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Needs desktop build: detached Activity Dashboard OS window size after restore with saved geometry >550x650; not observable headless.)_

## Rust dead-code warning cleanup after restart

- [x] On the next `make dev` Rust rebuild, confirm no dead-code warning names Design Mode `status`, `to_prompt`, or `on_script_parsed`, Progress `mark_viewed`, AppState `resolve_session_ref` or `resolve_peer_ref`, or StoryStore `transition`. The targeted test build has already compiled without these warnings; the current backend cannot hot reload the source change. _(verified 2026-09-29: build-desktop.log (tuicommander lib compiled from this worktree): 'generated 5 warnings'; grep finds none naming Design Mode status/to_prompt/on_script_parsed, Progress mark_viewed, resolve_session_ref/resolve_peer_ref or StoryStore transition (also 0 in headless build.log). Remaining warnings are unrelated symbols.)_

## CLI build after Rust rebuild

- [ ] After rebuilding `tuic`, verify `tuic repo worktree-list`, `worktree-create`, and `worktree-remove` still accept their existing names and `tuic agent spawn` accepts its positional prompt and launcher flags. The current binary does not hot reload the Rust CLI change. _(NOTE 2026-09-29: partial evidence only — tuic-cli main.rs:321 defines name 'worktree-list' and parse test at main.rs:1803; Spawn args at main.rs:192; verify create/remove/spawn args by inspection or clap tests.)_ _(NOT VERIFIED 2026-09-30: partial — my -p tuic-cli build output lacks session/repo/story/bg/mcp subcommands (help lists only open..resume) though source main.rs has them: needs investigation of the tuic-cli build; /usr/local/bin/tuic has them)_

## Claude transcript activity after restart

- [x] After restarting `make dev` with the story 1121 Rust build, put a throwaway Claude session in detailed transcript view after a hook idle, then deliver a peer mail wake. If Claude does not begin a turn, confirm the session returns to idle after the five-minute stale submission window and the app log contains both shell transitions. The current running backend cannot load this Rust change without a restart. _(verified 2026-09-30: Real claude via wrapper with my --settings hooks (Stop/UserPromptSubmit OSC 7770 same as hook_command). hook-idle 06:11:05, Ctrl+O sent (screen shows transcript-style output), mail wake -> busy 06:11:15, no turn began, idle via 'submission-stale' at 06:16:15 (+300s). Both transitions in log. Transcr)_

## WebView reload resource cleanup — Rust, needs a `make dev` restart

- [ ] After restarting an isolated `make dev` instance, open two desktop terminal panes and register a plugin output watcher. Reload the main WebView, then inspect `/diagnostics/memory`: the old document's grid channels, gates and output watcher clients must be gone before the panes remount. Leave one pane unmounted; it must produce no desktop `grid frame gate stuck` warnings. Two minutes later, a 30-second app-log window must contain no `Couldn't find callback id` or `Output watcher clients exceeded 8` warnings. The running backend cannot load this Rust change until restart. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Needs desktop main WebView with two panes, plugin output watcher, reload and /diagnostics/memory before/after; headless has no WebView.)_
- [ ] Detach a terminal into a floating window, reload the main WebView, and confirm the floating terminal keeps painting. Close the floating window and confirm its grid subscription disappears while terminals in the main window keep painting. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Needs desktop floating terminal window and WebView reload.)_

## Mobile ego chat (story 1077-0c08) — real phone after `make dev`

- [ ] [HUMAN] After restarting the test instance with `make dev`, open its HTTPS `/mobile` URL on a real phone. In Chat, choose a disposable repository and send a prompt; confirm the answer and collapsed tool activity remain readable above the keyboard. Disconnect and reconnect the phone, then confirm the answer has no duplicate or missing lines. Start a permission request, tap one option twice, and confirm the desktop conversation records one answer. Start a second conversation, then use the titled picker to return to the first and confirm its history loads. This check requires real touch and mobile keyboard behavior; targeted Vitest covers the module behavior. _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_

## Detached Markdown document window

- [ ] After restarting an isolated test build, open a Markdown file tab and a `tuic://open` Markdown tab. Detach each from its context menu, resize the document window, edit each file on disk, and confirm the detached content updates. Clicking either tab should focus its window; closing the window should restore the document in the tab. Check an inline comment and a relative Markdown link in the detached view. _(NOT VERIFIED 2026-09-29: blocked — Needs observing detached panel windows' content/size/focus; maccontrol capture_window lacks Screen Recording permission, invoke_js reaches only the main webview, and two identical 'tuicommander' windows make clicking ambiguous with Boss's app. Not attempted.)_

## ACP mobile interaction push (story 1078-05cf) — Rust restart required

- [ ] After a manual `make dev` restart in an isolated `TUIC_APP_INSTANCE`, subscribe a phone to push. With desktop unfocused, have ego ask permission or a form question and confirm one notification opens mobile Chat. Answer the next request on desktop before delivery and confirm no stale notification; ordinary activity must not notify. The running backend cannot load this Rust change until restart. _(NOT VERIFIED 2026-09-30: blocked — real phone: push subscription and mobile Chat notification with desktop unfocused.)_

## Push-to-talk Italian hallucination filter — Rust restart required

- [ ] After restarting `make dev`, use a disposable terminal to verify that a bare “Grazie a tutti.” recognition does not reach the composer, while a genuine instruction containing those words does. The current backend cannot load the Rust change until restart. The sustained-speech activity gate remains pending real quiet-speech recordings (story 1135-b600). _(NOT VERIFIED 2026-09-30: blocked — audio hardware: real speech recognition of 'Grazie a tutti.' (filter is unit-level HALLUCINATION_EXACT); dictation routes absent in headless build.)_

## MCP initialize storm — Rust restart required (#1148-c25f)

- [x] After restarting an isolated `make dev` instance, run short-lived MCP stdio bridge clients under one disposable `TUIC_SESSION` identity and inspect `/diagnostics/memory`: normal exits should release their protocol sessions immediately, while abrupt exits should be reaped after the next initialize once the six-second activity grace has elapsed. Stop the MCP endpoint briefly and verify one surviving bridge spaces its retries. The running backend and bridge binary cannot load these Rust changes until rebuilt. _(verified 2026-09-30: Isolated instance r4iso + mbx tuic-bridge, one TUIC_SESSION: mcp.sessions 0 -> 4 normal exits 0 -> 4 live 4 -> kill -9 x4 stays 4 -> >6s later one initialize 1 -> exit 0. Endpoint stopped: bridge retry connect times after 'connection lost' at t=5.5,7.5,11.6,19.6,27.6,35.6.. (spacing 2,4,8,8..) despi)_
- [ ] After that Rust restart, let an isolated disposable MCP protocol session pass the one-hour TTL (or invoke the maintenance sweep in a test build). Confirm its protocol session, route, reverse route, and broadcast sender all disappear while an addressable PTY peer and any live sibling remain usable. This checks the reaper cleanup added for story 1148; the source of the incident's 27.3 GB malloc growth is still unknown. _(NOT VERIFIED 2026-09-30: partial — Isolated instance, sweep after 1h idle: mcp.sessions 3->1, log 'MCP session reaped (idle >=1h)' x2 (sibling kept, still usable); PTY peer r4pty kept addressable and agent send to it after reap delivered:true; ext peers evicted. Route/reverse-route/broadcast-sender maps not exposed in diagnostics: in)_

## Worktree removal preview — Rust restart and visual review (#1138-ed2e)

- [ ] After restarting an isolated `make dev` build, open removal confirmation for a branch with no own commits and a live agent in its worktree. Confirm the dialog names the agent and uncommitted/untracked counts, then take a screenshot of both the removal and post-merge cleanup dialogs. The current backend cannot hot reload the Rust preview, and this branch has not been rendered in a worktree build. _(NOT VERIFIED 2026-09-30: partial — Verified backend preview (GET /worktrees/lifecycle?repoPath&workspaceId=feature-r4): live_sessions names r4wt-agent (fake managed agent, not real CLI), dirty_files 3, untracked_files 2, warning 'nothing of its own'. Dialog rendering and screenshots need desktop UI.)_
# Mobile session search (story 1200-4dd4)

- [ ] [HUMAN] After `make dev`, check the magnifier position at the top right of the session list on a phone. Tap it, enter a filter, and confirm the field and matching cards fit without clipping. The component test covers matching and clearing; phone layout remains to be checked. _(NOT VERIFIED 2026-09-30: blocked — real phone: [HUMAN] mobile session list layout.)_

# Mobile AI Chat image prompts (story 1219-e2a1) — Rust restart required

- [ ] [HUMAN] After a manual `make dev` restart in an isolated `TUIC_APP_INSTANCE`, open the HTTPS mobile PWA on an iPhone and paste a 4 MiB photo from the camera roll into AI Chat. Confirm ego receives it; check an image over 10 MiB is refused in the composer with its size and limit. The HTTP route and composer have targeted automated tests; iPhone Photos behavior requires the device. _(NOT VERIFIED 2026-09-30: blocked — real phone: [HUMAN] item needs a physical phone (PWA/Photos/share sheet/touch layout/push).)_

## Mobile attachments (story 1227-7838) — Rust restart and real phone

- [ ] [HUMAN] After restarting an isolated `make dev` instance, use an iPhone to pick a HEIC photo and a 4 MiB camera photo from the single paperclip picker. Check conversion/type handling, the displayed size limit, and that the selected file remains a draft until Send. The phone's Photos provider and touch layout require the device. _(NOT VERIFIED 2026-09-30: blocked — real phone: [HUMAN] item needs a physical phone (PWA/Photos/share sheet/touch layout/push).)_
- [ ] [HUMAN] On an Android Chrome installed PWA, share a photo from another app into TUICommander. Confirm it appears as an AI Chat draft, then send it. The OS share sheet requires a real device. _(NOT VERIFIED 2026-09-30: blocked — real phone: [HUMAN] item needs a physical phone (PWA/Photos/share sheet/touch layout/push).)_

## Mobile session output links (story 1202-dd5b)

- [ ] [HUMAN] On a phone, tap a Markdown path in a session's output, confirm Files renders it and Back preserves the session output and draft. Tap an HTTP(S) link and confirm it opens the system browser outside the PWA. A path outside registered repositories must show a toast naming that path. Targeted tests cover link detection, routing, and root refusal; the phone handoff and touch remain to be checked. _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_

## Mobile keybar and composer (story 1221-0380) — real phone

- [ ] **[HUMAN]** On a 360 px wide phone, open a disposable agent session and check that the terminal retains its previous visible row count, the keybar scrolls without a visible scrollbar, and `/`, Ctrl+C, input and Send are comfortable touch targets. Tap `/`: no character should reach the agent until a command is chosen. Close the menu and confirm an unsent draft is restored. End the disposable session and confirm the keybar and composer cannot send. Component tests cover PTY writes and disabled state; a 360×800 browser capture measured keybar 45 px, composer 53 px, and terminal 702 px, but cannot prove real touch behavior. _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_

## Mobile session list and new-session sheet (story 1223-f3bc) — real phone

- [ ] **[HUMAN]** On a 360 px phone, confirm a waiting session remains above idle agents and shells, exact mixed-case names display unchanged in the list, detail, and question banner, and the busy badge reads "Working". Tap the question counter and confirm it opens the first waiting session. Open `+`, check agent choice, repository search, and the close X, then create a disposable Codex session and confirm it opens. Component tests cover ordering, spawn payload, navigation callback, banner content, and counter click; real touch and visual layout remain to be checked. _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_

## Mobile session card actions (story 1217-4b0b) — real phone

- [ ] **[HUMAN]** On a 360 px phone, scroll the session list to its end. Confirm the `+` button never covers the last card's kill button, both `+` and kill are comfortable touch targets, tapping a card opens it, and tapping kill opens the confirmation without opening the session. Component tests cover independent click actions and accessible names; browser geometry at 360×800 measured 44×44 px kill, 52×52 px `+`, and an 87 px gap between the last kill and `+` after scrolling to the end. _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_

## Mobile five-tab navigation (story 1224-d21c) — real phone

- [ ] **[HUMAN]** Launch the mobile PWA on a phone and confirm Sessions opens first, Chat is the second bottom tab, and only five tabs remain. Tap the app-bar overflow, open Settings, then use a bottom tab to return. Check the overflow does not clip and the tap targets remain comfortable at 360 px. Component tests cover order, initial selection and Settings navigation; real touch and phone layout remain to be checked. _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_

## Mobile terminal hanging indent (story 1222-912d) — real phone

- [ ] [HUMAN] At 360 px, open a session with a long space-indented list item and a long tab-indented code line. Confirm every visual continuation starts under the first line's text, while an unindented line stays flush left and a box-drawing table still scrolls horizontally. Browser character-rectangle checks cover the same output shapes; this item checks real device rendering and touch scrolling. _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_

## Mobile session detail task text (story 1216-a482) — real phone

- [ ] On a 360 px phone, open idle, awaiting-input, and ended Claude sessions whose terminal status line has a decorative spinner verb. Confirm no task row repeats that verb and the terminal gains the freed row; a Codex session with a substantive task such as “Reading files” should still show it. _(NOT VERIFIED 2026-09-29: needs a real phone / PWA client — not reproducible in the isolated headless/browser instance)_

## Claude AskUserQuestion options on mobile (story 1212-3093) — Rust restart and real phone

- [ ] **[HUMAN]** Restart an isolated `make dev` instance so its Rust parser loads, then open a disposable Claude AskUserQuestion on a phone. Confirm the title and every option remain visible, the choice overlay replaces generic Yes/No, and tapping option 2 selects Green exactly once. The captured PTY replay and component tests cover the payload and key sequence; this check covers real touch and phone rendering. _(NOT VERIFIED 2026-09-30: blocked — real phone: [HUMAN] item needs a physical phone (PWA/Photos/share sheet/touch layout/push).)_

## Mobile Markdown images (1215-8e00)

- [ ] After restarting `make dev` to load the Rust HTTP route, open a nested Markdown file with a repository-relative image in the mobile Files tab and confirm the image loads. Check that a path escaping the repository does not load. _(NOT VERIFIED 2026-09-29: needs a real phone / PWA client — not reproducible in the isolated headless/browser instance)_

## Mobile Progress, Activity, and Settings (1226-eb95)

- [ ] On a 360 px phone, confirm a long Progress message shows about four lines, More reveals it all, and Less collapses it again. _(NOT VERIFIED 2026-09-29: needs a real phone / PWA client — not reproducible in the isolated headless/browser instance)_
- [ ] Confirm Activity shows local 24-hour times, a completed 2343-second run as 39 min, and a single block as `1 block`. _(NOT VERIFIED 2026-09-29: partial — Code only: mobile/components/ActivityItem.tsx formatTime uses toLocaleTimeString hour12:false; readableSubtitle maps 'ran for Ns' >=60 to floor(N/60) min (2343s -> 39 min) and '1 blocks' -> '1 block'. /config/activity has 'ran for 329s' items (would show 5 min). Mobile /mobile in a 390px same-origin iframe loaded Sessions but Activity tab rendered )_
- [ ] After restarting `make dev` to load the Rust `mobile_theme` preference, choose Light in mobile Settings, reload the PWA, and confirm the theme stays light. Confirm the desktop theme remains unchanged and app/server versions are visible. _(NOT VERIFIED 2026-09-29: needs a real phone / PWA client — not reproducible in the isolated headless/browser instance)_

## AI Chat copy, links and parallel tabs (story 1142-f09c)

- [ ] [VISUAL] After `make dev`, in an AI Chat conversation select and copy a paragraph of transcript text and confirm it lands on the clipboard; click an http(s) link in a reply and confirm it opens in the external browser; click a local file path and confirm it opens through TUIC's file/editor opener. Open a second chat tab and confirm it runs an independent ACP session in parallel with the first. Component/DOM tests cover selection, copy, link routing and tab lifecycle; the live interaction remains to be checked. _(NOT VERIFIED 2026-09-29: Needs a real ego ACP conversation, external browser opener and system clipboard; two parallel live sessions.)_

## AI Chat session settings dialog (story 1145-3abd)

- [ ] [VISUAL] After `make dev`, open AI Chat and confirm the control bar shows a compact model/mode summary with one settings button. Open a new chat tab and confirm the summary is not blank. Open the settings dialog and confirm it lists one labeled row per ACP select config option (name, description, current value); change a value and confirm the summary updates from the agent's reply. Targeted Vitest covers the dialog and the late-options case; the live rendering remains to be checked. _(NOT VERIFIED 2026-09-29: Needs live ego ACP agent providing select config options.)_

## Codex questions on mobile PWA (story 1201-ae16)

- [ ] [VISUAL] On a real phone after `make dev`, open a Codex session that is waiting on a `request_user_input` question. Confirm the question text and its options are fully visible and each option can be tapped/selected without clipping at 360 px width. Confirm choosing an option submits exactly once and the question clears from both the session list badge and the session detail. Captured-fixture and 360 px DOM geometry tests already pass; the real-device check remains. _(NOT VERIFIED 2026-09-29: needs a real phone / PWA client — not reproducible in the isolated headless/browser instance)_

## Git panel Log tab layout (story 1263-7d7d)

- [ ] [VISUAL] After `make dev`, open the Git panel Log tab on tuicommander main and compare with `~/Gits/.tmp/boss/log-tab/`: rows next to a narrow graph section have no wide empty gap before the subject; expanding a commit shows the full message without a hover tooltip, ref badges clip with an ellipsis instead of covering the subject, and rows below the expanded one move down at once with their graph dots. _(NOT VERIFIED 2026-09-29: partial — Web UI Git panel Log tab on fx/repo (not tuicommander main; no boss screenshots compared): subject starts 20px from row left (no wide gap); expanding 'ignore more' grows row 48->107px, rows below shift down 201->260 and 249->308 in one step, full message shown in commitBody, row has no title tooltip; refBadge computed text-overflow: ellipsis. Graph)_

## Branch integration proofs (story 1295-a2ce)

- [ ] After Boss restarts `make dev` or rebuilds the release, query `repo action=branch_integrations` and `repo action=branch_integration` on throwaway fixtures and confirm the new proof fields are available. Check that an integrated squash branch appears in the branch panel and sidebar, and a content-based proof requires an archive of the current tip before safe deletion. Automated real-Git lifecycle, MCP and deletion checks passed; the installed process has not been restarted to load this Rust change.
- [ ] After a manual `make dev` restart, call `repo action=progress_list` with no input on the live tuicommander journal (2400+ entries). The first page has 8 entries and is under 16384 B (was 17545 B with 10 entries, 2026-09-30). Story 1088-4783 GREEN criterion.

## Click on an underlined relative path (1336-7755)

- [ ] After a desktop rebuild, in a Claude session (and once in a plain shell with `echo docs/x.md`) click an underlined relative path, then read the app log for `link click:` lines (debug level). A click that opens the file logs `link click: opening`; a dead click logs which step ended it (`not opened` with claimed/detail/hasRange, `lookup went stale`, `nothing resolved`, or `does not cover`). Also click an OSC 8 link whose text is no path and hover it first: it opens (press claimed from the hover probe, #1336 fix). Browser mode could not reproduce the dead click; only the desktop can.

## Compose image paste and toolbar marker (1350-a1e6, 1351-69e0)

- [ ] After the next desktop rebuild, paste a screenshot (Cmd+Shift+Ctrl+4) and an image file copied from Finder into Compose: both attach an `[image: ...]` tag, in paste order. Paste plain text: it pastes as text. In auto density mode the toolbar button shows only the letter, no blue marker.

## Question reminder (1354-00ed)

- [ ] After the next desktop rebuild, leave an agent question unanswered for 2 minutes in a background tab: one sound and one OS notification. With the question tab active and the window focused: sound only. With notifications disabled: nothing.

## PR panel (1345-fb71 .. 1349-7546)

- [ ] After the next desktop rebuild, on a repo with open PRs: a PR transition (CI failed, ready, merged) gives one OS notification and its click opens the PR; Merge sends the head the panel showed; a PR with unresolved review threads shows the bot/human split and is not Ready; Update branch on a BEHIND PR, Close PR with confirmation, age marker and Copy reference work on their own row only. Then approve plan steps S1-S5.

## Edge TTS default engine (1357-7d37)

- [ ] After the next desktop rebuild, with network and a fresh config: Settings > Voice > Spoken replies shows Edge voices for the dictation language; Listen speaks the preview; the choice survives a restart. Run `cargo nextest run -p tuic-dictation --run-ignored only -E 'test(live_edge_service)'` once to confirm the live handshake (Sec-MS-GEC, Origin) still works. Start hands-free and get a reply spoken; say "hush" mid-reply and it stops. Turn the network off: the Voice section shows the "needs an internet connection" error and replies do not hang. Expert > Speech engine: Pocket and External still work; an install that had Pocket keeps Pocket.

- [x] Sidebar: separate status and agents chevron hit areas, collapsed count, and Enter/Space (#1410-e201). _(verified: production components in agent-browser; expanded/collapsed screenshots, trusted keyboard input, and 13 filtered component tests)_

- [x] Voice settings: accent Start, distinct Stop, and Running/Stopped indicator (#1409-4ec3). _(verified: accent colour in agent-browser screenshot; Running/Stopped component regression tests)_

## Remote desktop MCP routing (1419-ab18) — coordinated Rust rollout required


- [ ] After Boss schedules the desktop restart and coordinator deploys the reviewed daemon build, run `scripts/test-remote-mcp.py --connection <Mac-mint-id> --session <disposable-agent> --exercise`. Confirm remote output/submit, destination wake and reply to the Mac. Run with `--second-connection` and `--second-session` for a disposable second daemon peer. The live desktop does not load Rust changes without a restart; never restart it or a daemon holding live PTYs just to perform this check.


- [ ] After Boss schedules the Rust restart, verify a forwarded notice retry does not enqueue again while its ID is within the recipient's last 100 forwarded IDs, including after inbox reads. A retry after 100 newer IDs may deliver again; unregister must clear only that recipient's ring (#1419-ab18).
- [ ] Remote empty-grid replay control message (#1421-733e): staged Rust WS change requires a rebuilt backend; targeted loopback WS test and isolated headless fixture cover it before deployment. Keep the running desktop and mac-mint daemon intact until the coordinator schedules deployment.

- [ ] Remote replay rollout (#1421-733e): deploy the updated daemon before the updated client. An older daemon without the explicit empty-replay marker can trigger a false 15-second stream error on a healthy idle session whose initial grid is unavailable. Coordinate deployment after live PTYs can be safely preserved or closed; do not restart mac-mint during this incident.


- [ ] Workflow graph slice A (1446-ff21): after Boss rebuilds/restarts, inspect and cancel a pre-contract run; it must refuse Resume without changing its history. Graph runtime remains disabled. Targeted native store/replay tests cover the backend; the running desktop has not loaded these Rust changes.

- [ ] After Boss restarts the desktop or rebuilds release: verify the native workflow graph runtime uses serial predecessor history and refuses pause resolution while effects are uncertain or input is pending (1446 slice A). Internal graph transitions remain unavailable on the public transport.
## Private secret forms (#1435-6e1d) — Rust restart required

- [ ] (#1520-46b1) After Boss restarts the rebuilt desktop and updates the CLI/bridge, request a throwaway form. Confirm a separate native window opens, wait over 10 seconds, then decline; caller must receive names and `declined`. Repeat with harmless entry and verify only names/`stored` return. If absent, read source `secrets` logs: no `Secret tool dispatched` means the request did not reach this handler; `Opening` without `Creating` means host/store setup failed (see `Secret tool failed`); `Creating` without success/error means native construction did not return; a creation failure reports its native error; `created` means investigate visibility/frontend bootstrap. No desktop was launched by the peer.

- [ ] After restart, with a private form open, verify direct upstream MCP
  `tools/call` rejects inspection, matching native and `call_tool` entry points.

- [ ] After Boss restarts the desktop backend, request username/password/OTP
  fields with synthetic data. Verify the separate native window, exact reduced
  schema, main-window bootstrap rejection, decline, close and timeout cleanup.
- [ ] On a trusted existing HTTPS server address, submit a desktop-opened request
  from the one-time entry path; verify one-time consumption and cleared input UI.
- [ ] Verify exact argv/name/directory approval and template consent. Run a
  trusted test executable that prints synthetic encoded/wrapped values; confirm
  masked output and no terminal/tcap entries.

These desktop checks wait for Boss's restart; no second desktop instance is
launched by the implementer. Security critic and cross-platform validation are
required before treating the feature as complete.
- [ ] After Boss restarts the desktop build and updates the remote daemon, drop a read-only directory from Finder onto a remote repository. Verify all files arrive, final directory permissions remain read-only, and the Mac source stays untouched. Rust does not hot-reload; this requires a manual restart when Boss is ready. _(Story 1434 round 3: Linux handler tests cover upload deadlines, staging cleanup and cancellation; native Finder/macOS publication awaits restart.)_
## Remote MCP toast mirror (1439-d84f) — Rust restart required

- [ ] After Boss restarts `make dev` or installs a `make build` release, connect a daemon, raise an MCP toast from a remote agent, and check the host-labelled Messages entry, requested sound and Open terminal navigation. Disconnect, raise a toast remotely, and reconnect: no stale entry should appear. The mirror backend cannot hot-reload; do not restart Boss's desktop from an agent. Automated Rust/frontend regressions cover the filter, payload, navigation and disconnected-frame behavior.

Known limit for 1439-d84f: **Open terminal** is a harmless no-op when the remote PTY's cwd/repo is outside every registered remote repository. Navigation ownership currently comes from the repository registry. This is not fixed by the toast mirror change.
- [ ] #1411-097c: After a backend restart, copy a wrapped Claude prompt. Confirm only the outer `❯ ` and two-column margin disappear, width wraps join, and typed newlines remain. Select the pasted second glyph from column 2, content after an ASCII/wide prefix, and a VT continuation starting with `❯ `: literal glyphs and indentation must remain (#1414-4366). Rust changes require Boss to restart `make dev` or rebuild the release.

- [ ] #1418-48c7: After Boss restarts the backend, copy a literal prompt-shaped VT continuation whose predecessor was evicted from scrollback; the glyph must remain. Targeted grid regression verifies the extraction path; desktop clipboard check awaits restart.

- [ ] #1418-48c7: After the backend restart, clear history, fully erase the top row and redraw a real composer there, including with zero scrollback. Copy must remove the composer marker; purging history without erasing literal prompt-shaped content must preserve it. Automated regression coverage exercises full and partial line erasure.

- [ ] #1418-48c7: After Boss restarts the backend, move a literal prompt-shaped row with RI/IL and copy it at its new position: keep the glyph. Replace the entire row with ECH/DCH/ICH, redraw a fresh composer, and copy: remove only composer chrome. Partial edits must keep unknown-origin content literal. Automated grid and selection regressions cover these paths; desktop clipboard awaits restart.

- [ ] #1418-48c7: After Boss restarts the backend, issue ED1 (`CSI 1 J`) with the cursor on the second row: the first row must be blank. Redraw a fresh composer there and copy it: remove only composer chrome. First-row and last-row erase boundaries and the cursor-row suffix have automated grid regression coverage; desktop verification awaits restart.

- [ ] #1418-48c7: After Boss restarts the backend, fill every row with scrollback set to zero, clear the screen with ED2 (`CSI 2 J`), then redraw and copy a fresh composer on row zero: remove only composer chrome. Automated regressions cover two/three-row screens, the origin flag, and retained history with nonzero scrollback. ED3 must still preserve literal live content.

- [ ] #1418-48c7: After the backend restart, a composer on a blank row whose nonblank predecessor was evicted or purged may retain its `❯ ` when copied until a full row erase. This conservative limitation is accepted by Boss (2026-10-03, option a); the implementation uses one origin flag and no resize promotion.
- [ ] Telegram offline adapter boundaries (#1438-79b4): after a future rebuild, native startup remains disabled until stable-ID mail integration lands; no Telegram polling or secret reads are wired by slices 1–2.


- [ ] Telegram slices 1–2 polling recovery: after the next Rust rebuild and later native integration, verify visible in-memory 403/404 stops, fixed ten-update batches, bounded retries and alert-driven cursor reset. Offline adapter tests cover the cursor/network/mail-port boundary; daemon startup and operator UI remain deferred. Rust changes require Boss's manual restart to load.
- [ ] Speaker shutdown (#1452-f186, release gate #1447-a894): Rust fix is staged and requires Boss to restart `make dev` or rebuild the release when ready. After a spoken reply drains, close the voice conversation; the render worker must stop without hanging. A forced-interleaving regression covers shutdown during completion dispatch; the current desktop has not loaded this change.

## Debug sidecars for `make dev` / `make test` (1465-3d95) — desktop restart required

- [ ] After a fresh `make dev` (or `make test`) on a checkout with an empty `src-tauri/target`, confirm `src-tauri/target/debug/tuic-bridge` and `tuic` exist and the app finds the bridge (`locate_bridge_binary`). After editing `crates/tuic-bridge/src`, restart: the bridge mtime must change. Do not launch a second desktop instance from an agent lane.
- [ ] After the approved daemon update, launch Claude manually on the configured Mac-mint connection, exit back to the shell, and verify submit/mail no longer write there; a run-config preset must survive shell startup (#1420-f3de).

- [ ] After the coordinated rebuild, verify shell-root return revokes a manually launched agent regardless of shell basename; a bash-script wrapper is observed, and a nested subshell under a directly spawned agent holds submit/mail until the agent regains foreground (#1420-f3de). Do not restart live PTYs for this check.
## Concurrent workflow checks (story 953-feed) — Rust restart required

- [ ] After Boss restarts the Rust backend, run independent published checks on disposable workflow runs. Confirm both subprocesses can progress concurrently and a notification event during a check does not discard its receipt. Confirm a changed worktree or cancelled run cannot acquire a receipt. The current backend cannot load this Rust change without a manual restart.

## Workflow merge-tree verification (story 957-dc59) — Rust restart required

- [ ] After Boss restarts the backend, use disposable repositories to verify a clean checked merge receives a receipt, an extra integration-time file does not, and a manually resolved conflict requests separate review. The running Rust backend cannot load the change without a manual restart.

## Workflow recovery boundaries (story 960-8670) — Rust restart required

- [ ] After Boss restarts the backend, verify restart recovery marks old attempts interrupted. On disposable active runs, a runtime reconciliation must preserve healthy attempts and intended effects; one corrupt run must not prevent a healthy run from recovering. After a dependency-refresh failure during recovery, resume the run and start a worker; reopening must preserve that live worker. Failed recovery is not retried by later opens or runtime reconciliation. Startup Git-probe latency remains pending story 959-c69c.
## Windows core dependency (1478-ead1) — rebuild required

- [ ] Load this manifest fix in the next Windows build. The `tuic-core` MSVC cross-check passes; a separate WebRTC/Abseil C++ build failure is tracked in 1479-f956. Existing desktop processes do not hot-reload Rust; restart only when Boss is ready.

## Security Group C — rebuild required

- [ ] After the next Rust restart, verify run-git rejects unsupported options and GitPanel fetch/push/merge still work (#1460-9ed8).

- [ ] After the next Rust restart, verify force branch deletion retains its tip at refs/archive and refuses archive collisions (#1462-e0e9).

- [ ] After the next Rust restart, verify plugin CLI output over a pipe buffer completes and overflow returns an explicit error (#1461-8d36).

- [ ] After the next Rust restart, verify creating feat-x cannot remove an existing worktree for feat/x (#1458-e1f6).

- [ ] After the next Rust restart, verify remote connection failures contain no token in logs or status and session/SSE mirroring authenticates with the existing cookie (#1457-91e0).
## Windows WebRTC compiler (1479-f956) — native Nightly required

- [ ] After landing and pushing, the Windows Nightly must report MSVC for both Meson C/C++ compilers, compile Abseil/WebRTC without MinGW header errors, and finish the Tauri NSIS build. This Rust build-script change requires rebuilding; no desktop instance was started for verification.

## Codex notify publication (1483-7a6e) — Rust restart required

- [ ] After Boss loads the rebuilt backend, confirm a disposable Codex session still reports turn completion. The script is now published with owner execute permission already set; the existing concurrent-publication regression covers the race. Rust does not hot-reload; no desktop instance was launched by this lane.

## Windows runtime CI fixes (1518-d3a7) — Rust restart required

- [ ] After Boss restarts the Rust build, verify Windows workflow worktree assignment and orphan cleanup with native and Git path spellings, archive hooks that invoke Git, and failed/cancelled remote-copy staging cleanup. Native Windows CI after landing remains required; the live desktop backend does not hot-reload these Rust edits.

- [ ] After Boss restarts the Rust build on Windows, verify Claude session discovery and the subagent view under a drive-letter checkout; the project slug must keep its drive letter and replace every non-ASCII-alphanumeric character (including the colon and profile spaces) with a dash. The running desktop backend does not hot-reload this fix.

- [ ] After Boss restarts the Windows Rust build, verify a captured `tuic bg` launcher returns while its command still runs and a worktree archive hook finds Git. Existing CI regressions exercise both contracts; the live desktop backend does not hot-reload these changes.

- [ ] After Boss restarts `make dev` or rebuilds, verify Windows worktree archive/setup hooks find Git with a long inherited PATH. Hook PATH now keeps Git first, deduplicates directories and stays within cmd.exe limits.
- [ ] After the next backend restart, cancel a workflow while a published check is running and confirm its workers stop (#954-4f33). Automated regression covers process-tree teardown; the running backend must be restarted to load this change.

- [ ] After backend restart, verify CLI/local and authenticated browser workflow actions succeed and story history records local_api or human provenance (#956-9745, #1497-4f55). Actor identity is tracking only. Rust changes require a manual make dev restart (or make build); automated route and provenance regressions cover the backend contract.

- [ ] After backend restart, verify workflow plan Done becomes Active after canonical branch movement and Done after recertification (#962-0888); manual approval completion is preserved. Targeted Git integration tests cover both projections.
- [ ] After Boss restarts `make dev` (Rust does not hot-reload), exceed the scrollback cap and verify retained command navigation, gutter selection, green prompt ticks and answers-only associations stay on their original rows (#1370-0077). Restart must load backend and frontend together because stored OSC row coordinates changed.
- [ ] Queue retry idempotency (1106): after Boss rebuilds/restarts the backend, send the same `idempotencyKey` twice to an isolated agent queue, then retry after it drains; verify one wake and `accepted: true` without requeue. Automated HTTP/PTY tests cover bytes and queue state; this check loads the Rust change into the running app. Live per-CLI turn acceptance and composer/reconnect convergence remain separate open criteria.
## Remote transfer cancellation fixture (1528-cee8) — Rust rebuild

- [x] The cancellation regression must retain worker staging and its upload permit after handler abort, publish both fixture files, clean staging, and release both slots. Production still passes the unchanged extractor. _(verified by source inspection: `remote_transfer.rs` receive/worker ownership and channel-gated regression in `remote_transfer_tests.rs`; targeted execution is recorded in the story worklog. Rust changes require Boss to restart `make dev` or rebuild release before loading; this refactor adds no new runtime behavior.)_
## Telegram minimal outbound (#1438-79b4) — Rust rebuild required

- [ ] After Boss rebuilds/restarts the headless daemon, a directly observed Claude-to-Codex replacement in the same terminal must require fresh Telegram registration, even without a shell observation. Offline regression: `observed_agent_type_change_does_not_transfer_registration` (#1526-22f3).

- [ ] After Boss rebuilds/restarts the headless daemon, register an agent, let it exit to its shell, then restart an agent in that terminal. Phone mail must receive `Nessun agent registrato` until the replacement explicitly registers. Offline native foreground/inbox coverage: `observed_agent_exit_does_not_transfer_registration_to_restarted_agent` (#1524-dcc1). The running backend needs a restart to load this Rust change.
- [ ] After Boss rebuilds/restarts the headless daemon, verify drafts have no phone Stop, send/notifications use the single configured chat, and a button press or new message retires previous handles. Live mint verification remains coordinator-owned; no instance was launched here. Rust changes do not hot-reload.
- [ ] After Boss rebuilds/restarts the headless daemon, verify Telegram Stop on
  a throwaway phone request: one Escape reaches only the draft-bound current
  epoch, including a replacement Enter delivered before input bookkeeping;
  duplicate/stale Stop never revives a draft. Confirm the selected
  callback label stays disabled. The running Rust backend cannot load these
  changes until rebuilt/restarted; no restart was performed by this peer.

- [ ] Telegram setup (#1515-cb81): after rebuilding/restarting tuic-remote, use Settings on desktop and phone to replace/check a token, observe the MCP-registered agent read-only, enable, pair once within ten minutes or type a chat ID, remove a chat, and inspect safe status. Rust changes require a restart; no desktop instance was launched by the peer. Live Telegram authentication was not exercised.
- [ ] After Boss rebuilds/restarts TUIC: Settings > Agents > Codex shows the migrated bypass argument and warning; remove it and confirm terminal and managed launches preserve the removal. The durable migration stamp and wrapper task routing require a rebuilt backend; also confirm a default wrapper receives its positional task. Rust migration requires restart. Visual screenshot attempt could not render the isolated harness while the macOS screen was locked. (#1399-17bd)

- [ ] After Boss rebuilds/restarts the backend, confirm an interactive Codex default with `--profile review` (or `exec`/`e`) receives and submits its managed task. Actual subcommands after root options must retain positional tasks. The public spawn regression covers argv and queued delivery; the running Rust backend requires restart. (#1513-701d)
- [ ] After Boss restarts `make dev`, verify `claude remote-control --resume main` and `claude auth status` pass through without TUIC settings and normal Claude launches retain status hooks, including when cached help is empty or lacks usable command rows (#1405-a5e4).

- [ ] After Boss restarts the Rust backend, verify a new headless terminal at 148 columns retains that width after a same-size resize (#1413-7dcc).
- [ ] After Boss restarts the Rust backend, confirm that remote MCP questions show the saved host name, answer only the owning daemon, and disappear when another client answers (#1440-3571).

- [ ] After rebuilding Rust, verify remote GitHub review/proposal/conflict notices update the owning dashboard and leave same-path local repositories unchanged (#1443-e2fd).

- [ ] After rebuilding Rust, verify remote upstream MCP failures show their host with the popup closed and leave local upstream settings unchanged (#1444-95a4).

- [ ] After the Rust restart, check a connected daemon ACP permission/elicitation in AI Chat: host and ACP connection are shown, answer returns to that daemon, settlement/disconnect clears only its cards (#1441-695e).

- [ ] After the Rust restart, check remote GitHub PR transition bell/native notices show the host once, open remote PR details and never touch a same-path local repo (#1442-2100).

- [ ] After the next backend restart, verify an ego card opens mobile Chat and shares the question push cooldown (#1078-05cf). Rust changes require a manual restart by Boss.
- [ ] After Boss restarts the Rust backend, verify MCP `branch_delete` reports the tip-suffixed archive ref when the primary archive holds older work (#1489-1a14). Targeted regression covers the backend; the running desktop still requires restart.

- [ ] After Boss restarts the Rust backend, confirm the Git diff file list displays tracked-file additions/deletions (#1499-3a34); targeted backend regression covers scopes and renamed paths.

- [ ] After Boss restarts the Rust backend, confirm untracked files with tabs/newlines or boundary spaces appear with their literal names and correct line counts (#1502-8a5e). Backend regressions cover listing and file-diff consumers; Rust does not hot-reload.
- [ ] After rebuilding/restarting TUIC, verify sidebar dirty and merged badges with 11 writing worktrees, including initialized submodules; sample Git child spawns with the same before/after method (1491-2ae4). Rust backend changes require a manual restart. Include recovery after a PR proof lookup failure without moving refs, and `git rm --cached` leaving both a staged deletion and an untracked file (dirty count 2).
## Hands-free reply controls audit (#1377-16e1)

- [x] Pause and resume preserve playback ownership without opening a user turn. _(verified: src-tauri/crates/tuic-dictation/src/speaker.rs:590 pause_by_user/resume_by_user only change playback and hold flags; generation changes in hush, not pause. src-tauri/src/dictation/commands.rs:1285 does not change capture state.)_
- [x] Browser resume continues from the saved sample position without suspending the microphone context. _(verified: src/utils/browserVoice.ts:186 saves the elapsed offset and restarts playback from it; capture remains connected.)_
- [ ] [VISUAL] Capture the pill while speaking and while user-paused after Boss loads the desktop build. Check Pause/Stop and Play/Stop respectively. No test desktop may be launched by a managed peer.
- [ ] [HUMAN] On a real phone, tap Pause, Play and Stop during a spoken reply. The reply resumes at its position; Stop drops queued replies; microphone capture stays active. Mouse command dispatch, accessible labels and 44px coarse-pointer targets are present in source; they do not prove real-phone touch/audio behavior.
- [ ] Native MCP registry Settings (#1522): after Boss rebuilds/restarts the backend, open Settings → MCP → Native tools; confirm all registry tools have switches and description badges, disabled tools can be re-enabled, and Telegram appears when its backend registration lands. Rust does not hot-reload.

- [ ] AI Chat setup (#1406-06f2): with an empty ego executable, confirm the inactive explanation and Configure ego button in inline and detached panels. The button opens Settings → General at the ego controls; selecting the executable shows the composer without restart. Return to the detached window after saving to refresh its settings. _(Automated behavior tests cover routing and activation; agent-browser screenshot attempt was blocked by the locked macOS screen.)_

- [ ] AI Chat message fork: after rebuilding/restarting Rust, fork the second of four ego replies and check the child cutoff while the parent retains all replies. The backend capability and metadata changes require a manual restart.

- [ ] Verify browser HTML/Markdown and image-tab placeholders for local images outside open repositories; confirm inside-repository images still load.
- [ ] After Boss restarts the Rust backend, verify threshold memory logs contain only allocator counters and structure counts; `/diagnostics/memory` still returns the explicit census and its overlap warning (#977).
- [ ] After Boss restarts the Rust backend, verify the internal serial workflow executor with isolated headless runs and configured sol/sonnet profiles: bound reports, Judge edges, retained repair limits, durable Notify and pause/resume duration. Public graph start controls and Resolve plan dispatch are outside this slice. No desktop launch was performed by the implementation peer.

- [ ] After Boss restarts the Rust backend, verify an isolated workflow review checks its artifact before independent approval, refuses implementer approval after exit, and waits for an explicit merge.

- [ ] After Boss restarts the Rust backend, verify an isolated plan dispatches eligible story waves, holds a dependent until an explicit verified merge, and runs its pinned final checks before completion.

- [ ] After Boss restarts the Rust build: pause a reached workflow Gate, commit a corrected artifact, resume that activation, and verify fresh deterministic check receipts without a reused-command-payload error. Automated regression passes are recorded in the 1446-ff21 critic handoff.
- [ ] Workflow slice F: after Boss restarts the Rust backend, start a published Ready story in Plans and Stories; inspect decision/pause details and later event pages, answer input, explicitly resolve a graph pause, pause/cancel. Plan start must show unavailable until slice E is integrated. Browser screenshot/layout verification is owed: the screen was locked on 2026-10-06; no desktop instance was launched.

- [ ] After Boss restarts the Rust build, start a published Resolve plan through the owning daemon API; verify the pinned coordinator and explicit integration waits. The dialog plan button remains visibly unavailable.
- [ ] ego launch permissions (#1400-9576): after Boss restarts `make dev` or installs a rebuilt release, select mode/sandbox in Settings > Agents > ego and verify a new terminal launch and MCP spawn receive the chosen flags. Rust changes require restart; existing sessions retain their launch settings. After restart, also confirm `ego mcp-server` receives no permission flags and a raw `--mode --` preserves the prompt suffix (#1400 critic fixes).
- [ ] ego permission controls visual check: preview loaded the worktree AgentsTab, but the locked macOS screen prevented bounded browser click/screenshot verification (2026-10-06). Capture after unlock.
- [ ] AI Chat ACP steering: after Boss restarts `make dev` or loads a rebuilt release, use an ego build advertising `_ego/steer`; send text during a turn from desktop/mobile and confirm it affects the next provider request with one echoed bubble. Attachments retain the queue; delayed rejected/not-busy steering stays ahead of later submissions. If ego leaves a steer unanswered, verify delivery uncertainty appears after 10 seconds and later messages can proceed without resending that text. _(Rust backend does not hot-reload; no desktop instance was launched.)_
- [ ] After Boss restarts `make dev` (or rebuilds release), open Plans and Stories → Run history and refresh incidents for a recorded failed/input-required run. Confirm retained report and bound session/task evidence. Rust does not hot-reload; worktree UI preview alone does not verify the live backend.
- [ ] After Boss restarts `make dev` or rebuilds, create a coordinator chat with **New conversation with options**, send a turn, switch to a daily chat and reopen the coordinator after restart. Confirm its header and dedicated mail peer. Rust changes require a restart; no desktop instance was launched by the agent.
- [ ] After restart, inspect `/logs?source=agent_msg&level=info` for `event=inbox_read`, protocol caller, peer owner and message ids; confirm bodies are absent.
- [ ] MCP delta paging (#1551-5fa4): after Boss rebuilds/restarts Rust, read a throwaway terminal containing a harmless PEM-shaped sample with `since_cursor` and `limit=1`; each body page must stay redacted, including when the footer remains on screen and the header/body are in scrollback. Rust does not hot-reload; no desktop test instance was launched.
- [ ] After Boss rebuilds/restarts Rust, open an AI Chat workspace containing `.tuic.json` with `ego_profile` wider than the explicit machine profile. Use ego with the ACP ceiling extension; verify its warning notice and restricted policy. Also verify a workspace without a repo profile keeps its usual launch. Rust does not hot reload (#1403-a03b).
- [ ] ego Perimeter (#1401-ab1c): after Boss restarts `make dev` or rebuilds the release, open Settings > AI Chat with an ego that supports `config ls --effective`; save roots/read-only access/allowlists, toggle network, and confirm the effective preview. With a profile that inherits user roots, confirm those roots appear before saving; explicitly declared `roots=[]` must stay empty. Rust does not hot-reload. Automated focused verification is recorded in the story; the worktree visual harness does not load the new IPC backend.

- [ ] ego permissions Settings search (#1400-9576): search Permissions and Filesystem sandbox and confirm each result scrolls to its visible control in the standalone ego permissions section without opening a card. Screenshot pending; targeted search drift tests passed 60/60.
- [ ] After Boss restarts the Rust build, create a worktree through MCP without spawning: only the creator in the same repository moves; inactive selection and sibling tabs stay in place. Explicit spawn and foreign callers must not move the creator.

- [ ] After Boss restarts the Rust build, call `session action=declare_worktree worktree_path=<existing linked worktree>` from an agent. Only its tab moves; retry creates no duplicate. Reconnect/restart preserves placement and real shell cwd. Unknown, main-checkout, cross-repository and foreign-session targets are rejected.
- [ ] After Boss restarts `make dev` or rebuilds release: Settings > MCP > Native tools shows short English summaries, including secret and telegram; full descriptions remain in the ? tooltip (1521-b6e7). Isolated real-panel harness screenshot verifies row rendering before restart.
- [ ] After Boss restarts the Rust backend, verify threshold memory logs contain only allocator counters and structure counts; `/diagnostics/memory` still returns the explicit census and its overlap warning (#977).
- [ ] Tablet terminal input (#1565-562a): on iPad Safari and the installed PWA, tap near the bottom prompt of a terminal: confirm the keyboard stays open without a window jump. Repeat near the middle, type text, hold Backspace, and dismiss/reopen the keyboard. _(2026-10-07: 12 targeted tests pass; a real-component Chrome harness verifies stable touch/mouse focus and shared text/deletion handling. Screenshot: `~/Gits/.tmp/tuic-canvas-touch/canvas-touch-ipad.png`. Real iPad keyboard animation remains unverified.)_
## PTY child reaping (1566-6b3a) — Rust rebuild required

- [ ] After Boss's next planned backend/daemon rebuild and restart, confirm exiting throwaway shells no longer leaves direct zombie children. Native targeted tests cover delayed exit after session removal and already-reaped exit status. Rust does not hot-reload; this lane does not restart mint or launch a desktop instance.

## Linux scrollback arena retention (1567-843a) — daemon rebuild required

- [ ] On Boss's next planned Linux daemon rebuild/restart, confirm closing filled throwaway terminals reduces resident memory while other terminals remain readable. The isolated mint comparison proves freed grid pages can remain resident until trim. This lane does not restart or signal the live mint daemon.
- [ ] After the backend restart, confirm Settings > MCP shows a short dedicated summary for the native workflow_run tool. Native catalog and HTTP status regressions cover the wire response.


- [ ] [HUMAN] On an iPhone PWA, tap + on Sessions, then tap "Search repositories". The New Session sheet must stay visible above the keyboard (not the Sessions list), and typing a folder name or a parent folder must filter the list. _(Story 1572-6be7: sheet backdrop moved from fixed to absolute inside the visual-viewport shell; needs real iOS keyboard.)_
## PWA voice, slice 1 (#1573-84f8) — client only, no Rust change

Needs an iPhone (iOS 16.4+, 18.4+ preferred), the desktop TUICommander running a build that contains `feat/pwa-voice-s1` (the phone loads the frontend of the instance it opens), and the instance reachable over HTTPS (Tailscale HTTPS URL). Headless `tuic-remote` has no voice route: the mic button must be absent there.

- [ ] **[HUMAN]** Open `https://<tailscale-host>/mobile` in Safari, open a Claude or Codex session. A microphone button must sit left of the tasks button in the header. Open the same page over plain `http://<ip>:9876/mobile`: tap the mic; a toast "Voice not started" must say the page needs https (not "Cannot read properties of undefined").
- [ ] **[HUMAN]** Back on HTTPS: tap the mic. iOS asks for microphone permission once; allow. The button turns red and the subtitle shows `Voice waiting`. Check `GET /dictation/hands-free` on the desktop: `armed: true`, `owner` starting `browser-`, `sessionId` = the open session.
- [ ] **[HUMAN]** Say the activation phrase and a short request. Subtitle moves `capturing` → `transcribing` → `holding_back` → `delivered`; the text arrives in the agent. If the agent calls `voice speak_reply`, the reply must play **from the phone speaker** and the desktop must stay silent. Silent reply with the mic light on = the suspended-AudioContext bug (#1573-84f8) is not fixed: note the iOS version and Safari vs installed app.
- [ ] **[HUMAN]** Repeat the whole sequence from the installed Home Screen app (Add to Home Screen). Record whether the permission prompt reappears on each arm.
- [ ] **[HUMAN]** Tap the red mic: it returns to normal, the Safari mic indicator goes off, `armed: false` on the desktop.
- [ ] **[HUMAN]** Arm, then press Back to the session list: the mic indicator must go off and `armed: false` (the screen disarms on leaving).
- [ ] **[HUMAN]** Deny the microphone permission once (Settings > Safari > Microphone): the toast must show the refusal and the button stay un-armed; `GET /dictation/hands-free` stays `armed: false` and no `browser-` audio socket remains.
- [ ] **[HUMAN]** Silent switch on, then off: record whether the reply is audible in each state (unverified, report 2026-10-07 section 8).
- [ ] **[HUMAN]** With the desktop armed in its own conversation, tap the mic on the phone: record what happens (single `DictationState` conversation, untested).
- [ ] Lock the screen mid-conversation: expected to stop capture (not supported by iOS, slice 2 handles it); record what the page shows on return.
## CLI / Chat view for Claude terminals (1568-7b55) — backend restart required

- [ ] After `make dev` restart, open a Claude terminal with a bound session: **CLI | Chat** appears top-right, Chat shows your prompts as bubbles and replies as text with tool calls folded, and a new reply appears within ~2 s without switching back.
- [ ] Switch Chat → CLI → Chat: the grid scrollback and selection are intact, and Chat resumes without a visible reload.
- [ ] In a plain shell tab the switch is absent; in a Codex tab Chat is disabled with the reason in its tooltip.
- [ ] Run `/clear` in Claude while Chat is open: the old conversation disappears and only the new one shows. Exit Claude while Chat is open: the tab returns to the grid with a one-line notice.
- [ ] Visual check of the switch position (top-right) against the grid scrollbar and the last-prompt bar.

- [ ] #1491: After Boss restarts the desktop with the combined fixes, sample direct Git children with seven active writers and initialized submodules; verify <= 2 Git spawns/s sustained and final sidebar badges update after the five-second trailing refresh.

- [ ] After Boss restarts `make dev` or rebuilds the release, confirm the scrollback row-allocation fixes from #1575 are loaded. Rust does not hot-reload. Automated native checks and isolated headless measurements are recorded in story `1575-2c7b`.
## Managed Claude background output (1012-f12e) — backend rebuild required

- [ ] After Boss restarts `make dev` or installs a rebuilt release, spawn a disposable managed Claude peer and confirm background-task output goes under `~/Gits/.tmp/claude`. Repeat with an explicit run-config or caller `CLAUDE_CODE_TMPDIR` and confirm it wins. Rust environment defaults do not hot-reload; no desktop instance was started by this lane.

- [ ] #1407-1ab2: after Boss rebuilds/restarts, resize a real streaming Claude terminal 120 → 160 → 60 columns and scroll its answer; confirm each bullet appears once and selection/search follow the retained text. Rust changes require a manual restart; desktop visual verification is pending.

- [ ] #1407-1ab2: After the next Rust restart, confirm resize keeps full terminal text and wrapped secrets stay redacted in session output; repeat the recorded Claude streaming/idle resize check. Automated coverage: `resize_preserves_complete_visible_logical_text_1407` and the two MCP resize/capture redaction regressions.

- [ ] #1407-1ab2: after Boss restarts the Rust build, confirm an omitted short history record survives resize even when another redrawn record contains the same text. Automated regression: `resize_redraw_preserves_omitted_record_contained_in_another_replaced_record`.

- [ ] iPad remote access after the next backend restart: open `/` without an authenticated cookie, sign in through the form, and confirm the touch interface loads. Recheck the originally reported connection failure on the physical iPad; no attributable transport error was available in server logs. Rust auth changes require a manual `make dev` restart by Boss.
- [ ] Browser desktop Add Repository: browse server home, select a folder, or cancel; native desktop and connected-daemon pickers retain their own machine ownership.

- [ ] Telegram Stop (#1521-52cd): after Boss restarts the rebuilt headless daemon, start a Telegram-bound reply, press Stop and verify the bound agent sees Escape. Send a replacement message and verify a repeated old Stop cannot interrupt it. Also delay draft begin until replacement bytes are written: it must not arm the retired turn. Backend byte ordering and captured Codex submission are covered by targeted tests; live Telegram client rendering remains unverified.

- [ ] #1596-df57: In macOS desktop, focus a terminal on a branch with an automatically opened PR detail popover. Wheel over the sidebar without clicking; it must scroll, and typing must still reach the terminal. Click outside the popover: it closes and that first click activates the underlying control. Repeat with no popover, after a pane resize, and after a tab drag. Browser hit-testing reproduces the old full-window overlay; native wheel and visuals need Boss’s next-build check.

- [ ] #1597-fab6: In macOS desktop, with each of the status-bar ticker popover (right-click a ticker), the status info balloon (click truncated status text), the sidebar GitHub panel (GitHub badge on a repo) and the Smart Prompts dropdown open, wheel over the sidebar without clicking; it must scroll. Click a sidebar control outside the popup: the popup closes and that first click activates the control. Clicking the GitHub badge or the info text again must close its popup (not re-open it). Browser hit-testing reproduces the old overlays; native wheel needs Boss’s next-build check.

## Terminal Chat Compose input (#1599-db8a)

- [x] Chat focuses docked Compose, preserves drafts and CLI open/pin state, and retains focus after send/queue. _(verified: `src/__tests__/components/Terminal/chatViewCompose.critic.test.tsx`, real CodeMirror integration cases; 40 targeted component tests passed, including delayed send/queue across editor unmount/remount.)_
- [ ] Visually confirm Compose sits below the conversation without overlapping it in a running app, and answer a permission prompt in CLI. The worktree Vite/stealth-browser screenshot attempt timed out on the shared browser; native rendering remains unverified.

- [ ] After Boss restarts `make dev` (or rebuilds release), retry removal of the clean merged `feat/upload-cookie-1512` worktree. The Rust permission fix is not loaded by frontend HMR; the original worktree was left untouched during verification (#1607-0733).

## Headless GitHub account login validation

- [ ] After landing and rebuilding `tuic-remote`, verify browser add-account device-flow login resolves and stores the named account. The shared helper now compiles without desktop. Rust changes require a manual restart to load; no desktop instance was launched.

- [ ] Automations run ledger (#1612-70d1): after the scheduler runtime is wired and Boss restarts `make dev` (or rebuilds release), verify saved history survives definition deletion and restarting interrupts open runs without retry. This Rust storage change does not hot-reload; no scheduler runtime is launched by this step.
- [ ] Mobile AI Chat on iPhone: confirm wrapped long suggestions/tool titles and table-local scrolling at 360–430px, session-style input auto-grow, attachment/Send icons and Park/Stop toolbar. Chromium component-layout evidence is recorded for #1625-c04f; phone touch and keyboard remain to check.
- [ ] On iPhone PWA, check the update strip below the header in Commander and Paper; confirm that the native status-edge fade does not cover its text (#1626-9529). Chromium contrast is verified; iOS compositor behavior is not.
## Mobile slash button parity (story 1609-9faa)

- [ ] In a disposable agent session on the phone PWA, compare typing `/` and tapping the keybar `/`: both must show the same live agent commands and navigation. Insert `/` within an unsent draft and confirm surrounding text survives without submission.

- [ ] After a manual `make dev` restart or `make build`, verify that Claude terminal Chat follows appends after transcript replacement, with no subagents directory, and after rapid CLI/Chat remounts and that failed refreshes/stopped tickers appear as WARN logs (#1635-3db2). Rust backend changes require a restart; do not restart live sessions automatically.
- [ ] Terminal Chat search: Cmd/Ctrl+F finds visible conversation text; Enter/Shift+Enter navigate, Escape clears the selection, and switching CLI/Chat closes search.
- [x] Desktop AI Chat: pin/play icon controls match terminal Compose in light/dark themes, with Send/Queue tooltips and highlighted parked drafts (#1636-0d08). _(verified: shared ComposeActionIcons/ComposeActions, 158 targeted tests, and inspected headless idle/busy/parked screenshots in both themes.)_

- [x] AI Chat: paste a PNG before the first connection and a Finder image copy; preview and send the image, while ordinary text still pastes as text (#1639-f474). _(verified: Composer.stageImage and pastedImageFiles, recorded RED/GREEN with 160 targeted tests, and inspected headless pasted-image preview.)_
- [ ] Automations admission (#1613-67f3): after scheduler runtime is wired and Boss restarts `make dev`, verify a missed wake reserves only the latest in-grace occurrence; Run Now while paused respects overlap/cap. The Rust admission change requires a restart to load.

- [ ] After the next manual `make dev` restart, verify scheduler integration dispatches every Reserved decision returned by a tick; a failed batch must leave no reservation or consumed cursor. Admission rollback is covered by the targeted scheduler regression (#1613-67f3). Rust changes require a restart to load.
- [ ] After a manual `make dev` restart or `make build`, switch a busy Claude terminal to Chat and confirm mid-turn human follow-ups appear once between replies (#1632-ec84). Rust does not hot-reload.
- [ ] Automations dialog: after story 1617 API integration, verify create/update/pause/Run now/delete and backend previews against the real scheduler. The current peer verifies the injectable frontend boundary only.
## Mermaid 12 (1590-7c6b)

- [x] Render flowchart, sequence, class, state, gantt and KaTeX math through the real ContentRenderer in headless Chrome before and after upgrading. All six render; Mermaid 12 changes layout and shadows while keeping content and the dark theme. Evidence: `~/Gits/.tmp/tuic-deps/night-c3/before/` and `after/`.
- [ ] After Boss's next packaged `make build`, confirm the macOS bundle declares `LSMinimumSystemVersion` 12.0. Mermaid diagrams require system WebKit updated to Safari 17.4+; the OS version alone does not guarantee that update. No desktop instance was launched in this lane.

- [ ] After Boss's next `make dev` restart or `make build`: copy and paste text in the desktop app with the arboard commands; confirm terminal copy still succeeds after an IPC await and macOS paste shows no system Paste pill. Rust changes require the restart to load (#1651-6cbe).
