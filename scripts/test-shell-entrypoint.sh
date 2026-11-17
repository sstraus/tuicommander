#!/usr/bin/env bash
set -euo pipefail

project_root="$(git rev-parse --show-toplevel)"
test_tmp="$(. "$project_root/scripts/test-tmp-lib.sh" && tuic_test_tmp_root "$project_root")"
mkdir -p "$test_tmp"
# Fail closed: no git here may find a repository above the test root.
export GIT_CEILING_DIRECTORIES="$(. "$project_root/scripts/test-tmp-lib.sh" && tuic_git_ceiling_with "$test_tmp")"
# Explicit git config: never the developer's global/system files.
. "$project_root/scripts/test-tmp-lib.sh"
tuic_git_isolate_config "$test_tmp"
fixture_dir="$(mktemp -d "$test_tmp/shell-entrypoint.XXXXXX")"
trap 'rm -rf "$fixture_dir"' EXIT

# Catches: a repository-local .tmp fixture being skipped by the parent ignore rules.
# Exercise the unchanged runner in its own checkout, including its temp wrapper.
fixture_checkout="$fixture_dir/checkout"
git init --quiet "$fixture_checkout"
mkdir -p "$fixture_checkout/scripts"
cp "$project_root/Makefile" "$fixture_checkout/Makefile"
cp "$project_root/scripts/with-test-tmp.sh" "$project_root/scripts/test-tmp-lib.sh" "$fixture_checkout/scripts/"
fixture="$fixture_checkout/test-failing-fixture.sh"
printf '#!/usr/bin/env bash\nexit 1\n' > "$fixture"

if output="$(SHELL_TEST_DIR="$fixture_checkout" make -C "$fixture_checkout" test-shell 2>&1)"; then
  echo "expected the shell test runner to fail" >&2
  exit 1
fi

if ! printf '%s\n' "$output" | grep -Fq "$fixture"; then
  echo "shell test runner did not execute the failing fixture" >&2
  printf '%s\n' "$output" >&2
  exit 1
fi

printf '#!/usr/bin/env bash\nexit 0\n' > "$fixture"
SHELL_TEST_DIR="$fixture_checkout" make -C "$fixture_checkout" test-shell

# A local script that .gitignore hides (scripts/* is ignored unless
# allowlisted) is not a repository test and must not run (#883-5967).
ignored_dir="$(mktemp -d "$project_root/scripts/.shell-probe.XXXXXX")"
trap 'rm -rf "$fixture_dir" "$ignored_dir"' EXIT
printf '#!/usr/bin/env bash\necho IGNORED-PROBE-RAN\n' > "$ignored_dir/test-ignored.sh"
git -C "$project_root" check-ignore -q "$ignored_dir/test-ignored.sh"
if output="$(SHELL_TEST_DIR="$ignored_dir" make -C "$project_root" test-shell 2>&1)"; then
  echo "a directory with only ignored scripts must fail as having no tests" >&2
  exit 1
fi
if printf '%s\n' "$output" | grep -Fq IGNORED-PROBE-RAN; then
  echo "shell test runner executed a git-ignored script" >&2
  exit 1
fi
printf '%s\n' "$output" | grep -Fq "no shell tests found"

# The shipped tests must stay tracked, or the skip above would hide them.
for shipped in scripts/test-shell-entrypoint.sh scripts/test-with-test-tmp-prune.sh \
  scripts/test-with-test-tmp-readonly-home.sh scripts/test-run-remote-fixture.sh \
  scripts/hooks/test-install-hooks.sh; do
  if git -C "$project_root" check-ignore -q "$shipped"; then
    echo "$shipped is git-ignored and would never run" >&2
    exit 1
  fi
done
