#!/usr/bin/env bash
set -euo pipefail
# Same scratch root capture.mjs uses (TUIC_TEST_TMP_ROOT, else <temp dir>/tuic-tests).
scratch="$(node scripts/test-tmp-root.mjs)/mobile-question-visual"
mkdir -p "${scratch}"
node node_modules/vite/bin/vite.js --config scripts/mobile-question-visual/vite.config.ts > "${scratch}/vite.log" 2>&1 &
vite_pid=$!
trap 'kill "$vite_pid" 2>/dev/null || true; wait "$vite_pid" 2>/dev/null || true' EXIT
for attempt in {1..50}; do
  if curl --silent --fail http://127.0.0.1:15473/scripts/mobile-question-visual/index.html > /dev/null; then break; fi
  sleep 0.1
done
node scripts/mobile-question-visual/capture.mjs
