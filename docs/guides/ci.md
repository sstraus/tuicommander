# Continuous Integration

Five GitHub Actions workflows compile Rust:

| Workflow | Trigger | What it compiles |
|---|---|---|
| [`ci.yml`](https://github.com/sstraus/tuicommander/blob/main/.github/workflows/ci.yml) | pull request, push to `main` | clippy + `cargo nextest` + doctests on Linux, `tuic-remote` without the desktop feature, macOS and Windows builds on push |
| [`release.yml`](https://github.com/sstraus/tuicommander/blob/main/.github/workflows/release.yml) | version tag | the signed artifacts users install |
| [`remote-daemon.yml`](https://github.com/sstraus/tuicommander/blob/main/.github/workflows/remote-daemon.yml) | called by `release.yml` and `nightly.yml` | `tuic-remote` and `tuic-bridge` for Linux x64/ARM64, Windows x64 and macOS ARM64, uploaded to the `v*` release or to the rolling `nightly` release |
| [`nightly.yml`](https://github.com/sstraus/tuicommander/blob/main/.github/workflows/nightly.yml) | push to `main` | the rolling `nightly` release |
| [`audit.yml`](https://github.com/sstraus/tuicommander/blob/main/.github/workflows/audit.yml) | Monday 09:00 UTC | nothing — a prebuilt `cargo-audit` runs the advisory scan; accepted advisories live in `src-tauri/.cargo/audit.toml` |

## Frontend test shards

CI runs frontend checks once and Vitest on four independent `ubuntu-22.04`
runners with `--shard=1/4` through `--shard=4/4`. Vitest discovers the full
configured file set and partitions it. No test-name filter or exclusion is added.
Each shard retains the four-worker cap, default fork pool, per-file isolation,
`happy-dom` (except explicit per-file environments), setup files and async-leak
detection from `vitest.config.ts`. Coverage is not enabled in CI.

The required check keeps its name, **Frontend (typecheck + tests)**. It always
runs after `frontend-checks` and the complete `frontend-tests` matrix, and accepts
only `success` from both. Failure, cancellation or a skipped dependency fails
the gate. Matrix fail-fast is disabled so other shards can finish and report
failures. Typecheck, shell tests, workflow contracts, tracked-script checks and
third-party notices remain required. Each shard keeps the old 20-minute budget;
the aggregate check has a five-minute budget and runs no tests.

### Duration evidence (story 2347-f621)

Run `40252619205` on `b96b2b0fec` exceeded the old 20-minute frontend job budget
while test output still advanced. Setup and checks used about 33 seconds;
tests ran from 01:07:45 to cancellation at 01:27:26 UTC. This was not an idle
worker hang. The coordinator's local run on the same commit completed in
326.91 seconds (917 files passed, two files skipped; 10910 tests passed, three
skipped). The local machine reports 14 logical CPUs. This public repository's
hosted Linux runner has four vCPUs and 16 GB RAM according to the
[GitHub runner specification](https://docs.github.com/en/actions/reference/runners/github-hosted-runners#standard-github-hosted-runners-for-public-repositories).
The new resource step records actual Node, CPU, parallelism and memory values.

Two earlier frontend jobs did **not** hit the job timeout:

| Commit / run | Vitest duration | Result |
|---|---:|---|
| `d3404c764` / `40193236753` | 1133.59 s | One failed test: `MobileFontSize`, 5000 ms test timeout |
| `72c7c6823` / `40189776374` | 1140.79 s | The same failed test and timeout |

Both completed 912 passing files and 10871 passing tests. The subsequent font
fixture fix `ad834cb712` moves reset module graphs out of the timed assertion.
Sharding does not replace that fix or increase test deadlines.

The 72c7 run attributes tracked time to imports (39%), tests (30%), setup
(14%), environment (11%), transforms (5%) and workers (1%). It evaluated
2739 modules 50769 times, spending 1706.49 aggregate seconds importing. The
local b96 run shows a similar profile: imports 36%, tests 33%, environment
14%, setup 13%, transforms 5%; 2749 modules evaluated 51144 times, with
422.31 aggregate import seconds. These are aggregate worker timings, not wall
time. Expensive repeated isolated evaluation and DOM/setup work are present
on both machines; the config has no CI-only pool, worker or coverage branch.
The evidence supports distributing the work, not disabling isolation or raising
worker counts. It does not isolate CPU speed, contention and verbose reporter
I/O as separate causes of the hardware-dependent slowdown.

The cancelled b96 verbose log has these largest sums of reported individual
test timings per file (not complete file durations, which include imports and
setup):

| File under `src/__tests__/` | Timed test sum |
|---|---:|
| `hooks/useAgentPolling.test.ts` | 112.924 s |
| `stores/repositories.test.ts` | 44.681 s |
| `stores/mdTabs.test.ts` | 44.656 s |
| `stores/terminals.test.ts` | 31.227 s |
| `components/FileBrowserPanel/FileBrowserPanel.test.tsx` | 26.863 s |

The default reporter now emits file-level timings rather than a line for every
passing test. This makes future slow-file analysis direct; no measured speedup
is attributed to the reporter change. Four parallel shards target roughly
5-8 minutes each based on the old 19-minute run, with allowance for imbalance
and cold transforms. This is an estimate, not a CI result. Two consecutive
green runs on main remain the acceptance gate after the coordinator pushes.

Local verification used `CI=true`, the same config and shard arguments, and
an additional JSON reporter to compare executed file sets:

| Shard | Discovered/reported files | Passed tests | Existing skipped tests | Vitest duration |
|---|---:|---:|---:|---:|
| 1/4 | 230 | 2525 | 2 | 75.97 s |
| 2/4 | 230 | 2589 | 0 | 95.78 s |
| 3/4 | 230 | 2687 | 1 | 75.89 s |
| 4/4 | 229 | 3109 | 0 | 86.51 s |

The actual shard file sets are disjoint and their union equals all 919 files
from unfiltered Vitest discovery. Totals match the coordinator's unsharded run:
10910 passed tests and three existing skips. No test was skipped by this change.
The font fixture passes too. Runs were sequential to bound local resource use;
these numbers do not claim a Linux CI pass. Vitest 5 `list --filesOnly` ignores
`--shard`, so do not use it as a shard manifest: compare the JSON reports from
actual runs instead. The workflow contract test also exercises the aggregate
shell command against all 16 pairs of success, failure, cancellation and skip.

## Nightly release ordering

The cleanup job moves the `nightly` tag, deletes the old release and recreates an
empty prerelease before the desktop and daemon matrices start. Successful daemon
builds can publish even if every desktop build fails. `tauri-action` reuses the
release with that tag and alone writes `latest.json`; daemon uploads contain only
`tuic-remote` and `tuic-bridge` binaries.

## The Rust toolchain is pinned

Every `dtolnay/rust-toolchain` step takes its version from a workflow-level
`RUST_VERSION` variable:

```yaml
env:
  RUST_VERSION: "1.99.1"

# ...

      - name: Install Rust ${{ env.RUST_VERSION }}
        uses: dtolnay/rust-toolchain@stable
        with:
          toolchain: ${{ env.RUST_VERSION }}
```

The action's `toolchain` input overrides the `@stable` branch default, so the
branch name in `uses:` no longer decides the compiler.

### Why

`@stable` on its own floats. The day rustup publishes a new stable, the compiler
changes under the repository with no commit to point at: fresh clippy lints and
rustfmt reflows become a red `main` for whoever pushes next, not for whoever
wrote the code. That has happened three times — `7db734f9`, `20e58629`, and the
1.98 lints cleared on 2026-08-31 while CI was still green on an older stable, so
the tree failed locally and passed in CI at the same commit.

Pinning also makes the release reproducible: a tag built today and rebuilt in six
months uses the same compiler.

### How to bump the pin

1. Install the target toolchain locally (`rustup toolchain install <version>`)
   and make it the default for this checkout.
2. From `src-tauri/`, confirm both gates are clean:

   ```bash
   cargo clippy --all-targets -- -D warnings
   cargo fmt --check
   ```

   Fix whatever the new compiler flags. This is the work the pin defers to a
   deliberate moment instead of ambushing the next push.
3. Raise `RUST_VERSION` in **all five** workflows — `ci.yml`, `release.yml`,
   `remote-daemon.yml`, `nightly.yml`, `audit.yml` — in a single commit.
4. Push. CI runs on the new toolchain as part of that commit, so any breakage
   lands on its author and is bisectable.

Never raise the pin in one workflow only: `ci.yml` would then vouch for a
compiler that `release.yml` does not use.

> The local `rustc` is not pinned by a `rust-toolchain.toml`, on purpose —
> contributors keep their own rustup default. `cargo fmt --check` and clippy are
> the contract; the CI pin is what makes that contract stable over time.

## Note on clippy scope

CI runs `cargo clippy --workspace --exclude bm25 --all-targets -- -D warnings`.
It includes test and bench code. The vendored `bm25` patch is excluded from
Clippy, but remains in workspace tests.

## Windows CMake debug flags

CI, release and nightly set `CMAKE_C_FLAGS` and `CMAKE_CXX_FLAGS` to `-Z7`
on Windows for sccache-compatible MSVC debug information. MSVC accepts both
slash and dash prefixes. Use the dash form because Git Bash converts `/Z7`
in environment variables into a Windows path before launching native Cargo.
That path reaches the CMake compiler probe as a source filename and fails the
`whisper-rs-sys` build. The Tests step uses Git Bash through
`scripts/with-test-tmp.sh`, so these flags must be safe in both shell paths.

## Headless workspace scope

The remote job checks `tuic-remote` with `--no-default-features`. Dictation is
already an optional dependency behind `desktop`; the headless daemon does not
use WebRTC. Its workspace test command also excludes `tuic-dictation`, because
`--workspace` selects that desktop-only member independently of feature flags.
The desktop workspace jobs still compile and test dictation with Meson and Ninja.

## Nextest setup scope check

`make test-shell` runs the frontend job's tool-independent shell tests. The
Nextest fixture setup scope check is named `scripts/check-nextest-fixture-scope.sh`
and runs explicitly in the Linux Rust job after workspace tests. That job
installs Nextest and already has the compiled test binaries. The check retains
its positive fixture-consumer and negative non-consumer assertions.

## macOS CI test compilation

The macOS cross-platform job has a 90-minute timeout. On `9a49ff1cb`, Clippy
finished in 9m27s and the test build took 67m17s. Nextest fixture setup then
spent 532.140s building the headless fixture binaries. Actual test execution
had only about 4m18s before cancellation, with 6483 of 7391 tests passing.
The longest completed test took 41.268s. This log does not separate compiler
optimization from linking, and does not measure a complete suite duration.

The macOS and Windows Tests steps set `CARGO_PROFILE_TEST_OPT_LEVEL=0` to avoid
optimizing workspace test harnesses. Non-workspace dependencies retain the
inherited `[profile.dev.package."*"]` optimization level of 1. The explicit
`tuic-terminal` test override also stays at 1 for corpus replays. Test selection,
retries, hang detection and the job budget stay the same. This does not change
release builds. The next CI run must measure the compile-time benefit and check
for runtime regressions.

## Fixture build profile and Windows cache

On Windows run `40150611058`, the test build took 45m36s. Nextest setup then
built the fixtures with the dev profile and no default features for 18m15s.
Dev includes debug information that the test profile disables. This second
profile and feature graph prevent reuse of many artifacts from the test build.

Local lib-only fixture setup uses `cargo build --profile test
--no-default-features`. Desktop CI sets `TUIC_FIXTURE_FEATURES=dictation`, which
selects `cargo test --workspace --no-run --features dictation` instead. This
matches nextest's workspace test selection, including dev-dependency feature
unification. A binary-only build with `--profile test` does not activate dev
dependencies: under resolver 2 it omits features such as `tokio/test-util` and
`tuic-core/test-support`, so profile and top-level feature alignment alone do
not establish graph reuse.

The CI setup command runs no tests. Integration targets make Cargo build the
normal fixture executables; setup can reuse the workspace artifacts nextest
already built. Both paths put the executables beside the test binary's `deps`
directory under `debug`, so consumer path resolution does not change. Setup
scoping remains unchanged: unrelated lib tests do not build fixtures.
`scripts/check-fixture-build-graph.py` captures each builder's actual command
and compares its resolved package-feature graph with CI's workspace test graph.
It does not replace a full-workspace warm-build measurement on Windows.

That Windows run restored no Cargo target cache. Rust sccache reported 131 hits
and 1291 misses (9.21% hit rate); linked binaries and proc macros were among its
non-cacheable calls. sccache does not replace the Cargo target cache. The Windows
Rust cache now saves after a failed or cancelled job too, so later runs can
restore the existing `C:/t` artifacts instead of depending only on sccache.
These changes do not prove that Windows fits its 90-minute bound: a complete
Windows CI run remains the required runtime evidence.

## Release notes size

The nightly `update-notes` job groups commits since the latest stable tag.
Before publication, `scripts/cap-release-notes.py` limits the notes to 120000
UTF-8 bytes, below GitHub's 125000-character body limit. Oversized notes end at
a complete line and include a link to the full comparison against `main`.
Short notes keep their original contents.

The version-tag workflow uses the matching `CHANGELOG.md` section and a download
legend. It does not use the nightly commit-history notes step.
