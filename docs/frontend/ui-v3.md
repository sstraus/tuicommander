# UI v3 — detail panels

UI v3 is the layout for every panel that shows one item and its actions:
a pull request, an issue, and future work items. Boss approved it on
2026-11-22 from the PR sidebar mock "E2". New detail panels use it, and
existing panels move to it when they are next changed.

**The visual reference is [`ui-v3/issue-panel-reference.html`](ui-v3/issue-panel-reference.html).**
Open it next to the running panel and compare them side by side before you
call a UI v3 change done. This file says what goes where; the reference
says how it looks. When they disagree, ask Boss.

## The three layers

A UI v3 panel has three layers, from top to bottom.

1. **Sticky header.** It stays visible while the body scrolls.
   - The full title. It wraps and is never truncated.
   - One status row of pills. Each fact appears once: no badge in the
     row repeats a pill in the header.
   - A comments pill (comment icon + N). It is always visible, also at 0,
     and it opens the full conversation. Icons are monochrome inline SVGs,
     never emoji or Unicode glyphs; the mocks used glyphs as placeholders.
   - The **next action** block.
2. **Fixed toolbar.** The same buttons, in the same order, in every state.
   - Each button has an icon and a short label. Icon-only buttons are not
     allowed in the toolbar: Checkout and Worktree look the same as glyphs,
     and a wrong click changes the branch.
   - Show only the 3 most frequent actions, then `⋯`.
   - `⋯` contains rare actions with their full names. Destructive
     actions (Close) go last, in red.
3. **Accordion sections.** Each section is collapsed by default. The header
   of a collapsed section shows a short summary (`5 ✗ 9 ✓`, `3 open`).
   The sections always appear in the same order.

## Visual rules

Measured from the reference. They apply to the PR and the issue panel alike.

| Piece | Look |
|---|---|
| Meta line | First line, muted, 12px: `#N · opened by author · 3d ago` (issue), `author · head → base · +a −d · 3d ago` (PR). One line, no chips with their own row. |
| Status row | One row of small pills (11px, 1px×7px padding, 9px radius): state, readiness, CI (`✗ 5/14`), review, the labels inline, then the comments pill. Never two pills with the same text. |
| Next-action box | Tinted box (accent at ~8%, accent border at ~35%), 8px padding. A muted 11px *why* line, then **one** row of buttons. |
| Next-action buttons | Compact (12px text, 4px×9px padding, 5px radius), icon + label. The first is primary. Others are secondary (bordered, filled), never ghost. |
| Split button | A real button: the main part and `▾` share the variant (primary or secondary) and are joined by a 1px divider. `▾` opens a grouped menu: **Start** (Auto-fix…), **Workflow**, **Agent prompt**, each group with an uppercase 10.5px header; the last used item is marked. |
| Toolbar | Muted ghost buttons (12px), icon + label, `⋯` pushed to the right. No borders. |
| Section header | 12px, `▸`/`▾` caret, title left, muted summary right, 7px×12px padding. Nothing else in the header except one small action (Run). |
| Section body | Content only. Toggles and secondary buttons go to `⋯`, not above the content. |
| Comment card | Filled card (6px radius): bold author, muted time right, text below. |

## The next-action block

- It shows one or two buttons with an icon and a label, plus one line that
  says *why* these actions are suggested.
- The item state selects the buttons. For a pull request, the first rule
  that matches wins:

  | State | Next action |
  |---|---|
  | Draft, merged or closed | none |
  | Another author, the viewer can approve | **Approve** · Review ▾ · Request changes |
  | Own PR, conflicts | **Resolve conflicts** · Review ▾ |
  | Own PR, failed checks | **Re-run failed** · Fix CI ▾ |
  | Own PR, unresolved threads or changes requested | **Open in Tasks** · Address comments ▾ |
  | Readiness verdict "ready" | **Merge** · Auto-merge |
  | Checks running | Auto-merge |

  The order follows the shared readiness verdict (`prReadinessOf`), so
  the panel never suggests an action that the status badge contradicts.

- For an issue, the first rule that matches wins:

  | State | Next action |
  |---|---|
  | Closed | **Reopen** |
  | `autofix/issue-N` has a worktree and no PR | **Create draft PR** · Open worktree |
  | Open | **Work on it ▾** (main part: Auto-fix in worktree, or the last pick) |

  The issue toolbar is GitHub · Copy `#N` · Tasks · `⋯`. The `⋯` menu holds
  Create draft PR from the auto-fix branch and, last behind a separator,
  Close issue. Description opens by itself on an open issue; Comments never does.

- Never show an action that the viewer cannot do. For example, GitHub
  refuses an approval of your own PR, so Approve does not appear there.
- One split button collects every way to start an agent or a workflow on
  the item (`✦ Review ▾`, `✦ Work on it ▾`). A click runs the last one
  used. The arrow opens a menu with the groups Start, Workflow and Agent
  prompt, and ends with **Manage workflows…**. The Workflow group lists the
  project's published workflows; a pick starts the published revision with the
  PR or issue as subject. A workflow with a required input that has no default
  opens the Workflows space instead.
- The row `⋯` menu of a PR holds **Hide from list**; rows have no floating ×.
- An open issue offers **Assign me** next to Work on it when the viewer is not
  assigned. Open review threads have **Reply** and **Resolve**; issue comments
  have a **Comment** composer. A collapsed Description shows its size ("N lines").
- New comments since the last visit (backend `visit_work_item`): the pill reads
  "💬 4 · 2 new", the why line "2 new comments since the last visit", **Reply**
  becomes the primary next action beside a secondary Work on it, Comments opens
  by itself instead of Description, and each new comment carries a "New" mark.
  A first visit marks nothing new.
- The issue `⋯` menu holds **Hide from list**; the section header shows
  **Show N** to bring hidden issues back.

## Dynamic content

- What changes with the state: the next-action buttons, and **one**
  section that opens by itself (PR: Checks when CI is red, otherwise
  Threads when some are open; issue: Description while it is open).
- What never changes: the toolbar, the order of the sections, and the
  position of the title and the status row. Muscle memory must work.

## Details stay in the panel

- Description and comments render inside the panel, through the same
  components as the Tasks detail (`TaskMarkdown`, `TaskComment`). Do not
  write a second renderer.
- A long view leaves the panel only through an explicit `↗`: the full
  conversation in Tasks, the full CI log, the diff, GitHub.
- A check chip opens its log excerpt in place.

## Where the logic lives

The next action is a pure function of the item and the viewer. It sits
next to the other verdicts (`prReadinessOf`, `canApprovePr`) and has
unit tests for each row of the table.

## Reference implementations

- Pull request: the GitHub sidebar panel (`src/components/Sidebar/PrSection.tsx`).
- Issue: the issues section of the same panel (`GitHubPanel.tsx`, `IssueDetailPopover/IssueDetailContent.tsx`).
- Shared pieces: `ui/PanelSection.tsx`, `shared/PanelToolbar.tsx`, `shared/WorkItemPreview.tsx`, `shared/NextAction.module.css`.
