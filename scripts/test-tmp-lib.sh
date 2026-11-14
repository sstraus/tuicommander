# shellcheck shell=sh
# Test scratch roots for every shell entry point of the test harness
# (scripts/with-test-tmp.sh, src-tauri/scripts/nextest-test-tmp.sh,
# scripts/mutants.sh, the shell tests). POSIX sh, sourced, defines functions
# only. Same order as tuic-test-support's host_temp_dir()/test_base()/
# test_temp_root(); never reads $HOME and never defaults into a checkout.
#
#   host temp  TUIC_TEST_HOST_TMPDIR (exported once, before anything points
#              TMPDIR at a test root, so a default can never nest), else
#              TMPDIR, else the OS default temp dir
#   base       TUIC_TEST_TMP_BASE (explicit opt-in), else <host temp>/tuic-tests
#   root       TUIC_TEST_TMP_ROOT (inherited: the caller owns it), else
#              <base>/tuic-co-<checkout hash>

tuic_test_host_tmpdir() {
  _tuic_host=${TUIC_TEST_HOST_TMPDIR:-${TMPDIR:-}}
  if [ -z "$_tuic_host" ]; then
    # What Rust's std::env::temp_dir() answers on macOS without TMPDIR.
    _tuic_host=$(getconf DARWIN_USER_TEMP_DIR 2>/dev/null || true)
  fi
  if [ -z "$_tuic_host" ]; then
    _tuic_probe=$(mktemp -d)
    _tuic_host=$(dirname "$_tuic_probe")
    rmdir "$_tuic_probe"
  fi
  while [ "${_tuic_host%/}" != "$_tuic_host" ]; do _tuic_host=${_tuic_host%/}; done
  printf '%s\n' "${_tuic_host:-/}"
}

tuic_test_tmp_base() {
  _tuic_base=${TUIC_TEST_TMP_BASE:-$(tuic_test_host_tmpdir)/tuic-tests}
  printf '%s\n' "${_tuic_base%/}"
}

# FNV-1a 64 of a checkout's physical path: the same 16 hex digits as
# tuic-test-support's checkout_hash(), so every entry point picks one dir.
tuic_checkout_hash() {
  _tuic_hash=-3750763034362895579 # 0xcbf29ce484222325 as a signed 64-bit value
  for _tuic_byte in $(printf '%s' "$1" | od -An -v -tu1); do
    _tuic_hash=$(((_tuic_hash ^ _tuic_byte) * 1099511628211))
  done
  printf '%016x\n' "$_tuic_hash"
}

# Usage: tuic_test_tmp_root <dir inside the checkout>. Creates and prints it.
tuic_test_tmp_root() {
  if [ -n "${TUIC_TEST_TMP_ROOT:-}" ]; then
    _tuic_root=${TUIC_TEST_TMP_ROOT%/}
  else
    _tuic_checkout=$(cd "$(git -C "$1" rev-parse --show-toplevel)" && pwd -P)
    _tuic_root="$(tuic_test_tmp_base)/tuic-co-$(tuic_checkout_hash "$_tuic_checkout")"
  fi
  mkdir -p "$_tuic_root"
  printf '%s\n' "$_tuic_root"
}

# Usage: tuic_git_ceiling_with <dir>... Prints $GIT_CEILING_DIRECTORIES with each
# existing <dir> appended in its lexical and its physical spelling (macOS /var
# vs /private/var), each entry once, never an empty one: git skips entries it
# cannot compare and stops resolving symlinks after an empty entry. An outer
# wrapper's value is kept verbatim, so a stricter ceiling survives. Git never
# ascends INTO an entry, so pass the parent of the dir the fence protects.
tuic_git_ceiling_with() {
  _tuic_ceiling=${GIT_CEILING_DIRECTORIES:-}
  for _tuic_dir in "$@"; do
    for _tuic_entry in "$(CDPATH='' cd -- "$_tuic_dir" 2>/dev/null && pwd)" \
      "$(CDPATH='' cd -- "$_tuic_dir" 2>/dev/null && pwd -P)"; do
      [ -n "$_tuic_entry" ] || continue
      case ":$_tuic_ceiling:" in
        *":$_tuic_entry:"*) ;;
        *) _tuic_ceiling=${_tuic_ceiling:+$_tuic_ceiling:}$_tuic_entry ;;
      esac
    done
  done
  printf '%s\n' "$_tuic_ceiling"
}

# Usage: tuic_git_isolate_config <dir>. Points every git started from this shell
# at <dir>/gitconfig (the same content tuic-test-support writes) instead of the
# developer's global/system config, so a test never depends on how a machine's
# git is configured (merge.ff=only, url.*.insteadOf, hooksPath, signing, ...).
# Exports GIT_CONFIG_GLOBAL, so call it in the current shell, not in $(...).
# A test that needs a specific global setting exports GIT_CONFIG_GLOBAL after.
tuic_git_isolate_config() {
  mkdir -p "$1"
  _tuic_gitconfig=$1/gitconfig
  # Several runners may write this at once: install it by rename, never in place.
  printf '%s\n' \
    '# Written by scripts/test-tmp-lib.sh: tests never read the developer'\''s git config.' \
    '[user]' '	name = TUIC Test' '	email = tests@tuic.invalid' \
    '[init]' '	defaultBranch = main' \
    '[commit]' '	gpgSign = false' \
    '[tag]' '	gpgSign = false' \
    '[core]' '	autocrlf = false' >"$_tuic_gitconfig.$$.tmp"
  mv -f "$_tuic_gitconfig.$$.tmp" "$_tuic_gitconfig"
  GIT_CONFIG_GLOBAL=$_tuic_gitconfig
  GIT_CONFIG_NOSYSTEM=1
  export GIT_CONFIG_GLOBAL GIT_CONFIG_NOSYSTEM
  # Command-scope config an invoking `git -c ...` or shell may have exported.
  unset GIT_CONFIG_SYSTEM GIT_CONFIG_PARAMETERS GIT_CONFIG_COUNT
}

# Usage: tuic_pin_compilers_for_sccache. On macOS with sccache on PATH, exports
# CC=cc and CXX=c++ unless the caller set them (see scripts/with-test-tmp.sh for
# why); anywhere else it changes nothing. Call it in the current shell.
tuic_pin_compilers_for_sccache() {
  [ "$(uname -s 2>/dev/null)" = Darwin ] || return 0
  command -v sccache >/dev/null 2>&1 || return 0
  CC=${CC:-cc}
  CXX=${CXX:-c++}
  export CC CXX
}

# Usage: tuic_pin_archiver_for_xcrun_shim. On macOS, exports AR as the real
# toolchain `ar` that `xcrun --find ar` names, unless the caller set AR; anywhere
# else (or when xcrun cannot name an executable) it changes nothing. Call it in
# the current shell. /usr/bin/ar is a shim that runs xcrun, and where xcrun
# cannot write its lookup cache (a sandbox) a cache miss makes the shim print
# "couldn't create cache file" before its usage text; meson accepts Apple's ar
# only when stderr STARTS with "usage", so it fails with "Unknown linker(s): ar"
# (the bundled WebRTC build under tuic-dictation). The real binary never
# consults xcrun, so this holds whatever the cache holds.
tuic_pin_archiver_for_xcrun_shim() {
  [ "$(uname -s 2>/dev/null)" = Darwin ] || return 0
  [ -z "${AR:-}" ] || return 0
  command -v xcrun >/dev/null 2>&1 || return 0
  _tuic_real_ar=$(xcrun --find ar 2>/dev/null) || return 0
  [ -x "$_tuic_real_ar" ] && [ ! -d "$_tuic_real_ar" ] || return 0
  AR=$_tuic_real_ar
  export AR
}

# Usage: tuic_unix_socket_path_max <repo root>. Prints the longest Unix socket
# path the platform binds, read out of its one definition,
# `tuic_ipc::UNIX_SOCKET_PATH_MAX` (src-tauri/crates/tuic-ipc/src/lib.rs), so
# no script restates the number. Fails loudly (never a default) when that line
# cannot be read; scripts/test-run-remote-fixture.sh pins the value found.
tuic_unix_socket_path_max() {
  _tuic_ipc_src=$1/src-tauri/crates/tuic-ipc/src/lib.rs
  # The constant is defined once per host (Linux 107, everything else 103). The
  # conservative (non-Linux) value is the budget every platform must fit, so it wins
  # over the Linux one; a single unconditional definition is read as is.
  _tuic_path_max=$(awk '/^pub const UNIX_SOCKET_PATH_MAX: usize = [0-9]+;$/ { v = $NF; sub(/;/, "", v); if (prev ~ /not\(target_os = "linux"\)/ || !seen) { best = v; seen = 1 } } { prev = $0 } END { print best }' "$_tuic_ipc_src" 2>/dev/null)
  case $_tuic_path_max in
    '' | *[!0-9]*)
      echo "cannot read UNIX_SOCKET_PATH_MAX from $_tuic_ipc_src" >&2
      return 1
      ;;
  esac
  printf '%s\n' "$_tuic_path_max"
}
