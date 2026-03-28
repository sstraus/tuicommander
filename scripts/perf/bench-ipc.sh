#!/usr/bin/env bash
# Benchmark IPC latency for key git commands via the HTTP API (Unix socket).
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

# Resolve the Unix socket path (same logic as Rust config_dir + mcp.sock)
resolve_socket() {
  local candidates=(
    "$HOME/Library/Application Support/com.tuic.commander/mcp.sock"
    "${XDG_CONFIG_HOME:-$HOME/.config}/com.tuic.commander/mcp.sock"
  )
  for sock in "${candidates[@]}"; do
    if [[ -S "$sock" ]]; then
      echo "$sock"
      return 0
    fi
  done
  return 1
}

SOCK=$(resolve_socket) || {
  echo "ERROR: TUICommander Unix socket not found."
  echo "Start the app first, then re-run this script."
  exit 1
}

ITERATIONS="${2:-20}"

# Verify health
if ! curl -sf --unix-socket "$SOCK" "http://localhost/health" >/dev/null 2>&1; then
  echo "ERROR: TUICommander not responding on socket $SOCK"
  exit 1
fi

# curl wrapper for Unix socket
scurl() {
  curl --unix-socket "$SOCK" "$@"
}

# Auto-detect repo path
if [[ -n "${1:-}" ]]; then
  REPO="$1"
else
  REPO=$(scurl -sf "http://localhost/health" 2>/dev/null | python3 -c "
import sys, json
# health doesn't return repos — fall back to current directory if it's a git repo
import subprocess
try:
    result = subprocess.run(['git', 'rev-parse', '--show-toplevel'], capture_output=True, text=True)
    if result.returncode == 0:
        print(result.stdout.strip())
except:
    pass
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
echo "Socket: $SOCK"
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
  scurl -sf "$url" >/dev/null 2>&1 || true

  for ((i=0; i<ITERATIONS; i++)); do
    local t
    t=$(scurl -sf -o /dev/null -w '%{time_total}' "$url" 2>/dev/null || echo "0")
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

bench "repo_info"         "http://localhost/repo/info?path=$ENCODED_REPO"
bench "panel_context"     "http://localhost/repo/panel-context?path=$ENCODED_REPO"
bench "diff_stats"        "http://localhost/repo/diff-stats?path=$ENCODED_REPO"
bench "changed_files"     "http://localhost/repo/files?path=$ENCODED_REPO"
bench "branches"          "http://localhost/repo/branches?path=$ENCODED_REPO"
bench "recent_commits"    "http://localhost/repo/recent-commits?path=$ENCODED_REPO"
bench "stash_list"        "http://localhost/repo/stash?path=$ENCODED_REPO"
bench "remote_url"        "http://localhost/repo/remote-url?path=$ENCODED_REPO"

echo ""
echo "Results saved: $RESULTS"
