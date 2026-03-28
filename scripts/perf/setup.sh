#!/usr/bin/env bash
# Install profiling tools for TUICommander performance analysis.
# Run once before first profiling session.
set -euo pipefail

echo "=== TUICommander Profiling Setup ==="

# samply: CPU flamegraph profiler for Rust (macOS/Linux)
if command -v samply &>/dev/null; then
  echo "[ok] samply $(samply --version 2>&1 | head -1)"
else
  echo "[install] samply..."
  cargo install samply
fi

# tokio-console: live Tokio runtime inspector
if command -v tokio-console &>/dev/null; then
  echo "[ok] tokio-console installed"
else
  echo "[install] tokio-console..."
  cargo install tokio-console
fi

# hyperfine: command-line benchmarking tool
if command -v hyperfine &>/dev/null; then
  echo "[ok] hyperfine $(hyperfine --version)"
else
  echo "[install] hyperfine..."
  brew install hyperfine 2>/dev/null || cargo install hyperfine
fi

echo ""
echo "=== Setup complete ==="
echo ""
echo "Available profiling commands:"
echo "  scripts/perf/record-cpu.sh      — CPU flamegraph via samply"
echo "  scripts/perf/record-tokio.sh    — Live Tokio task inspector"
echo "  scripts/perf/bench-ipc.sh       — IPC latency for git commands"
echo "  scripts/perf/bench-pty.sh       — PTY throughput measurement"
echo "  scripts/perf/snapshot-memory.sh  — Frontend heap snapshot guide"
