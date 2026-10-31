# Sourced. Several coordinators run at once (COORDINATOR, COORDINATOR2, ...); each has its own state dir.
# coord_base: the shared dir (critic rounds, vitest ledger, landing lock, session registrations) and the dir
# of COORDINATOR. coord_state: this coordinator's dir: $TUIC_COORD_STATE, else the dir that coord-resume.sh
# registered in <base>/sessions/$TUIC_SESSION, else the base. open_work_state.py holds the same rule.
coord_base() { printf '%s\n' "${TUIC_COORD_STATE:-$HOME/Gits/.tmp/coordinator}"; }
coord_state() {
  local b; b=$(coord_base)
  if [[ -z ${TUIC_COORD_STATE:-} && -n ${TUIC_SESSION:-} && -s "$b/sessions/$TUIC_SESSION" ]]; then
    head -1 "$b/sessions/$TUIC_SESSION"
  else printf '%s\n' "$b"; fi
}
# coord_dirs: every coordinator dir. A worktree is safe to sweep only when no coordinator has a child in it.
# Both succeed when a file is missing: a failed cat would fail a caller's pipe under pipefail.
coord_dirs() { local b; b=$(coord_base); { printf '%s\n' "$b"; cat "$b"/sessions/* 2>/dev/null || true; } | awk 'NF && !seen[$0]++'; }
coord_children_all() { local d; while IFS= read -r d; do cat "$d/children.tsv" 2>/dev/null || true; done < <(coord_dirs); }
# owns_result <child-name>: its results file belongs to this coordinator: the child is in this children.tsv,
# or no coordinator registered it and this is COORDINATOR (the base dir).
owns_result() {
  local s; s=$(coord_state)
  [[ -f $s/children.tsv ]] && coord_has_child "$1" <"$s/children.tsv" && return 0
  [[ $s == "$(coord_base)" ]] || return 1
  ! coord_children_all | coord_has_child "$1"
}
# awk, not `grep -q` in a pipe: an early grep exit fails the pipe under pipefail.
coord_has_child() { awk -F'\t' -v n="$1" '$2 == n {f = 1} END {exit !f}'; }
