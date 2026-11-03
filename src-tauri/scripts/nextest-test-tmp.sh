#!/bin/sh
# Nextest setup script: give every test process the same scratch roots
# scripts/with-test-tmp.sh would. TUIC_TEST_TMP_ROOT when a wrapper already
# chose one, else this checkout's default under the host temp dir (order in
# scripts/test-tmp-lib.sh). Never derived from $HOME.
set -eu

# shellcheck source=../../scripts/test-tmp-lib.sh
. ../scripts/test-tmp-lib.sh
host=$(tuic_test_host_tmpdir)
root=$(tuic_test_tmp_root ..)
root=$(cd "$root" && pwd)
# An empty template dir keeps `git init`/`git clone` in test fixtures from
# copying git's own hooks/*.sample files into every scratch repo: slower,
# machine-dependent, and EPERM where `.git/hooks` writes are forbidden. Test
# processes only; production git calls never see this variable.
git_template="$root/git-template-empty"
mkdir -p "$git_template"
# Fail closed: git in a test process must never discover a repository above
# the root (a fixture whose `git init` failed once fell through to the real
# checkout and renamed its branches). GIT_CEILING_DIRECTORIES = the root's
# parent plus the host temp dir (home of the t.XXXXXX socket roots), lexical
# and physical spelling (macOS /var vs /private/var), each appended once to any
# value an outer wrapper already set.
# The physical root's parent too, for a root that is itself a symlink.
git_ceiling=$(tuic_git_ceiling_with "$(dirname "$root")" "$(dirname "$(cd "$root" && pwd -P)")" "$host")
# TUIC_TEST_FAKE_HOME=1: the same throwaway HOME scripts/with-test-tmp.sh gives
# its command, for the test processes only (nextest itself, cargo and rustup
# keep the real one), so a direct `cargo nextest run` honours the flag too.
# CARGO_HOME/RUSTUP_HOME are pinned for tests that start cargo themselves.
fake_home=
if [ "${TUIC_TEST_FAKE_HOME:-}" = 1 ]; then
    fake_home="$root/home"
    mkdir -p "$fake_home"
    real_home=${HOME:-}
    if [ "$real_home" = "$fake_home" ]; then
        real_home= # the wrapper moved HOME already and pinned both below
    fi
fi
{
    if [ -n "$fake_home" ]; then
        printf 'HOME=%s\n' "$fake_home"
        if [ -n "${CARGO_HOME:-}" ]; then
            printf 'CARGO_HOME=%s\n' "$CARGO_HOME"
        elif [ -n "$real_home" ]; then
            printf 'CARGO_HOME=%s/.cargo\n' "$real_home"
        fi
        if [ -n "${RUSTUP_HOME:-}" ]; then
            printf 'RUSTUP_HOME=%s\n' "$RUSTUP_HOME"
        elif [ -n "$real_home" ]; then
            printf 'RUSTUP_HOME=%s/.rustup\n' "$real_home"
        fi
    fi
    printf 'TUIC_TEST_HOST_TMPDIR=%s\n' "$host"
    printf 'TUIC_TEST_TMP_ROOT=%s\n' "$root"
    printf 'TMPDIR=%s/\n' "$root"
    printf 'TMP=%s/\n' "$root"
    printf 'TEMP=%s/\n' "$root"
    printf 'GIT_TEMPLATE_DIR=%s\n' "$git_template"
    printf 'GIT_CEILING_DIRECTORIES=%s\n' "$git_ceiling"
} >> "$NEXTEST_ENV"
