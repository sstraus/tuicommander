#!/usr/bin/env bash
# Boundary test: fake only gh's network boundary, run the production publisher.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
# Scratch under the caller's temp dir (mktemp honours TMPDIR), never inside the checkout.
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
export BETA_TEST_WORK="$work"
export BETA_TEST_ROOT="$root"
export GITHUB_REPOSITORY=sstraus/tuicommander GITHUB_SHA=fixture-commit
export BETA_EXISTS=0 BETA_DOWNLOAD_FAIL=0 BETA_CURRENT_VERSION=2.0.0-beta.1
jq '.version = "2.0.0-beta.2"' "$root/src-tauri/test-fixtures/updater/stable-manifest.json" > "$work/source.json"
gh() {
  printf '%s\n' "$*" >> "$BETA_TEST_WORK/calls"
  case "$1 $2" in
    'release download')
      [[ "$BETA_DOWNLOAD_FAIL" == 0 ]] || return 1
      local tag=$3 dest=""
      while (( $# )); do
        if [[ $1 == --dir ]]; then dest=$2; break; fi
        shift
      done
      if [[ $tag == beta ]]; then
        jq --arg v "$BETA_CURRENT_VERSION" '.version = $v' "$BETA_TEST_WORK/source.json" > "$dest/latest.json"
      else
        cp "$BETA_TEST_WORK/source.json" "$dest/latest.json"
      fi ;;
    'release view')
      [[ "$BETA_EXISTS" == 1 ]] || return 1
      # The real GitHub release JSON fixture records assets[].name.
      jq -r '.assets[].name' "$BETA_TEST_ROOT/src-tauri/test-fixtures/updater/github-stable-release.json" ;;
    'release upload') cp "$4" "$BETA_TEST_WORK/uploaded.json" ;;
    'release create'|'release edit') ;;
    *) echo "Unexpected gh command: $*" >&2; return 2 ;;
  esac
}
export -f gh
run() { : > "$work/calls"; rm -f "$work/uploaded.json"; bash "$root/scripts/publish-beta-feed.sh" "$1"; }
# Catches: stable or nightly publication overwrites the beta feed.
for version in 2.0.0 2.0.0-nightly.1 2.0.0-beta.1-nightly.1; do
  run "$version"
  [[ ! -s "$work/calls" ]]
done
# Catches: first beta has no usable feed, or publishing drops signed platform data.
run 2.0.0-beta.2
grep -q 'release create beta .*--prerelease --latest=false' "$work/calls"
cmp "$work/source.json" "$work/uploaded.json"
# Catches: next beta recreates the release or becomes stable's latest.
export BETA_EXISTS=1
run 2.0.0-beta.2
grep -q 'release edit beta .*--prerelease=true --latest=false' "$work/calls"
cmp "$work/source.json" "$work/uploaded.json"
# Catches: a failed download damages the existing feed.
export BETA_DOWNLOAD_FAIL=1
if run 2.0.0-beta.2; then echo 'Failed download accepted' >&2; exit 1; fi
! grep -Eq 'release (upload|edit|create)' "$work/calls"
export BETA_DOWNLOAD_FAIL=0
# Catches: a mismatched manifest is published under the requested beta.
if run 2.0.0-beta.3; then echo 'Mismatched version accepted' >&2; exit 1; fi
! grep -Eq 'release (upload|edit|create)' "$work/calls"
echo 'publish-beta-feed ok'
