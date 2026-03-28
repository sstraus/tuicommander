#!/usr/bin/env bash
# Run all automated profiling benchmarks in sequence.
# Requires TUICommander to be running.
#
# Usage: scripts/perf/run-all.sh [repo_path]
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO="${1:-}"

echo "╔══════════════════════════════════════╗"
echo "║  TUICommander Performance Benchmark  ║"
echo "╚══════════════════════════════════════╝"
echo ""
echo "Date: $(date)"
echo ""

# IPC benchmark
echo "━━━ IPC Latency ━━━"
if [[ -n "$REPO" ]]; then
  bash "$SCRIPT_DIR/bench-ipc.sh" "$REPO"
else
  bash "$SCRIPT_DIR/bench-ipc.sh"
fi
echo ""

# PTY benchmark
echo "━━━ PTY Throughput ━━━"
bash "$SCRIPT_DIR/bench-pty.sh" 5
echo ""

# Memory guide
echo "━━━ Memory Profiling ━━━"
echo "Run manually: scripts/perf/snapshot-memory.sh"
echo ""

# CPU profiling hint
echo "━━━ CPU Profiling ━━━"
echo "Run separately: scripts/perf/record-cpu.sh --duration 60"
echo "Run separately: scripts/perf/record-tokio.sh"
echo ""

echo "Results saved in: scripts/perf/results/"
echo "Compare across runs to track regressions."
