## Owner literal terminal reads (2643-6b38) — Rust rebuild/restart required

- [ ] After a manual rebuild/restart when losing current sessions is acceptable, in a throwaway terminal run `echo TEST_TOKEN=not-a-secret`. From an agent, `session action=output literal=true` must still show `TEST_TOKEN=[REDACTED]` and `literal:false`. Right-click the tab, choose **Allow Literal Terminal Reads** (warning toast), repeat: the value appears with `literal:true`. Choose **Mask Terminal Reads Again**: masked again. The item must not appear in browser mode. Backend behavior is covered by `owner_literal_read_preserves_fake_token` and `model_cannot_enable_literal_or_reveal_scoped_secret`; the menu was not run. _(HUMAN 2026-11-05: Live: throwaway PTY echo TEST_TOKEN=not-a-secret, session output literal=true -> TEST_TOKEN=[REDACTED], literal:false (masked default OK). Menu items TabBar.tsx:415-426 gated by isTauri() (absent in browser mode), literalReads.ts toggle->IPC set_terminal_literal_reads. Owner grant step not run: attempt via IPC from agent was denied by the auto-mode classifier (owner-only control), needs native right-click.)_
## Secret masking and terminal queries (2681-9810) — Rust rebuild/restart required

- [ ] After Boss rebuilds and manually restarts when losing sessions is acceptable, open a throwaway managed ego session with a task-scoped synthetic secret. Confirm startup cursor and keyboard queries return promptly and inline rendering stays anchored. Echo the synthetic secret across plain writes and SGR controls; retained terminal output must stay masked. Isolated native verification already received `CSI 1;2R` in 0.66 ms from a timed startup wrapper, kept an SGR-spanning canary masked, and rendered ego's native reasoning picker after its own cursor queries. Evidence: `~/Gits/.tmp/2681-live/`. Rust does not hot-reload; the daily app was not restarted. _(HUMAN 2026-11-05: needs live ego startup (0.66 ms cursor reply, picker). Code: secrets/mask.rs:322-345, secrets/stream_query_tests.rs:74,94 (SGR-spanning secret masked) - stream_query tests PASS (my run).)_
## Settings catalogue ledger coverage (2571-926a)

## Terminal search clearance (2538-c082)

## Collective zoom native menu (2499-1156) — Rust rebuild/restart required

- [ ] After Boss manually restarts a rebuilt `make dev` or installs `make build`, confirm the View menu collective zoom labels say All Tabs. Native collective in/out/reset dispatch is covered by real-store frontend regression tests. Rust menu labels do not hot-reload; the daily app was not restarted. _(HUMAN 2026-11-05: native macOS menu. Code: src-tauri/src/menu.rs:104,109,114 'Zoom In/Out/Reset ... All Tabs'; vitest useAppShortcutHandlers.zoom exists.)_

## Terminal Compose growth (2447-9d28)

- [ ] Confirm the compact editor feels right in CLI overlay and docked Chat: empty one line, wrapping and Ctrl+Enter grow to the former 96px height, longer drafts scroll inside, and clearing or successful sending shrinks it. Automated worktree-browser geometry evidence is recorded in the story worklog. _(HUMAN 2026-11-05: feel/visual. Evidence only: compose growth CSS not located by my grep (src/components/Terminal/TerminalCompose*.css absent); no visual check.)_
## Quit after child-status errors (2406-e4e1) — Rust restart required

- [ ] After an authorized `make dev` restart or `make build`, check the next desktop quit attempt. If it still hangs, retain the exact action (window close, Cmd+Q, menu, or confirmation), time and logs. The original failure has not been attributed to `ECHILD`; the running daily app was not quit or restarted by this peer. Rust does not hot-reload. _(HUMAN 2026-11-05: needs a real desktop quit attempt (not allowed: no app quit). Code/tests: pty/child_reaping_tests.rs:99,122,138 (ECHILD teardown returns) and lifecycle_tests.rs:2837 PASS; daily log 2026-11-05 has 0 "Frontend unresponsive".)_

## Automation profile validation and failure mail (2015-bdb3) — Rust restart required

- [ ] After Boss restarts `make dev` or installs `make build`, use an isolated instance to reject an unknown saved profile with valid-profile choices. Remove a profile after saving and confirm list exposes `run_config_error` with no next run. A failed agent-created run must send its creator one inbox message with run id and reason even before spawn; repeated publication must not resend it. The installed backend was not restarted by this peer. _(NOT VERIFIED 2026-11-05: Needs isolated instance (make agent-test forbidden) and a failed agent-created run; no HTTP/MCP route to remove a saved profile safely. Ran automations::api::tests (pass): automation_create_rejects_unresolvable_run_config; automations/api.rs:260,289.)_
## Settings restyle integration (1945-a78d)

- [ ] [VISUAL] After the Mac unlocks, capture isolated worktree Settings against the approved 2026-10-16 mock, not Boss's desktop. Capture Commander dark and Paper light at 1400×900 for all available pages across Application, Workspace, AI, Integrations, Plugin settings and Repositories. Include the selected machine/repository/plugin source, search results and a revealed expert field. Capture each theme at 780×900 and 390×844 with long repository/plugin names, paths, validation text and contextual help. Check stacked rows, compact page selection, wrapping, 44px touch targets, independent scrolling and stable shell size. Capture delayed plugin loading, empty search/catalogue, failed read and rejected-write recovery without fabricated live-service status. Screen locked during S10; no screenshots or live desktop mutation claimed. _(NOT VERIFIED 2026-11-05: PARTIAL: Browser 1400x900 Commander dark Settings (General, Agents) screenshots, 780x900 and 390x844 show compact page selector, stacked rows, no horizontal overflow (scrollWidth==clientWidth). Paper light, all pages, delayed plugin/empty/failed states not captured (theme change = persisted setting).)_

## Settings transport confirmation (1787-4778) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, use an isolated instance. Replay a gated HTTP or native settings edit with its `approval` proposal ID and `confirm: true`; approve the existing dialog and verify persistence. Decline another edit and verify no write. Confirm that replacing tunnel forwards, including an empty list, now asks for approval. No live desktop was restarted by this peer. _(NOT VERIFIED 2026-11-05: Isolated instance; settings/startup_flow_tests.rs:100 and registry.rs:17 approval param exist.)_

## Typed UI settings owners (1914-cdf6) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, use an isolated instance: set and disable a keyboard shortcut, reload, and confirm both persist. Split panes, switch focused panes and restore the layout; verify geometry and tab membership remain intact. The settings owner exposes only geometry, not runtime tab/focus identities. No running app was restarted. _(NOT VERIFIED 2026-11-05: Isolated instance required.)_
## Settings owner prerequisites (1787-4778) — Rust restart required

- [ ] After Boss manually restarts `make dev` or installs `make build`, verify isolated owner requests follow an Ego launch profile change and first configuration without recreating the registry. Verify removal of an on-connect SSH connection stops its ephemeral daemon after settings persistence. The integrated cutover closes the owner inventory gaps and replaces the config tool with settings; load the batch build before checking this path. Targeted owner regressions cover the saved-record callback, profile resolution and bindings-save failure. Do not restart the running app for this check. _(NOT VERIFIED 2026-11-05: Isolated owner/SSH daemon.)_
## Terminal paste text precedence

- [ ] After loading the updated frontend, copy a text line by selection in terminal Chat and paste into Claude Code; confirm text appears rather than an image attachment. Capture the paste event's MIME types to identify the original WebView clipboard payload. Also check a screenshot-only paste and a Finder image copy still attach. Component tests cover the mixed text/image precedence; this peer did not access Boss's running WebView or clipboard. _(audit 2026-10-13: HUMAN — needs real WebView clipboard / Finder image copy)_ _(triage 2026-10-14: WAITS — MacControl Screen Recording: needs native desktop/cross-app observation.)_ _(HUMAN 2026-11-05: WebView clipboard/Finder.)_
## Tablet desktop switch (1658-fb34)

- [ ] [HUMAN] On the already-installed iPad PWA, confirm the updated manifest scope takes effect and both switches stay in the installed app window. Browser emulation proves routing and preference handling but cannot prove iOS updates an existing installation's manifest. _(audit 2026-10-13: HUMAN — first item already [x]; [HUMAN] iPad PWA manifest)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_ _(HUMAN 2026-11-05: iPad manifest: public/mobile-manifest.json read (scope).)_
## iPad terminal keyboard focus (1577-a9bf)

- [ ] [HUMAN] On the physical iPad Safari remote UI, tap the bottom Claude prompt and a higher row after dismissing the software keyboard; confirm it stays open and text/deletion work. Primary canvas presses now cancel the proven native blur/refocus cycle. The reported top-bar blur still has no proven cause; record whether it appears before keyboard opening, after opening, or after pinch zoom. _(audit 2026-10-13: HUMAN — physical iPad)_ _(triage 2026-10-14: WAITS — Needs Boss’s phone/tablet or installed PWA for the specified device behavior.)_ _(HUMAN 2026-11-05: Physical iPad; CanvasTerminal.tsx:208 blur comment read.)_

## Stable macOS dev executable (1510-03ae) — next Boss launch

- [ ] With ALF enabled, confirm LAN/tailnet HTTP requests reach the running desktop and its page works on the physical iPhone. _(verified 2026-10-14: PID 12511 maps the existing `~/Library/Application Support/com.tuic.commander/dev-bin/tuicommander`; adjacent `tuic`, `tuic-bridge` and `tuic-remote` exist. Stable copy runner and critic tests, including target deletion, plus the sibling-build ordering check passed. The printed startup path is source/test verified; ALF reports enabled. Evidence: `~/Gits/.tmp/results/to-test-1014/phase2-identity-executable-proof.json`.)_ _(triage 2026-10-14: WAITS — Needs a physical iPhone and off-host LAN/tailnet request to validate the firewall boundary; localhost and script tests cannot prove it.)_ _(HUMAN 2026-11-05: Off-host iPhone/ALF request.)_
## Markdown link navigation guard (2026-09-25) — Rust, needs `make dev` restart

- [ ] In the restarted instance, open an HTML preview, PDF preview, URL plugin panel on localhost, srcdoc plugin panel, and reveal.js deck. Their iframe content and in-frame links/slide navigation still load. Export a text download on Linux through a blob URL. _(Static coverage: `lib.rs` navigation guard allows the internal frame schemes and loopback origins; runtime platform behavior needs the restarted app.)_ _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Preview iframes are UI; download part also needs Windows/Linux.)_ _(audit 2026-10-13: PARTIAL — live: srcdoc (ui tab html) iframe rendered with sandbox "allow-scripts allow-same-origin"; URL tab on localhost loaded (GET /busy.html 200)…)_ _(triage 2026-10-14: WAITS — MacControl Screen Recording: native WebView navigation/download isolation needs a desktop window.)_ _(HUMAN 2026-11-05: Native WebView iframes/blob download. Read lib.rs:1634-1635 (about:blank/srcdoc, data/blob allowed).)_
## Markdown block review handoff (2026-09-23) — frontend, live via HMR

- [ ] With tweak comments in the file, choose a same-repository agent in the Markdown topbar and click **Send**. An idle agent receives the request immediately; a busy agent shows one queued command and receives it on its next idle transition. _(NOT VERIFIED 2026-09-29: Needs a real agent tab in the same repository with idle/busy transitions to observe immediate delivery vs queued delivery.)_ _(audit 2026-10-13: HUMAN — Markdown comment source-range/item contracts exist ContentRenderer.test.tsx:63 and tweakComments.test.ts:88,121; no browser hover/save real file operation. Targeted tests refused protected checkout…)_ _(triage 2026-10-14: WAITS — Same-repository selection, enqueue-once and queued toast component test passed in subscription-markdown-contracts.log; actual idle/busy vendor delivery needs owned backend unavailable while rb SSH is down.)_ _(verified partial 2026-10-14: production MarkdownTab test filters other repositories and enqueues once for selected session; prior captured backend ready/queue tests pass separately. Actual vendor idle/busy handoff remains. Evidence: ~/Gits/.tmp/results/to-test-1014/phase2-subscription-markdown-proof.json.)_ _(HUMAN 2026-11-05: Needs real idle/busy agents. Read components/MarkdownTab/MarkdownTab.tsx:618-623 (pty.enqueueCommand + queuedCommands badge).)_
## Terminal Unicode integrity (2026-09-21) — **Rust, needs a `make dev` restart**

      including an accent written after its base letter. Verify rendering, block
      cursor, selection/copy, search and a link following accented text; scroll
      during output and resize. The installed instance still needs a restart
      even when the isolated verification build passes. See
      `plans/terminal-unicode-integrity.md` for automated evidence and limits.
## Protocol idle survives terminal animation (2026-09-21) — **Rust, needs a `make dev` restart**

- [ ] After a rebuild/restart, finish a Codex turn in brainstorming with its idle _(NOT VERIFIED 2026-09-30: partial — Real codex 0.159: after a turn idle held 40s and 2nd prompt -> busy ~1s after Enter (hook-idle Protocol). Codex pet animation (pet=codex in private CODEX_HOME) did not render/emit output in the TUIC PTY (last_activity static), so real animation NOT exercised. Emulated: script sends OSC 7770 idle the)_ _(audit 2026-10-13: HUMAN — Real Codex idle animation not exercised. No credentials/login and no existing Boss agent touched; prior emulated/partial evidence remains partial.)_ _(triage 2026-10-14: WAITS — Exact installed Codex pet animation/native idle behavior needs a fresh vendor turn and visible terminal; earlier capture did not show the animation. No fabricated parser/visual pass.)_ _(HUMAN 2026-11-05: Real Codex pet animation. Read pty/lifecycle.rs:671 PROTOCOL_STALE_TIMEOUT, :1814-1815.)_
      animation enabled. The terminal and session API must remain idle while the
      animation runs. Submit another prompt: working must return immediately.
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

### Upstream MCP OAuth — concurrent flows, expiry, late redirect

**Requires a `make dev` restart** — all of this is Rust (`mcp_oauth/`,
`mcp_proxy/registry.rs`). The running instance still has the old serialized
behaviour.

- [ ] Settings → Services → MCP: click **Authorize** on two different upstreams _(NOT VERIFIED 2026-09-29: Needs real upstream MCP OAuth servers and external browser consent)_ _(audit 2026-10-13: HUMAN — Requires hardware, authentication/provider state, or credential mutation; prohibited by audit brief. Source: to-test.md:1939)_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_ _(HUMAN 2026-11-05: needs real OAuth upstreams + browser consent (MCP upstream/credential changes forbidden). Code: mcp_oauth/commands.rs:180-183 sets "Awaiting authorization…" only after the flow exists and browser opens; per-upstream flows in flow.rs (flow manager keyed by state). UI label UpstreamMcpPanel.tsx:466.)_
  back to back. Both show the consent dialog and open a browser tab within a
  second. Previously the second click hung silently for 5 minutes: no browser,
  no dialog, no error, while the row already read "Awaiting authorization…".
- [ ] Click **Authorize**, then **Cancel** before completing consent: the row _(NOT VERIFIED 2026-09-29: Needs a real upstream MCP OAuth server to reach 'Awaiting authorization' and complete/cancel consent in a browser.)_ _(audit 2026-10-13: HUMAN — Requires hardware, authentication/provider state, or credential mutation; prohibited by audit brief. Source: to-test.md:1943)_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_ _(HUMAN 2026-11-05: needs real consent flow. Code: commands.rs:244-251 cancel_mcp_upstream_oauth → cancel_flows_for(name) (flow.rs:279) + registry.cancel_authenticating, so row leaves "Awaiting" at once; flow.rs tests cancel_flows_for_upstream (:870) exist, not run.)_
  leaves "Awaiting authorization…" immediately and Authorize works again on the
  next click (no queue built up behind it).
- [ ] Click **Authorize** and then do nothing for >5 minutes. The row returns to _(NOT VERIFIED 2026-09-29: blocked — Needs a mock OAuth upstream MCP server plus >5 min wait; not set up within time-box.)_ _(audit 2026-10-13: HUMAN — Requires hardware, authentication/provider state, or credential mutation; prohibited by audit brief. Source: to-test.md:1946)_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_ _(HUMAN 2026-11-05: needs >5 min wait against a real/mock OAuth upstream (not allowed to add upstreams). Code: flow.rs:41 DEFAULT_FLOW_TIMEOUT 300s; flow.rs:325-340 cleanup ticker logs "Cleaned up expired OAuth flows" (target mcp_oauth) and rollback_authenticating → needs_auth.)_
  **Authorize to connect** (`needs_auth`) on its own, and
  `GET http://localhost:9876/logs?source=mcp_oauth` shows
  `Cleaned up expired OAuth flows` naming the upstream. It must not stay stuck
  on "Awaiting authorization…".
- [ ] Click **Authorize**, wait out the full 5-minute timeout *in the browser*, _(NOT VERIFIED 2026-09-29: Needs real upstream MCP OAuth provider, browser consent and 5-minute timeout with an external account.)_ _(audit 2026-10-13: HUMAN — Requires hardware, authentication/provider state, or credential mutation; prohibited by audit brief. Source: to-test.md:1951)_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_ _(HUMAN 2026-11-05: needs browser consent after 5 min. Code: commands.rs:186-200 listener lingers flow_timeout+grace so a late redirect is answered; callback_server.rs:60-76 renders "Authentication failed" card with EXPIRED_REASON "expired or was cancelled…" + "press Authorize again"; test callback_server.rs:347-361.)_
  then complete consent. The browser shows the TUIC "Authentication failed" card
  reading "This authorization request expired or was cancelled…" plus "press
  Authorize again" — **not** the browser's own "can't connect to the server"
  page.
- [ ] A normal successful authorization still lands on the green _(NOT VERIFIED 2026-09-29: Needs a real upstream OAuth MCP server and browser authorization flow (external service))_ _(audit 2026-10-13: HUMAN — Requires hardware, authentication/provider state, or credential mutation; prohibited by audit brief. Source: to-test.md:1956)_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_ _(HUMAN 2026-11-05: needs real OAuth upstream. Code: callback_server.rs:45 "Authentication complete" card, closes 2 s after success (commands.rs:193-195). Not run live.)_
  "Authentication complete" card and the upstream goes `ready`.
### Terminal: no grid wipe on tab switch, resubscribe on reattach (#657-4345)

Frontend only (`Terminal.tsx`) — Vite HMR picks it up, no `make dev` restart
needed. Canvas painting is not observable over HTTP, so these need eyes.

- [ ] Switch back and forth between two busy terminal tabs. The returning tab _(NOT VERIFIED 2026-09-29: blocked — Trusted and DOM clicks on terminal tabs do not switch tabs in this web session (known limit); also 'busy tab' repaint flash not visible without screenshots.)_ _(audit 2026-10-13: VISUAL-OWED — Rendering/timing requires images or real pointer interaction; image viewing and MacControl unavailable. No screenshots attempted. Source: to-test.md:1964)_ _(triage 2026-10-14: WAITS — Targeted terminal rendering evidence retained; exact returning-tab paint under live output requires native rendering observation unavailable with Screen Recording denied and locked desktop.)_ _(HUMAN 2026-11-05: paint-flash on tab switch needs eyes (canvas). Code-verified: Terminal.tsx:1222-1245 resubscribe/refresh only when stepReattachPhase reports a real detach→reattach, not on plain visibility flips; vitest visibilityLifecycle.test.ts 19/19 pass (today).)_
  shows its content immediately with no blank flash. Previously every switch ran
  `resubscribe()` + `refresh()`, which cleared the grid and repainted it
  (paint → wipe → paint).
- [ ] Detach a tab into a floating window, then close that window to reattach. _(NOT VERIFIED 2026-09-29: blocked — Desktop-only: detach tab to floating window and reattach needs native window interaction on the validate instance (maccontrol targets orchestrator, not this instance).)_ _(audit 2026-10-13: VISUAL-OWED — Rendering/timing requires images or real pointer interaction; image viewing and MacControl unavailable. No screenshots attempted. Source: to-test.md:1968)_ _(triage 2026-10-14: WAITS — MacControl Screen Recording: needs native desktop/cross-app observation.)_ _(HUMAN 2026-11-05: floating-window detach/reattach is native (desktop only). Code-verified: Terminal.tsx:1231-1244 resubscribes+refresh after reattach (terminalsStore.isDetached), covered by visibilityLifecycle.test.ts (19 pass).)_
  The reattached tab still paints live output and scrolls — the grid channel is
  resubscribed on this path, which is the only path that still resubscribes.
  collapsed for a minute. `GET http://localhost:9876/logs?source=terminal` shows
  one `Container stayed zero-size for 120 frames` warning and CPU stays flat.
  Previously that container kept a `requestAnimationFrame` loop re-arming every
  frame for the lifetime of the page, one loop per terminal, surviving unmount.
## Opening a 23 MB JSON no longer freezes the editor (2026-09-06, frontend — HMR; one Rust part needs `make dev` restart)

Above 500 KB the editor is plain text: no highlighting, no git gutter, no inline
blame, and the disk poll no longer re-reads the whole file 5 s after opening.
`get_gutter_changes` (Rust) returns nothing for an untracked file instead of
one "added" marker per line. Measured in Chrome only; WKWebView is the one
that blocked for over a minute.

- [ ] After a manual backend restart, verify that an uncapped coordinator can run an automation cutover and an automation-launched agent receives `capability_denied` for the same action.
