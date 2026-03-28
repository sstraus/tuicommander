#!/usr/bin/env bash
# Benchmark IPC latency for key git commands via the HTTP API.
# Requires TUICommander to be running with at least one repo configured.
#
# Measures:
#   - repo_info (cached vs cold)
#   - git_panel_context (cached vs cold)
#   - diff_stats
#   - changed_files
#   - branches
#   - recent_commits
#   - stash_list
#
# Usage:
#   scripts/perf/bench-ipc.sh                    # auto-detect repo
#   scripts/perf/bench-ipc.sh /path/to/repo      # specific repo
#   scripts/perf/bench-ipc.sh /path/to/repo 50   # custom iterations
set -euo pipefail

PORT=9877
BASE="http://localhost:$PORT"
ITERATIONS="${2:-20}"

# Check if app is running
if ! curl -sf "$BASE/health" >/dev/null 2>&1; then
  echo "ERROR: TUICommander not running on port $PORT"
  echo "Start the app first, then re-run this script."
  exit 1
fi

# Auto-detect repo path from workspace
if [[ -n "${1:-}" ]]; then
  REPO="$1"
else
  REPO=$(curl -sf "$BASE/api/workspace" 2>/dev/null | python3 -c "
import sys, json
data = json.load(sys.stdin)
repos = data if isinstance(data, list) else data.get('repos', [])
if repos:
    r = repos[0]
    print(r.get('path', r) if isinstance(r, dict) else r)
" 2>/dev/null || echo "")
  if [[ -z "$REPO" ]]; then
    echo "ERROR: Could not auto-detect repo. Pass path as argument."
    echo "Usage: $0 /path/to/repo [iterations]"
    exit 1
  fi
fi

ENCODED_REPO=$(python3 -c "import urllib.parse; print(urllib.parse.quote('$REPO'))")

echo "=== IPC Latency Benchmark ==="
echo "Repo: $REPO"
echo "Iterations: $ITERATIONS"
echo "Port: $PORT"
echo ""

OUTPUT_DIR="$(cd "$(dirname "$0")" && pwd)/results"
mkdir -p "$OUTPUT_DIR"
TIMESTAMP=$(date +%Y%m%d-%H%M%S)
RESULTS="$OUTPUT_DIR/ipc-$TIMESTAMP.txt"

# Benchmark function: runs curl N times and reports stats
bench() {
  local name="$1"
  local url="$2"
  local times=()

  # Warm up (1 call)
  curl -sf "$url" >/dev/null 2>&1 || true

  for ((i=0; i<ITERATIONS; i++)); do
    local t
    t=$(curl -sf -o /dev/null -w '%{time_total}' "$url" 2>/dev/null || echo "0")
    times+=("$t")
  done

  # Calculate stats with python
  local stats
  stats=$(python3 -c "
import statistics
times = [float(t)*1000 for t in '${times[*]}'.split()]
times = [t for t in times if t > 0]
if not times:
    print('FAILED')
else:
    print(f'p50={statistics.median(times):.1f}ms  p95={sorted(times)[int(len(times)*0.95)]:.1f}ms  mean={statistics.mean(times):.1f}ms  n={len(times)}')
")
  printf "  %-25s %s\n" "$name" "$stats"
  echo "$name: $stats" >> "$RESULTS"
}

echo "Endpoint latencies (${ITERATIONS} iterations each):"
echo "" > "$RESULTS"
echo "# IPC Benchmark — $(date) — repo: $REPO — iterations: $ITERATIONS" >> "$RESULTS"
echo "" >> "$RESULTS"

bench "repo_info"         "$BASE/api/git/repo-info?path=$ENCODED_REPO"
bench "git_panel_context" "$BASE/api/git/panel-context?path=$ENCODED_REPO"
bench "diff_stats"        "$BASE/api/git/diff-stats?path=$ENCODED_REPO"
bench "changed_files"     "$BASE/api/git/changed-files?path=$ENCODED_REPO"
bench "branches"          "$BASE/api/git/branches?path=$ENCODED_REPO"
bench "recent_commits"    "$BASE/api/git/recent-commits?path=$ENCODED_REPO"
bench "stash_list"        "$BASE/api/git/stash-list?path=$ENCODED_REPO"
bench "remote_url"        "$BASE/api/git/remote-url?path=$ENCODED_REPO"

echo ""
echo "Results saved: $RESULTS"
