#!/usr/bin/env bash
# Lists commits in <range> that skipped the agent-state fixture gate, with the
# reason each gave. A skip is declared by a `Fixture-Gate-Skip: <reason>` trailer
# in the commit message (AGENTS.md → "Agent state detection"). The landing check
# runs this on the pinned range and reports every line it prints.
# Usage: report-fixture-gate-skips.sh [<range>]   (default: origin/main..HEAD)
set -euo pipefail

range="${1:-origin/main..HEAD}"
git log --format='%H' "$range" | while read -r sha; do
  reason="$(git log -1 --format='%(trailers:key=Fixture-Gate-Skip,valueonly,unfold)' "$sha" | sed '/^$/d')"
  if [ -n "$reason" ]; then printf '%s %s: %s\n' "$(git rev-parse --short "$sha")" "$(git log -1 --format=%s "$sha")" "$reason"; fi
done
