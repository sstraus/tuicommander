#!/bin/sh
# Builds a fixture superproject whose "plugins" gitlink points at a pushed commit
# (check must pass) and at a commit that only exists locally (check must fail).
set -eu

here=$(cd "$(dirname "$0")" && pwd)
work=$(mktemp -d "${TMPDIR:-/tmp}/gitlink.XXXXXX")
trap 'rm -rf "$work"' EXIT
# Explicit git config: never the developer's global/system files.
. "$here/test-tmp-lib.sh"
tuic_git_isolate_config "$work/gitcfg"
export GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@t GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@t

git init -q --bare -b main "$work/remote.git"
git init -q -b main "$work/sub"
git -C "$work/sub" remote add origin "$work/remote.git"
git -C "$work/sub" commit -q --allow-empty -m pushed
git -C "$work/sub" push -q origin main
pushed=$(git -C "$work/sub" rev-parse HEAD)
git -C "$work/sub" commit -q --allow-empty -m unpushed
unpushed=$(git -C "$work/sub" rev-parse HEAD)

# make_super <dir> <sha>: superproject with a gitlink to <sha> and .gitmodules -> fixture remote
make_super() {
  git init -q -b main "$1"
  printf '[submodule "plugins"]\n\tpath = plugins\n\turl = %s\n' "$work/remote.git" >"$1/.gitmodules"
  git -C "$1" add .gitmodules
  git -C "$1" update-index --add --cacheinfo "160000,$2,plugins"
  git -C "$1" commit -q -m pin
}

make_super "$work/super-pushed" "$pushed"
make_super "$work/super-unpushed" "$unpushed"

"$here/check-gitlink-reachable.sh" "$work/super-pushed" ||
  { echo "FAIL: pushed pin was rejected" >&2; exit 1; }
if "$here/check-gitlink-reachable.sh" "$work/super-unpushed"; then
  echo "FAIL: unpushed pin was accepted" >&2
  exit 1
fi
echo "ok: gitlink check accepts a pushed pin and rejects an unpushed one"
