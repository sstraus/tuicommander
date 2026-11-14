# Agent State Detection Rules

These rules moved here from `AGENTS.md` to keep that file inside the
32,768-byte project-instruction budget. They stay binding for every agent;
`AGENTS.md` keeps a summary and a link to this page.

## The bottom zone is not agent output — never parse it

Below an agent's input box sits a status line **the user configures**: a Claude
Code `statusLine` command, a HUD plugin, a shell theme. Its height, glyphs and
wording are arbitrary, differ per install, and it may be absent entirely.

```
  ✻ Simmering… (5m 48s · ↓ 20.7k tokens)      ← agent output. Parse this.
  ─────────────────────────────────────────
  ❯                                           ← input box (2 rows)
  ─────────────────────────────────────────
  [Opus 5 (1M) | Team] ██░░ 22% | 📚 8        ← user's status line, ANY height.
  5h: 0% | 7d: 2% | $15.48 | 📅 $136.41         Ignore all of it.
  ◐ Bash: cargo test | ✓ Bash ×14
  ⏵⏵ bypass permissions on (shift+tab)        ← agent chrome. Also ignore.
```

**Rule: nothing at or below the input box may reach a parser.** Whatever is down
there is coincidence — a path reads as a plan file, a `?` as a question, a
numbered list as a choice prompt, `$15.48` as a token count. The agent's own
spinner sits *above* the input box, so trimming costs no signal.

Enforced by `chrome::find_chrome_cutoff`, applied to changed rows in `pty.rs`
before `parse_clean_lines`. It anchors on the input box and extends upward past
its padding. The unwindowed fallback accepts either a strict empty prompt or a
separator followed within four rows by a prompt; the latter preserves a draft
in a non-empty input box above an arbitrarily tall HUD without treating a lone
separator or markdown quote as chrome.

**When you touch that cutoff, the failure mode to fear is failing open:** no
anchor found returns `None`, and `None` means no trim, so *every* status-line row
reaches *every* parser. That is exactly what happened with a status line taller
than `CHROME_SCAN_ROWS` — silent and total. Hence the unwindowed fallback to the
lowest empty prompt row (`lowest_input_box_row`). Never widen the loose
`is_prompt_line` search: unwindowed it matches a markdown blockquote.

**Deliberate exceptions** — these read the full screen on purpose:

| Site | Why |
|---|---|
| `parse_slash_menu` | Claude Code v2.1+ renders autocomplete items *below* the prompt chrome |
| `parse_choice_prompt` | scans bottom-up for a strict dialog shape (title + ≥2 numbered options) |
| question dedup screen-absence check (`pty.rs`) | asks "is this prompt still visible anywhere", not "is this content" |

## Agent state detection — capture before you theorise

Working / idle / awaiting is decided from bytes an agent writes **once**, and
nothing retains them in the shape the decision saw. The ring holds 2 MB
(`OUTPUT_RING_BUFFER_CAPACITY`) and the VT log 10.000 lines
(`VT_LOG_BUFFER_CAPACITY`), so a busy agent still rolls past both, and what
survives is rendered rows rather than the chunk boundaries and timing the
detector actually read. Do not reason about the code first — record the stream,
then replay it.

(The 8192 that looks like a ring size is the default byte `limit` of
`GET /sessions/{id}/output`, `mcp_http/session.rs:460` — a page size, not a
retention bound.)

```bash
curl -X POST localhost:9876/diagnostics/capture -H 'content-type: application/json' \
     -d '{"enabled":true}'                      # every session
     -d '{"enabled":true,"session_id":"<id>"}'  # one session
curl localhost:9876/diagnostics/capture         # state + bytes written per session
```

Captures land in `<config dir>/captures/<session-id>.tcap`, capped at 512 KB each. TUICCAP2 preserves the initial terminal rows/columns plus output/input direction, original chunk boundaries, ordering, and monotonic timestamps. The decoder remains backward-compatible with geometry-less TUICCAP1 and legacy output-only `.raw` fixtures; a faithful replay of either old format must supply the observed geometry explicitly rather than silently assuming 41x128.
Off by default (one relaxed atomic load per chunk when off) — code in
`src-tauri/src/pty_capture.rs`.

**A reproduced failure becomes a fixture, always.** Drop the `.tcap` in
`src-tauri/src/fixtures/agent_prompts/` and add a case to the
`Awaiting-signal fixtures` block in `pty/tests.rs`: it replays the capture
through `raw_stream_events` + `parse_clean_lines` + `suppress_heuristic_question`
— the same composition production runs, shared on purpose so a test can never
assert against a pipeline that does not exist. Unit tests on the individual
parsers were never the gap; the pipeline around them was.

**That rule is mechanical, not honour-system.** `scripts/hooks/pre-commit`
(installed by `make hooks` / `make dev`) blocks a commit that changes detection
logic without staging anything under `src-tauri/src/fixtures/agent_prompts/`.
It is deliberately narrow — only added/removed lines count, comments and blank
lines are stripped, and outside `chrome.rs` (detection end to end) a detection
symbol must be named by a changed line or by the hunk's enclosing function.
Touching `pty.rs` is not the trigger; touching `awaiting_input`, or any line
inside `suppress_heuristic_question`, is. Replayed over the last 120 commits
that touch a gated file it fired on 24 — every `fix(agent-state):` among them.
The same hook refuses a staged `.tcap` whose replay-oracle capture or
`corpus.jsonl` entry is missing; `scripts/replay-oracle.sh regenerate` writes them.

The gate ignores formatting-only hunks (rustfmt reflow, trailing commas), pure
block moves, and plain reads of `.awaiting_input` / `.question_confident` /
`.question_text` (consumers such as push scheduling). If it still fires, the edit
probably does change detection: record the capture.

`TUIC_SKIP_FIXTURE_GATE=1` is allowed only when no capture can apply: a rename
or refactor whose behaviour the existing fixtures already replay unchanged.
Never for a fix, and never to save time. Put the reason in the commit message
as a trailer, because the env var leaves no trace:

```
Fixture-Gate-Skip: <why no detection behaviour changed>
TUIC_SKIP_FIXTURE_GATE=1 git commit ...     # never --no-verify: it also skips the Makefile gate
```

The orchestrator landing check will run `scripts/hooks/report-fixture-gate-skips.sh <range>`
and report every commit that carries the trailer (orchestrator story pending; it does not run yet).

**Three signals report awaiting, and they are not interchangeable:**

| Signal | Source | Applies to |
|---|---|---|
| OSC 7770 `state=awaiting` | TUIC hook | hook-instrumented agents, **only** on `PreToolUse(AskUserQuestion)` |
| OSC 777 `notify` | agent's own desktop notification | unambiguous `needs your permission` / `approval required` wording only; Claude's generic `is waiting for your input` also follows an ordinary completed turn and never sets awaiting |
| `Enter to select` footer | rendered screen | Ink dialogs, including hook-instrumented sessions through the full-screen presence recovery; the changed-row parser's heuristic copy is suppressed for hooked agents |

Busy/idle evidence is ranked within one submitted-turn epoch. Lower-ranked
evidence never closes a turn held busy by a protocol signal, and the same rule
protects protocol-ranked awaiting state from the `question-cleared` screen
backstop. After a foreground agent delivers native busy, only that producer decides
busy/idle; screens, timers, shell OSC 133 and descendants cannot override it.
Configuration alone does not select this rule. Agents without a delivered native
busy keep the screen/process/silence fallback; a completion-only notifier does
not establish a complete turn source. Foreground identity withdrawal
retires native authority. Ego includes its owned children and background execs in
its existing busy signal; no additional wire state is needed.

| Rank | What it knows | Recorded by |
|---|---|---|
| `Silence` | nothing moved for a while | the silence timer |
| `Screen` | what the rendered screen currently looks like | ready/working screen adapters, **and OSC 133** — see below |
| `Process` | the process itself changed | `protocol-stale` only, today (#771-4733) |
| `Protocol` | this turn began or ended | OSC 7770 `state=`, Codex `notify` turn-complete, a submitted line on a ready-adapter agent |

**Rank is about what a signal knows, not how it travelled. Arriving in an escape
sequence does not make something Protocol rank.** OSC 133 is the worked example
and the mistake to not repeat: it is *shell* integration, so `133;C` fires when a
foreground command starts and `133;D` when it exits — on a long-lived TUI agent,
once at launch and once at death. It cannot tell one turn from the next, so it
records at `Screen` rank and a stable Ready screen is allowed to close it.
Ranking it `Protocol` strands the tab BUSY for the agent's whole lifetime, which
is issue #535-d4f5.

That distinction is easy to lose because `SilenceState::explicit_busy()` accepts
`osc133-busy` alongside `hook-busy`. It is a **provenance** predicate — an
explicit marker set this, rather than inferred screen/activity — and deliberately
*not* a rank predicate; its sources do not share a rank. Anything deciding
whether evidence may hold a turn reads `evidence.busy.rank`. Reading
`explicit_busy()` instead is exactly how a past commit came to widen the
`note_ready_screen` guard and then invert one of three byte-identical tests to
match (#745-8ff1). A `SilenceState` carries no agent type, so the three
`*_recovers_long_lived_shell_busy` tests must always agree; if one of them is
red, making the trio disagree is never the fix.

The footer regex anchors at **column 0 of the rendered row**, never the trimmed
text (`is_ink_dialog_footer_row`). A dialog is drawn full-bleed; everything an
agent streams is indented inside its own frame, so the indentation is the whole
difference between the footer and an agent quoting it. Trim first and an agent
that pastes a screen it just read marks *itself* awaiting, confidently, with
nothing to retract it.

A hook-instrumented agent showing a picker that is *not* AskUserQuestion (plan
pickers, skill menus, anything with `Type something` / `Chat about this`) uses
the open Ink footer's full-screen presence recovery. The generic OSC 777
notification is insufficient evidence: it also arrives after normal prose at
the ready composer. Unambiguous permission notifications remain raw-stream
signals because the VT parser consumes escape sequences before clean-row parsing.

**Uncertain ego injection recovery** uses the native empty-composer adapter, not
silence alone. Only a post-uncertainty repaint followed by 1.5 seconds of stable
Ready may clear the uncertainty flag. Retained or wrapped drafts and modal rows
fail closed. The foreground probe still gates idle; recovery never requeues the
uncertain payload, but normal idle flush can deliver the next queued BG wake.
The adapter shapes are recorded in `ego-composer-1957*.tcap`.

Ego selector recovery accepts both single-answer and checkbox footers, including mid-word terminal wrapping. The complete footer must be the last nonblank content: answered selector history above newer output or a composer must never re-arm awaiting. Checkbox state uses `toggle-enter` and changes only on a toggle; Enter or Escape settles the dialog. Free-text modal text enriches an existing confident ego wait, never a ready composer. The mobile header question action supports all three forms.

Ego empty-title native awaiting markers are lifecycle events, not screen text: never deduplicate them through selector repaint state. MCP approval titles can contain explicit CRLF continuation rows before `Risk:`; preserve the complete contiguous `Allow …?` title and the exact native controls.

Ego selector and free-text titles share diagnostic, working-row and submitted-composer boundaries. Default-mode approvals require the native Allow/Risk/y-a-n-e-d shape; n replaces choices with the refusal explanation prompt, and Enter settles it. Approval footer height is variable: read all rows after the last Risk anchor, require ordered native keys and the final details control, and reject retained approvals above newer output (`ego-wrapped-approval-2102.tcap`).

Codex native question parsing targets installed 0.162.1: `⌃]` / `⇧→` footer, Shift+Left to open, immediate numeric answers. The queued-panel heading is required; imperative titles are accepted only behind that exact panel shape. Enforced command approvals instead require their exact confirm/cancel footer and command question title; numeric refusal submits immediately. While the same native dialog and its unanswered controls remain active, a delayed idle hook cannot retract the approval or a /plan question. Answer input or a departed dialog permits normal completion. The distinct `/plan` question requires its Question counter and exact tab/enter/esc footer, either one wide row or three narrow rows. Narrow answer and description continuations stay in their native columns; second-column descriptions become hints, not answer labels (`codex-first-plan-narrow-2094.tcap`).

Grok native radio forms require a strict framed Ask or enforced-approval shape. Ask ends with z and a waiting header; enforced approvals carry title, command and scope controls. Numeric keys answer immediately. Selecting z retires choice buttons before publishing a text-only wait.

**Every signal that sets awaiting needs a path that clears it.** The badge is
`SessionState.awaiting_input`, not an event, and it is sticky by construction —
whatever sets it owns nothing until something retracts it. These paths clear it:

| Clear | Fires on | Misses when |
|---|---|---|
| `user-input` | a non-empty typed line | the answer is a bare Enter |
| `status-line` | a parsed busy tick (low-confidence only) | busy is inferred from screen movement |
| `resolve_choice_prompt_input` | an option keypress | no `choice_prompt` was ever set |
| `question-cleared` | silence timer sees the question gone from the screen | — (the backstop; low-confidence only) |
| `protocol-question-cleared` | Claude renders `User declined to answer questions` after Esc and returns to a ready composer; or an accepted foreground observation revokes the agent on confirmed shell return | cancellation requires the matching dialog; exit withdrawal is turn-epoch scoped and does not fire for unknown helpers, stale samples or an unobserved startup preset |
| `progress-superseded` | the same PTY reports `progress done`, journals a `delegated` hand-off, or successfully receives a queued user reply after a `progress blocked` | a dialog or any other confident question replaced the blocked one — only `source=progress-blocked` evidence clears |

`question-cleared` is the backstop that catches the rest. It never touches a
confident question: grok repaints while it waits, so "not on screen this tick"
is not proof of an answer.

**Dialog recovery requires an active agent identity.** After foreground observation returns to the shell, a retained Ink footer or question title above its prompt is history, not a live dialog. Both full-screen recovery and silence-timer chat heuristics require that identity. Never re-arm awaiting from shell output just because that footer remains visible (`claude-question-exit-2091.tcap`).

**The mirror failure is a SET that never comes back.** A multi-question
`AskUserQuestion` answers one sub-question at a time; each repaints its title and
options while the `Enter to select` footer stays byte-identical. The changed-rows
parser needs a row to *change*, so sub-questions 2+ produce no signal at all and
the tab reads "working" while the agent waits. `rearm_awaiting_for_open_dialog`
(`pty.rs`) closes it by reading that footer off the **full screen** as a presence
level, not an edge, and re-arming only when the badge is off — one event per
spurious clear, never one per repaint. Do not extend it to parse the title,
options or the `⊠ … ✓ Submit` tab bar: those all move as the wizard advances,
which is precisely why the footer is the key.

**Legacy output-only `.raw` fixtures cannot reproduce a latched badge.** New
`.tcap` captures include user input and can replay SET/CLEAR ordering, but the
`Awaiting RETRACTION` block must still drive the real event-bus accumulator and
assert `SessionState` — the thing a tab actually renders.

**Do not exclude hook-instrumented sessions from dialog recovery.** A retained
PTY capture on 2026-09-21 showed `state=awaiting`, then fifteen `state=busy`
markers, then an open dialog while the session reported working. Recovery must
use the last pending Question/UserInput event in a chunk, not the presence of
any Question: a later clear supersedes it. Keep the column-0 footer anchor and
the guards against an existing badge, choice prompt, or pending question.
