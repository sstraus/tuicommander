# Project Progress

Progress answers one question: **what happened in this terminal while I was not
watching?**

It is a journal. Entries are appended, never edited. You read it, and you delete
what you do not want to keep. There is nothing else to operate.

## What lands in the journal

Five kinds of entry.

| Kind | Written by | Meaning |
|---|---|---|
| `done` | the agent | The work an `intent:` announced is finished. |
| `blocked` | the agent | The agent stops and waits for you, because it cannot continue without you. |
| `intent` | TUICommander | The agent said what it was starting. Recorded from its `intent:` marker. |
| `delegated` | TUICommander | The agent started another agent with `agent action=spawn`. The text is the task it handed over. |
| `message` | TUICommander | The agent sent another agent a message with `agent action=send`. |

A `delegated` or `message` entry keeps its text redacted and cut to 500
characters. Status mail that TUICommander sends on an agent's behalf is not
recorded.

`intent` is the reliability floor. An agent's obligation to report sits in the
`initialize` blob it read hours ago, but its `intent:` marker fires at the start
of every task — so even an agent that never calls the tool leaves a trail of
what it set out to do. An agent cannot write an `intent` entry itself; the
reporting tool refuses that kind.

TUI agents can redraw an intent one word at a time. Progress waits for the
completed line or turn boundary before saving it, so a growing preview appears
as one entry rather than a series of partial entries.
The journal redacts secrets from intents and agent reports before writing them.
Long intents keep their full terminal event, while the stored note is shortened
to 500 characters after redaction.

## How an agent reports

The MCP `progress` tool takes three fields.

| Field | Required | Limit | Meaning |
|---|---|---|---|
| `type` | yes | `done` or `blocked` | The kind of entry |
| `text` | yes | 500 characters | One standalone sentence |
| `step` | no | 80 characters | What the entry belongs to |

```json
{"type": "done", "text": "OpenRouter applications can now be identified.", "step": "Shadow AI Detection"}
```

A `blocked` report marks the agent's tab as waiting for your answer. The mark
clears when you reply in the terminal, or when the same terminal later reports
`done` or starts another agent. A progress report never clears a question that
the agent shows on its screen, such as an approval dialog.

TUICommander adds the project, the time, the agent's name and the source PTY
when it can identify one. The agent does not supply them.

For a repository on a connected remote machine, Progress commands run on that
machine. Its new entries appear in the local desktop and browser views through
the mirrored `progress-recorded` event.

The receipt is `{"id": <n>}` and nothing more. Each call appends one entry: the
journal does not deduplicate agent reports, because an agent that reported the
same step twice did the work twice, and only you can decide what that means.
Intents are the exception. TUICommander reads them off the screen, and a repaint
of the same `intent:` line is not a new intent, so a repeat of the PTY's
newest intent — same text, same agent — returns that entry instead of adding one.

`step` is a free-text label, not a registered object. Nothing has to be created
in advance, and two agents writing the same label are simply two entries with
the same label.

## Reading it

Open the Progress dialog from the command palette (`progress`), with
`Cmd/Ctrl+Shift+P`, or from **Terminal Progress** in the toolbar bell. The bell
entry remains available when there are no unread updates; its badge counts
entries that arrived since you last opened the dialog.
An outcome toast's **Go to repo** action also opens the reporting terminal
in its workspace when that terminal is still open. If it has closed, the
action opens the repository instead.
Clicking the toast body closes it without changing the current repository or
terminal. Use **Go to repo** when you want to navigate.

The dialog opens on the active PTY, newest first. The selector switches to
another PTY in the project or **All repo**, which combines their histories.
List opens with the newest 100 entries or fewer. Scroll to **Load older** to
append the next page. Changing the project, terminal or **Blocked only** filter
starts again at the newest page. A live update or deletion also refreshes the
newest page. Older-page failures keep the entries already loaded and can be
retried. The same paging applies on desktop and mobile.
The mobile header identifies the project in its project selector, without a
second project-name subtitle. If the project selector is unavailable, the
subtitle identifies the selected project instead. In the mobile Progress tab,
a long entry initially shows about four lines;
tap **More** to read it all and **Less** to collapse it again.
Closed PTYs with saved entries remain selectable. A PTY with no entries has an
empty view; it does not inherit another PTY's work. Entries recorded before
terminal identity was stored, and direct local reports with no PTY binding,
appear in **All repo** as **Terminal unknown**. They are never assigned by guess.

Each PTY and **All repo** has its own last-visit divider. The divider stays
fixed while you read a view; switching views loads that view's mark. Closing
the dialog marks every view you visited during that opening.

Blocked entries are red. `intent` entries are muted, because they are what an
agent set out to do rather than a result. A checkbox narrows the list to blocked
entries only. A row can be deleted, and deletion is permanent.

On a phone, the Progress tab shows the same controls. The header wraps across
rows so the project and terminal selectors, **List | Flow**, and **Blocked only**
remain available at narrow widths.

### The Flow view

**List | Flow** in the dialog header switches to a sequence diagram of the same
journal. Each terminal is a column, and so is each Claude subagent of an open
terminal. Rows run from oldest at the top to newest at the bottom; there is no
time scale.

- A delegation is an arrow from the parent to the child, labelled with the task.
- A child's `done` (green) or `blocked` (red) is an arrow back to its parent.
- A message is an arrow from sender to recipient.
- A subagent's task (dashed) and its final report are arrows between it and its
  terminal.
- An `intent` is a muted note on its own column, and the newest one is also
  shown under the column's name.

Click a label to read all of it. A journal text opens in place; a subagent's
full prompt or report is fetched when you click, with secrets redacted. **All
repo** shows every terminal. One terminal shows itself, the terminal that
started it, and the terminals it started. A closed terminal keeps its column,
but its subagents disappear with it, because TUICommander can only find their
transcripts while the agent runs.

There is no export, no pause, no clear and no correction. An entry is appended
once, and it is either there or deleted.

## Turning it off

**Settings → Agents → Collect project progress** turns collection off for every
agent. The `progress` tool then disappears from every agent's tool list, and no
`intent:` marker is recorded.

Each agent also has its own **Collect progress** toggle in its expanded settings
card. Off there keeps that one agent out of the journal while the rest keep
reporting. The global switch wins: off globally means off everywhere.

## Where it is stored

One SQLite database, `progress.sqlite3`, in TUICommander's configuration
directory, with the project and nullable PTY ID as columns. **Nothing is written inside your
repositories** — no `.tuic` directory, nothing for Git to ignore, nothing for a
file watcher to react to.

A worktree's entries are filed under its parent project, so switching between a
repository and its worktrees shows one history rather than several.

An agent running in a directory that belongs to no registered repository writes
nothing. There is no project to file the entry under, and the repository you
happen to be looking at is not the answer.

Flow refreshes cache subagent metadata by modification time and file size.
Unchanged metadata is not reopened; transcript byte cursors continue to read
appended complete rows. Metadata and transcript caches expire after ten minutes
without use.
