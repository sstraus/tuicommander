# Real Claude mobile resume evidence (1971-5834)

Recorded on 2026-10-17 with Claude Code 2.1.286 and an isolated native macOS
`tuic-remote`, port 19871, HOME under `~/Gits/.tmp/mp-home`. The worktree mobile
client ran on Vite port 19872 at an 820 × 1180 viewport. Only throwaway PTYs and
own seeded conversations were used; no production TUIC session was changed.

## Before the fix

- `full-screen-before.json` and `grid-before.json` retain the session list in
  the terminal. The latter includes the raw grid frame as base64 and decoded rows.
- `log-screen-before.json` shows the presentation cutoff after Search, without
  the session rows that the mobile CLI client needs.
- `search-no-matches-before.json` records the real single-character `z` search.

The replay fixture is
`src-tauri/src/fixtures/agent_prompts/claude-mobile-resume-1971.tcap`, with
provenance in its sibling Markdown file. Capture started before PTY creation;
its TUICCAP2 header records 41 × 110 geometry. SHA-256:
`eccd95ccdb99da0d7891f697e3e73c72dc6bdaf17b2809cad862200439c007a3`.

## After the fix

The native binary was rebuilt before this comparison. `fixed-build.json`
records its health/build identity. The new throwaway PTY was
`b1081971-2bc1-46d7-9cf5-49cfa29c0197`.

- `log-screen-after.json` and `mobile-after.json`: HTTP log screen and mobile
  DOM both contain the own seeded session rows.
- `down-after.json` / `up-after.json`: Down selects the own `/clear` session;
  Up returns to `Mobile resume seed`.
- `navigation-writes.json`: Down emits CSI B and leaves the composer empty.
- `navigation-writes-complete.json`: Down, Up, Enter emit CSI B, CSI A, CR.
- `selected-after.json`: Enter resumes the own seed transcript containing
  `MOBILE_RESUME_SEED_1971`; the modal is gone.
- `search-no-matches-after.json` and `mobile-search-after.json`: reopening
  `/resume` and typing `z` preserves the no-match result and `Esc to clear`.

The test session was deleted and the owned browser tab was closed afterward.
DOM `button.click()` exercised actual app handlers and the real backend/Claude.
This is not physical-touch proof. Screenshots hung while macOS was locked;
the coordinator authorized their omission and will capture them after unlock.
No generated screenshot or visual pass is claimed.

## Regression and validation

`captured_claude_mobile_resume_survives_screen_trim` failed on pre-fix production
commit `8400aa85b7`: `HTTP screen lost "Mobile resume seed"`. It replays complete
captured modal frames through both HTTP trim and log-WebSocket screen polling,
checks populated and no-match states, preserves the shared parser cutoff, and
checks normal trimming after a later ready composer.

RED command (remote Linux rb, through build-slot):

```sh
scripts/with-test-tmp.sh bash -lc 'cd src-tauri && cargo nextest run --lib -E "test(captured_claude_mobile_resume_survives_screen_trim)"'
```

The intended regression failed (one test). Log:
`~/Gits/.tmp/mobile-pickers-1971-red-complete.log`. The remote runner marked
this RED snapshot stale because documentation/story updates continued; the
regression and pre-fix production source did not change during the run.

Final targeted GREEN (remote Linux rb, through build-slot):

```sh
scripts/with-test-tmp.sh bash -lc 'cd src-tauri && cargo nextest run --lib -E "test(mcp_http::session::tests::trim_) | test(mcp_http::session::tests::captured_claude_) | test(mcp_http::session::tests::screen_poll_)"'
```

All 10 selected tests passed in 0.046 s; compile time was 4m32s. The remote
result was not stale and reported `RB_BOX_DIRTY=no`. The authorized native
macOS incremental headless build then passed in 1m47s:

```sh
cargo build --manifest-path src-tauri/Cargo.toml --bin tuic-remote --no-default-features
```

Cargo ran through mbx and the local build-slot scheduler. Combined log:
`~/Gits/.tmp/mobile-pickers-1971-green-native.log`. No source or test changed
after this GREEN/build. No full suite or release/platform matrix was run.
The production desktop still needs a manual restart/build to load the Rust fix.
