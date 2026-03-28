#!/usr/bin/env bash
# Build TUICommander with tokio-console support and launch both.
# tokio-console connects automatically and shows live Tokio task stats.
#
# What to look for:
#   - Tasks with high "busy" time → CPU-bound work on async executor
#   - Tasks with high "idle" time → blocked on I/O or locks
#   - Many short-lived tasks → excessive spawn_blocking churn
#   - Tasks stuck in "waiting" → possible deadlock or contention
#
# Usage: scripts/perf/record-tokio.sh
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

echo "=== Building with tokio-console feature ==="
cd "$ROOT/src-tauri"
RUSTFLAGS="--cfg tokio_unstable" cargo build --features tokio-console 2>&1 | tail -3

BINARY="$ROOT/src-tauri/target/debug/tuicommander"

echo ""
echo "=== Launching TUICommander + tokio-console ==="
echo "tokio-console will open in this terminal."
echo "TUICommander will open in a new window."
echo "Press Ctrl+C to stop both."
echo ""

# Launch the app in background
RUSTFLAGS="--cfg tokio_unstable" "$BINARY" &
APP_PID=$!

# Give it a moment to start the console subscriber
sleep 2

# Launch tokio-console (connects to default port 6669)
tokio-console || true

# Cleanup
kill "$APP_PID" 2>/dev/null || true
wait "$APP_PID" 2>/dev/null || true
echo "Done."
