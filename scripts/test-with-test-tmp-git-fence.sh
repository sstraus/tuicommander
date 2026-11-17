#!/usr/bin/env bash
# The wrapper and the nextest setup script keep git inside the test root: an empty
# template dir (no hooks/*.sample copied into fixture repos) and a
# GIT_CEILING_DIRECTORIES that stops repository discovery at the root's parent,
# so a fixture whose `git init` failed can never fall through to this checkout.
set -euo pipefail

project_root="$(git rev-parse --show-toplevel)"
scratch="$(mktemp -d "${TMPDIR:?}/git-fence.XXXXXX")"
trap 'rm -rf "$scratch"' EXIT
fail() { echo "git-fence: $*" >&2; exit 1; }

# 1. The wrapper's command sees both variables, and they work.
out="$scratch/wrapper-env"
(cd "$project_root" && scripts/with-test-tmp.sh bash -c '
  set -euo pipefail
  root="$TUIC_TEST_TMP_ROOT"
  printf "%s\n%s\n%s\n" "$GIT_TEMPLATE_DIR" "$GIT_CEILING_DIRECTORIES" "$root"
  test -d "$GIT_TEMPLATE_DIR/hooks" && test -d "$GIT_TEMPLATE_DIR/info" && test -z "$(find "$GIT_TEMPLATE_DIR" -type f ! -path "*/info/exclude")"
  case ":$GIT_CEILING_DIRECTORIES:" in *":$(dirname "$root"):"*) ;; *) echo "ceiling lacks the root parent" >&2; exit 1 ;; esac
  # An outer value survives: the wrapper only appends.
  mkdir -p "$root/probe"
  if found="$(git -C "$root/probe" rev-parse --show-toplevel 2>&1)"; then echo "git found $found above the root" >&2; exit 1; fi
  git init -q "$root/fixture"
  test -d "$root/fixture/.git/hooks" && test -f "$root/fixture/.git/info/exclude"
  test ! -e "$root/fixture/.git/hooks/pre-commit.sample" || { echo "template copied hooks into a fixture" >&2; exit 1; }
' ) >"$out" 2>"$scratch/wrapper-err" || { cat "$scratch/wrapper-err" >&2; fail "wrapper command failed"; }

# 2. An outer wrapper's stricter ceiling is kept verbatim.
kept="$(cd "$project_root" && GIT_CEILING_DIRECTORIES=/pre/set scripts/with-test-tmp.sh bash -c 'printf %s "$GIT_CEILING_DIRECTORIES"')"
case "$kept" in /pre/set:*) ;; *) fail "the wrapper dropped an inherited ceiling: $kept" ;; esac

# 2b. Inherited repository-location variables never reach the wrapper's command.
leaked="$(cd "$project_root" && GIT_DIR=/nonexistent GIT_WORK_TREE=/nonexistent GIT_COMMON_DIR=/nonexistent \
  GIT_INDEX_FILE=/nonexistent GIT_OBJECT_DIRECTORY=/nonexistent scripts/with-test-tmp.sh \
  bash -c 'env | grep -E "^GIT_(DIR|WORK_TREE|COMMON_DIR|INDEX_FILE|OBJECT_DIRECTORY)=" || true')"
[ -z "$leaked" ] || fail "the wrapper passed on repository-location variables: $leaked"

# 3. The nextest setup script exports the same two variables for test processes.
# The root sits inside an enclosing repository, so the probe below discriminates: without
# the ceiling git finds it, with the emitted ceiling it must not.
enclosing="$scratch/enclosing"
git init -q "$enclosing"
root="$enclosing/nextest-root"
env_file="$scratch/nextest-env"
: >"$env_file"
(cd "$project_root/src-tauri" && TUIC_TEST_TMP_ROOT="$root" NEXTEST_ENV="$env_file" sh scripts/nextest-test-tmp.sh)
tmpl="$(sed -n 's/^GIT_TEMPLATE_DIR=//p' "$env_file")"
ceiling="$(sed -n 's/^GIT_CEILING_DIRECTORIES=//p' "$env_file")"
[ -d "$tmpl/hooks" ] && [ -d "$tmpl/info" ] && [ -z "$(find "$tmpl" -type f ! -path "*/info/exclude")" ] || fail "nextest script: template dir missing or not empty of files ($tmpl)"
case ":$ceiling:" in *":$(cd "$enclosing" && pwd -P):"*) ;; *) fail "nextest script: ceiling lacks the root's parent: $ceiling" ;; esac
mkdir -p "$root/probe"
env -u GIT_CEILING_DIRECTORIES git -C "$root/probe" rev-parse --show-toplevel >/dev/null 2>&1 \
  || fail "nextest script: the probe is not inside an enclosing repository, so it proves nothing"
if found="$(GIT_CEILING_DIRECTORIES="$ceiling" git -C "$root/probe" rev-parse --show-toplevel 2>&1)"; then
  fail "nextest script: git under the root found $found"
fi
# 4. A scratch root whose parent contains ':' cannot be fenced: both scripts refuse.
colon="$scratch/a:b"
mkdir -p "$colon"
if (cd "$project_root/src-tauri" && TUIC_TEST_TMP_ROOT="$colon/root" NEXTEST_ENV="$scratch/colon-env" sh scripts/nextest-test-tmp.sh) 2>"$scratch/colon-err"; then
  fail "nextest script: accepted a root whose parent contains ':'"
fi
grep -q "cannot express" "$scratch/colon-err" || fail "nextest script: no message for a ':' root: $(cat "$scratch/colon-err")"
echo "git fence: wrapper and nextest setup export a template and a ceiling"
