#!/usr/bin/env bash
# Story 2791-94c7: the gate must stay quiet for edits that do not change detection
# (formatting, reordering, plain reads of awaiting_input) and keep blocking real changes.
set -euo pipefail
# These snippets exercise the fixture gate only; formatting has its own test.
export TUIC_SKIP_FORMAT_GATE=1

project_root="$(git rev-parse --show-toplevel)"
test_tmp="${TUIC_TEST_TMP_ROOT:-$project_root/.tmp/tuic-tests}"
mkdir -p "$test_tmp"
scratch="$(mktemp -d "$test_tmp/fixture-gate-scope.XXXXXX")"
trap 'rm -rf "$scratch"' EXIT

repo="$scratch/repo"
git -C "$scratch" init -q repo
mkdir -p "$repo/src-tauri/src"

commit_all() {
  git -C "$repo" add -A
  git -C "$repo" -c user.name=Test -c user.email=test@example.com commit -qm "$1"
}
gate() { (cd "$repo" && bash "$project_root/scripts/hooks/pre-commit") > "$scratch/out" 2>&1; }
expect_pass() { gate || { echo "gate blocked: $1" >&2; cat "$scratch/out" >&2; exit 1; }; }
expect_block() {
  if gate; then echo "gate passed: $1" >&2; exit 1; fi
  grep -Fq 'detection changed with no capture' "$scratch/out"
}

cat > "$repo/src-tauri/src/pty.rs" <<'RS'
fn quiet(session: &Session) -> bool { session.awaiting_input && session.idle_for_long_enough_to_matter(threshold) }
fn other() {}
fn another() {}
RS
cat > "$repo/src-tauri/src/state.rs" <<'RS'
fn schedule_push(s: &Session) {
    notify(s);
}
RS
commit_all baseline

# 2. rustfmt reflow of a line that names a detection symbol is not a behaviour change.
cat > "$repo/src-tauri/src/pty.rs" <<'RS'
fn quiet(session: &Session) -> bool {
    session.awaiting_input
        && session.idle_for_long_enough_to_matter(threshold)
}
fn other() {}
fn another() {}
RS
git -C "$repo" add -A
expect_pass 'reflow of a detection line'
commit_all reflow

# 3. A pure reorder of functions that mention detection symbols is not a behaviour change.
cat > "$repo/src-tauri/src/pty.rs" <<'RS'
fn other() {}
fn another() {}
fn quiet(session: &Session) -> bool {
    session.awaiting_input
        && session.idle_for_long_enough_to_matter(threshold)
}
RS
git -C "$repo" add -A
expect_pass 'reorder of functions'
commit_all reorder-fns

# 4. Push scheduling that only READS awaiting_input is a consumer, not detection.
cat > "$repo/src-tauri/src/state.rs" <<'RS'
fn schedule_push(s: &Session) {
    if s.awaiting_input {
        notify(s);
    }
}
RS
git -C "$repo" add -A
expect_pass 'read of awaiting_input'
commit_all read

# 5. Writing awaiting_input is a state transition and still needs a capture.
cat > "$repo/src-tauri/src/state.rs" <<'RS'
fn schedule_push(s: &mut Session) {
    if s.awaiting_input {
        notify(s);
    }
    s.awaiting_input = false;
}
RS
git -C "$repo" add -A
expect_block 'write of awaiting_input'
git -C "$repo" restore --staged .
git -C "$repo" checkout -- .

# 6. Reflowing a write while changing its value is a real change and still blocks.
cat > "$repo/src-tauri/src/pty.rs" <<'RS'
fn clear(session: &mut Session) { session.awaiting_input = false; }
RS
commit_all write-baseline
cat > "$repo/src-tauri/src/pty.rs" <<'RS'
fn clear(session: &mut Session) {
    session.awaiting_input = true;
}
RS
git -C "$repo" add -A
expect_block 'changed value on a reflowed detection write'

# 7. Moving a write out of its condition keeps every line and changes behaviour: block.
cat > "$repo/src-tauri/src/state.rs" <<'RS'
fn finish(s: &mut Session) {
    if turn_done {
        s.awaiting_input = false;
    }
    notify(s);
}
RS
commit_all move-baseline
cat > "$repo/src-tauri/src/state.rs" <<'RS'
fn finish(s: &mut Session) {
    if turn_done {
    }
    s.awaiting_input = false;
    notify(s);
}
RS
git -C "$repo" add -A
expect_block 'detection write moved out of its condition'
git -C "$repo" restore --staged .
git -C "$repo" checkout -- .

# 8. Swapping two statements is a pure move of one line and changes ordering: block.
cat > "$repo/src-tauri/src/state.rs" <<'RS'
fn finish(s: &mut Session) {
    s.awaiting_input = false;
    notify(s);
}
RS
commit_all swap-baseline
cat > "$repo/src-tauri/src/state.rs" <<'RS'
fn finish(s: &mut Session) {
    notify(s);
    s.awaiting_input = false;
}
RS
git -C "$repo" add -A
expect_block 'detection write reordered after notify'

git -C "$repo" restore --staged .
git -C "$repo" checkout -- .

# 9. A compound assignment or a mutating call is a write, not a read.
cat > "$repo/src-tauri/src/state.rs" <<'RS'
fn finish(s: &mut Session) {
    notify(s);
}
RS
commit_all compound-baseline
cat > "$repo/src-tauri/src/state.rs" <<'RS'
fn finish(s: &mut Session) {
    notify(s);
    s.awaiting_input |= done;
}
RS
git -C "$repo" add -A
expect_block 'compound assignment to awaiting_input'
git -C "$repo" restore --staged .
git -C "$repo" checkout -- .
cat > "$repo/src-tauri/src/state.rs" <<'RS'
fn finish(s: &mut Session) {
    notify(s);
    let old = s.question_text.take();
}
RS
git -C "$repo" add -A
expect_block 'take() on question_text'
git -C "$repo" restore --staged .
git -C "$repo" checkout -- .

# 10. Whitespace inside a string literal or a regex quantifier is content, not formatting.
cat > "$repo/src-tauri/src/pty.rs" <<'RS'
fn clear(s: &mut Session) { s.awaiting_input = matches("Enter to select", "a{1,}"); }
RS
commit_all literal-baseline
cat > "$repo/src-tauri/src/pty.rs" <<'RS'
fn clear(s: &mut Session) { s.awaiting_input = matches("Enter  to select", "a{1,}"); }
RS
git -C "$repo" add -A
expect_block 'whitespace changed inside a string literal'
git -C "$repo" restore --staged .
git -C "$repo" checkout -- .
cat > "$repo/src-tauri/src/pty.rs" <<'RS'
fn clear(s: &mut Session) { s.awaiting_input = matches("Enter to select", "a{1}"); }
RS
git -C "$repo" add -A
expect_block 'regex quantifier changed inside a string literal'
git -C "$repo" restore --staged .
git -C "$repo" checkout -- .

# 11. Story 2801-8077: a change inside an item that follows a detection fn belongs to
# that item, not to the fn before it. A new enum variant is not detection logic.
parser="$repo/src-tauri/crates/tuic-terminal/src/output_parser.rs"
mkdir -p "$(dirname "$parser")"
cat > "$parser" <<'RS'
pub fn parse_provider_recovery(payload: &str) -> Option<ParsedEvent> {
    decode(payload)
}

pub enum ParsedEvent {
    Recovered { reason: Option<String> },
    RateLimit { pattern_name: String },
}
RS
commit_all enum-baseline
cat > "$parser" <<'RS'
pub fn parse_provider_recovery(payload: &str) -> Option<ParsedEvent> {
    decode(payload)
}

pub enum ParsedEvent {
    Recovered { reason: Option<String> },
    ToolActivity { name: Option<String> },
    RateLimit { pattern_name: String },
}
RS
git -C "$repo" add -A
expect_pass 'new enum variant after a detection fn'
git -C "$repo" restore --staged .
git -C "$repo" checkout -- .

# 12. A changed line in the body of that detection fn still blocks.
cat > "$parser" <<'RS'
pub fn parse_provider_recovery(payload: &str) -> Option<ParsedEvent> {
    decode(payload.trim())
}

pub enum ParsedEvent {
    Recovered { reason: Option<String> },
    RateLimit { pattern_name: String },
}
RS
git -C "$repo" add -A
expect_block 'body of parse_provider_recovery changed'

echo 'fixture gate scope regressions passed'
