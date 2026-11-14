# Contributing to TUICommander

Thanks for your interest in contributing! This guide covers what we expect from pull requests.

## Before You Start

1. **Check existing issues** before starting work on a new feature or fix.
2. **Open an issue first** for non-trivial changes (new features, architectural changes). Small bug fixes and typo corrections can go straight to a PR.
3. **Keep PRs focused.** One logical change per PR. Don't mix refactors with features. If your diff exceeds ~500 lines, consider splitting it.

## Development Setup

```bash
# Prerequisites: Node.js 24+, Rust stable, platform dependencies (see README)
pnpm install --frozen-lockfile
cd src-tauri && cargo build

# Run the app in development mode (installs the git hooks on first run)
make dev
```

On macOS, `make dev` also requires Python 3. It launches a stable executable
copy outside Cargo/mbx targets; see [Development Setup](docs/guides/development-setup.md).

`make check` also needs two cargo subcommands:

```bash
cargo install cargo-nextest cargo-audit
```

## Quality Gates

Every PR must pass these checks before review. CI enforces them automatically.

### Tests Are Required

**All PRs must include tests for new or changed behavior.** This is non-negotiable.

- Bug fix? Add a regression test that fails without your fix.
- New feature? Add tests covering the happy path, error cases, and edge cases.
- Refactor? Existing tests must still pass. If you change behavior, update the tests.

We have thousands of frontend and Rust tests. We expect that number to go up with every feature PR.

**What counts as a test:**
- Vitest unit/integration tests for frontend code (`src/__tests__/`)
- `cargo test` for Rust code
- Both, when your change spans the stack

**What does NOT count:**
- "I tested it manually" without automated tests
- Tests that only verify mocked behavior instead of real logic
- Tests with no assertions

### Run the Full Suite Locally

```bash
make fmt    # auto-format the frontend (Biome) and Rust (rustfmt)
make check  # every gate CI enforces
```

`make check` must pass with zero errors and zero warnings. It runs, in order:
`tsc --noEmit`, Biome, the architecture cycle checks, the plugin capability doc
sync check, `cargo fmt --check`, `cargo clippy --release -- -D warnings`,
`cargo nextest run --workspace` plus doctests, Vitest, the plugin tests,
`pnpm audit`, and `cargo audit`.

Cargo owns the executables in `src-tauri/target/`: sidecar staging never
copies back into that directory. `pnpm build:sidecar` builds the CLI and bridge
with Cargo and stages them in `src-tauri/binaries/` for development and
packaging. `build.rs` disables only tauri-build's reverse sidecar copy;
`tauri.conf.json` and CLI packaging overrides keep the bundle entries.

To run a single gate, copy its command out of the `check` target in the
[Makefile](Makefile) — that target is the source of truth for what CI enforces.
Prefix standalone test commands with `scripts/with-test-tmp.sh` from the
repository root (`../scripts/with-test-tmp.sh` from `src-tauri/`). It creates
a per-run directory under `.tmp/tuic-tests/` and removes it when the command
ends, including when a test fails. `make check`, `make test-shell`, `make cov`, and
`make mutants` do this automatically. `make dev` and `cargo run` retain their
normal runtime temporary directory.

When `src-tauri/src/design_mode/extract.js` changes, run `TUIC_DESIGN_BROWSER=/path/to/browser-stealth/ab-stealth.sh scripts/with-test-tmp.sh pnpm test:design-mode-browser`; this test requires real Chromium and is not in CI.

Mutation testing runs on a change, not on the tree: `make mutants RANGE=<base>`
(default `HEAD~1`) needs `cargo install --locked cargo-mutants` and reports the
mutants of the changed Rust lines that no test caught. A surviving mutant is a
missing test. Pass simple cargo-mutants filters as trailing goals, for example
`make mutants RANGE=<base> -- --re parse_clean_lines`. For filters containing
spaces or other complex arguments, use `MUTANTS_ARGS`.

The Makefile prefixes each gate with `rtk`, an optional output compactor, when
it is on your `PATH`. It is not a project dependency: with `rtk` absent every
gate runs raw and unfiltered. Use `make RTK= check` to bypass it even when it
is installed.

### Code Style

- Match the style of surrounding code. Consistency within a file matters more than any external style guide.
- Rust: `cargo clippy --release -- -D warnings` must be clean — that is the exact gate in `make check`.
- TypeScript: `tsc --noEmit` must be clean. No `any` types.
- Use `appLogger` for logging, never `console.log`.

## PR Guidelines

### Commit Messages

We use [Conventional Commits](https://www.conventionalcommits.org/):

```
feat(terminal): add scrollback search
fix(sidebar): prevent double-click race on rename
refactor(config): extract settings validation
```

### PR Description

Include:
- **Summary:** What changed and why (not how -- the diff shows how).
- **Test plan:** Checklist of what was tested, both automated and manual.

Add user-visible changes under `[Unreleased]` in `CHANGELOG.md`, and add manual checks to the matching `to-test/<area>*.md` file (indexed by `to-test.md`) when needed. These files use Git's union merge driver so concurrent additions survive a merge. Review their final ordering and remove duplicate entries before landing.

### What We Look For in Review

| Area | Expectation |
|------|-------------|
| Tests | New behavior has automated tests. Regression tests for bug fixes. |
| Types | No `any`, no non-null assertions (`!`) outside tests. Prefer narrowing. |
| Errors | Handle errors explicitly. No `let _ =` on Results, no empty `catch {}`. |
| Architecture | Business logic in Rust, not TypeScript. Frontend renders and handles interaction. |
| Scope | PR does one thing. No drive-by refactors mixed with features. |
| CI | All checks green. Don't merge with failures. |

### Common Rejection Reasons

Based on past PRs, here's what gets PRs closed or sent back:

1. **No tests.** The most common issue. Even a 1-line bug fix needs a regression test.
2. **PR too large.** 100+ changed files is unreviewable. Split into smaller, independent PRs.
3. **Breaking CI.** Check that sidecar stubs and platform dependencies are handled (see `.github/workflows/ci.yml`).
4. **Missing error handling.** Silent failures (`let _ =`, `Err(_) =>`, empty catch blocks) hide bugs.
5. **Not matching existing patterns.** Read the surrounding code before writing new code.

## Architecture Quick Reference

- **Tauri v2** desktop app: Rust backend, SolidJS frontend
- **Business logic lives in Rust.** The frontend only renders and handles user interaction.
- **PTY interaction:** Always use `sendCommand()` from `src/utils/sendCommand.ts`, never write `text + \r` directly.
- **Repo-dependent UI:** Use `repositoriesStore.getRevision(repoPath)` in `createEffect`, not file watchers.
- **Logging:** Use `appLogger` from `src/stores/appLogger.ts`, never `console.log`.

See [AGENTS.md](AGENTS.md) for the full project rules and [docs/](docs/) for detailed documentation.

## AI-Generated PRs

PRs generated by AI coding tools (Claude Code, Copilot, Cursor, etc.) are welcome, but they're held to the **same quality bar** as human-written code. AI-generated PRs without tests will be closed.

## License

By contributing, you agree that your contributions will be licensed under the project's [Apache 2.0 License](LICENSE).
