#!/usr/bin/env bash
set -euo pipefail

project_root="$(git rev-parse --show-toplevel)"
test_tmp="${TUIC_TEST_TMP_ROOT:-$project_root/.tmp/tuic-tests}"
mkdir -p "$test_tmp"
scratch="$(mktemp -d "$test_tmp/install-hooks.XXXXXX")"
trap 'rm -rf "$scratch"' EXIT

git -C "$scratch" init -q repo
repo="$scratch/repo"
git -C "$repo" -c core.hooksPath=/dev/null -c user.name=Test -c user.email=test@example.com commit -q --allow-empty -m initial
git -C "$repo" worktree add -q --detach "$scratch/other"

for checkout in "$repo" "$scratch/other"; do
  checkout="$(git -C "$checkout" rev-parse --show-toplevel)"
  mkdir -p "$checkout/scripts/hooks"
  cp "$project_root/scripts/hooks/install-hooks.sh" "$checkout/scripts/hooks/install-hooks.sh"
  cp "$project_root/scripts/hooks/test-install-hooks.sh" "$checkout/scripts/hooks/test-install-hooks.sh"
  printf '#!/bin/sh\nprintf "%%s\\n" "%s" >> "$HOOK_RESULTS"\n' "$checkout" > "$checkout/scripts/hooks/pre-commit"
  chmod +x "$checkout/scripts/hooks/pre-commit"
done

# The test harness inits repos with an empty template, so there is no
# .git/hooks until something creates it.
mkdir -p "$repo/.git/hooks"
printf '#!/bin/sh\n# external hook\n' > "$repo/.git/hooks/pre-push"
# An install made by the old installer: a symlink into one checkout.
ln -s "$repo/scripts/hooks/pre-commit" "$repo/.git/hooks/pre-commit"
cp "$repo/scripts/hooks/pre-commit" "$repo/scripts/hooks/pre-push"

(cd "$repo" && bash scripts/hooks/install-hooks.sh)
(cd "$scratch/other" && bash scripts/hooks/install-hooks.sh)
grep -Fqx '# external hook' "$repo/.git/hooks/pre-push"
# The old symlink is replaced by a dispatcher.
[ ! -L "$repo/.git/hooks/pre-commit" ]
head -n 2 "$repo/.git/hooks/pre-commit" | grep -Fqx '# tuic-managed-hook'
# This test lives beside the hooks but is not one.
[ ! -e "$repo/.git/hooks/test-install-hooks.sh" ]

: > "$scratch/results"
HOOK_RESULTS="$scratch/results" git -C "$repo" -c user.name=Test -c user.email=test@example.com commit -q --allow-empty -m main
HOOK_RESULTS="$scratch/results" git -C "$scratch/other" -c user.name=Test -c user.email=test@example.com commit -q --allow-empty -m other
printf '%s\n%s\n' "$(git -C "$repo" rev-parse --show-toplevel)" "$(git -C "$scratch/other" rev-parse --show-toplevel)" > "$scratch/expected"
diff -u "$scratch/expected" "$scratch/results"

# Reinstalling one checkout must not redirect hooks in the other.
(cd "$repo" && bash scripts/hooks/install-hooks.sh)
: > "$scratch/results"
HOOK_RESULTS="$scratch/results" git -C "$scratch/other" -c user.name=Test -c user.email=test@example.com commit -q --allow-empty -m again
git -C "$scratch/other" rev-parse --show-toplevel > "$scratch/expected"
diff -u "$scratch/expected" "$scratch/results"

# A checkout without the hook (an older branch) commits as if no hook existed.
rm "$scratch/other/scripts/hooks/pre-commit"
: > "$scratch/results"
HOOK_RESULTS="$scratch/results" git -C "$scratch/other" -c user.name=Test -c user.email=test@example.com commit -q --allow-empty -m no-hook
[ ! -s "$scratch/results" ]

# A failed dispatcher write must not leave its mktemp file in Git's hooks dir.
cleanup_repo="$scratch/cleanup"
git -C "$scratch" init -q cleanup
mkdir -p "$cleanup_repo/scripts/hooks" "$cleanup_repo/fake-bin"
cp "$project_root/scripts/hooks/install-hooks.sh" "$cleanup_repo/scripts/hooks/install-hooks.sh"
printf '#!/bin/sh\nexit 0\n' > "$cleanup_repo/scripts/hooks/pre-commit"
printf '#!/bin/sh\nexit 1\n' > "$cleanup_repo/fake-bin/cat"
chmod +x "$cleanup_repo/fake-bin/cat"

if (cd "$cleanup_repo" && PATH="$cleanup_repo/fake-bin:$PATH" bash scripts/hooks/install-hooks.sh); then
  echo "expected dispatcher write to fail" >&2
  exit 1
fi

if find "$cleanup_repo/.git/hooks" -name '.pre-commit.*' -print -quit | grep -q .; then
  echo "installer left a temporary hook file after failure" >&2
  exit 1
fi
