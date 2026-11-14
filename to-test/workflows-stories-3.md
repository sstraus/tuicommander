## GitHub poller survives a dropped connection (story 648-051b, Rust — needs `make dev` restart) (continued)
- [ ] With the network still down, disable GitHub polling in Settings. It must _(NOT VERIFIED 2026-09-30: partial — Not run: depends on the network outage of 1760 and a Settings toggle (frontend).)_ _(audit 2026-10-13: HUMAN — Requires interaction with Boss-owned active app/repository or vendor/provider process; no ownership of that state. Contract test/source search performed…)_ _(triage 2026-10-14: WAITS — Needs owned outage/poller execution; github_poller.rs has stop_aborts_a_poll_that_never_resolves and stop_poller_reaches_a_loop_parked_inside_a_request, but remote SSH is currently unreachable. No Boss network/settings changed.)_ _(HUMAN 2026-11-05: needs network outage + Boss's polling toggle (not allowed to cut network / change Boss settings). Code-verified: github_poller.rs:778-791 stop_poller notifies stop (poller.stop.notify_one) and takes poller out of state; :285-296 closed channel breaks loop; tests stop_aborts_a_poll_that_never_resolves (:1230) and stop_poller_reaches_a_loop_parked_inside_a_request (:1255) exist, not run (cargo nextest fails here: mbx SUN_LEN).)_
  stop immediately, not after the in-flight request gives up.
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

- [ ] Desktop, normal path: fetch/pull/push from the Git panel still work and _(NOT VERIFIED 2026-09-30: partial — POST /repo/run-git fetch: reachable local remote success:true; hanging remote (silent TCP listener) -> after 180s 'git timed out after 180.0s and was killed'. Side finding: orphaned 'git remote-http' helpers (ppid 1) survived until the peer closed. Git panel UI/pull/push not driven.)_ _(audit 2026-10-13: HUMAN — Requires interaction with Boss-owned active app/repository or vendor/provider process; no ownership of that state. Contract test/source search performed…)_ _(triage 2026-10-14: WAITS — Needs Boss’s provider authentication, external service fixture or authorized live account operation.)_ _(HUMAN 2026-11-05: Git panel clicks (fetch/pull/push) on desktop not driven. Live HTTP half today: POST /repo/run-git fetch origin on scratch repo → {success:true,exit_code:0}; fetch nonexistent → HTTP 200 {success:false,exit_code:128,stderr:...}; reset → 400 not allowed. Push blocked by my capability cap. Bound code git_routes.rs:1040-1075.)_
  still report failures the way they did before. No visible change expected.
  remote is reachable. It should behave exactly as on desktop.
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

  scrollback and scroll with the wheel and by dragging the scrollbar: the
  viewport must move, not just the thumb.
- [ ] With that browser attached, close the same terminal's tab in the desktop _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: Needs a browser-attached scrolling session AND a desktop instance to close the same tab; both frontends; headless instance has neither.)_ _(audit 2026-10-13: VISUAL-OWED — Live disposable HTTP-created PTY: 504 lines, screen_lines 50; scroll-to-offset 120 -> scroll-info offset120. scroll-probe.json. Browser painting/desktop-close half is VISUAL-OWED.)_ _(triage 2026-10-14: WAITS — Cross-client native tab close plus browser live teardown needs isolated desktop and owned backend; peers cannot launch native app, rb unavailable, Boss tabs untouched.)_ _(HUMAN 2026-11-05: needs a desktop-app window closing the same tab while a browser is attached (two frontends; the only desktop is Boss's live app). Code touched: Terminal.tsx unsubscribe path (Terminal.tsx:277-278 safeUnlisten(unsubscribePty)); browser-side scroll itself verified for L1763.)_
  app. The browser must keep scrolling — the unsubscribe no longer drops the
  session's scroll target.
## Linked worktrees start WARM (story `767-3968`, 2026-09-13) — **Rust, needs a `make dev` restart**

- [ ] Time it. Expect ~38 s on this repo; if it feels worse than a cold build, _(NOT VERIFIED 2026-09-30: partial — Not run on the tuicommander repo (would create a worktree in Boss's real repo; forbidden). Analog: 200-file ignored dir fixture warmed in under 1s; 29/09 analog with 60k files ~13s. ~38s expectation on the real repo not measured.)_ _(audit 2026-10-13: HUMAN — Real main worktree creation exercised and pending->done observed, but elapsed warming not instrumented. Recycled clean archived checkout, so cannot claim cold 38s performance…)_ _(triage 2026-10-14: WAITS — Needs coordinator-owned benchmark/mutation scheduling; no expensive batch gate started by this peer.)_ _(NOT VERIFIED 2026-11-05: rule: no worktree creation in Boss's tuicommander repo (and a timing on this repo is not reproducible in scratch). Code read: tuic-git worktree.rs:26-62 begin_warm/finish_warm pending→done state. Today's logs have no worktree-warm duration line to read.)_
      say so rather than living with it.
## Progress rewritten to one journal, one database and a dialog — needs a `make dev` restart

The whole Progress feature was re-implemented against the 2026-09-14 revision of
`plans/project-progress.md`: one append-only journal in a single database at
`<config dir>/progress.sqlite3`, two reportable kinds plus a host-written
`intent`, a dialog replacing the sidebar panel, eight `repo progress_*` actions
cut, and no Markdown export. Rust and frontend both changed, so the running
build has the old behaviour until restart.

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
- [ ] Run diff-scoped mutation testing once on the final HEAD of this batch: _(NOT VERIFIED 2026-09-30: partial — Not run: diff-scoped mutation testing (make mutants) is an overnight cargo job (~5 min/mutant) and cargo/build is forbidden for this pass. Orchestrator batch task.)_ _(audit 2026-10-13: HUMAN — Overnight diff-scoped mutation gate remains coordinator-owned; no final batch range supplied. No cargo/mutants run, no gate pass claimed.)_ _(triage 2026-10-14: WAITS — Needs coordinator-owned benchmark/mutation scheduling; no expensive batch gate started by this peer.)_ _(NOT VERIFIED 2026-11-05: overnight diff-scoped mutation job (~5 min/mutant), orchestrator-owned per AGENTS.md "Mutation testing is per change"; scripts/mutants.sh and src-tauri/.cargo/mutants.toml present. Not run (no cargo builds allowed here).)_
      `make mutants RANGE=<commit before the Progress rewrite>`. It is an
      overnight-class job (~5 min per viable mutant), so it is deliberately not
      run during the day — 780-e99a criterion 4.
## The headless build announces repo-op progress too (#808-84e1)

Rust — needs a `make dev` restart, and the interesting half needs the headless
binary: `cargo build --bin tuic-remote --no-default-features`.

**Which binary:** `run_headless` (the main binary's headless mode), NOT
`tuic-remote`. The three routes are in `build_router`; `build_remote_router`
does not carry them, so `tuic-remote` answers 404 (#810-4986).

- [ ] Start the headless mode of a `--no-default-features` build, open the web _(NOT VERIFIED 2026-09-29: Needs PR review (ego/GitHub account) on a --no-default-features headless build.)_ _(audit 2026-10-13: VISUAL-OWED — Bus half PASS via three --no-default-features rb tests (rust.log/rust-contracts.log); actual finding/proposal/conflict panel rendering is owed…)_ _(triage 2026-10-14: WAITS — Dual-transport source and focused frontend event tests retained; actual headless review-progress delivery needs owned daemon plus real ego/GitHub fixture. rb unavailable.)_ _(HUMAN 2026-11-05: needs a --no-default-features headless binary + real PR review (ego/GitHub). Code-verified bus side: mcp_http/sse_routes.rs:337 ReviewProgress→"review-progress", :575-579 payload routing, :901-927; consumer githubOps.ts:64 OPS_EVENTS, :148 case "review-progress". Not run live.)_
      UI against it, and start a PR review on a repo. The Review findings column
      must move from "running" to a result on its own. Before this commit it
      stayed on "running" forever, because the `review-progress` event was
      dropped on that build.
- [ ] Same daemon, run an improvement scan. The proposals must appear in the _(NOT VERIFIED 2026-09-29: Improvement scan runs on ego against a real daemon; needs ego)_ _(audit 2026-10-13: VISUAL-OWED — Bus half PASS via three --no-default-features rb tests (rust.log/rust-contracts.log); actual finding/proposal/conflict panel rendering is owed…)_ _(triage 2026-10-14: WAITS — Actual proposals-progress daemon/web flow needs isolated backend/model/GitHub fixture; rb SSH unavailable. Existing event/render tests do not prove a live scan.)_ _(HUMAN 2026-11-05: needs a real improvement scan on a headless daemon. Code-verified: sse_routes.rs:338 ProposalsReady→"proposals-ready", :579; githubOps.ts:57,64 populate proposals from "proposals-ready" only.)_
      panel when the scan finishes — the return value never populates it, only
      the `proposals-ready` event does.
- [ ] Same daemon, run conflict assist on a PR with conflicts. The status must _(NOT VERIFIED 2026-09-29: Needs a real GitHub PR with conflicts, conflict assist (ego) on a headless daemon.)_ _(audit 2026-10-13: VISUAL-OWED — Bus half PASS via three --no-default-features rb tests (rust.log/rust-contracts.log); actual finding/proposal/conflict panel rendering is owed…)_ _(triage 2026-10-14: WAITS — Actual conflict-assist event flow requires owned conflicting PR/backend/model fixture; remote runner unavailable. Prior store/source evidence retained without live operation claim.)_ _(HUMAN 2026-11-05: needs a real PR with conflicts + conflict assist. Code-verified: sse_routes.rs:333 ConflictAssistStatus→"conflict-assist-status", :575; githubOps.ts:46,64 consumer.)_
      reach the panel rather than leaving it idle.
- [ ] Desktop build, same three operations: unchanged. The window emit still _(NOT VERIFIED 2026-09-29: blocked — Needs PR review, improvement scan and conflict assist (ego + real GitHub PR with conflicts) on a headless daemon plus desktop panels; ego/GitHub PR not available headless.)_ _(audit 2026-10-13: VISUAL-OWED — Bus half PASS via three --no-default-features rb tests (rust.log/rust-contracts.log); actual finding/proposal/conflict panel rendering is owed…)_ _(triage 2026-10-14: WAITS — Actual dual emit delivery for review/scan/conflict in native window requires desktop runtime observation; managed peers cannot launch another desktop or exercise Boss sessions.)_ _(HUMAN 2026-11-05: needs the desktop build (Boss's live app) running the three ops; the window emit path is not observable from browser mode. Code context only (state.rs:471-498 events, sse_routes.rs:333-338 dual path).)_
      fires, so nothing about the desktop panels may look different.
## One language, end to end (story `822-7d7a`, 2026-09-22) — **Rust, needs a `make dev` restart**

The first four items need an installed Italian bundle, for the same reason as
the block above: without one there is no voice to listen to. The release that
serves it exists as of 2026-09-22, so install it first. The point of every one
of these items is the same — the
model must never answer in a language the user is not speaking.

- [ ] Set Dictation language to **Italian**, arm hands-free, and say something in _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Needs Italian language, armed conversation and real agent reply; drive via WS audio client on desktop build.)_ _(audit 2026-10-13: HUMAN — Armed microphone/speaker/model conversation required. Read commands.rs:2514-2570 and dictation_routes.rs:361-401; safe read-only speech status proven unarmed…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_ _(HUMAN 2026-11-05: needs armed Italian conversation + speech. Code: commands.rs:867 conversation_language, :884-891 speech_language error when Auto and nothing said; browser Settings shows Language=Italian live (dictation page rendered today).)_
      Italian. The terminal entry must read `<what you said> (reply in Italian)`,
      on one line, and the agent must answer **in Italian**. Instruction
      delivery is not the proof: read the agent's reply.
- [ ] With the same setup, listen to the spoken reply. It must be the Italian _(NOT VERIFIED 2026-09-30: blocked — audio hardware: listening to the Italian TTS voice is speaker/ears; also headless build has no audio.)_ _(audit 2026-10-13: HUMAN — Armed microphone/speaker/model conversation required. Read commands.rs:2514-2570 and dictation_routes.rs:361-401; safe read-only speech status proven unarmed…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_ _(HUMAN 2026-11-05: listening to the Italian voice. Live: browser Settings → Voice shows Voice select value "giovanni" ("Default for this language", giovanni, alba — Not downloaded …).)_
      voice reading Italian — not Italian text read by another language's voice,
      and not an English sentence.
- [ ] Still on Auto, say something in Italian. `status` must then report _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Needs spoken Italian/English input (WS audio) and real agent reply.)_ _(audit 2026-10-13: HUMAN — Armed microphone/speaker/model conversation required. Read commands.rs:2514-2570 and dictation_routes.rs:361-401; safe read-only speech status proven unarmed…)_ _(triage 2026-10-14: WAITS — Needs real armed audio and Italian/English speech with a model reply; prior unarmed status and language-contract evidence cannot prove this observation.)_ _(HUMAN 2026-11-05: needs spoken Italian under Auto. Code: commands.rs:884-891 speech_language (Auto resolves only after something was said), :1465 conversation_language().unwrap_or_default() in speech status.)_
      `language: "it"`, and the entry must carry `(reply in Italian)`. Say the
      next turn in **English**: the entry must carry `(reply in English)` and the
      agent must switch with it.
- [ ] While a reply is being spoken, change the Dictation language in Settings. _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Needs Settings UI language/RMS change during playback; audible cut needs ears.)_ _(audit 2026-10-13: HUMAN — Armed microphone/speaker/model conversation required. Read commands.rs:2514-2570 and dictation_routes.rs:361-401; safe read-only speech status proven unarmed…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_ _(HUMAN 2026-11-05: needs playback + ears; change of language mid-reply. Code anchor: commands.rs:607 voice_language(), :817 choose_voice.)_
      The speaker must stop mid-sentence and `status` must report the new
      language. Then change only the **RMS threshold** while another reply
      plays: that one must keep playing to the end.
- [ ] Turn the hands-free entry/exit hints **off** (story 821's setting, when it _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Needs hints-off setting in UI and armed entry.)_ _(audit 2026-10-13: HUMAN — Armed microphone/speaker/model conversation required. Read commands.rs:2514-2570 and dictation_routes.rs:361-401; safe read-only speech status proven unarmed…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_ _(HUMAN 2026-11-05: needs hints-off setting + armed entry. Code: config.rs:77 hands_free_start_notice; continuous.rs:1390 entry_hint_text(configured), :1454 deliver_entry_hint, :2268 report_exit_hint; live: Settings shows a "Notify model when hands-free changes" toggle.)_
      lands) and repeat the first item. The `(reply in …)` requirement must still
      be in the entry — it is not a hint.
- [ ] In the armed terminal's Claude Code, `voice action=status` must report _(NOT VERIFIED 2026-09-30: partial — Verified: voice tool inputSchema has only action,text,turn,utterance_id (no language/voice param). Not verified: status.language from an armed terminal (no dictation/arm in headless build).)_ _(audit 2026-10-13: HUMAN — Armed microphone/speaker/model conversation required. Read commands.rs:2514-2570 and dictation_routes.rs:361-401; safe read-only speech status proven unarmed…)_ _(triage 2026-10-14: WAITS — Schema already verified without language/voice inputs; armed desktop status needs an audio-enabled isolated build and real conversation.)_ _(HUMAN 2026-11-05: needs an armed terminal (global mic). Code-verified contract: mcp_transport_catalogue.rs:704 voice tool "You do not choose the language or the voice… reported as status.language"; status returns {available, unavailable_reason, session_id, language, turn, voice, …}; mcp_transport_transport_tests.rs:715 asserts inputs are only [action,text,turn,utterance_id] (no language/voice param). Armed status.language not observed.)_
      `language`, and the tool schema must offer no way to pass a language or a
      voice.
## Barge-in over a real speaker (story `816-cbbf`, 2026-09-22) — **[HUMAN]**

Criterion 3 of the story asks for a real audio probe, and this is the half of it
no test can reach. Everything that can be measured offline already is —
`talking_over_the_reply_stops_it_without_losing_the_first_words` in
`continuous.rs` reports 50 ms stop latency and 0 false triggers against the real
AEC3 canceller, over a **modelled** room (40 ms delay, 0.35 gain, no
reverberation, no noise floor, no speaker distortion). See
`docs/backend/dictation.md` → "What barge-in measures".

Needs a real microphone and a real speaker, in a room, with no headphones.

- [ ] **[HUMAN]** Arm hands-free, ask something with a long answer, and let the _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_ _(audit 2026-10-13: HUMAN — Real phone/PWA touch, native authentication or audio hardware required; explicit brief excludes these probes.)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_ _(HUMAN 2026-11-05: real speaker/mic barge-in. Code anchors: commands.rs:2700-2706 barge-in port wired before the runtime; dictation/playback_gate_tests.rs (not run).)_
      reply play **out of the laptop speaker**. Say nothing for the whole reply.
      The reply must finish. A reply that cuts itself off is the echo path
      failing on real reverberation, which the modelled room cannot produce.
- [ ] **[HUMAN]** Same again, and talk over it after a couple of seconds. The _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_ _(audit 2026-10-13: HUMAN — Real phone/PWA touch, native authentication or audio hardware required; explicit brief excludes these probes.)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_ _(HUMAN 2026-11-05: real speaker/mic barge-in 2 s into a reply. Code: commands.rs:1153 speech_far_end (far-end reference for AEC); dictation/mod.rs:215 DictationState::new.)_
      reply must stop within about a quarter of a second, and the transcript
      that reaches the terminal must contain your **first** word — that is the
      pre-roll doing its job. A transcript that starts mid-sentence is the
      failure to report.
- [ ] **[HUMAN]** Repeat both at a high speaker volume, close to the _(NOT VERIFIED 2026-09-29: needs a human listening/speaking (audio hardware) — not reproducible in the isolated headless/browser instance)_ _(audit 2026-10-13: HUMAN — Real phone/PWA touch, native authentication or audio hardware required; explicit brief excludes these probes.)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_ _(HUMAN 2026-11-05: high speaker volume near the mic. Same code anchors: commands.rs:1153 speech_far_end; no audio hardware used.)_
      microphone. This is the case the linear room model is least like: a
      driven speaker clips, and AEC3 cannot subtract what the amplifier added.
      Report whether false interruptions appear and at roughly what volume.
## Speech and download progress on `/events` (story `833-6fd4`, 2026-09-22) — **Rust, needs a `make dev` restart**

The three pushes are dual-emitted: the desktop window gets an `emit`, the SSE
stream gets the identical body. Only a restart loads them.

- [ ] Arm hands-free, let a reply play, and watch the same stream: one _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Needs /events SSE watch on desktop build while a reply plays (speech-utterance frames).)_ _(audit 2026-10-13: HUMAN — Armed microphone/speaker/model conversation required. Read commands.rs:2514-2570 and dictation_routes.rs:361-401; safe read-only speech status proven unarmed…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_ _(HUMAN 2026-11-05: Read dictation/commands.rs:503,558 (SPEECH_UTTERANCE emitted) + sse_routes.rs:350 (bridged to /events). Live: GET /dictation/hands-free armed:false; hearing a spoken reply needs speaker+armed conversation.)_
      `speech-utterance` frame per transition, in the order
      `queued → rendering → speaking → finished`, with no polling of
      `GET /dictation/speech/status`.

- [ ] After an authorized manual Rust rebuild/restart, open the catalog Designer, choose "New from template: GitHub issue triage", set the model profile, model and effort, save and publish. Bind the publication to a throwaway GitHub repository with event `labeled` and filter `label` = your label, enable it, then add the label to a new issue. Confirm one run, one approval per write, one triage comment and the `triaged` label, and no second run after the next poll (#3036-90fc).
## Legacy workflow engine removed (story `3034-365e`, 2026-11-14) — **Rust, needs a `make dev` restart**

The first start of the new build backs up and then drops the legacy workflow
data of the real config folder. Only a restart loads it.

- [ ] After the restart, the config folder holds `workflows.sqlite3.legacy-workflows.bak`,
      `workflow_runs.sqlite3.legacy-workflows.bak`, `automation_runs.sqlite3.legacy-workflows.bak`
      and `automations.json.legacy-workflows.bak` (only for stores that had legacy data),
      and the prompt schedules in Automations are unchanged.
- [ ] Plans and Stories has no Run history and no Start story workflow button;
      Workflows opens the neutral Designer only.
- [ ] Automations shows no PR review, issue intake or triage settings.
