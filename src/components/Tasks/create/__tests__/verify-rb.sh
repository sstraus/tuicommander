#!/usr/bin/env bash
# Targeted verification inside an rb source replica, invoked through build-slot.
set -euo pipefail
pnpm install --offline --frozen-lockfile
pnpm exec tsc --noEmit
scripts/with-test-tmp.sh pnpm vitest run src/components/Tasks/create/__tests__ src/__tests__/work-item-provider.test.ts src/__tests__/work-item-critic.test.ts
: "${PLAYWRIGHT_DIR:?Set PLAYWRIGHT_DIR to a directory with node_modules/playwright-core installed; there is no default location}"
export PLAYWRIGHT_DIR
export PLAYWRIGHT_BROWSERS_PATH="${PLAYWRIGHT_BROWSERS_PATH:-${PLAYWRIGHT_DIR}/browsers}"
"${PLAYWRIGHT_DIR}/node_modules/.bin/playwright-core" install webkit
mkdir -p reports/visual
./node_modules/.bin/vite --host 127.0.0.1 --port 4180 --strictPort > reports/visual/tasks-create-vite.log 2>&1 &
vite_pid=$!
trap 'kill "$vite_pid" 2>/dev/null || true; wait "$vite_pid" 2>/dev/null || true' EXIT
for (( attempt = 0; attempt < 120; attempt++ )); do
	if curl --silent --fail http://127.0.0.1:4180/src/components/Tasks/create/__tests__/preview.html > /dev/null; then break; fi
	if ! kill -0 "$vite_pid" 2>/dev/null; then cat reports/visual/tasks-create-vite.log; exit 1; fi
	sleep 0.5
done
# The list stream's WebKit launcher is committed without the exec bit; set it in this disposable replica.
chmod +x src/components/Tasks/list/webkit-run.sh
LD_LIBRARY_PATH="${PLAYWRIGHT_DIR}/libs/usr/lib/aarch64-linux-gnu:${LD_LIBRARY_PATH:-}" node src/components/Tasks/create/__tests__/visual.mjs
scripts/with-test-tmp.sh bash -c 'cd src-tauri && cargo nextest run --lib -E "test(plugin_docs) | test(work_items::)"'
