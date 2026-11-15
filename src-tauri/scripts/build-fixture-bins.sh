#!/bin/sh
# `cargo nextest run --lib` builds only the lib target, but several lib unit
# tests spawn the package's [[bin]] fixtures (tuic-acp-fixture-agent,
# tuic-mcp-fixture-server) as subprocesses. Build them once, here, before the
# run starts: a per-test on-demand build races under nextest, which runs one
# process per test (ten nested `cargo build`s competing for the same lock).
#
# The headless default still exercises the exact binaries lib tests spawn:
# both fixtures require the test-only feature. This avoids desktop permission
# validation for local lib-only runs. CI supplies TUIC_FIXTURE_FEATURES and has already
# built workspace tests. Select that same test graph, including dev-dependency
# feature unification: cargo build --profile test alone does not include it.
# Integration targets build the normal fixture executables, not test harnesses.
set -eu
# llvm-cov passes --target-dir to nextest, not to this nested Cargo build.
# Its LLVM_PROFILE_FILE lives in that exact artifact root (also for custom
# coverage target directories). Keep fixtures beside the instrumented tests.
set --
if [ "${CARGO_LLVM_COV:-}" = 1 ]; then
    : "${LLVM_PROFILE_FILE:?llvm-cov must provide its profile output path}"
    set -- --target-dir "$(dirname "$LLVM_PROFILE_FILE")"
fi
if [ -n "${TUIC_FIXTURE_FEATURES:-}" ]; then
    cargo test --workspace --no-run --features "$TUIC_FIXTURE_FEATURES" "$@"
else
    cargo build --profile test --no-default-features --features test-fixtures --bin tuic-acp-fixture-agent --bin tuic-mcp-fixture-server "$@"
fi
