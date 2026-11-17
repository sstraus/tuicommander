#!/usr/bin/env bash
# Git config isolation in scripts/with-test-tmp.sh is opt-in
# (TUIC_TEST_ISOLATE_GIT_CONFIG=1, which `make test-shell` sets): without it a
# wrapped step keeps the caller's git config (credential helper, insteadOf,
# proxy: pnpm/cargo audit and the submodule checks talk to real remotes); with
# it git reads <root>/gitconfig only. That file must say exactly what
# tuic-test-support's TEST_GITCONFIG says, so the shell tests and the Rust
# suite run git under one configuration.
set -euo pipefail

root="$(git rev-parse --show-toplevel)"
base="$(. "$root/scripts/test-tmp-lib.sh" && tuic_test_tmp_root "$root")"
fixture="$(mktemp -d "${base%/}/tuic-git-config.XXXXXX")"
trap 'rm -rf "$fixture"' EXIT

# A stand-in for the developer's global config, with a setting tests must not see.
user_config="$fixture/user-gitconfig"
printf '[url "https://example.invalid/"]\n\tinsteadOf = git@example.invalid:\n' >"$user_config"
probe='printf "%s|%s|" "${GIT_CONFIG_GLOBAL-<unset>}" "${GIT_CONFIG_NOSYSTEM-<unset>}"; git config --global --get url.https://example.invalid/.insteadof || echo "<none>"'
run_wrapper() {
  env -u TUIC_TEST_TMP_ROOT -u TUIC_TEST_ISOLATE_GIT_CONFIG -u GIT_CONFIG_NOSYSTEM \
    GIT_CONFIG_GLOBAL="$user_config" "$@" "$root/scripts/with-test-tmp.sh" sh -c "$probe"
}

# Catches: isolation applied to every wrapped step again (audits and
# submodule checks then lose the user's credential helper and URL rewrites).
IFS='|' read -r seen_global seen_nosystem seen_value <<<"$(run_wrapper)"
test "$seen_global" = "$user_config" || { echo "without the opt-in GIT_CONFIG_GLOBAL became $seen_global" >&2; exit 1; }
test "$seen_nosystem" = "<unset>" || { echo "without the opt-in GIT_CONFIG_NOSYSTEM became $seen_nosystem" >&2; exit 1; }
test "$seen_value" = "git@example.invalid:" || { echo "without the opt-in the user's insteadOf was lost ($seen_value)" >&2; exit 1; }

# Catches: the opt-in not isolating (a shell test reading the user's config).
IFS='|' read -r seen_global seen_nosystem seen_value <<<"$(run_wrapper TUIC_TEST_ISOLATE_GIT_CONFIG=1)"
case "$seen_global" in
  */tuic-run.*/gitconfig) ;;
  *) echo "with the opt-in GIT_CONFIG_GLOBAL is $seen_global, not <root>/gitconfig" >&2; exit 1 ;;
esac
test "$seen_nosystem" = 1 || { echo "with the opt-in GIT_CONFIG_NOSYSTEM is $seen_nosystem" >&2; exit 1; }
test "$seen_value" = "<none>" || { echo "with the opt-in the user's insteadOf still applies ($seen_value)" >&2; exit 1; }

# Parity: the shell file and the Rust constant, minus each one's own
# "Written by ..." header comment.
(. "$root/scripts/test-tmp-lib.sh" && tuic_git_isolate_config "$fixture/shell")
grep -vE '^#' "$fixture/shell/gitconfig" >"$fixture/shell.body"
rust_src="$root/src-tauri/crates/tuic-test-support/src/lib.rs"
# The constant is a `"\` continued string literal ending at `";`, with \t escapes.
sed -n '/^const TEST_GITCONFIG: &str = "\\$/,/^";$/p' "$rust_src" \
  | sed -e '1d' -e '$d' -e 's/\\t/\t/g' | grep -vE '^#' >"$fixture/rust.body"
test -s "$fixture/rust.body" || { echo "could not read TEST_GITCONFIG out of $rust_src" >&2; exit 1; }
if ! cmp -s "$fixture/shell.body" "$fixture/rust.body"; then
  echo "scripts/test-tmp-lib.sh's gitconfig and tuic-test-support's TEST_GITCONFIG differ:" >&2
  diff "$fixture/rust.body" "$fixture/shell.body" >&2 || true
  exit 1
fi
