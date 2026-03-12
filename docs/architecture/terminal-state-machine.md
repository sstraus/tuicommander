# Terminal State Machine

Definitive reference for terminal activity states, notifications, and question detection.

## State Variables

Each terminal has these reactive fields in `terminalsStore`:

| Field | Type | Default | Source of truth |
|-------|------|---------|-----------------|
| `shellState` | `"busy" \| "idle" \| null` | `null` | **Rust** (emitted as parsed event) |
| `awaitingInput` | `"question" \| "error" \| null` | `null` | Frontend (from parsed events) |
| `awaitingInputConfident` | `boolean` | `false` | Frontend (from Question event) |
| `activeSubTasks` | `number` | `0` | **Rust** (parsed + stored per session) |
| `debouncedBusy` | `boolean` | `false` | Frontend (derived from shellState with 2s hold) |
| `unseen` | `boolean` | `false` | Frontend (set by `fireCompletion`, cleared on tab focus) |
| `agentType` | `AgentType \| null` | `null` | Frontend (from agent detection) |

Rust-side per-session state:

| Field | Location | Purpose |
|-------|----------|---------|
| `SilenceState.last_output_at` | `pty.rs` | Timestamp of last **real** output (not mode-line ticks) |
| `SilenceState.last_status_line_at` | `pty.rs` | Timestamp of last spinner/status-line |
| `SilenceState.pending_question_line` | `pty.rs` | Candidate `?`-ending line for silence detection |
| `active_sub_tasks` | `AppState.session_states` | Sub-agent count per session |
| `shell_states` | `AppState.shell_states` | `DashMap<String, AtomicU8>`: 0=null, 1=busy, 2=idle. Transitions use `compare_exchange` to prevent duplicate events when reader thread and silence timer race. |
| `last_output_ms` | `AppState.last_output_ms` | Epoch ms of last **real** output (not chrome-only). Stamped only when `!chrome_only`. |

## 1. Tab Indicator — Visual Priority

The tab dot reflects the terminal's **highest-priority** active state:

```
Priority    State       Color       CSS var         Condition
────────    ─────       ─────       ───────         ─────────
   1        Error       red         --error         awaitingInput == "error"
   2        Question    orange      --attention     awaitingInput == "question"
   3        Busy        blue ●̣      --activity      debouncedBusy && !awaitingInput
   4        Unseen      purple      --unseen        unseen && !debouncedBusy && !awaitingInput
   5        Done        green       --success       shellState=="idle" && !unseen && !debouncedBusy && !awaitingInput
   6        Idle        gray        (default)       shellState==null or none of above
```

Error and Question have pulse animation. Busy has pulse animation. Unseen and Done are static.

### Complete state combination matrix

Every valid combination of the 4 key fields and the resulting indicator:

```
awaitingInput  debouncedBusy  unseen  shellState   → Indicator
─────────────  ─────────────  ──────  ──────────   ──────────
"error"        true           any     any          → Error (red)
"error"        false          any     any          → Error (red)
"question"     true           any     any          → Question (orange)
"question"     false          any     any          → Question (orange)
null           true           any     any          → Busy (blue pulse)
null           false          true    "idle"       → Unseen (purple)
null           false          true    null         → Unseen (purple)
null           false          false   "idle"       → Done (green)
null           false          false   "busy"       → (transient: cooldown pending)
null           false          false   null         → Idle (gray)
```

### Lifecycle of each indicator

```
                    ┌──────────────────────── Error (red) ◄─── API error / agent crash
                    │                              │
                    │   ┌──────────────────── Question (orange) ◄─── agent asks ?
                    │   │                          │
                    │   │    ┌─────────────── Busy (blue) ◄─── real output detected
                    │   │    │                     │
                    │   │    │    ┌────────── Unseen (purple) ◄─── completion fired,
                    │   │    │    │                │                user not watching
                    │   │    │    │    ┌───── Done (green) ◄─── user viewed unseen tab,
                    │   │    │    │    │            │              or short idle session
                    │   │    │    │    │    ┌─ Idle (gray) ◄─── no session / fresh
                    │   │    │    │    │    │
                    ▼   ▼    ▼    ▼    ▼    ▼
                 [ Higher priority wins when multiple states active ]
```

## 2. shellState — Derived in Rust

Rust is the single source of truth. The reader thread classifies every PTY chunk:

```
PTY chunk arrives in reader thread
         │
         ▼
    ┌─────────────────────────────┐
    │ Compute chrome_only:        │
    │  = no regex question found  │
    │  AND no ?-ending line       │
    │  AND all events are         │
    │    StatusLine or            │
    │    ActiveSubtasks           │
    └─────────┬───────────────────┘
              │
         chrome_only?
        ╱            ╲
      YES             NO
       │               │
       ▼               ▼
  Mode-line tick    Real output
  (timer, spinner)  (agent working)
       │               │
       │               ├── last_output_at = now
       │               │
       │               └── if shell_state ≠ busy:
       │                      emit ShellState { "busy" }
       │                      shell_state = busy
       │
       └── if shell_state == busy
           AND last_output_at > 500ms ago
           AND active_sub_tasks == 0
           AND not in resize grace:
              emit ShellState { "idle" }
              shell_state = idle
```

A **backup timer** (the existing silence timer, 1s interval) also checks:

```
Silence timer (every 1s)
         │
         ▼
    shell_state == busy?  ─── NO ──► skip
         │ YES
         ▼
    last_output_at > 500ms ago?  ─── NO ──► skip
         │ YES
         ▼
    active_sub_tasks == 0?  ─── NO ──► skip
         │ YES
         ▼
    emit ShellState { "idle" }
    shell_state = idle
```

This catches the case where NO chunks arrive at all (agent truly silent).

### Session end

When the reader thread loop breaks (`Ok(0)`), Rust emits `ShellState { "idle" }` before
stopping, ensuring the frontend sees the final transition. The frontend then receives the
exit callback and sets `sessionId = null`.

### Frontend consumption

```
pty-parsed event: ShellState { state }
         │
         ▼
terminalsStore.update(id, { shellState: state })
         │
         ▼
handleShellStateChange(prev, next)  ← existing debounced busy logic
```

The frontend does NOT derive shellState from raw PTY data. `handlePtyData` writes to
xterm and updates `lastDataAt` — but never touches `shellState`.

### Transition table

| From | To | Trigger | Condition |
|------|----|---------|-----------|
| `null` | `busy` | First real output chunk | — |
| `busy` | `idle` | Chrome-only chunk or silence timer | `last_output_at > 500ms` AND `active_sub_tasks == 0` AND not resize grace |
| `idle` | `busy` | Real output chunk | — |
| `busy` | `idle` | Session ends (reader thread exit) | Always (cleanup) |
| any | `null` | Terminal removed from store | cleanup |

### What does NOT cause transitions

| Event | Why it's ignored |
|-------|-----------------|
| Mode-line timer tick (`✻ Cogitated 3m 47s`) | Classified as `chrome_only` |
| Status-line update (`▶▶ ... 1 local agent`) | Classified as `chrome_only` |
| ActiveSubtasks event | Updates counter, doesn't produce real output |
| Resize redraw | Suppressed by resize grace (1s) |

## 3. debouncedBusy — Derived from shellState

Smoothed version with a 2-second hold to prevent flicker:

```
shellState events from Rust:

  busy ─────────── idle ──── busy ─────── idle ──────────────────
                     │         │            │
debouncedBusy:       │         │            │
  true ──────────────┼─────────┼── true ────┼── true ──┐ false ──
                     │         │            │          │
                     └── 2s ───┘            └── 2s ───┘
                     cooldown               cooldown
                     cancelled              expires
```

| Event | debouncedBusy effect |
|-------|---------------------|
| shellState → busy | Immediately `true`. Cancel any running cooldown. Record `busySince` (first time only). |
| shellState → idle | Start 2s cooldown. If cooldown expires: set `false`, fire `onBusyToIdle(id, duration)`. |
| shellState → busy during cooldown | Cancel cooldown. Stay `true`. Keep original `busySince`. |

`onBusyToIdle` fires exactly once per busy→idle cycle, after the 2s cooldown fully expires.

## 4. awaitingInput — Question and Error Detection

### State diagram

```
                          Question event
                          (passes all guards)
    ┌──────┐             ┌──────────┐
    │ null │────────────►│ question │
    └──┬───┘             └─────┬────┘
       │                       │
       │  ◄── clear triggers ──┘
       │      (see table below)
       │
       │  Error event
       │  (API error, agent crash)
       │                 ┌────────┐
       └────────────────►│ error  │
                         └───┬────┘
                             │
                    ◄── clear triggers ──┘
                        (see table below)
```

### Clear triggers

| Trigger | Clears "question"? | Clears "error"? | Why |
|---------|--------------------|-----------------|-----|
| StatusLine parsed event | **Yes** | **Yes** | Agent is working again (showing a task) |
| Progress parsed event | **Yes** | **Yes** | Agent is making progress |
| UserInput parsed event | **Yes** | **Yes** | User responded to the prompt |
| Process exit | **Yes** | **Yes** | Session over |

### What does NOT clear awaitingInput

| Event | Why it doesn't clear |
|-------|---------------------|
| shellState idle → busy | **Not a reliable signal.** Mode-line ticks cause false idle→busy transitions that would destroy valid question state. Replaced by the explicit triggers above. |
| Mode-line tick | Chrome-only output, not agent activity |
| activeSubTasks change | Sub-agent count changing doesn't mean the main question was answered |

### Notification sounds

Sounds play on **transitions into** a state, never on repeated sets or clearing:

```
getAwaitingInputSound(prev, current):

  prev      current     sound
  ────      ───────     ─────
  null   →  question →  play "question"
  null   →  error    →  play "error"
  *      →  same     →  null (no sound)
  *      →  null     →  null (clearing, no sound)
  question→ error    →  play "error" (state changed)
  error  →  question →  play "question" (state changed)
```

## 5. unseen — Completion Visibility Tracking

`unseen` tracks whether the user has seen a completed task.

### Lifecycle

```
                                          ┌─────────┐
  fireCompletion() ──────────────────────►│ unseen  │
  (background tab, agent done)            │ = true  │
                                          └────┬────┘
                                               │
  User clicks/switches to this tab ───────────►│
  (setActive clears unseen)                    │
                                               ▼
                                          ┌─────────┐
                                          │ unseen  │
                                          │ = false │
                                          └─────────┘
```

### What sets unseen

Only ONE place: `App.tsx` `fireCompletion()` sets `unseen = true` (along with `activity = true`).

### What clears unseen

Only ONE place: `terminalsStore.setActive(id)` sets `unseen = false`.

### Tab color transitions for unseen

```
Agent working    Agent done     User switches    User switches
(background)     (background)   to other tab     to THIS tab
    │                │               │                │
    ▼                ▼               ▼                ▼
  Blue ●̣  ───►  Purple ● ────►  Purple ● ────►  Green ●
  (busy)       (unseen)        (stays unseen)   (done/idle)
```

## 6. activeSubTasks — Sub-agent Tracking

Parsed from the agent mode line by Rust `OutputParser`:

```
Mode line text                               Parsed count
──────────────────────────────────────────   ────────────
"▶▶ bypass permissions on · 1 local agent"  → 1
"▶▶ Reading files · 3 local agents"         → 3
"▶▶ bypass permissions on"                  → 0
(no mode line)                               → unchanged
```

Stored in **both** Rust (`AppState.active_sub_tasks`) and frontend (`terminalsStore`).

### Effects on other states

```
                    activeSubTasks
                    ┌─────────────────────────────────────────────┐
                    │                                             │
                    ▼                                             ▼
              > 0 (agents running)                    == 0 (no agents)
              ┌────────────────────┐                  ┌──────────────────┐
              │ shellState:        │                  │ shellState:      │
              │   stays busy       │                  │   normal rules   │
              │   (idle blocked)   │                  │   (500ms timer)  │
              │                    │                  │                  │
              │ Question guard:    │                  │ Question guard:  │
              │   low-confidence   │                  │   passes through │
              │   IGNORED          │                  │                  │
              │                    │                  │                  │
              │ Completion:        │                  │ Completion:      │
              │   SUPPRESSED       │                  │   normal rules   │
              └────────────────────┘                  └──────────────────┘
```

### Reset

| Event | Effect |
|-------|--------|
| `ActiveSubtasks { count: N }` parsed event | Set to N |
| `UserInput` parsed event | Reset to 0 (new agent cycle) |
| Process exit | Reset to 0 |

## 7. Completion Notification

Fires when an agent was busy for ≥5s then truly goes idle.

**Two independent paths** can trigger completion:

### Path 1: Session exit (Terminal.tsx)

```
Process exits → reader thread ends → exit callback fires
         │
         ├── terminal is active tab? → SKIP
         │
         └── play("completion")
             (does NOT set unseen — user may switch soon)
```

### Path 2: Busy-to-idle (App.tsx) — sets unseen

```
onBusyToIdle(id, durationMs)
         │
         ├── durationMs < 5s? ────────────────────── SKIP
         ├── terminal is active tab? ─────────────── SKIP
         │
         ├── agentType set? ── YES ─► defer 10s ──► fireCompletion()
         │                 NO ──────► fireCompletion()
         │
         ▼
    fireCompletion()
         │
         ├── terminal is active tab? ──── SKIP (user switched to it)
         ├── debouncedBusy still true? ── SKIP (went busy again)
         ├── terminal removed? ────────── SKIP
         ├── activeSubTasks > 0? ──────── SKIP (agents still running)
         ├── awaitingInput set? ────────── SKIP (question/error active)
         │
         ▼
    play("completion")
    set unseen = true
    → tab turns purple (Unseen)
    → when user views: tab turns green (Done)
```

### Sound deduplication

Both paths can fire for the same session. Path 1 fires immediately on exit. Path 2 fires
after cooldown + deferral. The notification manager handles dedup (cooldown between
identical sounds).

### Timing under the new architecture

```
t=0       Agent starts working (real output) → shellState: busy
t=0..T    Agent works. Mode-line ticks arrive but don't affect shellState.
t=T       Agent stops real output. Mode-line may continue.
t=T+0.5   Rust: last_output_at > 500ms, sub_tasks=0 → shellState: idle
           (If sub_tasks > 0: stays busy until they finish)
t=T+2.5   Cooldown expires → debouncedBusy: false → onBusyToIdle fires
t=T+2.5   duration = T seconds. If T ≥ 5s and agentType:
             → defer 10s → fireCompletion at t=T+12.5
t=T+12.5  fireCompletion checks all guards → play("completion"), unseen=true
```

## 8. Question Detection Pipeline

Two layers: **Rust detection** → **Frontend notification**.

### Rust: Two parallel detection strategies

```
┌─────────────────────────────────────────────────────────────────┐
│                    READER THREAD (per chunk)                     │
│                                                                 │
│  PTY data → parse_clean_lines(changed_rows) → events[]          │
│                                                                 │
│  ┌─ Strategy A: Regex (instant) ──────────────────────────────┐ │
│  │ parse_question() matches "Enter to select"                 │ │
│  │ → Question { confident: true }                             │ │
│  │ → emitted immediately in the events list                   │ │
│  └────────────────────────────────────────────────────────────┘ │
│                                                                 │
│  ┌─ Strategy B: Silence (delayed) ────────────────────────────┐ │
│  │ extract_question_line(changed_rows)                        │ │
│  │ → finds last line ending with '?' that passes              │ │
│  │   is_plausible_question() filter                           │ │
│  │ → stored as pending_question_line in SilenceState          │ │
│  │ → NOT emitted yet — waits for silence timer                │ │
│  └────────────────────────────────────────────────────────────┘ │
│                                                                 │
│  on_chunk() updates SilenceState:                               │
│    - regex fired? → clear pending, mark emitted                 │
│    - echo suppress window? → ignore '?' line                    │
│    - same line already emitted? → ignore (repaint)              │
│    - new '?' line? → set as pending candidate                   │
│    - real output after '?'? → increment staleness               │
│    - mode-line tick? → do nothing                               │
│    - staleness > 10? → clear pending (agent kept working)       │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                    SILENCE TIMER (every 1s)                      │
│                                                                 │
│  is_silent()?                                                   │
│    ├── question_already_emitted? → skip                         │
│    ├── is_spinner_active()? → skip (status-line < 10s ago)      │
│    └── last_output_at < 10s? → skip                             │
│                                                                 │
│  If silent (all three pass):                                    │
│                                                                 │
│  ┌─ Strategy 1: Screen-based ────────────────────────────────┐  │
│  │ Read VT screen → extract_last_chat_line()                 │  │
│  │ → find line above prompt (❯, ›, >)                        │  │
│  │ → ends with '?' AND is_plausible_question()?              │  │
│  │ → emit Question { confident: false }                      │  │
│  └───────────────────────────────────────────────────────────┘  │
│                                                                 │
│  ┌─ Strategy 2: Chunk-based fallback ────────────────────────┐  │
│  │ check_silence() → pending_question_line exists?           │  │
│  │ AND not stale (≤ 10 real-output chunks after)?            │  │
│  │ → verify_question_on_screen() (bottom 5 rows)            │  │
│  │ → emit Question { confident: false }                      │  │
│  └───────────────────────────────────────────────────────────┘  │
│                                                                 │
│  If neither strategy finds a question: continue sleeping.       │
└─────────────────────────────────────────────────────────────────┘
```

### SilenceState update rules

| Chunk type | last_output_at | last_status_line_at | staleness counter | pending_question_line |
|-----------|----------------|--------------------|--------------------|----------------------|
| Real output, no '?' | Reset to now | — | +1 (if pending exists) | Cleared if >10 |
| Real output with '?' | Reset to now | — | Reset to 0 | Set to new line |
| Real output + status-line | Reset to now | Reset to now | (per above rules) | (per above rules) |
| Mode-line tick only | **Not reset** | **Not reset** | **Not incremented** | **Not affected** |
| Regex question fired | Reset to now | — | Reset to 0 | Cleared (handled) |

### Frontend: event handler + notification

```
pty-parsed: Question { prompt_text, confident }
         │
         ▼
    ┌──────────────────────────────────────────────────┐
    │ Guard: low-confidence question while agent busy  │
    │                                                  │
    │ NOT confident                                    │
    │ AND (shellState == "busy"                        │
    │      OR activeSubTasks > 0)?                     │
    │                                                  │
    │ YES → IGNORE (likely false positive)             │
    │ NO  → continue                                   │
    └──────────────────┬───────────────────────────────┘
                       │
                       ▼
    setAwaitingInput(id, "question", confident)
                       │
                       ▼
    createEffect detects transition (null → "question")
                       │
                       ▼
    play("question")
```

## 9. Timing Constants

| Constant | Value | Location | Purpose |
|----------|-------|----------|---------|
| Shell idle threshold | 500ms | `pty.rs` (Rust) | Real output silence before idle |
| Debounce hold | 2s | `terminals.ts` | debouncedBusy hold after idle |
| Silence question threshold | 10s | `pty.rs` | Silence before '?' line → question |
| Silence check interval | 1s | `pty.rs` | Timer thread wake frequency |
| Stale question chunks | 10 | `pty.rs` | Real-output chunks before discarding '?' candidate |
| Resize grace | 1s | `pty.rs` | Suppress all events after resize |
| Echo suppress window | 500ms | `pty.rs` | Ignore PTY echo of user-typed '?' lines |
| Screen verify rows | 5 | `pty.rs` | Bottom N rows checked for screen verification |
| Completion threshold | 5s | `App.tsx` | Minimum busy duration for completion notification |
| Completion deferral | 10s | `App.tsx` | Extra wait for agent processes (sub-agents may still run) |

## 10. Scenarios

### A: Agent asks "Procedo?" — no sub-agents

```
t=0       Agent outputs "Procedo?" (real output)
          → shellState: busy
          → pending_question_line = "Procedo?"
          → last_output_at = now

t=0.5     No more real output. Rust idle check:
          last_output_at > 500ms, active_sub_tasks=0
          → shellState: idle  │  Tab: blue→(cooldown)
          → debouncedBusy cooldown starts (2s)

t=1-9     Mode-line ticks arrive (chrome_only=true)
          → shellState stays idle (Rust ignores chrome-only)
          → pending_question_line preserved

t=2.5     Cooldown expires → debouncedBusy: false
          → onBusyToIdle fires (duration ~0.5s < 5s → no completion)
          │  Tab: blue→green (Done)

t=10      Silence timer: is_silent()? YES
          → Strategy 1 or 2 finds "Procedo?"
          → emit Question { confident: false }
          → Frontend: guard passes (idle, subTasks=0)
          → awaitingInput = "question"
          → play("question") ✓
          │  Tab: green→orange (Question)

t=???     User types response → UserInput event
          → clearAwaitingInput
          │  Tab: orange→green (Done)
          → agent resumes → status-line → clearAwaitingInput (redundant, safe)
          │  Tab: green→blue (Busy)
```

### B: Agent asks "Procedo?" — sub-agents running

```
t=0       Agent outputs "Procedo?" while 2 sub-agents run
          → shellState: busy  │  Tab: blue
          → pending_question_line = "Procedo?"
          → active_sub_tasks = 2

t=0.5+    No more real output but active_sub_tasks > 0
          → shellState stays busy (Rust: idle blocked)

t=10      Silence timer: is_silent()? YES
          → emit Question { confident: false }
          → Frontend: activeSubTasks=2 > 0, NOT confident → IGNORED

t=60      Last sub-agent finishes → ActiveSubtasks { count: 0 }

t=60.5    Rust: last_output_at > 500ms, sub_tasks=0
          → shellState: idle  │  Tab: blue→(cooldown)

t=62.5    Cooldown expires → onBusyToIdle(duration=60s)
          → ≥ 5s, agentType set → defer 10s

t=72.5    fireCompletion()
          → activeSubTasks=0, awaitingInput=null
          → play("completion") ✓, unseen=true
          │  Tab: purple (Unseen)

          User switches to tab → unseen cleared
          │  Tab: purple→green (Done)
```

### C: Ink menu — "Enter to select"

```
t=0       Agent renders Ink menu with "Enter to select" footer
          → parse_question() regex match (INK_FOOTER_RE)
          → emit Question { confident: true } immediately
          → SilenceState: pending cleared, question_already_emitted = true

t=0       Frontend: confident=true → guard skipped (always passes)
          → awaitingInput = "question"
          → play("question") ✓
          │  Tab: orange (Question)

          No 10s wait needed — instant detection.
```

### D: False positive — agent discusses code with '?'

```
t=0       Agent outputs "// Should we use HashMap?"
          → is_plausible_question → false (starts with //)
          → NO candidate set

t=0       Agent outputs "Does this look right?"
          → is_plausible_question → true
          → pending_question_line = "Does this look right?"

t=0.1+    Agent continues with more real output (non-'?')
          → staleness +1, +2, ... +11 (> STALE_QUESTION_CHUNKS=10)
          → pending_question_line cleared

t=10+     Silence timer: pending is None → nothing emitted ✓
          │  No false notification
```

### E: Agent completes long task — no question

```
t=0       Agent starts working (real output)
          → shellState: busy  │  Tab: blue

t=120     Agent finishes, goes to prompt. No more real output.

t=120.5   Rust: last_output_at > 500ms, sub_tasks=0
          → shellState: idle  │  Tab: blue→(cooldown)

t=122.5   Cooldown expires → onBusyToIdle(duration=120s)
          → ≥ 5s, agentType set → defer 10s

t=132.5   fireCompletion()
          → all guards pass
          → play("completion") ✓, unseen=true
          │  Tab: purple (Unseen)

          User switches to tab → unseen cleared
          │  Tab: purple→green (Done)
```

### F: User watches terminal — active tab

```
t=0       Agent working in active tab (user watching)
          → shellState: busy  │  Tab: blue

t=60      Agent finishes → idle

t=62      onBusyToIdle fires
          → terminal IS active tab → SKIP
          → no sound, no unseen
          │  Tab: blue→green (Done) — user was watching
```

### G: Short command — under 5s

```
t=0       User runs `ls` → shellState: busy  │  Tab: blue

t=0.1     Output finishes → idle

t=2.1     Cooldown expires → onBusyToIdle(duration=0.1s)
          → duration < 5s → SKIP
          → no sound, no unseen
          │  Tab: blue→green (Done)
```

### H: Process exits in background tab

```
t=0       Agent working in background tab
          → shellState: busy  │  Tab: blue

t=60      Process exits → reader thread ends
          → Rust emits ShellState { "idle" }
          → Frontend exit callback:
            sessionId = null, clearAwaitingInput
            play("completion") [Path 1] ✓

t=62      Cooldown expires → onBusyToIdle(duration=60s)
          → fireCompletion [Path 2] → play("completion"), unseen=true
          │  Tab: purple (Unseen)

          User switches to tab → unseen cleared
          │  Tab: purple→green (Done)
```

### I: Rate-limit detected

```
t=0       Agent output matches rate-limit pattern
          → RateLimit parsed event emitted

t=0       Frontend handler:
          shellState == "busy"?
            YES → IGNORE (false positive from streaming code)
            NO  → agentType set, not recently detected?
              YES → play("warning") ✓, rateLimitStore updated
              NO  → SKIP (dedup)
```

### J: Resize during question display

```
t=0       "Procedo?" visible on screen, awaitingInput = "question"
          │  Tab: orange (Question)

t=X       User resizes terminal pane
          → resize_pty called → SilenceState.on_resize()
          → Shell redraws visible output (real PTY output)
          → Rust: shellState → busy (real output)

t=X       Resize grace active (1s):
          → All notification events SUPPRESSED (Question, RateLimit, ApiError)
          → "Procedo?" in redraw doesn't re-trigger question

t=X+1     Grace expires. awaitingInput still "question" (never cleared).
          │  Tab stays orange ✓
```

### K: Agent error (API error, stuck)

```
t=0       Agent output matches API error pattern
          → ApiError parsed event emitted

t=0       Frontend handler:
          → awaitingInput = "error"
          → play("error") ✓
          │  Tab: red (Error)

          User answers / agent retries → StatusLine event
          → clearAwaitingInput
          │  Tab: red→blue (Busy)
```

### L: Question then error (priority override)

```
t=0       Agent asks question → awaitingInput = "question"
          │  Tab: orange (Question)

t=5       API error while question is pending
          → awaitingInput = "error" (overrides question)
          → play("error") ✓
          │  Tab: orange→red (Error)

          Agent recovers → StatusLine event
          → clearAwaitingInput
          │  Tab: red→blue (Busy)
```

## 11. File Reference

| File | Responsibility |
|------|---------------|
| `src-tauri/src/pty.rs` | `SilenceState`, `spawn_silence_timer`, shellState derivation, `extract_question_line`, `verify_question_on_screen`, `extract_last_chat_line`, `spawn_reader_thread` |
| `src-tauri/src/output_parser.rs` | `parse_question` (INK_FOOTER_RE), `parse_active_subtasks`, `ParsedEvent` enum |
| `src-tauri/src/state.rs` | `AppState` (includes `shell_state`, `active_sub_tasks` maps) |
| `src/stores/terminals.ts` | `shellState`, `awaitingInput`, `debouncedBusy`, `handleShellStateChange`, `onBusyToIdle` |
| `src/components/Terminal/Terminal.tsx` | `handlePtyData` (xterm write), `pty-parsed` event handler, notification effect |
| `src/components/Terminal/awaitingInputSound.ts` | `getAwaitingInputSound` edge detection |
| `src/App.tsx` | `onBusyToIdle` → completion notification with deferral + guards |
| `src/stores/notifications.ts` | `play()`, `playQuestion()`, `playCompletion()` etc. |
| `src/components/TabBar/TabBar.tsx` | Tab indicator class priority logic |
| `src/components/TabBar/TabBar.module.css` | Indicator colors and animations |
