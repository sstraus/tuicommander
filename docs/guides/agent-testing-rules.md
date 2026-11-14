# Agent Testing Rules

These rules moved here from `AGENTS.md` to keep that file inside the
32,768-byte project-instruction budget. They stay binding for every agent;
`AGENTS.md` keeps a summary and a link to this page.

## The suite skips every test it cannot run on purpose — classify them, never pin the count

`cargo nextest run --lib` reports a handful of skips. Every one is `#[ignore]`
with a reason string. None is a `cfg` exclusion and none is a filter artifact,
so the skips are not missing coverage and not a harness defect: `N run, M
skipped` is a **complete** result for what an unattended run can execute.

| Category | Precondition an unattended run cannot meet |
|---|---|
| Environment | interactive Keychain (×4), network + GitHub token (×2), authenticated `gh` CLI, a downloaded whisper model, a downloaded Pocket TTS bundle (×3, `TUIC_POCKET_BUNDLE_DIR`), real `openpty` |
| Corpus-driven | `TUIC_CAPTURE_CORPUS`, `TUIC_DAMAGE_CORPUS`, `TUIC_REPLAY_FILE`, plus 744-138c's evidence capture |
| Benchmark | `bench_chunk_path_replay`, `tunnels::audit::tests::bulk_insert_performance`, `mcp_http::ws_compression::measurement::level_six_is_the_knee_of_the_curve` |

The per-category counts are deliberately not written down — see "Never assert
the skip count" below. On 2026-09-21 the three categories held 12, 4 and 3.

**`dump_committed_tcap_fixture_event_sequences_744` is not a pass/fail test.** It
is an evidence-capture harness for story 744-138c and the comment above it says
so. Un-ignoring it during a tidy-up of ignored tests is the failure to avoid.

**Re-derive the classification; do not trust a count.** Counting `#[ignore]`
attributes in the source happens to give the right answer for the wrong reason —
it counts one mechanism and cannot see the other two. This does discriminate:

```
cargo nextest list --lib --run-ignored all   ->  4851
cargo nextest list --lib                     ->  4832
```

The delta is the skip count and the set difference *is* those names (19 on
2026-09-21). A `cfg`-excluded test
is absent from **both** lists, so the delta would not close if any were excluded
that way; a filtered skip would move the second number alone.
`--run-ignored ignored-only` is stronger still: it lists the set instead of
implying it by subtraction, and run against both configurations it names the one
test in the difference rather than leaving a count to interpret.

**Parse that output carefully, because a wrong pattern returns zero and so does
an empty set.** `nextest list` prints `tuicommander <path>` with no leading
whitespace; a `grep -E '^\s+\S+::'` written on the assumption that it indents
returns 0 lines for every configuration, exits 0, and answers a different
question. That is the same failure as a vacuous `-E` filter and as counting
`#[ignore]` attributes — a command that ran and told you nothing, in a shape
indistinguishable from a real result.

**Never assert the skip count.** It breaks the first time someone adds a
legitimately ignore-worthy test, and it asserts nothing about whether the right
tests run — green-by-absence one level up. The mechanism is the durable fact;
the number is not. Note also that the raw totals above drift within a single
afternoon as agents land tests in a shared tree (5170 → 5171 → 5172 → 5173 on
2026-09-13, all benign), which is exactly why the re-derivation method belongs
here and the numbers do not.

`--no-default-features` reports four fewer, because `mod dictation` is
`#[cfg(feature = "desktop")]` (`lib.rs`) and its four ignored tests — one
whisper, three Pocket TTS — do not exist in that build: absent rather than
skipped. No `#[ignore]` anywhere is `cfg`-conditional, so nothing else moves
between the two configurations.

## Which timing assertions are load-bearing

A test that waits on wall-clock time asserts one of three things, and they are not
interchangeable. Before adding an `Instant` deadline, decide which row you are writing.

| Bound | Belongs to | Rule |
|---|---|---|
| the behaviour under test | "a mute upstream gives up" | keep it — and arm it *after* setup succeeds |
| setup reaching a state | handshake, fetch, process start | it must not be able to fail: size it so it cannot, or delete it |
| "did this hang forever" | the outer harness bound | strictly larger than every bound inside it, or its message lies |

**Never let one deadline serve two rows.** `call_tool_gives_up_on_a_mute_upstream`
handed its 300ms give-up deadline to the handshake as well; a loaded machine pushed
the handshake past it and the test failed as `call_tool never returned` — accusing the
exact mechanism it exists to prove works. Two named budgets is the fix.

**A freshly written executable is not a cheap thing to run.** With exec-time code
scanning (macOS `syspolicyd` plus an endpoint-security agent) the first exec of a new
file blocks while it is scanned, while re-exec'ing the *same* file costs ~6ms. The
scan cost is **episodic, not a constant**: a quiet scanner charges ~0.25s for a fresh
inode and a backlog charges tens of seconds — 6s to 102s was measured here, 393s at
the worst. There is also a smaller persistent penalty (~1.5-2.7x) on `/var/folders/…/T`,
which is where `tempfile::TempDir` lands. How the two compound is unresolved on
purpose: a multiplicative model predicts ~9s where 393s was measured. None of that
changes the remedy, and chasing either variable is wasted time. A
per-run temp script pays that scan inside the test's own timing window on every run,
which is why four `tunnels::supervisor` tests failed with the suite idle and passed
under a full parallel run. `fake_ssh_script` now keys the script by test name under
`target/fake-ssh/`, compares content, and execs it once with `TUIC_FAKE_SSH_WARMUP` set
before any supervisor starts: ~44s cold, once per machine, then ~1s a run. Do not
simplify it back to a `NamedTempFile`. The same reasoning applies to any test that
writes and runs a script — `sh <script>` is free, `./script` is not.

**A fetch that ran out of time looks exactly like a fetch that failed.** Both fall
through to the remote-tracking ref, so `FETCH_TIMEOUT`'s 5s `cfg(test)` value turned a
`conflict_assist` test about *which base ref wins* into a test of how fast git had
been. Where the deadline is not the subject, pass an explicit bound instead
(`resolve_rebase_target` takes one) and let nextest's `slow-timeout` catch a real hang.

**The failure mode to fear is a bound you cannot tell apart from a bug.** `#[ignore]`,
`#[serial]` and `--test-threads=1` all hide it rather than fix it, and cost coverage to
do so. When a timing assertion does fire, its message must name what actually broke —
otherwise the next reader spends a day re-diagnosing the wrong subsystem.

## Test instance vs orchestrator instance — READ BEFORE TESTING

There are TWO running TUICommander instances; do not confuse them:

- **Orchestrator instance** — the one this agent is embedded in. The `tuicommander` MCP tools and `debug invoke_js` target THIS instance (Mission Control on `:14319`, app logs on `:9876`). It does **NOT** run your worktree build, so testing it proves nothing about your changes.
- **Test instance** — the isolated worktree build started with `make agent-test` when desktop execution is authorized. Test it **only via its own HTTP API, default `http://127.0.0.1:19876`**. MCP/`invoke_js` cannot reach it. Implementation-only assignments must not launch it.

Test-instance endpoints (see `src-tauri/src/mcp_http/mod.rs`): `GET/POST /sessions`, `DELETE /sessions/{id}`, `POST /sessions/{id}/write`, `GET /sessions/{id}/output`, and terminal grid ops (all session-scoped) `POST /sessions/{id}/terminal/scroll {delta}`, `POST /sessions/{id}/terminal/scroll-to {line}` (absolute; `line`=top row, 0=oldest), `POST /sessions/{id}/terminal/scroll-to-offset {offset}` (coalesced display-offset jump; powers wheel + scrollbar-drag in browser mode), `GET /sessions/{id}/terminal/scroll-info`, `GET /sessions/{id}/terminal/lines?start&end`, `GET /sessions/{id}/terminal/row-text?row`, `POST /sessions/{id}/terminal/search-buffer {query}`. Create a throwaway session, exercise it, then `DELETE` it — never test against Boss's live sessions.

Canvas rendering (selection highlight, smooth-scroll visuals, cursor) is **not observable over HTTP** — those still need a visual check with Boss.

## Web-UI testing with agent-browser (browser mode, not Tauri)

You can exercise features through the **web UI** instead of the Tauri desktop app — every instance serves the full frontend at `http://localhost:<port>/` (`static_files.rs` `FRONTEND_DIST`). Loading it in a real browser is **browser mode** (`isTauri() === false`), the exact web/PWA path a remote client sees.

- **Port:** authorized agent desktop checks use `make agent-test`, preset to `http://127.0.0.1:19876/`. Do not launch another desktop through `make dev` or use the orchestrator's :9876 for change verification. The agent-test port is explicit and has no next-port fallback; see the development guide for overrides.
- **Drive it with `agent-browser`**, always via the stealth wrapper (see global rules). `@ref` CDP clicks are trusted and work (open modals/panels/tabs); **JS-dispatched keydown is `isTrusted:false`** so app keyboard shortcuts (Cmd+P, etc.) are ignored — click UI, never synthesize keys. Use a persistent `--session <name>`, restart the browser periodically (snapshots/clicks degrade after many calls), and wrap each call in a `perl alarm` timeout (macOS has no `timeout`/`gtimeout`).
- **Isolation:** `make agent-test` selects `TUIC_APP_INSTANCE=agent-test` with a distinct binary, app identity and socket. It does not copy the daily config and disables global input, microphone, Telegram and updater. Ordinary default-instance debug/release launches still share config, so do not use them as substitutes. Filesystem access remains shared: use throwaway repositories, never Boss's repositories, for repo-mutating checks.
- **The Command Palette DOES render in web mode.** `App.tsx:984` mounts it unconditionally as `<CommandPalette actions={actionEntries()} browserMode={!isTauri()} />`; `browserMode` filters the action list down to the HTTP-safe subset in `BROWSER_ACTION_IDS`/`BROWSER_ACTION_PREFIXES` (`CommandPalette.tsx:27-103`), which **includes** `search-files` and `search-file-contents`. Only the *opening shortcut* is unavailable to `agent-browser`: a JS-dispatched Cmd+P is `isTrusted:false`, so click the UI instead — that is a test-harness limit, not a missing feature.
- **Desktop-only features do NOT render in web mode** — IdeLauncher, Dictation, Global Hotkey, detach-panel windows, updater, native file drop, user-plugin install, MCP/hooks config. Built-in plugins DO load. See mdkb `web-mode-verification-2026-07-02` for the verified inventory before reporting a feature "missing".
