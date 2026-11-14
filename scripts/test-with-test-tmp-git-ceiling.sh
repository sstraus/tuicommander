#!/usr/bin/env bash
# Both test harnesses (scripts/with-test-tmp.sh and nextest's setup script) must
# fence git discovery at the test root: a repo-less scratch dir under the root
# must never resolve to an enclosing repository (a fixture whose `git init`
# failed once fell through to the real checkout and renamed its branches).
set -euo pipefail

root="$(git rev-parse --show-toplevel)"
# shellcheck source=test-tmp-lib.sh
. "$root/scripts/test-tmp-lib.sh"
test_tmp="$(tuic_test_tmp_root "$root")"
scratch="$(mktemp -d "$test_tmp/git-ceiling.XXXXXX")"
trap 'rm -rf "$scratch"' EXIT
# This test's own fixtures must not escape either.
GIT_CEILING_DIRECTORIES="$(tuic_git_ceiling_with "$test_tmp")"
export GIT_CEILING_DIRECTORIES

fail() { echo "$*" >&2; exit 1; }
has_entry() { case ":$1:" in *":$2:"*) return 0 ;; esac; return 1; }
no_dups() { [ "$(printf %s "$1" | tr ':' '\n' | sort | uniq -d)" = "" ]; }

# An outer repository standing in for the real checkout.
outer="$scratch/outer"
git init -q --template= "$outer"
[ "$(cd "$outer" && pwd -P)" = "$(git -C "$outer" rev-parse --show-toplevel)" ] || fail "fixture init failed: $outer"
outer="$(cd "$outer" && pwd)"

# A checkout copy inside the outer repo, so its default resolution happens there.
mkdir -p "$outer/scripts"
cp "$root/scripts/with-test-tmp.sh" "$root/scripts/test-tmp-lib.sh" "$outer/scripts/"
probe='mkdir -p "$TMPDIR/x"; git -C "$TMPDIR/x" rev-parse --show-toplevel'

# 1. with-test-tmp.sh with its per-run root below a base inside the outer repo
#    (the opt-in in-checkout layout): the run dir's parent is fenced.
base="$outer/.tmp/tuic-tests"
if found="$(env -u TUIC_TEST_TMP_ROOT -u GIT_CEILING_DIRECTORIES TUIC_TEST_TMP_BASE="$base" \
  "$outer/scripts/with-test-tmp.sh" sh -c "$probe" 2>&1)"; then
  fail "with-test-tmp.sh: git under the test root found $found"
fi
# A stricter value set by an outer wrapper survives, and nothing is listed twice.
value="$(env -u TUIC_TEST_TMP_ROOT GIT_CEILING_DIRECTORIES=/pre/set TUIC_TEST_TMP_BASE="$base" \
  "$outer/scripts/with-test-tmp.sh" sh -c 'printf %s "$GIT_CEILING_DIRECTORIES"')"
case "$value" in /pre/set:*) ;; *) fail "with-test-tmp.sh dropped the inherited ceiling: $value" ;; esac
has_entry "$value" "$base" || fail "with-test-tmp.sh: ceiling lacks the run root's parent: $value"
no_dups "$value" || fail "with-test-tmp.sh: duplicate ceiling entries: $value"

# 2. with-test-tmp.sh with an inherited root: fenced at THAT root's parent, not
#    at the base the wrapper would otherwise have used.
inherited="$outer/inherited/root"
if found="$(env -u GIT_CEILING_DIRECTORIES TUIC_TEST_TMP_ROOT="$inherited" \
  "$outer/scripts/with-test-tmp.sh" sh -c "$probe" 2>&1)"; then
  fail "with-test-tmp.sh (inherited root): git under the test root found $found"
fi
value="$(env -u GIT_CEILING_DIRECTORIES TUIC_TEST_TMP_ROOT="$inherited" \
  "$outer/scripts/with-test-tmp.sh" sh -c 'printf %s "$GIT_CEILING_DIRECTORIES"')"
has_entry "$value" "$outer/inherited" || fail "with-test-tmp.sh: ceiling lacks the inherited root's parent: $value"
no_dups "$value" || fail "with-test-tmp.sh: duplicate ceiling entries: $value"

# 3. nextest's setup script, with its root inside the outer repo.
ntroot="$outer/sub/root"
mkdir -p "$ntroot/x"
git -C "$ntroot/x" rev-parse --show-toplevel >/dev/null 2>&1 || fail "control: the outer repo is not discoverable"
(cd "$root/src-tauri" && GIT_CEILING_DIRECTORIES=/pre/set TUIC_TEST_TMP_ROOT="$ntroot" NEXTEST_ENV="$scratch/env" sh scripts/nextest-test-tmp.sh)
ceiling="$(sed -n 's/^GIT_CEILING_DIRECTORIES=//p' "$scratch/env")"
case "$ceiling" in /pre/set:*) ;; *) fail "nextest-test-tmp.sh dropped the inherited ceiling: $ceiling" ;; esac
has_entry "$ceiling" "$outer/sub" || fail "nextest-test-tmp.sh: ceiling lacks the root's parent: $ceiling"
no_dups "$ceiling" || fail "nextest-test-tmp.sh: duplicate ceiling entries: $ceiling"
if found="$(GIT_CEILING_DIRECTORIES="$ceiling" git -C "$ntroot/x" rev-parse --show-toplevel 2>&1)"; then
  fail "nextest-test-tmp.sh: git under the test root found $found"
fi

# 4. Regression for the combination of the TMPDIR-derived root and the fence:
#    nextest's setup script, with the default root (resolved from the host temp
#    dir) and with an inherited one, must fence at the parent of THAT root in
#    its lexical and physical spelling. An outer repo above the root stays
#    invisible; a fixture repo inside the root stays discoverable. The host is
#    reached through a symlink, so the two spellings really differ.
host_real="$outer/host"
mkdir -p "$host_real"
host_link="$scratch/host-link"
ln -s "$host_real" "$host_link"
host_phys="$(cd "$host_real" && pwd -P)"
fence_case() { # <label> <expected root> <expected lexical parent> <expected physical parent> [env...]
  label=$1 want_root=$2 want_lex=$3 want_phys=$4
  shift 4
  env_file="$scratch/env-$label"
  (cd "$root/src-tauri" && env -u TUIC_TEST_TMP_ROOT -u TUIC_TEST_TMP_BASE -u GIT_CEILING_DIRECTORIES \
    TUIC_TEST_HOST_TMPDIR="$host_link" NEXTEST_ENV="$env_file" "$@" sh scripts/nextest-test-tmp.sh)
  got_root="$(sed -n 's/^TUIC_TEST_TMP_ROOT=//p' "$env_file")"
  got_ceiling="$(sed -n 's/^GIT_CEILING_DIRECTORIES=//p' "$env_file")"
  [ "$got_root" = "$want_root" ] || fail "$label: TUIC_TEST_TMP_ROOT=$got_root, want $want_root"
  has_entry "$got_ceiling" "$want_lex" || fail "$label: ceiling lacks $want_lex: $got_ceiling"
  has_entry "$got_ceiling" "$want_phys" || fail "$label: ceiling lacks $want_phys: $got_ceiling"
  no_dups "$got_ceiling" || fail "$label: duplicate ceiling entries: $got_ceiling"
  probe_dir="$got_root/probe"
  mkdir -p "$probe_dir"
  git -C "$probe_dir" rev-parse --show-toplevel >/dev/null 2>&1 || fail "$label control: the outer repo is not discoverable"
  if found="$(GIT_CEILING_DIRECTORIES="$got_ceiling" git -C "$probe_dir" rev-parse --show-toplevel 2>&1)"; then
    fail "$label: git under the test root found the outer repo $found"
  fi
  git init -q --template= "$got_root/legit"
  mkdir -p "$got_root/legit/sub"
  legit="$(GIT_CEILING_DIRECTORIES="$got_ceiling" git -C "$got_root/legit/sub" rev-parse --show-toplevel)" ||
    fail "$label: the fence hid a fixture repo inside the root"
  [ "$legit" = "$(cd "$got_root/legit" && pwd -P)" ] || fail "$label: fixture repo resolved to $legit"
}
# Default: <host>/tuic-tests/tuic-co-<checkout hash>, fenced at <host>/tuic-tests.
co_hash="$(tuic_checkout_hash "$(cd "$root" && pwd -P)")"
fence_case default "$host_link/tuic-tests/tuic-co-$co_hash" \
  "$host_link/tuic-tests" "$host_phys/tuic-tests"
# Inherited: used exactly as given, fenced at its own parent, not at the base.
fence_case inherited "$host_link/inh/root" "$host_link/inh" "$host_phys/inh" \
  TUIC_TEST_TMP_ROOT="$host_link/inh/root"
