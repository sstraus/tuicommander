#!/usr/bin/env bash
# Explicitly regenerate or verify the recorded production replay oracles.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
case "${1:-verify}" in
  regenerate) export TUIC_REGENERATE_ORACLES="${2:-1}" ;;
  verify) unset TUIC_REGENERATE_ORACLES ;;
  *) echo "usage: scripts/replay-oracle.sh [verify|regenerate [relative-path,...]]" >&2; exit 2 ;;
esac
runner=()
if [[ $(uname -s) == Darwin ]]; then
  export PATH="$HOME/Library/Application Support/mbx/bin:$PATH"
  which cargo
  mbx doctor
fi
# Optional build admission wrapper (e.g. a build-slot scheduler).
if [[ -n "${TUIC_BUILD_SLOT:-}" && -x "$TUIC_BUILD_SLOT" ]]; then
  runner=("$TUIC_BUILD_SLOT")
fi
cd "$root/src-tauri"
exec "$root/scripts/with-test-tmp.sh" ${runner[@]+"${runner[@]}"} cargo nextest run --lib -E 'test(replay_oracle)'
