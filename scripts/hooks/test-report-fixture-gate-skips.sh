#!/usr/bin/env bash
set -euo pipefail

project_root="$(git rev-parse --show-toplevel)"
test_tmp="${TUIC_TEST_TMP_ROOT:-$project_root/.tmp/tuic-tests}"
mkdir -p "$test_tmp"
scratch="$(mktemp -d "$test_tmp/gate-skips.XXXXXX")"
trap 'rm -rf "$scratch"' EXIT

git -C "$scratch" init -q repo
repo="$scratch/repo"
commit() { git -C "$repo" -c user.name=Test -c user.email=t@example.com commit -q --allow-empty -F -; }
printf 'base\n' | commit
printf 'fix: plain\n' | commit
printf 'refactor: reorder mods\n\nFixture-Gate-Skip: rustfmt reorder, no detection change\n' | commit
printf 'fix: mentions it\n\nThe Fixture-Gate-Skip word in prose is not a trailer.\n' | commit

out="$(cd "$repo" && bash "$project_root/scripts/hooks/report-fixture-gate-skips.sh" HEAD~3..HEAD)"
[ "$(printf '%s\n' "$out" | wc -l | tr -d ' ')" = 1 ]
grep -Fq 'refactor: reorder mods: rustfmt reorder, no detection change' <<<"$out"
echo 'report-fixture-gate-skips passed'
