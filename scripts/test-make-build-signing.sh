#!/usr/bin/env bash
# Catches `make build` failing locally because updater artifact signing is
# requested without TAURI_SIGNING_PRIVATE_KEY. Dry-run only: no build runs.
set -euo pipefail
project_root="$(cd "$(dirname "$0")/.." && pwd)"
override='"createUpdaterArtifacts":false'

dry_run() {
  # $1 = target; remaining args are extra env assignments for `env`.
  local target=$1
  shift
  env -u TAURI_SIGNING_PRIVATE_KEY "$@" make -n -C "${project_root}" "${target}"
}

for target in build build-dmg; do
  # Unset key: the override must be passed to tauri.
  out="$(dry_run "${target}")"
  grep -qF "${override}" <<<"${out}" || { echo "make ${target}: no updater override without a key" >&2; exit 1; }
  # Empty key counts as unset (a blank CI secret must not enable signing).
  out="$(dry_run "${target}" TAURI_SIGNING_PRIVATE_KEY=)"
  grep -qF "${override}" <<<"${out}" || { echo "make ${target}: no updater override with an empty key" >&2; exit 1; }
  # Key set: release builds stay unchanged.
  out="$(dry_run "${target}" TAURI_SIGNING_PRIVATE_KEY=dummy)"
  if grep -qF "${override}" <<<"${out}"; then
    echo "make ${target}: override leaked into a signed build" >&2
    exit 1
  fi
  grep -qF "pnpm tauri build" <<<"${out}" || { echo "make ${target}: tauri build missing" >&2; exit 1; }
done
echo "make-build-signing ok"
