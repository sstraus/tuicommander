#!/usr/bin/env bash
# Record a CPU flamegraph of TUICommander using samply.
# Builds in release mode with debug info, then launches under samply.
# When you close the app, the flamegraph opens in Firefox Profiler.
#
# Usage:
#   scripts/perf/record-cpu.sh              # default: 60s then auto-stop
#   scripts/perf/record-cpu.sh --duration 120  # custom duration
#   scripts/perf/record-cpu.sh --manual     # stop manually (close the app)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
DURATION=60
MANUAL=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --duration) DURATION="$2"; shift 2 ;;
    --manual)   MANUAL=true; shift ;;
    *)          echo "Unknown arg: $1"; exit 1 ;;
  esac
done

echo "=== Building release with debug info ==="
cd "$ROOT/src-tauri"

# Release build with debug symbols for meaningful flamegraphs
CARGO_PROFILE_RELEASE_DEBUG=2 cargo build --release 2>&1 | tail -3

BINARY="$ROOT/src-tauri/target/release/tuicommander"
if [[ ! -f "$BINARY" ]]; then
  echo "ERROR: Binary not found at $BINARY"
  exit 1
fi

OUTPUT_DIR="$ROOT/scripts/perf/results"
mkdir -p "$OUTPUT_DIR"
TIMESTAMP=$(date +%Y%m%d-%H%M%S)

echo ""
echo "=== Recording CPU profile ==="
if $MANUAL; then
  echo "Close the app when done. Flamegraph will open automatically."
  samply record --save-only --output "$OUTPUT_DIR/cpu-$TIMESTAMP.json" "$BINARY"
else
  echo "Recording for ${DURATION}s. Use the app normally."
  echo "Scenarios to exercise:"
  echo "  1. Open 3-5 terminal tabs"
  echo "  2. Run a command that produces output (e.g., find /)"
  echo "  3. Open the Git panel"
  echo "  4. Switch between tabs"
  echo "  5. Open Settings panel"
  echo ""
  samply record --duration "$DURATION" --save-only --output "$OUTPUT_DIR/cpu-$TIMESTAMP.json" "$BINARY" &
  SAMPLY_PID=$!
  sleep "$DURATION"
  kill -INT "$SAMPLY_PID" 2>/dev/null || true
  wait "$SAMPLY_PID" 2>/dev/null || true
fi

echo ""
echo "Profile saved: $OUTPUT_DIR/cpu-$TIMESTAMP.json"
echo "Open with: samply load $OUTPUT_DIR/cpu-$TIMESTAMP.json"
