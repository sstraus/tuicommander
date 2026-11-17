#!/usr/bin/env bash
# Run a test command with its scratch files in a per-run directory under the
# caller's temp dir (roots: scripts/test-tmp-lib.sh). Never reads or writes $HOME.
#
#   TUIC_TEST_HOST_TMPDIR  the caller's temp dir. Exported, so nested wrappers
#                          and test processes still see it after TMPDIR moves.
#   TUIC_TEST_TMP_BASE     opt-in parent of the per-run dirs (default
#                          <host>/tuic-tests). Pointing it into the checkout
#                          is unsafe where writes to the checkout's .git are
#                          denied (sandboxes) and is opt-in only.
#   TUIC_TEST_TMP_ROOT     inherited: the caller's root, used as is and never
#                          deleted (only its stale tuic-run.* children are
#                          pruned); else a fresh <base>/tuic-run.XXXXXX,
#                          removed on exit. TMPDIR/TMP/TEMP point at it too.
#   TUIC_TEST_ISOLATE_GIT_CONFIG=1  git reads <root>/gitconfig, never the
#                          developer's global/system config (`make test-shell`).
#   TUIC_TEST_FAKE_HOME=1  HOME=<root>/home for the command; CARGO_HOME and
#                          RUSTUP_HOME stay pinned to the real home (the Rust
#                          test steps of `make check`/`make cov` and CI).
set -euo pipefail

# An exported GIT_DIR (or friends) selects a repository outright and the ceiling never
# restricts it, so neither this script's own git calls nor the command may inherit one.
# Same list as tuic-test-support's GIT_LOCATION_OVERRIDES.
unset GIT_DIR GIT_WORK_TREE GIT_COMMON_DIR GIT_INDEX_FILE GIT_OBJECT_DIRECTORY \
  GIT_ALTERNATE_OBJECT_DIRECTORIES GIT_NAMESPACE GIT_PREFIX
root="$(git -C "$(dirname "$0")/.." rev-parse --show-toplevel)"
# A linked worktree must not inherit the parent TUICommander's Cargo target.
# Otherwise its tests can reuse dependency artifacts compiled from another checkout.
if [ -f "$root/.git" ]; then
  unset CARGO_TARGET_DIR
fi

# shellcheck source=test-tmp-lib.sh
. "$(dirname "$0")/test-tmp-lib.sh"
host="$(tuic_test_host_tmpdir)"
test_tmp_base="$(tuic_test_tmp_base)"
mkdir -p "$test_tmp_base"
# mbx binds its cache socket at "$TMPDIR/mbx-session-XXXXXX/cache-agent.sock", and a
# Unix socket path holds at most 103 bytes on macOS, so the per-run root must be at
# most 67 bytes: 103 - 1 ("/") - 35 ("mbx-session-XXXXXX/cache-agent.sock"). The run dir
# is "tuic-run.XXXXXX" (15). When <host temp>/tuic-tests is too long for that, the run
# dir goes directly in the host temp dir; when even that is too long, the wrapper
# refuses up front and names the limit, but only if mbx is installed (CI and most
# machines have none). An inherited TUIC_TEST_TMP_ROOT is the caller's choice.
tuic_run_dir_len=15
tuic_mbx_room=67
tuic_base_is_host=0
if [[ -z "${TUIC_TEST_TMP_ROOT:-}" ]]; then
  physical_base="$(cd "$test_tmp_base" && pwd -P)"
  if (( ${#physical_base} + 1 + tuic_run_dir_len > tuic_mbx_room )); then
    physical_host="$(cd "$host" && pwd -P)"
    if (( ${#physical_host} + 1 + tuic_run_dir_len <= tuic_mbx_room )); then
      test_tmp_base="$host"
      tuic_base_is_host=1
    elif command -v mbx >/dev/null 2>&1; then
      echo "with-test-tmp.sh: $physical_host is ${#physical_host} bytes; the mbx socket path limit leaves $((tuic_mbx_room - 1 - tuic_run_dir_len)) for the temp dir. Set TMPDIR to a shorter directory." >&2
      exit 1
    fi
  fi
fi

# Housekeeping only: a prune that cannot delete must never fail the command.
warn_prune() {
  echo "with-test-tmp: could not prune stale scratch in $1; continuing" >&2
}
# Per-run and per-checkout dirs (current and older names) untouched for six
# days, in the current base, the old in-checkout base and an inherited root.
prune_stale_runs() {
  [[ -d "$1" ]] || return 0
  find "$1" -mindepth 1 -maxdepth 1 -type d \
    \( -name 'tuic-run.*' -o -name 'socket-*' -o -name 'tuic-co-*' \
    -o -name 'tuic-proc-*' -o -name 'tuic-nextest-*' \) -mtime +6 \
    -exec rm -rf -- {} + 2>/dev/null || warn_prune "$1"
}
# tuic-test-support's socket roots in the host temp dir, ours only: a private
# t.XXXXXX holding its .tuic-socket-root marker (each test process removes its
# own at exit; this catches the ones a killed process left), and the older
# per-checkout tuic-s<16 hex> dirs. Never anything in /tmp.
prune_stale_socket_dirs() {
  [[ -d "$1" ]] || return 0
  local stale name
  # A pipe, not `< <(find ...)`: process substitution needs /dev/fd/*, which
  # sandboxes deny. The loop keeps no state, so its subshell is harmless.
  find "$1" -mindepth 1 -maxdepth 1 -type d \
    \( -name 't.??????' -mtime +1 -o -name 'tuic-s????????????????' -mtime +6 \) \
    -print0 2>/dev/null | while IFS= read -r -d '' stale; do
    name=${stale##*/}
    [[ -O "$stale" && ! -L "$stale" ]] || continue
    if [[ $name =~ ^t\.[a-z0-9]{6}$ ]]; then
      [[ -f "$stale/.tuic-socket-root" ]] || continue
    elif [[ ! $name =~ ^tuic-s[0-9a-f]{16}$ ]]; then
      continue
    fi
    rm -rf -- "$stale" 2>/dev/null || warn_prune "$stale"
  done || true
}
if (( tuic_base_is_host )); then
  # The host temp dir is shared: remove only this wrapper's own run dirs there.
  find "$host" -mindepth 1 -maxdepth 1 -type d -name 'tuic-run.??????' -mtime +6 \
    -exec rm -rf -- {} + 2>/dev/null || warn_prune "$host"
else
  prune_stale_runs "$test_tmp_base"
fi
prune_stale_runs "$root/.tmp/tuic-tests"
prune_stale_socket_dirs "$host"

if [[ -n "${TUIC_TEST_TMP_ROOT:-}" ]]; then
  # The caller's root: use it as is, never delete it.
  test_tmp="${TUIC_TEST_TMP_ROOT%/}"
  mkdir -p "$test_tmp"
  prune_stale_runs "$test_tmp"
else
  # Physical spelling (macOS /var -> /private/var): paths a test builds from
  # TMPDIR then compare equal to the canonical paths code under test stores
  # or an FSEvents watcher reports.
  test_tmp="$(cd "$(mktemp -d "$test_tmp_base/tuic-run.XXXXXX")" && pwd -P)"
  # Real permission tests may leave unreadable directories or read-only files.
  # This root is disposable; physical traversal never changes symlink targets.
  cleanup_test_tmp() {
    local command_status=$?
    chmod -R u+rwX "$test_tmp"
    rm -rf "$test_tmp"
    return "$command_status"
  }
  trap cleanup_test_tmp EXIT
fi
export TUIC_TEST_HOST_TMPDIR="$host"
export TMPDIR="$test_tmp/"
export TMP="$test_tmp/"
export TEMP="$test_tmp/"
export TUIC_TEST_TMP_ROOT="$test_tmp"
# Test-fixture `git init`/`git clone` must not copy git's hooks/*.sample files
# into every scratch repo (slow, machine-dependent, EPERM where `.git/hooks`
# writes are forbidden). nextest's setup script sets its own under the same root.
# hooks/ stays an empty directory and info/ holds git's one-line default exclude file:
# tests write .git/hooks/* and read or append .git/info/exclude as after a real `git init`;
# only the sample hooks are gone.
mkdir -p "$test_tmp/git-template-empty/hooks" "$test_tmp/git-template-empty/info"
printf '%s\n' '# git ls-files --others --exclude-from=.git/info/exclude' > "$test_tmp/git-template-empty/info/exclude"
export GIT_TEMPLATE_DIR="$test_tmp/git-template-empty"
# Fail closed: git under the test root never discovers a repository above it
# (a fixture whose `git init` failed once fell through to this checkout and
# renamed its branches). Ceiling = the parent of the root actually in use (an
# inherited TUIC_TEST_TMP_ROOT's own parent, not the base) plus the host temp
# dir, which holds the t.XXXXXX socket roots; appended once each, so an outer
# wrapper's stricter value survives.
# The physical root's parent is listed too: when the root itself is a symlink,
# git compares against where it really lives.
GIT_CEILING_DIRECTORIES="$(tuic_git_ceiling_with "$(dirname "$(cd "$test_tmp" && pwd)")" \
  "$(dirname "$(cd "$test_tmp" && pwd -P)")" "$host")"
export GIT_CEILING_DIRECTORIES
# Opt-in (TUIC_TEST_ISOLATE_GIT_CONFIG=1, set by `make test-shell`): explicit git
# config instead of the developer's global/system files. Opt-in, not default:
# the other wrapped steps (pnpm/cargo audit, the gitlink and submodule checks)
# talk to real remotes and need the user's credential helper, url.*.insteadOf
# and proxy settings. The Rust suite isolates itself (tuic-test-support's
# TEST_GITCONFIG, kept identical to the shell copy by
# scripts/test-with-test-tmp-gitconfig-parity.sh).
if [[ "${TUIC_TEST_ISOLATE_GIT_CONFIG:-}" == 1 ]]; then
  tuic_git_isolate_config "$test_tmp"
fi
# macOS with sccache on PATH only: meson auto-wraps its compiler with sccache,
# whose server socket lands under $TMPDIR, now a long scratch path, so every
# compile fails with "path must be shorter than SUN_LEN" (the bundled WebRTC
# build under tuic-dictation). Naming the compilers keeps any launcher out.
# Nowhere else: an explicit CC/CXX is part of the cc crate's build fingerprint,
# so forcing it everywhere would rebuild the native deps of every wrapped run
# against an unwrapped one (and override CI's own compiler choice). A caller's
# CC/CXX still wins.
tuic_pin_compilers_for_sccache
# macOS only: name the real `ar` so meson never runs the xcrun shim, whose cache
# write a sandbox denies (see tuic_pin_archiver_for_xcrun_shim). A caller's AR wins.
tuic_pin_archiver_for_xcrun_shim
# Opt-in (TUIC_TEST_FAKE_HOME=1): the command gets a throwaway $HOME under the
# test root. Tests that use the home directory as scratch space (plugin_fs,
# terminal image transmission) or resolve the platform config dir (dirs::config_dir
# reads $HOME on macOS) then never touch the developer's real files, and a sandbox
# that denies writes there cannot fail them. Cargo and rustup keep their real
# homes, pinned before $HOME moves (from the passwd entry when HOME is unset).
# Not applied to pnpm/cargo audit, which need the real caches. nextest's setup
# script (src-tauri/scripts/nextest-test-tmp.sh) applies the same flag to the
# test processes of a direct `cargo nextest run`.
if [[ "${TUIC_TEST_FAKE_HOME:-}" == 1 ]]; then
  real_home="${HOME:-}"
  # Unset HOME: bash expands ~ from the passwd entry instead.
  [[ -n "$real_home" ]] || real_home=~
  if [[ -n "$real_home" && "$real_home" != "~" ]]; then
    export CARGO_HOME="${CARGO_HOME:-$real_home/.cargo}" RUSTUP_HOME="${RUSTUP_HOME:-$real_home/.rustup}"
  fi
  fake_home="$test_tmp/home"
  mkdir -p "$fake_home"
  export HOME="$fake_home"
fi
"$@"
