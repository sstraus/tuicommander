# AI Chat

AI Chat is a conversation with **ego**, which TUICommander launches and speaks to
over ACP. TUICommander is the environment — terminals, repositories, the MCP
server — and ego is the intelligence. TUICommander keeps no provider, no API key,
no tool loop and no sandbox of its own.

## Before it can talk

1. Turn on **Experimental Features** in `Settings > General`. The panel is behind
   it and there is no separate AI Chat switch.
2. Set **ego executable** in `Settings > General` (**Select…** opens a file picker). While it is empty, ACP is not
   configured: the panel explains why it is inactive and offers **Configure ego**, which opens
   the ego section in General. Use `Settings > AI Chat` to log in to a provider and select
   the default model (`ego config set model`). Saving the executable enables the composer
   immediately, without restarting TUICommander.
   If the panel is detached, return to its window after saving; it refreshes the settings on focus.
3. Optionally set **ego profile** in `Settings > General` to select a profile from ego's user configuration.
   An empty value leaves ego's usual profile selection in effect. TUICommander passes only the name at launch;
   it does not send profile rules in `session/new`.

A repository can select an ego profile with `"ego_profile": "team"` in its
`.tuic.json`. For that conversation's workspace, TUIC sends the selected name and
the explicit machine profile from Settings as `ceilingProfile`. Ego restricts mode,
sandbox and perimeter to that ceiling and returns warnings, shown in AI Chat's
existing warning notices. The repository cannot raise the machine policy.
A repo selection requires an explicit machine profile and an ego build that
acknowledges the ceiling. Without a repo selection, launch behavior is unchanged.

## Opening it

`Cmd+Alt+A` (macOS) / `Ctrl+Alt+A` toggles the panel; so do the status-bar
button and the command palette. The detach control moves it into its own window,
and the main window shows the *Bring back* placeholder. Drag the left edge to
resize — the width applies for the session and is not persisted.
The conversation view loads when you first open it; terminal input is available
while it loads.

On the mobile PWA, **Chat** is the second tab, after the default **Sessions**
tab, and opens without choosing a repository. It uses the same workspace root
and saved sessions as desktop. The composer shares the session input styling,
auto-grow, right-side attachment icon and round Send button. Enter sends;
Shift+Enter inserts a new line. Long transcript content wraps, while tables scroll
inside their own block.
A repository in a push link is sent with a message as context, not used as the
chat root. The conversation picker shows the titles of saved sessions;
choose one to load its history, or tap **New**. Messages, collapsed tool
activity and pending permission or form cards use the same ACP stream as desktop.
Tap a file link in the transcript to open it in **Files**. A directory link
opens that directory in **Files**. Links use the AI Chat workspace as their base.
If the connection drops, the client resumes from its last received event. A
missing part of the journal is shown as a gap with a **Recover** action.

When ego requests permission or a form response, a subscribed phone receives
one push linking to that conversation in mobile Chat if the desktop is unfocused or idle. Answering
on desktop before the notice is delivered suppresses the alert. Repeated
requests in one conversation share a 30-second push limit. Activity updates
do not alert the phone.

## What it is bound to

**One chat for the whole app, never a terminal.** Every conversation works
across all your repositories: ego runs in `~/Gits`, and the repository on screen
is sent with each message as context — a hint, never a limit on what ego may
reach. The header names that repository. Switching repository keeps the same
tabs and the same conversation, and starts or loads nothing.
If desktop and mobile open the chat together, they attach to the same running ego.

ego starts when you send the first message, click **+**, or select a saved tab; opening the panel
starts nothing. There is one ego for the app: a reloaded window or the phone
gets the one already running, and quitting TUICommander ends it. **New** in the
control bar starts another conversation; the picker beside it lists previous
conversations by title, newest first. Selecting one loads its history. The open
tabs and the selected tab are restored after restarting TUICommander, and their
history is replayed when ego starts. Selecting a saved tab connects and loads it
without sending a message. The panel shows **Connecting conversation…** while
it attaches, or the failure reason and **Retry** if it cannot open the conversation.

The panel has chat tabs for parallel conversations.
Click **+** or press `Cmd+T` (`Ctrl+T` on Windows/Linux) while the panel has
focus to start another ACP session; in browser mode use `Cmd/Ctrl+Alt+T` so the
browser keeps its own new-tab shortcut. Click a tab to switch, or close it to
remove it from the panel. Closing a chat tab does not delete ego's conversation:
the conversation picker can reopen it. Each tab keeps its own transcript and
unsent composer draft. Open tabs and the selected tab return when the panel is
hidden and shown or detached into its own window; the detached view replays
their histories from ego.

Tabs and the picker use the conversation title, its first prompt, or its latest
activity time. A conversation without these details shows **Untitled conversation**;
its session ID is available in the tooltip. The model/mode row remains visible
with **pending** values until the selected session publishes its options.

When ego updates the session title, the panel header and conversation picker
show the new title. During a turn, the footer shows context-window use as a
percentage and shows the cumulative cost when ego reports one.

ego reaches terminals and repositories by calling TUICommander's own MCP
server, served on the ACP connection itself — no bridge process in between. The
entry for it is built by TUICommander, not by whoever opened the session, so a
conversation can never be pointed at some other endpoint, and a second copy
started with `TUIC_APP_INSTANCE=<id>` serves its own terminals and repositories.
ego also reads a workspace summary and its inbox from that server, and new mail
wakes an idle ego with a short notice.

## During a turn

Select text in user messages, answers, code blocks, and tool output and use
`Cmd/Ctrl+C` to copy it. **Copy** appears when a message is hovered or has
keyboard focus. It copies the raw message text. Code blocks have their own
Copy action. Both use the same clipboard adapter as the terminal. Web links open in the
system browser; file links and plain source paths are resolved by the backend.
Files open in TUICommander's viewer or editor, while directories open in the
file browser, including when AI Chat is detached.
With focus in the transcript, `Cmd/Ctrl+A` selects that transcript,
`Cmd/Ctrl+F` opens its search, and `Cmd/Ctrl+K` clears the visible history of
the current tab. Clearing the view does not delete ego's saved conversation.

Paste a PNG, JPEG, GIF or WebP image into the composer to preview it before
sending. Remove a preview with its close button if you change your mind. An
image can be sent without text. The composer refuses images when the connected
agent did not advertise image prompts, or when the pasted images exceed 10 MiB
in total. Text paste works as usual.
Pastes longer than 200 words appear as a numbered `[Pasted text #… +N words]`
marker while you compose; the full text is sent when you press Send. The
composer grows with shorter text up to its height limit, then scrolls.
Press `Ctrl+S` in the composer, or tap the **Park draft** archive icon on a phone, to set aside the
current text and image previews. The **Parked draft** control on desktop, or the
**Restore parked draft** icon on mobile, shows that a
draft is waiting. Press `Ctrl+S` or tap it again while the composer is empty to
restore the draft; when the composer contains another draft, the two swap.
On mobile, Park and Stop use labelled icons above the input row to preserve
the input, attachment and Send positions.
Sending the intervening prompt also restores the parked draft automatically.
Parking survives closing and reopening the panel or reloading its window, and
stays with its chat tab. If browser storage is unavailable, the composer warns
that a reload may lose the parked draft.
Claude Code's [documented `Ctrl+S` behavior](https://code.claude.com/docs/en/interactive-mode)
is the reference for stashing and
restoring an empty prompt; automatic restoration after Send and swapping two
nonempty drafts are TUICommander behaviors requested for this composer.
The transcript follows new output while you are at the bottom and keeps your
position when you scroll up. Tool activity rows show names; expand a call to
read its full command and output.

- **Streamed answer.** Text arrives a chunk at a time. Reasoning is folded into
  a *Thinking* disclosure, kept apart from the answer. When a saved
  conversation is loaded, its earliest replayed chunks remain in order even
  if ego sends them before the load request finishes.
- **Turn markers.** AI Chat hides ego's TUICommander connection acknowledgement,
  shows `intent:` as a labelled status, and turns a `suggest: [ A | B | C ]`
  line or trailing token at the end of an answer into reply buttons. Choosing
  one sends that text as the next prompt.
  Mentions in ordinary prose and code examples remain in the answer.
  Sent messages appear once, including when ego streams an echo after a reply
  button is chosen.
- **Failed or empty turn.** An ACP prompt error appears in the conversation with
  the agent's diagnostic. A turn that finishes without an answer says so; the
  composer becomes available for another prompt.
- **Provider retries.** When the provider or the network fails before the
  agent shows anything, ego retries up to six times and waits longer each time.
  The conversation shows one orange line with the cause, the wait and the
  attempt, for example `… — connection problem, retrying in 2s (attempt 2/6)`.
  The line goes away when an attempt succeeds. After the sixth failure, the
  turn error replaces the line and the next prompt can be sent.
- **Tool activity** appears as one collapsed line per turn, with a count,
  observed duration, status and the first two call titles. Expand it to see
  each call's title, kind and status; expand a call to see its output. The
  duration measures only time observed while this panel is open. A replayed
  conversation has no recorded timing data.
- **The plan** ego publishes is shown as a list and replaced whole each time it
  changes.
- **Stop** cancels the turn. **Pause** and **Resume** hold it where ego supports
  them; **Compact** shortens the conversation. Each button is drawn only when ego
  advertised that extension, so a build without it shows no button rather than a
  button that fails. Resume remains available when a paused turn ends at its
  boundary; select it to continue the conversation.
- **Send during a turn** integrates text into the running query when ego
  advertises steering. Ego consumes it at the next provider request and echoes
  it into the conversation; TUICommander never sends accepted steering twice.
  If the turn has already ended, the message follows the normal prompt path.
  Attachments, agents without steering, and rejected steering use the **Queue**:
  messages stay in order until the running turn ends. A paused queue waits for
  Resume. Every connected view can remove a queued item before ego receives it.
  Stop affects the running turn for every view. A queued message appears in the
  conversation when it reaches ego; steering appears when ego echoes it.
- **Session settings** are published by the current conversation. The control bar
  shows the model's short name and the current mode. Its summary shortens before
  the icon controls, so Pause, Resume, Compact and New stay on one row. Each
  control has a tooltip and a name for assistive technology. The settings button opens a dialog with
  labeled choices and descriptions for every select option ego offers, including
  reasoning effort and sandbox when available. Changes apply to this conversation
  and the displayed values follow ego's reply. A rejected change shows its error
  in the dialog. TUICommander keeps no separate model list.
  The model every *new* run starts from is ego's own default, editable in
  Settings → **AI Chat** (see [Settings](settings.md#ai-chat)).

## Questions ego asks back

- **Permission.** The buttons are the options ego published, answered with one of
  its own option ids. TUICommander never invents an Allow/Deny pair of its own.
- **A form.** An elicitation is drawn as a form built from the schema ego sent.
  A single choice field with one to three values appears as direct answer
  buttons plus **Cancel**; larger or more complex forms keep their fields.
  Only `form` mode is ever drawn; any other mode is declined before it reaches
  the panel.

A question raised while this window was not listening is still shown: the panel
fetches what is open when it attaches, rather than waiting for an announcement
that already happened.

When AI Chat is hidden, its status-bar button shows the number of unanswered
permissions and forms. The desktop sends one notification for each new question;
answering it removes the badge and closes its notification.

## When it misses something

- **"Missed part of this conversation"** means the journal no longer holds the
  point the panel had read up to — a gap, not a dropped connection. **Recover**
  starts a fresh ego process and replays the conversation into it.
- **"Not receiving updates"** means the connection is up but nothing is reading
  its journal any more. The conversation on screen is intact; it has stopped
  moving.

## From a terminal

Right-click a terminal: **Explain with AI** and **Fix this error** put a question
about the selection — or about the last 50 lines when nothing is selected — into
the panel's composer and open it. They write the question; you send it.

## The session knowledge store

Command outcomes are still recorded per terminal — exit codes from OSC 133 where
the shell supports it, an `Inferred` outcome from the PTY silence timer where it
does not — and persist to `<config_dir>/ai-sessions/<session_id>.json`. Nothing
reads them today. They are kept because the recording has nothing to do with a
model and re-deriving the history later is impossible.

## Not here

| Feature | Where it went |
|---------|---------------|
| Providers, models, slots, API keys, Ollama detection | Settings → **AI Chat** shows what ego is configured with and sets its default model (#786-4a6d). Slots, API keys and Ollama detection did not come back: no key is stored by TUICommander and no provider call is made from it |
| Agent mode (ReAct loop), the safety checker, the file sandbox | nothing. ego runs its own tool loop and reaches terminals through TUICommander's MCP server |
| Terminal watchers (autonomous rules) | not scheduled |
| PR AI review, changelog generation, improvement scan | 795-320b |
| Smart Prompts `api` execution mode | One unattended ego turn (#787-ee50), with no interactive tools and the final text routed to the prompt's configured output |

## See also

- [`docs/backend/acp.md`](../backend/acp.md) — the ACP client, its journal and
  the authority a session is given.
- [`docs/backend/mcp-http.md`](../backend/mcp-http.md) — how an external agent
  drives TUICommander terminals, which is the path ego uses.
- [`docs/backend/pty.md`](../backend/pty.md) — PTY lifecycle, OSC 133, TUI
  detection, silence-based idle.

Pending permissions and ego notice cards can alert your phone when the desktop is away. Both share a 30-second limit per conversation; ordinary activity does not send a push.

AI Chat offers **Fork from here** beside a reply when ego advertises message-point forks. The new tab keeps history through that reply’s completed turn; the parent remains open.

Forked AI Chat tabs label inherited history. A separator marks where the child’s own conversation begins, including after loading saved history.

The AI Chat conversation picker groups fork and compaction descendants beneath their ancestors. Deleted immediate parents remain visible as disabled “Deleted conversation” rows.

Closed refusal turns show one plain-text card with the agent’s existing ACP refusal text. A refusal without text shows a generic refusal message.

### Conversation launch options

Use **New conversation with options** beside the `+` button to override the ego profile, workspace or executable for one conversation. Empty fields use Settings defaults. The header shows the resolved values. The conversation keeps these values and its peer identity after a restart; other chats keep their existing launch configuration. The plain `+` action creates a chat with the global defaults.

A custom conversation runs its own ego ACP process. Its mail identity is separate from daily chats, even when both use the same workspace.
