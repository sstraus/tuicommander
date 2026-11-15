# ego terminal Chat verification

Story: `1969-ac9b`. Verification date: 2026-10-17.

## Producer contract

The consumer uses the local transcript v1 contract from ego commit
`0821caee292a314517d86f81929977bd1f9a1188`, on
`feat/ego-532-transcript-export` (ego story `532-3c39`). The authoritative
sources are `docs/reference/transcript.md`,
`crates/ego-core/src/public/transcript.rs` and the real CLI contract test
`crates/ego-cli/tests/transcript.rs` in that commit.

The recorded consumer fixture is
`src-tauri/src/fixtures/chat_view/ego/transcript-v1.jsonl`. Its SHA-256 is
`8024e04e7919715e9ac5dfe26105a0918b738e1a380ad4f700210bb5f7f0a3e1`.
Its adjacent README records the original real-binary capture command. It
includes tool arguments/results, a goal continuation and a compaction successor.
It is not a hand-written approximation of the producer's storage.

The consumer checks the v1 header and exact session, accepts sequence gaps,
skips additive unknown kinds, omits preview deltas and preserves tool item
identity. Goal continuations and checkpoints become notices. It requests
committed events through `ego export SESSION --format transcript --after SEQ`;
it does not read private conversation logs.

## Live native integration

The native binaries were built with the mbx shim and the shared scheduler:

```sh
"$HOME/Gits/personal/orchestrator/tools/build/build-slot.sh" bash -lc '
  set -e
  export PATH="$HOME/Library/Application Support/mbx/bin:$PATH"
  which cargo
  mbx doctor
  cd "$HOME/Gits/personal/ego__wt/feat-ego-532-transcript-export"
  cargo build -p ego-cli --bin ego
  cd "$HOME/Gits/personal/tuicommander__wt/fix-1969-ego-chat-view/src-tauri"
  cargo build --bin tuic-remote --no-default-features
'
```

The scheduled shell also checked that no build freeze excluded this peer.
Build output is in `~/Gits/.tmp/tuic-1969-live-build.log`.
Both builds passed; `mbx doctor`
reported zero failures. No installed executable was replaced.

An isolated headless TUIC ran on `127.0.0.1:19869` with instance
`ego-chat-1969`, a throwaway HOME below the worktree's `.tmp/1969-live`, and
`--no-agent-configs`. Its PTY ran the real ego 532 executable with its own
`EGO_HOME` and a scripted loopback provider. No paid provider was called.
The provider supplies model responses only: ego itself creates the session,
lease, events and exported transcript. The Chat backend and frontend are not
mocked.

The final run bound TUIC PTY `b0448980-829d-4e69-ab9d-e881dd126436` to ego
session `01a149f6-cfcc-7e33-83d4-a3142e116d40`, whose lease named PID `78945`.
The initial snapshot contained the real prompt and completed answer. A second
prompt, submitted with the production `sendCommand`, produced a second answer.
The existing ticker emitted `chat-view-changed` with cursors 3 and 4. Both the
browser subscriber and a direct HTTP SSE subscriber recorded those wakes.
The final snapshot retained four updates, without duplicated answers or
malformed records.

## Desktop and mobile rendering

A worktree Vite page mounted the production desktop `TerminalChatView` and
mobile `useSessionChatView` with the actual PTY/session identity. HTTP snapshot,
write and SSE calls were proxied to the isolated backend. The mobile host used
the production header classes and injected its wake subscription. The harness
sets the theme and host layout; it does not supply transcript updates.

All four captures contain both real prompts and answers, an active Chat view,
no unavailable banner and no horizontal document overflow. The desktop dark
capture also records both live SSE wakes. Desktop was 1400 by 900 CSS pixels;
mobile was 390 by 844. Each view was printed to PDF and rasterized with
`pdftoppm -png -singlefile -r 144`.

Evidence under this worktree's `.tmp/1969-live/`:

- `binding.json`, `sessions.json`, `initial-snapshot.json`, `final-snapshot.json`:
  live ownership and production snapshot output.
- `live-transcript-v1.jsonl`: a fresh export from the real live ego session.
- `provider-requests.json`, `output.json`, `headless.log`: loopback requests,
  terminal output and backend lifecycle.
- `sse-events.txt`: direct production SSE wakes at cursors 3 and 4.
- `{desktop,mobile}-{dark,light}-live.{pdf,png,state.json,snapshot.txt}`:
  live renderer PDF rasters and DOM/accessibility evidence.
- `record-live.mjs`, `proof.tsx`, `index.html`: the rendering harness.

The successful UI command and output are in
`~/Gits/.tmp/tuic-1969-live-ui-3.log`. An earlier harness run forwarded the
Vite Origin unchanged and received `403 Untrusted Origin` on writes; correcting
the proxy Origin resolved that failure without changing application policy.
The first successful capture observed incremental rendering but did not record
an SSE wake. The final run paced the loopback response and required a recorded
wake as well as the rendered second answer.

Every owned PTY, headless backend, Vite server and named browser tab was closed.
No desktop application, production session or production config was changed.

## Scope and deployment

This is native macOS backend/real-ego integration plus production desktop/mobile
component rendering in browser mode. It is not a Tauri WebView or physical-phone
check. The earlier five adapter tests passed on rb Linux; those historical
results were not rerun or presented as post-merge suite results. No new
production defect was found, so this merge/evidence update adds no RED test.
The batch integrator owns post-merge targeted GREEN execution.

Ego 532 was not an ancestor of ego `master` at verification time. Deployment
still requires that producer change to land, a rebuilt ego executable to be
configured in Settings, and Boss to rebuild/restart TUIC to load its Rust
consumer. The installed-app deployment check remains in `to-test.md`.
