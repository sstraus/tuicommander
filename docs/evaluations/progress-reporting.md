# Progress reporting evaluation

> **Historical.** This measurement was taken against the first Progress design —
> four event kinds, workstreams, pause/resume and `repo`'s nine `progress_*`
> actions. The 2026-09-14 revision of `plans/project-progress.md` replaced all of
> that with one append-only journal, so the conditions below no longer describe
> any shipped surface. The finding that survived the rewrite and shaped it is the
> one about *where* the instruction lives: a tool description says what a tool
> does, never that you must call it, and 39 repositories recorded zero entries
> under the description-only condition. The obligation now sits in `initialize`
> and is imperative. The numbers are kept as a record, not as a spec.

This page records a measured comparison of the two ways to instrument an agent
for [Project Progress](../user-guide/project-progress.md):

- **Condition A — short default.** Only the shipped `progress` tool description
  and its JSON schema (`src-tauri/src/mcp_http/mcp_transport.rs:1138`). This is
  what every agent receives with no extra configuration.
- **Condition B — optional prompt.** The same tool description and schema, plus
  the optional instruction block from the user guide.

The two conditions differ only in that block.

## Method

| Item | Value |
|---|---|
| Date | 2026-09-13 |
| Model | Claude Sonnet 5 (`claude-sonnet-5`) |
| Settings | Default sampling. No tool execution. One turn for each condition. |
| Tool inventory | `progress`, `ui` (`toast`, `confirm`), `repo` with its nine `progress_*` actions |
| Scenarios | Seven, given in the same order to both conditions, as one continuous session on one project |
| Output | For each scenario: the calls, and a one-sentence reason |

The model declared its calls; it did not execute them. Therefore this
evaluation measures the reporting choice, not the transport. It is a small
qualitative sample, not production telemetry. TUICommander collects no telemetry
for this purpose.

The scenarios are: a capability achievement (S1), an architecture decision (S2),
a changed approach after a discovery (S3), a partial validation (S4), a
worker-only conflict (S5), a genuine project blocker (S6), and a completed
objective whose receipt reports `paused` (S7).

## Result

| Measure | A: short default | B: optional prompt |
|---|---|---|
| `progress` calls | 5 | 5 |
| Duplicate `ui.toast` calls for the same event | 2 | 0 |
| Extra discovery or history calls | 1 | 0 |
| Missed meaningful events (S1–S4, S6) | 0 | 0 |
| Irrelevant events reported (S5) | 0 | 0 |
| Claims of unverified scope | 0 | 0 |
| Uncertainty kept in the wording | S4 | S4 |

Both conditions found every meaningful event and neither reported the
worker-only conflict. The differences are in the type, in the noise, and in the
answer to a paused receipt.

### What each scenario showed

| Scenario | A: short default | B: optional prompt |
|---|---|---|
| S1 capability | `done` for a capability that was verified | `milestone`, with the verification named |
| S2 decision | `milestone` | `milestone`, and it says that nothing is built yet |
| S3 changed approach | `milestone` | `milestone` |
| S4 partial validation | `milestone`, Windows named as untested | `milestone`, Windows named as untested |
| S5 worker-only conflict | no call | no call |
| S6 project blocker | `blocked`, **and** a `ui.toast` for the same event | `blocked` only |
| S7 paused receipt | a `repo progress_status` lookup **and** a `ui.toast` that called the paused state an anomaly | no further call |

## Findings

1. **The short default is sufficient to find the right events.** With no extra
   prompt, the model reported all five meaningful outcomes and skipped the
   worker-only conflict. The `Started/done apply to objectives, not agent tasks`
   sentence in the tool description carries that distinction on its own.

2. **The short default over-scopes `done`.** In S1 it closed the whole
   workstream for one verified capability. `done` sets every active blocker of
   that workstream inactive (`src-tauri/src/progress/store.rs:1088`), and a
   later `milestone` keeps a done workstream done
   (`src-tauri/src/progress/store.rs:1082`). A premature `done` therefore hides
   a blocker and stops the workstream from moving again until a person sets its
   state. The optional prompt gives the correct `milestone`. This is the
   clearest reason to use the optional prompt.

3. **The short default duplicates the toast.** The tool description already says
   `Persists and shows a toast`, but in S6 and S7 the model added a `ui.toast`
   for the same event. The optional prompt is explicit and removed both.

4. **The short default does not know what a `paused` receipt means.** In S7 it
   treated `paused` as a contradiction, made a `progress_status` lookup, and
   raised a warning. It did not retry and it did not resume, so the failure is
   noise and not a policy breach. The optional prompt stopped after the receipt.

5. **Both conditions kept honest uncertainty.** Neither claimed Windows support
   in S4, and neither claimed completion of scope that was not verified. The
   `Outcome, not implementation.` description of `summary` is enough for that.

## Recommendation

Keep the short default as it is. It reports the right events at the lowest
instruction cost, and its three faults are noise rather than lost or false
history.

Add the optional prompt when the correct workstream state matters — that is,
when you use the Blockers and Completed views to steer work. It removed the
type error, the duplicate toasts, and the extra lookups in this sample.

The optional prompt stays out of the default payload for that reason: it buys
precision that not every project needs, at a cost that every project would pay.
