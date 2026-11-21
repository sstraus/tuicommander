#!/usr/bin/env bash
set -euo pipefail
# Same scratch root capture.mjs uses (TUIC_TEST_TMP_ROOT, else <temp dir>/tuic-tests).
scratch="$(node scripts/test-tmp-root.mjs)/settings-visual"
mkdir -p "${scratch}"
node node_modules/vite/bin/vite.js --config scripts/settings-visual/vite.config.ts --strictPort > "${scratch}/vite.log" 2>&1 &
vite_pid=$!
trap 'kill "$vite_pid" 2>/dev/null || true' EXIT
for attempt in {1..50}; do
  if curl --silent --fail http://127.0.0.1:15391/scripts/settings-visual/index.html > /dev/null; then break; fi
  sleep 0.1
done
node scripts/settings-visual/capture.mjs
