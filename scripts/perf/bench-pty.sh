#!/usr/bin/env bash
# Benchmark PTY throughput by blasting data through a terminal session.
# Requires TUICommander to be running with an open terminal tab.
#
# This is a MANUAL benchmark — it prints commands to run in a TUICommander
# terminal and measures timing externally. The HTTP API does not support
# session creation directly.
#
# Usage:
#   scripts/perf/bench-pty.sh           # default: 10MB of output
#   scripts/perf/bench-pty.sh 50        # 50MB stress test
set -euo pipefail

SIZE_MB="${1:-10}"
LINES=$((SIZE_MB * 50000))

echo "=== PTY Throughput Benchmark ==="
echo ""
echo "Run these commands in a TUICommander terminal tab to measure throughput."
echo "Each command generates ~${SIZE_MB}MB of output through the full pipeline:"
echo "  PTY read → UTF-8 decode → escape processing → VT100 parse → Tauri emit → xterm.js render"
echo ""
echo "━━━ Test 1: Sequential numbered lines (compressible) ━━━"
echo "  time seq 1 $LINES > /dev/null"
echo "  # Measures raw PTY throughput without xterm rendering"
echo ""
echo "  time seq 1 $LINES"
echo "  # Measures full pipeline including xterm rendering"
echo ""
echo "━━━ Test 2: Random data (incompressible) ━━━"
echo "  time dd if=/dev/urandom bs=1024 count=$((SIZE_MB * 1024)) | base64"
echo "  # Stress-tests UTF-8 decode + escape processing"
echo ""
echo "━━━ Test 3: Repetitive data (tests write batching) ━━━"
echo "  time yes | head -n $LINES"
echo "  # Fast repetitive output — tests if batching helps"
echo ""
echo "━━━ Test 4: Realistic output ━━━"
echo "  time find / -type f 2>/dev/null | head -n $LINES"
echo "  # Real filesystem paths with varied line lengths"
echo ""
echo "━━━ What to measure ━━━"
echo "  - Wall clock time (from 'time' output)"
echo "  - CPU usage (Activity Monitor or 'top -pid \$(pgrep tuicommander)')"
echo "  - App responsiveness during output (try switching tabs)"
echo "  - WebGL renderer FPS (DevTools > Rendering > FPS meter)"
echo ""
echo "Record results in: scripts/perf/results/pty-throughput.log"
echo "Format: YYYYMMDD-HHMMSS  test_name  size_MB  duration_s  notes"
