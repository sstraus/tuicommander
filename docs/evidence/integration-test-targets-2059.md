# Integration target consolidation — story 2059-5902

## Scope and result

The six crates with multiple integration targets now use `tests/main.rs` and one
module per former target under `tests/cases/`. Assertions and test function names
are unchanged. The former target name is now the module prefix. ACP helpers are
compiled once and imported by the application test modules.

Integration targets decrease from **73 to 9** across the workspace. The other
three targets already belong to separate crates and need no consolidation.
There are no custom-harness exceptions. Fixture agents remain `[[bin]]` targets;
they are real subprocess images, not integration-test harnesses.

Cargo manifests, `.github`, `scripts/`, `Makefile`, and the Nextest configuration
were checked for target-name consumers. None names a removed integration target.
The fixture-bin Nextest filters select the unchanged library binary.

## Environment and method

Measurements were made on rb, `aarch64-unknown-linux-gnu`, with Rust 1.98.1
(`48a229ceaefd4985c50990b14116b6d856af0985`), Cargo 1.98.1
(`797e8a9bc`, 2026-08-05), and LLVM 22.1.8. Cargo resolves to
`/home/rb/.cargo/bin/cargo`. Both runs use the normal rb cache mode, not the
experimental mbx mode. No target directory or cache was deleted or overridden.
The baseline source commit is `7ad589c5e89a02f256c74c09d4dba3c22c9187a9`.

All Rust commands run through `build-slot.sh --remote` from the checkout root.
Test validation uses `scripts/with-test-tmp.sh`. Baseline measurements precede
source edits. Package selection changes Cargo feature unification and can rebuild
dependencies; the first invocation is therefore not a controlled cold-build
comparison. No claim of an empty dependency cache is made.

### Requested Nextest no-run timings

Commands, from `src-tauri`:

```sh
cargo nextest run --workspace --no-run
cargo nextest run -p tuic-terminal --no-run
cargo nextest run -p tuicommander --no-run
```

| Scope | Initial baseline (s) | Initial after (s) | Baseline repeat (s) | Final after repeat (s) |
| --- | ---: | ---: | ---: | ---: |
| Workspace | 464.906 | 15.131 | 14.873 | 0.957 |
| tuic-terminal | 18.854 | 2.301 | 1.398 | 0.623 |
| tuicommander | 302.985 | 14.146 | 1.915 | 0.939 |

These initial numbers include different amounts of dependency work. In particular,
the baseline workspace repeat rebuilt tuicommander after per-package selection.
Do not attribute these entire differences to fewer links.

A second baseline pass warmed each package separately. Its repeated no-run values
provide the following more conservative per-package comparison:

| Crate | Baseline warm (s) | Final warm (s) | Reduction (s) |
| --- | ---: | ---: | ---: |
| tuic-terminal | 0.671 | 0.623 | 0.048 |
| tuicommander | 1.969 | 0.939 | 1.030 |
| tuic-cli | 0.633 | 0.623 | 0.010 |
| tuic-ipc | 0.633 | 0.624 | 0.009 |
| tuic-dictation | 0.652 | 0.675 | -0.023 |
| tuic-bridge | 0.591 | 0.646 | -0.055 |

### Controlled test-source rebuild

To measure real target work without deleting caches, touch only the integration
source files in the rb snapshot, then run the following before and after:

```sh
cargo build --workspace --tests --profile test --timings
```

Before: touch top-level `tests/*.rs` in each crate. After: touch the new root and
case modules, and the unchanged top-level tests in single-target crates. Fixture
binaries and ACP helper source are not touched. Content does not change.

Wall time: **34.672 s before, 10.817 s after**, a reduction of **23.855 s (68.8%)**.
The timing HTML records these integration-target durations:

| Crate | Targets before → after | Sum of target durations before (s) | After (s) | Reduction (s) |
| --- | --- | ---: | ---: | ---: |
| tuic-terminal | 27 → 1 | 15.36 | 0.64 | 14.72 |
| tuicommander | 20 → 1 | 201.53 | 10.28 | 191.25 |
| tuic-cli | 9 → 1 | 2.66 | 0.38 | 2.28 |
| tuic-ipc | 7 → 1 | 2.98 | 0.24 | 2.74 |
| tuic-dictation | 4 → 1 | 5.68 | 1.38 | 4.30 |
| tuic-bridge | 3 → 1 | 2.60 | 0.32 | 2.28 |

These are summed concurrent rustc target durations, **not per-crate wall times or
pure linker timings**. Stable Cargo timing data reports `sections: null` for the
integration executables. It does not separate their compilation and linking.
The HTML was retained, but a pure link-versus-compile split is not established.
This measurement supports less test-target work, not a claim that linking alone
explains long builds. No statistical confidence is inferred from one run.

## Test preservation and validation

The full before/after Nextest JSON inventories were compared by package, kind,
normalized name, and ignored flag. For a former integration target, normalization
adds its former binary name as the module prefix. The complete sets are equal:
**8,709 tests**, including **33 ignored**. No count-only or empty-filter check is
used. Counts include library tests:

| Crate | Before = after | Ignored | Per-crate result |
| --- | ---: | ---: | --- |
| tuic-terminal | 984 | 3 | 981 passed |
| tuicommander | 6,010 | 24 | Merged integration target: 110 passed |
| tuic-cli | 98 | 0 | 98 passed |
| tuic-ipc | 44 | 0 | 44 passed |
| tuic-dictation | 500 | 6 | 494 passed |
| tuic-bridge | 28 | 0 | 28 passed |

Nextest runs each individual test in its own process. Existing environment,
static-state, cwd, and socket tests remain isolated despite sharing a linked
image. Unix and Windows module-level cfg attributes remain intact.

Final `cargo nextest run --workspace --no-fail-fast`: **8,675 passed, 1 failed,
33 skipped**, in 82.659 s. The only failure is the unchanged library test
`pty::tests::replay_oracle_all_committed_tcap_preserves_chunk_decisions`:
`src/fixtures/replay_oracle/captures/src-tauri/src/fixtures/agent_prompts/ego-slash-filter-2030.tcap.jsonl`
is absent from the source tree at the baseline commit. Golden expectations were
not regenerated, and this failing test was not excluded. The earlier application
package run stopped at the same failure (4,517 passed, 1 failed; 1,468 not run).
The full workspace run executed those remaining tests.

Additional checks:

- Touched-crate `cargo clippy --all-targets ... -- -D warnings`: passed.
- `cargo nextest run -p tuicommander --no-default-features --test main`:
  125 passed, including the headless-only named-instance CLI tests.
- `fmt-changed.sh <worktree> main...HEAD`: passed after formatting moved paths.
- Instruction symlink validation: passed.

No native macOS or Windows runtime verification is claimed. No production Rust
code changed, so no desktop restart is required for this consolidation.

## Evidence retained on the coordinator Mac

All paths below are under `~/Gits/.tmp/`:

- `chore-2059-baseline.log`: initial timings, toolchain, and baseline inventory.
- `chore-2059-baseline-timings.log`: unchanged-source rebuild and package repeats.
- `chore-2059-baseline-timing.html`: extracted timing HTML (interleaved Nextest
  stderr notice removed from the HTML only; original log retained).
- `chore-2059-terminal.log`, `chore-2059-tuicommander.log`,
  `chore-2059-tuicommander-integration.log`, `chore-2059-remaining.log`:
  package validation.
- `rb/20261019T102200Z-38348/artifacts/reports/visual/2059-after-timing.txt`:
  after timing HTML, with `.txt` extension for rb artifact collection.
- The same artifact directory contains `2059-after-list.json`.
- `chore-2059-final.log`: complete final workspace result and after timings.
- `chore-2059-lint-headless.log`: Clippy and headless integration results.

The story cannot claim a fully green workspace or a pure linker-time split.
