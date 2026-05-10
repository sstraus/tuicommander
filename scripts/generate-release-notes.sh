#!/usr/bin/env bash
set -euo pipefail

# Generate AI-written release notes from CHANGELOG.md for a specific version.
# Usage: ./scripts/generate-release-notes.sh <version>
# Example: ./scripts/generate-release-notes.sh 1.3.0
#
# Extracts the CHANGELOG section for the given version, asks Claude CLI to
# rewrite it as user-friendly highlights, shows the result for approval,
# then merges it into src/assets/release-notes.json.
# Also extracts contributors from the ### Community section.

VERSION="${1:?Usage: $0 <version>}"
CHANGELOG="CHANGELOG.md"
OUTPUT="src/assets/release-notes.json"
TODAY=$(date +%Y-%m-%d)

# Strip leading "v" if provided
VERSION="${VERSION#v}"

# Extract the CHANGELOG section for this version (between ## [VERSION] and the next ## [)
SECTION=$(awk -v ver="$VERSION" '
  /^## \[/ {
    if (found) exit
    if (index($0, "[" ver "]")) found=1
    next
  }
  found { print }
' "$CHANGELOG")

if [ -z "$SECTION" ]; then
  echo "ERROR: No CHANGELOG section found for version $VERSION"
  echo "Make sure CHANGELOG.md has a '## [$VERSION]' heading."
  exit 1
fi

# Extract contributions from ### Community section
# Format: - [@user](url) — description (#PR)
COMMUNITY_LINES=$(echo "$SECTION" | awk '/^### Community/,/^###/' | grep '^- ')
if [ -n "$COMMUNITY_LINES" ]; then
  echo "==> Community contributions found:"
  echo "$COMMUNITY_LINES"
fi

echo "==> Extracted CHANGELOG section for v$VERSION:"
echo "$SECTION"
echo ""
echo "==> Generating AI release notes..."

PROMPT=$(cat <<'PROMPTEOF'
You are writing release notes for TUICommander, a terminal session orchestrator for AI coding agents. The audience is developers who use this tool daily.

Rewrite the following changelog entries as 3-8 concise, user-friendly bullet points. Rules:
- Each bullet should be a single sentence, max 120 characters
- Focus on what the user gains, not implementation details
- Use active voice ("Added X" → "X is now available")
- Group related changes into single bullets when they form a coherent feature
- Skip trivial fixes (typos, minor refactors) unless they fix user-visible bugs
- No markdown formatting inside bullets — plain text only
- Skip the ### Community section — contributors are handled separately
- Output ONLY the bullet points, one per line, prefixed with "- "

Changelog entries:
PROMPTEOF
)

NOTES=$(echo "$SECTION" | claude -p "$PROMPT

$SECTION" 2>/dev/null)

if [ -z "$NOTES" ]; then
  echo "ERROR: Claude CLI returned empty output. Is 'claude' installed and authenticated?"
  exit 1
fi

echo ""
echo "==> Generated release notes for v$VERSION:"
echo "────────────────────────────────────────"
echo "$NOTES"
if [ -n "$COMMUNITY_LINES" ]; then
  echo ""
  echo "Community:"
  echo "$COMMUNITY_LINES"
fi
echo "────────────────────────────────────────"
echo ""

# Interactive approval
read -rp "Accept these notes? [Y]es / [e]dit / [r]egenerate / [q]uit: " choice
choice=$(echo "$choice" | tr '[:upper:]' '[:lower:]')
case "$choice" in
  e|edit)
    TMPFILE=$(mktemp /tmp/release-notes-XXXXXX.txt)
    echo "$NOTES" > "$TMPFILE"
    ${EDITOR:-vim} "$TMPFILE"
    NOTES=$(cat "$TMPFILE")
    rm -f "$TMPFILE"
    echo ""
    echo "==> Edited notes:"
    echo "$NOTES"
    echo ""
    ;;
  r|regenerate)
    echo "==> Regenerating..."
    exec "$0" "$VERSION"
    ;;
  q|quit)
    echo "Aborted."
    exit 1
    ;;
  *)
    # Accept as-is (Y or Enter)
    ;;
esac

# Convert bullet lines to JSON array
HIGHLIGHTS=$(echo "$NOTES" | grep '^- ' | sed 's/^- //' | python3 -c "
import sys, json
lines = [line.strip() for line in sys.stdin if line.strip()]
print(json.dumps(lines))
")

# Parse community lines into contributions JSON array [{text, author}]
CONTRIBUTIONS_JSON=$(echo "$COMMUNITY_LINES" | python3 -c "
import sys, json, re
contributions = []
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    # Parse: - [@user](url) — description (#PR)
    m = re.match(r'^- \[@([a-zA-Z0-9_-]+)\].*? — (.+)$', line)
    if m:
        author, desc = m.group(1), re.sub(r'\s*\(#\d+\)\s*$', '', m.group(2)).strip()
        contributions.append({'text': desc, 'author': author})
print(json.dumps(contributions))
")

# Merge into existing release-notes.json
if [ ! -f "$OUTPUT" ] || [ ! -s "$OUTPUT" ]; then
  echo "{}" > "$OUTPUT"
fi

python3 -c "
import json, sys
with open('$OUTPUT', 'r') as f:
    data = json.load(f)
entry = {
    'highlights': json.loads(sys.argv[1]),
    'date': '$TODAY'
}
contributions = json.loads(sys.argv[2])
if contributions:
    entry['contributions'] = contributions
data['$VERSION'] = entry
with open('$OUTPUT', 'w') as f:
    json.dump(data, f, indent=2, ensure_ascii=False)
    f.write('\n')
" "$HIGHLIGHTS" "$CONTRIBUTIONS_JSON"

echo "==> Written to $OUTPUT"
echo "==> Done. Don't forget to commit the updated release notes."
