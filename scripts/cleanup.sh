#!/usr/bin/env bash
# Remove scratch, leftovers and stale caches from this checkout and from the
# TUICommander data directory. Dry run by default; pass --apply to delete.
#
#   scripts/cleanup.sh            list what would be removed and the size
#   scripts/cleanup.sh --apply    remove it
#
# TUIC_DATA_DIR overrides the data directory. Nothing that a running process
# holds open is removed. Retired SQLite stores (now in tuic.sqlite3) are only
# removed on or after RETIRED_DB_AFTER, and only when no process has them open.
set -euo pipefail

RETIRED_DB_AFTER="${TUIC_RETIRED_DB_AFTER:-2026-11-28}"
RETIRED_DBS=(stories.sqlite3 workflows.sqlite3 managed-launches.sqlite mcp_usage.sqlite3
  mail-subscriptions.db tunnel_audit.db)
# Files and directories no code reads or writes any more (checked 2026-11-21).
ORPHANS=(ai-chat.json ai-prompts.json ai-watchers.json providers.json capability-hooks
  capability-shim ai-chat-conversations ai-sessions wiz-kanban-2.0.0.bak)
INSTANCE_DAYS=3
CAPTURE_DAYS=14
KEEP_MCP_BACKUPS=3
KEEP_REPO_BACKUPS=2

apply=0
case "${1:-}" in
  --apply) apply=1 ;;
  "" | --dry-run) ;;
  *) echo "usage: $0 [--apply]" >&2; exit 2 ;;
esac

repo="$(git -C "$(dirname "$0")/.." rev-parse --show-toplevel)"
if [[ -n "${TUIC_DATA_DIR:-}" ]]; then
  data="$TUIC_DATA_DIR"
elif [[ "$(uname)" == Darwin ]]; then
  data="$HOME/Library/Application Support/com.tuic.commander"
else
  data="${XDG_CONFIG_HOME:-$HOME/.config}/com.tuic.commander"
fi
version="$(sed -n 's/^  "version": "\(.*\)",$/\1/p' "$repo/package.json" | head -1)"
open_files="$(lsof -nP -Fn 2>/dev/null | sed -n 's/^n//p' || true)"
freed=0

is_open() { # path: true when a process holds the path or anything below it
  grep -qF -- "$1" <<<"$open_files"
}

remove() { # path [reason]
  local path="$1" reason="${2:-}"
  [[ -e "$path" || -L "$path" ]] || return 0
  if is_open "$path"; then
    echo "  keep (open)  ${path#"$HOME"/}"
    return 0
  fi
  local kb
  kb=$(du -sk "$path" 2>/dev/null | cut -f1)
  freed=$((freed + ${kb:-0}))
  printf '  %-12s %6s MB  %s\n' "${reason:-remove}" "$(( ${kb:-0} / 1024 ))" "${path#"$HOME"/}"
  if ((apply)); then
    chmod -R u+rwX "$path" 2>/dev/null || true
    rm -rf -- "$path"
  fi
}

keep_newest() { # count dir-or-glob-prefix...: remove all but the newest N matches
  local count="$1"; shift
  local paths=()
  for p in "$@"; do [[ -e "$p" ]] && paths+=("$p"); done
  ((${#paths[@]} > count)) || return 0
  while IFS= read -r p; do remove "$p" "old backup"; done < <(ls -1td -- "${paths[@]}" | tail -n +"$((count + 1))")
}

echo "== checkout: $repo"
if [[ -d "$repo/.tmp" ]]; then
  # Agent scratch: anything untouched for a day, except the test roots.
  while IFS= read -r entry; do
    case "${entry##*/}" in tuic-tests | orphan-persistent-fixtures) continue ;; esac
    [[ -z "$(find "$entry" -newermt '-24 hours' -print -quit 2>/dev/null)" ]] && remove "$entry" "scratch"
  done < <(find "$repo/.tmp" -mindepth 1 -maxdepth 1)
fi
remove "$repo/coverage" "generated"
remove "$repo/.vitest" "cache"
[[ -d "$repo/plugins__wt" ]] && [[ -z "$(ls -A "$repo/plugins__wt")" ]] && remove "$repo/plugins__wt" "empty"
for f in "$repo"/.wiz/*.json.*.tmp; do
  [[ -e "$f" ]] || continue
  pid="$(sed -E 's/.*\.json\.([0-9]+)\..*/\1/' <<<"$f")"
  kill -0 "$pid" 2>/dev/null || remove "$f" "dead writer"
done
while IFS= read -r f; do remove "$f" "finder"; done < <(find "$repo" -maxdepth 3 -name .DS_Store -not -path '*/node_modules/*')

echo "== data dir: $data"
if [[ -d "$data" ]]; then
  # Agent-test instances unused for INSTANCE_DAYS days; open ones are kept by remove().
  if [[ -d "$data/instances" ]]; then
    while IFS= read -r i; do remove "$i" "instance"; done \
      < <(find "$data/instances" -mindepth 1 -maxdepth 1 -type d -mtime +"$INSTANCE_DAYS")
  fi
  for name in "${ORPHANS[@]}"; do remove "$data/$name" "orphan"; done
  for f in "$data"/*.legacy-workflows.bak "$data"/themes.backup-* "$data"/no-session-inject.*; do
    [[ -e "$f" ]] && remove "$f" "leftover"
  done
  keep_newest "$KEEP_REPO_BACKUPS" "$data"/repositories.json.bak-* "$data"/repositories.pre-*.json \
    "$data"/repositories.restore-*.json "$data"/repositories.repair-backup-*.json
  [[ -d "$data/mcp-backups" ]] && keep_newest "$KEEP_MCP_BACKUPS" "$data"/mcp-backups/*
  if [[ -d "$data/captures" ]]; then
    while IFS= read -r f; do remove "$f" "capture"; done \
      < <(find "$data/captures" -type f -mtime +"$CAPTURE_DAYS")
  fi

  # remote-bin: same rule as remote_deploy::assets::prune_other_versions —
  # the running version plus the newest other one. Empty version dirs go too.
  if [[ -d "$data/remote-bin" ]]; then
    kept_other=0
    while IFS= read -r v; do
      dir="$data/remote-bin/$v"
      [[ "$v" == "$version" ]] && continue
      if [[ -z "$(ls -A "$dir")" ]]; then remove "$dir" "empty"; continue; fi
      if ((kept_other)); then remove "$dir" "old remote"; else kept_other=1; fi
    done < <(ls -1t "$data/remote-bin")
  fi

  # Design Mode browser profiles of repositories no longer registered.
  if [[ -d "$data/design-mode" && -f "$data/repositories.json" ]]; then
    while IFS= read -r profile; do remove "$data/design-mode/$profile" "profile"; done < <(
      python3 -I - "$data" <<'EOF'
import hashlib, os, re, sys
data = sys.argv[1]
text = open(os.path.join(data, "repositories.json"), encoding="utf-8").read()
known = set()
for path in re.findall(r'"(/[^"]+)"', text):
    for variant in (path, path.rstrip("/")):
        known.add(hashlib.sha256(variant.encode()).hexdigest()[:16])
for name in os.listdir(os.path.join(data, "design-mode")):
    if name not in known:
        print(name)
EOF
    )
  fi

  # Retired SQLite stores: their data moved to tuic.sqlite3 (story 3104).
  if [[ "$(date +%F)" < "$RETIRED_DB_AFTER" ]]; then
    echo "  retired databases kept until $RETIRED_DB_AFTER"
  else
    for db in "${RETIRED_DBS[@]}"; do
      for suffix in "" -wal -shm -journal; do remove "$data/$db$suffix" "retired db"; done
    done
  fi
fi

echo "== $((freed / 1024)) MB $( ((apply)) && echo freed || echo "would be freed (dry run; pass --apply)")"
