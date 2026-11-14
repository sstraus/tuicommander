# Chat wrapping, recall, and empty plans (2026-10-31)

## Browser evidence

`ChatWrapGeometry.test.ts` mounts the real shared `Transcript` in isolated Vite,
then measures native Chrome line boxes at 420px and 960px reading widths.
Set `TUIC_CHAT_WRAP_DIR` to retain JSON and screenshots; set
`TUIC_CHAT_WRAP_PHASE=before` for the pre-fix capture.

On base `664db9ace3`, a long single-line user prompt and a 400-character token
both wrap inside the bubble. Deliberate user newlines remain intact. The
reported user-bubble clipping is not reproduced by these cases. No prompt text
or prompt CSS is changed speculatively. The Claude adapter reads structured
JSONL messages, not PTY screen rows, so the view does not inherit terminal-column
line breaks from the grid. Authored newlines remain possible in the source.

The short inline token `--session` does reproduce the screenshot's split:
it occupies two line boxes. The shared Markdown `word-break: break-word` rule
allows splitting an inline code element at the line edge. Terminal Chat now
uses an atomic inline code box; a token wider than the column can still wrap
inside its box. Fenced code retains its existing scrolling behavior.

The terminal reading layout first appeared in `9726b02ae3`, already using the
shared Markdown rule. That rule was present in the extracted stylesheet in
`2548cef30c`; no previously working Terminal Chat version was found. This is
an inherited limitation, not a demonstrated regression from a working version.

## Queue ownership and recall coverage

| Surface | Queue owner | ArrowUp recall |
|---|---|---|
| AI Chat over ACP, including conversation-specific launches | TUIC connection actor | Latest text-only queued turn, including another client's submission. Full text comes from actor-owned content, never the 200-character preview. |
| Desktop terminal Compose, including terminal Chat, for any detected agent | TUIC PTY idle-gate FIFO | Latest `user_command` still in the FIFO. Notices and initial prompts are not recalled. Successful removal must win against the flush's locked pop. |
| Muted Pending transcript bubble after ordinary terminal Send (Claude, Codex, Grok, ego) | Native CLI | Not recalled. The bubble is a local receipt, not a TUIC queue entry or cancellation token. |
| ACP prompt containing images or resource links | TUIC connection actor | Refused without removing it. A lossless multimodal draft restore contract is not implemented. |
| Mobile terminal CommandInput | TUIC PTY FIFO / native CLI | Existing mobile queue controls remain; this desktop Compose change adds no mobile terminal ArrowUp binding. Mobile ACP uses the shared AI Chat composer. |

Unmodified ArrowUp applies only with an empty composer and the caret at the
start, outside slash-command navigation. Existing text and modified arrow keys
retain their native behavior. Recalled text is restored only after withdrawal;
edits made while waiting are preserved, and ACP tab switches keep the result in
the source conversation's draft. A dispatched, missing, or steering-in-flight
ACP turn is not restored as though it had been withdrawn.

Installed Claude Code `2.1.314` contains the native hints “Press up to select a
queued message, then Enter to edit it” and “Press up to edit queued messages”.
Its shipped code wires `popQueuedIntoDraft` into the cancellation/input control.
This proves native queue editing exists, but also shows that the key sequence
can depend on native selection mode. An isolated live probe was attempted with a local mock API endpoint, a fresh
config under `.tmp`, and the real `sendCommand` helper. It reached native theme,
custom-key, security, and workspace-trust onboarding, but did not establish a
busy queue. Probe 3 produced no completed result; the coordinator stopped further
probing. No live busy-Claude queue fixture was recorded here. No Up/Esc injection was added: without an acknowledgment bound
to the exact queued message, a PTY key may navigate history, select a different
message, or interrupt work. These limitations are explicit rather than a fake
bubble removal.

## Empty ACP plans

`ego-empty-plan-2558.json` records installed ego 0.4.0's actual `session/new`
notification, with no paid prompt. It publishes `plan` with an empty `entries`
list. The existing projection treats plans as complete replacement snapshots:
an empty update clears the old steps. The renderer now omits the empty card and
continues to display later nonempty plans.

Unconditional plan cards date to the original renderer in `d28773245b`.
Targeted MDKB story/memory searches and Git commit-message searches found no
prior empty-plan fix. This is not evidence that Boss never reported it before;
it only states what the searchable record contains.
