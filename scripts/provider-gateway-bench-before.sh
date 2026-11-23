#!/usr/bin/env bash
# Measure the provider gateway benchmark workloads on a tree WITHOUT the
# gateway (story 3151, R6 "before" numbers).
#
#   scripts/provider-gateway-bench-before.sh [REF]      default REF=master
#
# Clones REF beside the reports (a clone, not a worktree: see scripts/mutants.sh),
# adds the driver of scripts/provider-gateway-bench/ and the receiver of the
# gateway benchmark, and runs the one ignored test. No live GitHub.
# Output: .tmp/provider-gateway-bench/before.json. Copy it to
# src-tauri/src/fixtures/github_volume/before_master.json to refresh the
# comparison of `measure_provider_gateway`.
set -euo pipefail

REF="${1:-master}"
ROOT="$(git rev-parse --show-toplevel)"
SHA="$(git -C "$ROOT" rev-parse --verify "$REF^{commit}")"
OUT_DIR="$ROOT/.tmp/provider-gateway-bench"
SRC="$OUT_DIR/before-src"
DRIVER="$ROOT/scripts/provider-gateway-bench"

MBX_BIN="$HOME/Library/Application Support/mbx/bin"
[ -d "$MBX_BIN" ] && export PATH="$MBX_BIN:$PATH"
[ -d "$ROOT/dist" ] || { echo "$ROOT/dist missing — run 'pnpm exec vite build' first"; exit 1; }
[ -f "$ROOT/plugins/plan/manifest.json" ] || { echo "plugins submodule not checked out"; exit 1; }

mkdir -p "$OUT_DIR"
rm -rf "$SRC"
git clone --local --quiet "$ROOT" "$SRC"
git -C "$SRC" checkout --quiet --detach "$SHA"
# generate_context! embeds ../dist; tauri-build checks the gitignored sidecar;
# plugins is a submodule the clone does not carry.
mkdir -p "$SRC/dist" "$SRC/src-tauri/binaries" "$SRC/plugins"
cp -R "$ROOT/dist/." "$SRC/dist/"
cp -R "$ROOT/plugins/." "$SRC/plugins/"
for source in "$ROOT"/src-tauri/binaries/*; do
  [[ -f "$source" ]] && cp -p "$source" "$SRC/src-tauri/binaries/"
done

SRC_RS="$SRC/src-tauri/src"
mkdir -p "$SRC_RS/github_volume_bench"
cp "$ROOT/src-tauri/src/github_volume_bench/receiver.rs" "$SRC_RS/github_volume_bench/receiver.rs"
cp "$DRIVER/before_driver.rs" "$SRC_RS/provider_gateway_before_3151.rs"
cp "$DRIVER/before_trigger_door.rs" "$SRC_RS/workflows/github_poll_bench.rs"
printf '\n#[cfg(test)]\n#[path = "provider_gateway_before_3151.rs"]\nmod provider_gateway_before_3151;\n' \
  >> "$SRC_RS/github_poller.rs"
printf '\n#[cfg(test)]\npub(crate) mod github_poll_bench;\n' >> "$SRC_RS/workflows/mod.rs"

cd "$SRC/src-tauri"
TUIC_BENCH_OUT="$OUT_DIR/before.json" TUIC_BENCH_TREE="$REF@$SHA" \
  ../scripts/with-test-tmp.sh cargo nextest run \
  -E 'test(measure_before_gateway_3151)' --run-ignored all --success-output immediate
echo "before numbers: $OUT_DIR/before.json"
