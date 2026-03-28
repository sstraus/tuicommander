#!/usr/bin/env bash
# Benchmark PTY throughput by blasting data through a terminal session.
# Requires TUICommander to be running.
#
# Measures how fast the PTY pipeline processes data:
#   PTY read → UTF-8 decode → escape processing → VT100 parse → Tauri emit
#
# Usage:
#   scripts/perf/bench-pty.sh           # default: 10MB of output
#   scripts/perf/bench-pty.sh 50        # 50MB of output
set -euo pipefail

PORT=9877
BASE="http://localhost:$PORT"
SIZE_MB="${1:-10}"

if ! curl -sf "$BASE/health" >/dev/null 2>&1; then
  echo "ERROR: TUICommander not running on port $PORT"
  exit 1
fi

echo "=== PTY Throughput Benchmark ==="
echo "Data size: ${SIZE_MB}MB"
echo ""

OUTPUT_DIR="$(cd "$(dirname "$0")" && pwd)/results"
mkdir -p "$OUTPUT_DIR"
TIMESTAMP=$(date +%Y%m%d-%H%M%S)

# Create a session
SESSION_ID=$(curl -sf -X POST "$BASE/api/session" \
  -H "Content-Type: application/json" \
  -d '{"cwd":"/tmp"}' 2>/dev/null | python3 -c "
import sys, json
data = json.load(sys.stdin)
print(data.get('session_id', data.get('id', '')))" 2>/dev/null || echo "")

if [[ -z "$SESSION_ID" ]]; then
  echo "ERROR: Could not create PTY session via API."
  echo "The HTTP API may not support session creation."
  echo ""
  echo "Alternative: manual PTY throughput test"
  echo "  1. Open a terminal tab in TUICommander"
  echo "  2. Run: dd if=/dev/urandom bs=1024 count=$((SIZE_MB * 1024)) | base64"
  echo "  3. Observe CPU usage and responsiveness during output"
  echo "  4. Run: yes | head -n $((SIZE_MB * 50000))"
  echo "  5. Compare throughput between random data and repetitive data"
  echo ""
  echo "For automated measurement, use the commands above in an existing"
  echo "terminal and time them with: time <command>"
  exit 0
fi

echo "Session: $SESSION_ID"

# Send a command that generates predictable output
# Using seq which generates numbered lines — easy to verify completeness
LINES=$((SIZE_MB * 50000))  # ~20 bytes/line → ~1MB per 50k lines

echo "Generating ${LINES} lines (~${SIZE_MB}MB)..."
START=$(python3 -c "import time; print(time.time())")

# Send the command
curl -sf -X POST "$BASE/api/session/input" \
  -H "Content-Type: application/json" \
  -d "{\"session_id\":\"$SESSION_ID\",\"data\":\"seq 1 $LINES\\n\"}" >/dev/null 2>&1

# Poll output until we see the last line number or timeout
TIMEOUT=120
ELAPSED=0
FOUND=false
while [[ $ELAPSED -lt $TIMEOUT ]]; do
  OUTPUT=$(curl -sf "$BASE/api/session/output?session_id=$SESSION_ID&lines=5" 2>/dev/null || echo "")
  if echo "$OUTPUT" | grep -q "$LINES"; then
    FOUND=true
    break
  fi
  sleep 0.5
  ELAPSED=$((ELAPSED + 1))
done

END=$(python3 -c "import time; print(time.time())")
DURATION=$(python3 -c "print(f'{$END - $START:.2f}')")

# Cleanup session
curl -sf -X DELETE "$BASE/api/session/$SESSION_ID" >/dev/null 2>&1 || true

if $FOUND; then
  THROUGHPUT=$(python3 -c "print(f'{$SIZE_MB / ($END - $START):.1f}')")
  echo ""
  echo "Results:"
  echo "  Duration:   ${DURATION}s"
  echo "  Throughput: ${THROUGHPUT} MB/s"
  echo "  Lines:      $LINES"
  echo ""
  echo "$TIMESTAMP  ${SIZE_MB}MB  ${DURATION}s  ${THROUGHPUT}MB/s" >> "$OUTPUT_DIR/pty-throughput.log"
else
  echo ""
  echo "TIMEOUT: Output did not complete within ${TIMEOUT}s"
  echo "Duration so far: ${DURATION}s"
fi
