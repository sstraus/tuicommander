#!/bin/bash
# Integration tests for the it2 shim script.
# Extracts the shim from the Rust source, mocks curl, and verifies behavior.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

PASS=0
FAIL=0

pass() { PASS=$((PASS + 1)); echo "  PASS: $1"; }
fail() { FAIL=$((FAIL + 1)); echo "  FAIL: $1 — $2"; }

# --- Extract the shim script from Rust source ---
SHIM="$TMPDIR/it2"
# The script is between 'const IT2_SHIM_SCRIPT: &str = r#"' and '"#;'
sed -n '/^const IT2_SHIM_SCRIPT/,/^"#;/p' "$REPO_ROOT/src-tauri/src/agent_mcp.rs" \
  | tail -n +1 \
  | sed '1s/.*r#"//' \
  | sed '$d' \
  > "$SHIM"
chmod +x "$SHIM"

# Verify extraction worked
if [ ! -s "$SHIM" ]; then
  echo "ERROR: Failed to extract shim script"
  exit 1
fi

# --- Create mock curl ---
MOCK_CURL="$TMPDIR/curl"
cat > "$MOCK_CURL" << 'MOCK'
#!/bin/bash
# Mock curl that records calls and returns canned responses
echo "$@" >> "$MOCK_CURL_LOG"
# Detect which endpoint was called
for arg in "$@"; do
  case "$arg" in
    *"/sessions"*)
      if echo "$@" | grep -q "POST"; then
        echo '{"session_id":"mock-uuid-1234","status":"ok"}'
      elif echo "$@" | grep -q "DELETE"; then
        echo '{"status":"ok"}'
      else
        echo '[{"session_id":"s1"},{"session_id":"s2"}]'
      fi
      exit 0
      ;;
  esac
done
echo '{}'
MOCK
chmod +x "$MOCK_CURL"

export MOCK_CURL_LOG="$TMPDIR/curl_calls.log"
export PATH="$TMPDIR:$PATH"
export TUIC_SOCKET_PATH="$TMPDIR/mock.sock"

echo "Running it2 shim integration tests..."

# --- Test 1: --version ---
echo ""
echo "Test: --version"
OUTPUT=$("$SHIM" --version 2>&1)
if echo "$OUTPUT" | grep -q "it2 (TUICommander shim)"; then
  pass "--version returns version string"
else
  fail "--version" "got: $OUTPUT"
fi

# --- Test 2: session split ---
echo ""
echo "Test: session split"
> "$MOCK_CURL_LOG"
OUTPUT=$("$SHIM" session split 2>&1)
if echo "$OUTPUT" | grep -q "Created new pane: mock-uuid-1234"; then
  pass "session split outputs Created new pane: <uuid>"
else
  fail "session split" "got: $OUTPUT"
fi
# Verify curl was called with POST /sessions
if grep -q "POST" "$MOCK_CURL_LOG" && grep -q "/sessions" "$MOCK_CURL_LOG"; then
  pass "session split calls POST /sessions"
else
  fail "session split curl call" "log: $(cat "$MOCK_CURL_LOG")"
fi

# --- Test 3: session split with flags ---
echo ""
echo "Test: session split -v -s parent-id"
> "$MOCK_CURL_LOG"
OUTPUT=$("$SHIM" session split -v -s parent-id 2>&1)
if echo "$OUTPUT" | grep -q "Created new pane:"; then
  pass "session split -v -s accepts flags"
else
  fail "session split with flags" "got: $OUTPUT"
fi

# --- Test 4: session run ---
echo ""
echo "Test: session run"
> "$MOCK_CURL_LOG"
"$SHIM" session run -s my-session echo hello 2>&1 || true
if grep -q "/sessions/my-session/write" "$MOCK_CURL_LOG"; then
  pass "session run calls /sessions/<id>/write"
else
  fail "session run" "log: $(cat "$MOCK_CURL_LOG")"
fi
if grep -q "echo hello" "$MOCK_CURL_LOG"; then
  pass "session run sends command in payload"
else
  fail "session run payload" "log: $(cat "$MOCK_CURL_LOG")"
fi

# --- Test 5: session close ---
echo ""
echo "Test: session close"
> "$MOCK_CURL_LOG"
"$SHIM" session close -s close-me 2>&1 || true
if grep -q "DELETE" "$MOCK_CURL_LOG" && grep -q "/sessions/close-me" "$MOCK_CURL_LOG"; then
  pass "session close calls DELETE /sessions/<id>"
else
  fail "session close" "log: $(cat "$MOCK_CURL_LOG")"
fi

# --- Test 6: session list ---
echo ""
echo "Test: session list"
> "$MOCK_CURL_LOG"
OUTPUT=$("$SHIM" session list 2>&1)
if echo "$OUTPUT" | grep -q "session_id"; then
  pass "session list returns session data"
else
  fail "session list" "got: $OUTPUT"
fi

# --- Test 7: missing TUIC_SOCKET_PATH ---
echo ""
echo "Test: missing TUIC_SOCKET_PATH"
OUTPUT=$(TUIC_SOCKET_PATH="" "$SHIM" --version 2>&1 || true)
if echo "$OUTPUT" | grep -q "TUIC_SOCKET_PATH not set"; then
  pass "errors when TUIC_SOCKET_PATH is empty"
else
  fail "missing socket path" "got: $OUTPUT"
fi

# --- Test 8: session run without -s flag ---
echo ""
echo "Test: session run missing -s"
OUTPUT=$("$SHIM" session run 2>&1 || true)
if echo "$OUTPUT" | grep -q "Usage:"; then
  pass "session run without -s shows usage"
else
  fail "session run missing -s" "got: $OUTPUT"
fi

# --- Summary ---
echo ""
echo "=========================="
echo "Results: $PASS passed, $FAIL failed"
echo "=========================="

if [ "$FAIL" -gt 0 ]; then
  exit 1
fi
