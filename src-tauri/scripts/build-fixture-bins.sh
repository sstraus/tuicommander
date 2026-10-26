#!/bin/sh
# `cargo nextest run --lib` builds only the lib target, but several lib unit
# tests spawn the package's [[bin]] fixtures (tuic-acp-fixture-agent,
# tuic-mcp-fixture-server) as subprocesses. Build them once, here, before the
# run starts: a per-test on-demand build races under nextest, which runs one
# process per test (ten nested `cargo build`s competing for the same lock).
#
# The headless default still exercises the exact binaries lib tests spawn:
# neither fixture is feature-gated. It avoids desktop permission validation
# for local lib-only runs. CI supplies TUIC_FIXTURE_FEATURES and has already
# built workspace tests. Select that same test graph, including dev-dependency
# feature unification: cargo build --profile test alone does not include it.
# Integration targets build the normal fixture executables, not test harnesses.
set -eu
if [ -n "${TUIC_FIXTURE_FEATURES:-}" ]; then
    cargo test --workspace --no-run --features "$TUIC_FIXTURE_FEATURES"
else
    cargo build --profile test --no-default-features --bin tuic-acp-fixture-agent --bin tuic-mcp-fixture-server
fi
