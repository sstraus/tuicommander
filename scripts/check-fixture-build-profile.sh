#!/usr/bin/env bash
# Catches: nextest setup rebuilding fixture dependencies with the dev profile
# or a headless feature graph after CI has already built dictation tests.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
work="$(mktemp -d "${TMPDIR:-$root/.tmp}/fixture-build-profile.XXXXXX")"
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/bin"
cat > "$work/bin/cargo" <<'EOF'
#!/bin/sh
printf '%s\n' "$@" > "$FIXTURE_BUILD_ARGV"
exit "${FIXTURE_BUILD_EXIT:-0}"
EOF
chmod +x "$work/bin/cargo"
export PATH="$work/bin:$PATH"
export FIXTURE_BUILD_ARGV="$work/actual"

check() {
  printf '%s\n' "$@" > "$work/expected"
  (cd "$root/src-tauri" && "${builder[@]}")
  diff -u "$work/expected" "$work/actual"
}
# PowerShell's cache must also stay inside this disposable test root.
export XDG_CACHE_HOME="$work/cache"
export PSModuleAnalysisCachePath="$work/ps-analysis"
for shell in sh pwsh; do
  if [ "$shell" = pwsh ]; then
    command -v pwsh >/dev/null || { echo 'PowerShell unavailable; its builder was not executed'; continue; }
    builder=(pwsh -NoProfile -NonInteractive -File scripts/build-fixture-bins.ps1)
  else
    builder=(sh scripts/build-fixture-bins.sh)
  fi
  unset TUIC_FIXTURE_FEATURES FIXTURE_BUILD_EXIT
  check build --profile test --no-default-features \
    --bin tuic-acp-fixture-agent --bin tuic-mcp-fixture-server
  export TUIC_FIXTURE_FEATURES=dictation
  # Catches: CI selecting a binary-only graph without test dev-dependencies.
  check test --workspace --no-run --features dictation
  # Catches: a failed fixture build being reported as successful setup.
  export FIXTURE_BUILD_EXIT=7
  set +e
  (cd "$root/src-tauri" && "${builder[@]}")
  status=$?
  set -e
  [ "$status" -eq 7 ] || { echo "fixture build returned $status instead of 7" >&2; exit 1; }
  echo "$shell fixture build profile, features and failure propagation passed"
done
