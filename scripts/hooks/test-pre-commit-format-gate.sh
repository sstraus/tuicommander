#!/usr/bin/env bash
# Catches: the commit-time formatter not running, so unformatted Rust/TS reaches landing.
# Real `git commit` through the installed dispatcher in a linked worktree.
set -euo pipefail

project_root="$(git rev-parse --show-toplevel)"
test_tmp="${TUIC_TEST_TMP_ROOT:-$project_root/.tmp/tuic-tests}"
mkdir -p "$test_tmp"
scratch="$(mktemp -d "$test_tmp/format-gate.XXXXXX")"
trap 'rm -rf "$scratch"' EXIT

repo="$scratch/repo"
git init -q "$repo"
mkdir -p "$repo/scripts/hooks" "$repo/src-tauri/src" "$repo/src"
cp "$project_root/scripts/hooks/pre-commit" "$project_root/scripts/hooks/fixture-production-diff.py" "$repo/scripts/hooks/"
cp "$project_root/scripts/hooks/install-hooks.sh" "$repo/scripts/hooks/"
cp "$project_root/biome.json" "$project_root/src-tauri/rustfmt.toml" "$repo/" && mv "$repo/rustfmt.toml" "$repo/src-tauri/"
ln -s "$project_root/node_modules" "$repo/node_modules"
printf '[package]\nname = "app"\nedition = "2024"\n\n[workspace]\nmembers = ["patches/bm25"]\n' > "$repo/src-tauri/Cargo.toml"
printf 'fn main() {}\n' > "$repo/src-tauri/src/main.rs"
mkdir -p "$repo/src-tauri/patches/bm25/src"
printf '[package]\nname = "bm25"\nedition = "2021"\n' > "$repo/src-tauri/patches/bm25/Cargo.toml"
: > "$repo/src-tauri/patches/bm25/src/lib.rs"
printf 'node_modules\ntarget\nCargo.lock\n' > "$repo/.gitignore"
commit() { git -C "$1" -c user.name=Test -c user.email=test@example.com commit -qm "$2"; }
git -C "$repo" add -A && commit "$repo" initial
# Install into the repo's own hooks dir; an inherited core.hooksPath (TUIC's
# capability hook) still chains to it on commit.
(cd "$repo" && env -u GIT_CONFIG_COUNT bash scripts/hooks/install-hooks.sh >/dev/null)
git -C "$repo" worktree add -q "$scratch/wt" -b feature
wt="$scratch/wt"
ln -s "$project_root/node_modules" "$wt/node_modules"
mkdir -p "$wt/src-tauri/src" "$wt/src"

printf 'fn main(){let x=1;println!("{}",x);}\n' > "$wt/src-tauri/src/main.rs"
printf 'export const a={b:1}\n' > "$wt/src/a.ts"
git -C "$wt" add src-tauri/src/main.rs src/a.ts
if commit "$wt" unformatted > "$scratch/out" 2>&1; then
  echo 'unformatted Rust and TS were committed' >&2
  exit 1
fi
grep -Fq "'src-tauri/src/main.rs'" "$scratch/out"
grep -Fq 'cd src-tauri && cargo fmt' "$scratch/out"
grep -Fq "pnpm exec biome check --fix 'src/a.ts'" "$scratch/out"

(cd "$wt/src-tauri" && cargo fmt) && (cd "$wt" && "$project_root/node_modules/.bin/biome" check --fix src/a.ts >/dev/null 2>&1)
git -C "$wt" add src-tauri/src/main.rs src/a.ts
commit "$wt" formatted

# Catches: the gate formatting every file as edition 2024 while `cargo fmt --check`
# uses each crate's own edition, so a CI-clean file in the edition-2021 bm25
# member is refused and the suggested fix makes landing fail.
printf 'use a::{read_recipes, Recipe};\n' > "$wt/src-tauri/patches/bm25/src/lib.rs"
git -C "$wt" add src-tauri/patches/bm25
if ! commit "$wt" edition-2021 > "$scratch/out" 2>&1; then
  cat "$scratch/out" >&2
  echo 'a file that cargo fmt --check accepts for its edition-2021 crate was refused' >&2
  exit 1
fi
echo 'format gate regressions passed'
