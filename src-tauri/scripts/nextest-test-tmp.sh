#!/bin/sh
set -eu

root=${TUIC_TEST_TMP_ROOT:-"$(cd .. && pwd)/.tmp/tuic-tests"}
mkdir -p "$root"
root=$(cd "$root" && pwd)
# Git hermeticity, as scripts/with-test-tmp.sh does for its command: an empty
# template dir (no hooks/*.sample copied into every fixture repo) and a ceiling
# at the root's parent (a failed `git init` must never fall through to this
# checkout). Test processes only; production git calls never see these.
# Same function in scripts/with-test-tmp.sh and src-tauri/scripts/nextest-test-tmp.sh
# (each stays one self-contained file: tests copy the wrapper into fixture checkouts).
# Usage: tuic_git_ceiling_with <dir>... Prints $GIT_CEILING_DIRECTORIES with each
# existing <dir> appended in its lexical and its physical spelling (macOS /var
# vs /private/var), each entry once, never an empty one: git skips entries it
# cannot compare and stops resolving symlinks after an empty entry. An outer
# wrapper's value is kept verbatim, so a stricter ceiling survives. Git never
# ascends INTO an entry, so pass the PARENT of the dir the fence protects.
tuic_git_ceiling_with() {
  _tuic_ceiling=${GIT_CEILING_DIRECTORIES:-}
  for _tuic_dir in "$@"; do
    for _tuic_entry in "$(CDPATH='' cd -- "$_tuic_dir" 2>/dev/null && pwd)" \
      "$(CDPATH='' cd -- "$_tuic_dir" 2>/dev/null && pwd -P)"; do
      [ -n "$_tuic_entry" ] || continue
      # git splits this variable on ':' with no escape, so an entry containing one would
      # fence two other directories: refuse (fail closed) rather than fence nothing.
      case "$_tuic_entry" in
        *:*) echo "git ceiling: '$_tuic_entry' contains ':', which GIT_CEILING_DIRECTORIES cannot express; use a scratch root without one" >&2; return 1 ;;
      esac
      case ":$_tuic_ceiling:" in
        *":$_tuic_entry:"*) ;;
        *) _tuic_ceiling=${_tuic_ceiling:+$_tuic_ceiling:}$_tuic_entry ;;
      esac
    done
  done
  printf '%s\n' "$_tuic_ceiling"
}
git_template="$root/git-template-empty"
# hooks/ stays an empty directory and info/ holds git's one-line default exclude file
# (tests write .git/hooks/* and read or append .git/info/exclude); only the sample hooks are gone.
mkdir -p "$git_template/hooks" "$git_template/info"
printf '%s\n' '# git ls-files --others --exclude-from=.git/info/exclude' > "$git_template/info/exclude"
# The parent of the physical root too: a symlinked root lives somewhere else.
# (A setup script can only add variables, not unset them: GIT_DIR and friends are
# cleared by the tuic-test-support constructor in every test binary instead.)
git_ceiling=$(tuic_git_ceiling_with "$(dirname "$root")" "$(dirname "$(cd "$root" && pwd -P)")")
{
    printf 'TUIC_TEST_TMP_ROOT=%s\n' "$root"
    printf 'TMPDIR=%s/\n' "$root"
    printf 'TMP=%s/\n' "$root"
    printf 'TEMP=%s/\n' "$root"
    printf 'GIT_TEMPLATE_DIR=%s\n' "$git_template"
    printf 'GIT_CEILING_DIRECTORIES=%s\n' "$git_ceiling"
} >> "$NEXTEST_ENV"
