#!/usr/bin/env bash
# One-command sandbox evidence for stories 2521-bbb8 and 2530-79d5.
#   live-sandbox-trial.sh prepare   probe the token, open one PR and one labelled issue
#   live-sandbox-trial.sh verify <pr-number>   list the reviews on that PR and check the head
# Only sstraus/tuic-e2e-sandbox is touched. No merge API is called. The token is
# read from the orchestrator .env and never printed (it reaches curl on stdin).
set -euo pipefail
REPO="sstraus/tuic-e2e-sandbox"
OUT="${TUIC_TRIAL_OUT:-$HOME/Gits/.tmp/2530}"
mkdir -p "$OUT"
set -a; . "$HOME/Gits/personal/orchestrator/.env"; set +a
: "${GITHUB_E2E_TOKEN:?GITHUB_E2E_TOKEN missing in the orchestrator .env}"

api() { # method path [json-body]
  local method="$1" path="$2" body="${3:-}"
  printf 'header = "Authorization: Bearer %s"\n' "$GITHUB_E2E_TOKEN" |
    curl -sS --fail-with-body -K - -X "$method" -H "Accept: application/vnd.github+json" \
      ${body:+-d "$body"} "https://api.github.com/repos/$REPO${path:+/$path}"
}
code() { printf 'header = "Authorization: Bearer %s"\n' "$GITHUB_E2E_TOKEN" |
  curl -sS -K - -o /dev/null -w '%{http_code}' "https://api.github.com/repos/$REPO/$1"; }

probe() {
  for p in pulls issues contents; do
    c="$(code "$p")"; echo "GET $p -> $c"
    [ "$c" = 200 ] || { echo "token lacks $p access (needs Pull requests, Issues, Contents read/write): stop"; exit 2; }
  done
}

case "${1:-}" in
  prepare)
    probe
    stamp="$(date +%s)"; branch="trial/$stamp"
    base="$(api GET "" | jq -r .default_branch)"
    sha="$(api GET "git/ref/heads/$base" | jq -r .object.sha)"
    api POST git/refs "$(jq -nc --arg r "refs/heads/$branch" --arg s "$sha" '{ref:$r,sha:$s}')" >/dev/null
    api PUT "contents/trial-$stamp.md" "$(jq -nc --arg b "$branch" --arg c "$(printf 'trial %s\n' "$stamp" | base64)" '{message:"test: trial file",content:$c,branch:$b}')" >/dev/null
    pr="$(api POST pulls "$(jq -nc --arg h "$branch" --arg b "$base" --arg t "Trial $stamp" '{title:$t,head:$h,base:$b,body:"Throwaway PR for the workflow trial."}')")"
    issue="$(api POST issues "$(jq -nc --arg t "Trial issue $stamp" '{title:$t,body:"Throwaway issue.",labels:["agent-ready"]}')")"
    jq -n --argjson pr "$pr" --argjson issue "$issue" --arg branch "$branch" \
      '{pr:$pr.number,prUrl:$pr.html_url,head:$pr.head.sha,branch:$branch,issue:$issue.number,issueUrl:$issue.html_url}' | tee "$OUT/sandbox-$stamp.json"
    echo "Next: enable the pr-review / issue-label binding on the agent-test instance, then: $0 verify <pr>"
    ;;
  verify)
    pr="${2:?pr number}"
    head="$(api GET "pulls/$pr" | jq -r .head.sha)"
    api GET "pulls/$pr/reviews" | jq --arg head "$head" '{head:$head,reviews:[.[]|{state,commit_id,onCurrentHead:(.commit_id==$head),user:.user.login}]}' | tee "$OUT/verify-$pr.json"
    ;;
  *) echo "usage: $0 prepare | verify <pr-number>" >&2; exit 64 ;;
esac
