#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
test_tmp_base="$(. "$root/scripts/test-tmp-lib.sh" && tuic_test_tmp_root "$root")"
mkdir -p "$test_tmp_base"
# Fail closed: no git here may find a repository above the test root.
export GIT_CEILING_DIRECTORIES="$(. "$root/scripts/test-tmp-lib.sh" && tuic_git_ceiling_with "$test_tmp_base")"
fixture="$(mktemp -d "${test_tmp_base%/}/tuic-target-test.XXXXXX")"
trap 'rm -rf "$fixture"' EXIT
git init --quiet --separate-git-dir "$fixture/git-dir" "$fixture/checkout"
mkdir -p "$fixture/checkout/scripts"
cp "$root/scripts/with-test-tmp.sh" "$root/scripts/test-tmp-lib.sh" "$fixture/checkout/scripts/"
test -f "$fixture/checkout/.git"

assert_target_not_inherited() {
  local label=$1 inherited=$2
  if ! CARGO_TARGET_DIR="$inherited" "$fixture/checkout/scripts/with-test-tmp.sh" sh -c 'test -z "${CARGO_TARGET_DIR+x}"'; then
    echo "$label: inherited Cargo target reached the test process" >&2
    exit 1
  fi
}

# Catches: a parent Cargo target silently makes worktree tests use another checkout's artifacts.
assert_target_not_inherited "foreign absolute target" "/nonexistent/other-checkout/src-tauri/target"
# Catches: clearing only absolute paths leaves relative target overrides active.
assert_target_not_inherited "relative target" "shared-artifact-target"

# Catches: the wrapper accidentally adds a target override when none was inherited.
env -u CARGO_TARGET_DIR "$fixture/checkout/scripts/with-test-tmp.sh" sh -c 'test -z "${CARGO_TARGET_DIR+x}"'

git init --quiet "$fixture/primary"
mkdir -p "$fixture/primary/scripts"
cp "$root/scripts/with-test-tmp.sh" "$root/scripts/test-tmp-lib.sh" "$fixture/primary/scripts/"
# Catches: the worktree guard also erases an intentional primary-checkout target.
CARGO_TARGET_DIR="$fixture/primary-target" "$fixture/primary/scripts/with-test-tmp.sh" sh -c 'test "$CARGO_TARGET_DIR" = "$1"' sh "$fixture/primary-target"
