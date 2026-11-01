# AI Chat

While an agent works, Chat shows a breathing dot and ring in the theme accent color beside **Working…**. Desktop and mobile share this indicator. With reduced motion enabled, the dot is static and the ring is hidden. The indicator disappears when the turn ends; provider retry messages take its place while a request waits to retry.

The open AI Chat conversation follows across desktop, phone and tablet connected to the same host. Open or select a conversation on either device to show it on the others. The last selection saved by the host wins. A device that was offline reads the current selection when it reconnects. This does not change a session header's **CLI | Chat** choice: that view stays per device. Drafts, tab layout and repository pins also stay local.

On desktop, AI Chat and session Chat share the same 14px default reading scale. Terminal zoom in, zoom out and reset also update AI Chat using the active terminal's zoom step. With no active terminal, AI Chat uses its default size. Font family, line height, weight and colors do not change when you switch between the two Chat views. Mobile keeps its device-local Chat font preference.

Use **⋯ → Delete conversation…** to delete saved conversations. Each row has a trash button. Confirm the conversation name before deletion; this permanently removes its history and closes its open tab. If ego refuses deletion (for example, while the conversation is running), the conversation stays and the error is shown. The list offers deletion only when ego advertises the ACP delete capability. If you select another conversation while deletion cleanup is pending, that selection stays active.

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

The desktop composer uses one compact card. Its header shows **Repository · name** and an independent repository pin. The document button on the right parks the draft; its tooltip offers restore or swap when a draft is parked. The blue play button sends while idle and queues while ego is busy. Stop remains beside the input during a running turn. The desktop input grows with the draft. Stop and Send keep their fixed control size and align to its bottom edge. The header, input row and footer use the same content edges.

Pasting an image starts ego when needed before checking image support. Finder image copies attach the image rather than its filename; mixed clipboard data with substantive text pastes as text, as in Ideas and terminal Compose. An empty text clipboard flavor does not suppress an available image.

While ego waits to retry a provider request, AI Chat shows the cause, attempt
number and a live countdown. The countdown starts when the update arrives and
ends at **retrying now**. Recovery removes the line; a final failure replaces it
with the error. The Copy action beside a user message stays on one line.

## Slash commands

Start a draft with `/` to see the commands available in the selected conversation.
Type a command name to filter the list. Each agent command shows its description
and input hint. Use Up/Down and Tab or Enter to complete a name, or click a row.
Completion does not submit it. Add arguments and press Enter or Send to run it.
Escape closes the list without stopping a turn.

Agent commands come from ACP `available_commands_update` and are sent as
`/<name> <arguments>`. The latest offer replaces the previous list, including an
empty offer. Commands from another conversation do not enter this list.

TUICommander handles these actions locally instead of sending them to the model:

| Command | Action |
|---|---|
| `/new` | Start a new conversation with the existing New action. |
| `/clear` | Clear this composer's text and attachments, not conversation history. |
| `/quit`, `/q` | Stop the current AI Chat connection through the existing stop action. Saved history remains. Other tabs on that connection also disconnect. |
| `/usage` | Show the context usage and cost already reported for this conversation. |
| `/compact` | Run the same action as **Compact context**. Available only when advertised; wait for the turn to finish. |
| `/fork` | Run **Fork conversation** without a message argument. Available only when advertised; wait for the turn to finish. |
| `/pause`, `/resume` | Run the advertised pause/resume controls in their applicable turn state. |

Host actions do not accept arguments or attachments, except `/clear`, which
removes attachments. Unsupported commands, including `/name` and `/retry`, stay
in the composer with an inline **not available in AI Chat** explanation and a
list of available names. A leading path token with another `/`, such as
`/Users/x/file.ts` or `/tmp/error.log`, is sent as prompt text unless it matches
an advertised command name. Bare names such as `/retry` remain command attempts;
write `/tmp/` with its trailing slash when referring to a top-level directory.
Nothing is sent silently. Open a conversation with
**New** or `/new` to receive its agent command offer.

## Automatic context compaction

When ego reports an exhausted context and supports compaction, AI Chat shows **Compacting conversation**, opens the compacted continuation and sends the original prompt again once. Text, images, file references and repository context stay the same. **Conversation compacted** confirms that the continuation opened. A compaction or load failure stops recovery and shows the error. If the repeated prompt exhausts context again, AI Chat stops instead of compacting in a loop. The manual **Compact** action uses the same continuation path. Switching conversations while compaction is pending keeps your newer selection and prevents an automatic replay into it. A different conversation can still send while compaction is pending. Its own manual or automatic compaction waits for the first operation, then runs only if that conversation is still selected. In the compacting conversation, Send or Queue leaves your draft and attachments in the composer until compaction finishes; submit them again when ready.

## Compact panel controls

The header contains the conversation title picker, **New**, **⋯**, and window controls. The tab strip appears only when two or more tabs are open. **⋯** contains labelled session settings, pause/resume, compact, fork, new-conversation options, and deletion actions. Capability and busy-state restrictions still apply.

Changing **Session settings → Model** updates the current conversation in place.
It does not reconnect ego or create a conversation, and the transcript stays visible.
After ego confirms the option, the next turn uses that model. **Settings → AI Chat →
Default model** is different: it changes the default for new ego runs only. An open
conversation keeps its selected model; use Session settings to change it.

The footer shows context occupancy, used/window tokens and approximate free tokens when ego publishes usage. It also keeps any reported cost. The model, reasoning and permission summary stays on the right and opens Session settings. Repository pinning stays in the card header. Process titles and tool disclosures share one line; failed groups and calls stay collapsed until you open them, with a red dot and Failed label. Suggested replies appear only under the latest answer. Loaded history does not show execution durations measured from the view mount.

## Tables

AI Chat keeps short fields and headers on one line. Longer text uses the remaining space. Tables with multiple long columns use the approved wide layout; the first column stays visible while you scroll. A scroll hint appears only when the table is wider than its viewport. Priority values use compact badges.

Each table has **Markdown**, **Copy as Markdown**, and **Open as document** actions. Markdown toggles the original source, including inline formatting and escaped pipes; Copy uses that source in either view. Open creates a read-only TUIC Markdown tab. In a detached AI Chat window, the document opens in the main window. On mobile, it opens a document view with **Back to Chat**. Other Markdown documents and terminal transcripts retain their existing layout.

## Opening it

`Cmd+Alt+A` (macOS) / `Ctrl+Alt+A` toggles the panel; so do the status-bar
button and the command palette. The detach control moves it into its own window,
and the main window shows the *Bring back* placeholder. Drag the left edge to
resize — the width applies for the session and is not persisted.
Opening the configured panel automatically connects ego and restores the selected
conversation. With no saved conversation, it opens a new one without sending a
message. Terminal input remains available while it connects. Switching repositories
does not start another agent. A failed connection stays visible for explicit Retry;
Retry starts the connection and shows its connecting state without requiring Send.
It does not retry in a loop. Stopping ego keeps the selected transcript visible.
Reopening the panel, or sending another message, reconnects the same conversation.

Mobile offers the **Chat** tab only when the host has a configured ego executable, using the same availability predicate as desktop. Host settings changes update the tab without reloading. Removing ego configuration returns an open Chat view to **Sessions**; unavailable saved Chat links also stay on **Sessions**.

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
Saved tabs are attached before a message is sent, including when Chat is reopened after switching mobile tabs. If ego no longer has a saved conversation, Chat opens a fresh one automatically and removes only the stale local tab. Other load failures remain visible for explicit retry; they do not replace history.
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

ego connects when you open the panel. Sending after a stop, clicking **+**, or
selecting a saved tab also connects it when needed. There is one ego for the app: a reloaded window or the phone
gets the one already running, and quitting TUICommander ends it. **New chat tab** in the
tab bar starts another conversation; the conversation picker lists previous
conversations by title, newest first. Selecting one loads its history. The open
tabs and the selected tab are restored after restarting TUICommander, and their
history is replayed when ego starts. Selecting a saved tab connects and loads it
without sending a message. The panel shows **Connecting conversation…** while
it attaches, or the failure reason and **Retry** if it cannot open the conversation.
Reopening the panel on an existing connection also attaches the selected saved tab.
If ego no longer has that conversation, Chat opens a fresh one automatically,
without requiring a message or **Retry**. Other load failures remain visible.
Opening a new chat leaves that failure behind, including when its connection is still pending. A saved conversation that finishes
loading later does not switch you away from the new chat.

The saved-conversation list loads separately from the active chat. You can keep sending while the list is slow or unavailable. A missing background tab does not fail the selected conversation; its refusal is handled when that tab is selected.

The panel has chat tabs for parallel conversations.
Click **+** or press `Cmd+T` (`Ctrl+T` on Windows/Linux) while the panel has
focus to start another ACP session; in browser mode use `Cmd/Ctrl+Alt+T` so the
browser keeps its own new-tab shortcut. **New chat** opens a fresh conversation
without first loading the saved selection. A saved greeting or title does not
become the new tab's name, and an unavailable saved conversation cannot block it.
Click a tab to switch, or close it to
remove it from the panel. Closing a chat tab does not delete ego's conversation:
the conversation picker can reopen it. Each tab keeps its own transcript and
unsent composer draft. Open tabs and the selected tab return when the panel is
hidden and shown or detached into its own window; the detached view replays
their histories from ego.

Tabs and the picker use the conversation title, its first prompt, or its latest
activity time. A conversation without these details shows **Untitled conversation**;
its session ID is available in the tooltip. The model/mode row remains visible
with **pending** values until the selected session publishes its options.

The header and conversation picker stay on the selected transcript when the saved-conversation list loads or refreshes, even when other sessions are attached.
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
keyboard focus. It copies the whole raw message, including fenced code, through
the same clipboard adapter as the terminal. Answers have no separate code-block
Copy button. The compact Copy and **Fork** row stays below its answer
and does not overlap the next message. Web links open in the
system browser; file links and plain source paths are resolved by the backend.
Files open in TUICommander's viewer or editor, while directories open in the
file browser, including when AI Chat is detached.
With focus in the transcript, `Cmd/Ctrl+A` selects that transcript,
`Cmd/Ctrl+F` opens its search, and `Cmd/Ctrl+K` clears the visible history of
the current tab. Search matches displayed prose with collapsed whitespace; code blocks retain literal whitespace. Clearing the view does not delete ego's saved conversation.

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
On mobile, Park and Stop use labelled icons in the input row beside Attach and
Send. They do not reserve a separate row above the composer.
Sending the intervening prompt also restores the parked draft automatically.
Parking survives closing and reopening the panel or reloading its window, and
stays with its chat tab. If browser storage is unavailable, the composer warns
that a reload may lose the parked draft.
Claude Code's [documented `Ctrl+S` behavior](https://code.claude.com/docs/en/interactive-mode)
is the reference for stashing and
restoring an empty prompt; automatic restoration after Send and swapping two
nonempty drafts are TUICommander behaviors requested for this composer.
The transcript follows new output while you are at the bottom and keeps your
position when you scroll up. A touch pauses automatic scrolling until you release
or cancel it. If you did not scroll up, the transcript then catches up to the latest
output, even when the answer finished during the touch. Tool activity rows show names; expand a call to
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
- **Session settings** are published by the current conversation. On desktop, click the model,
  reasoning and mode summary below the input, or use **⋯ → Session settings**.
  Both open the same dialog. Before the conversation publishes its options, the
  settings action stays visible but disabled, with a tooltip that explains why.
  The dialog has
  labeled choices and descriptions for every select option ego offers, including
  reasoning effort and sandbox when available. Changes apply to this conversation
  and the displayed values follow ego's reply. The summary uses the published
  choice labels, including **Choose a value** when a model change leaves reasoning
  effort unselected. A rejected change shows its error
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

The write-only command knowledge store was removed in #2115-1333. TUICommander no longer records or periodically saves command outcomes for AI context. Existing `<config_dir>/ai-sessions/<session_id>.json` files remain untouched. Shell activity and command blocks still use OSC 133 markers.

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

AI Chat offers **Fork** beside a reply when ego advertises message-point forks. The new tab keeps history through that reply’s completed turn; the parent remains open.

Forked AI Chat tabs label inherited history. A separator marks where the child’s own conversation begins, including after loading saved history.

The AI Chat conversation picker groups fork and compaction descendants beneath their ancestors. Deleted immediate parents remain visible as disabled “Deleted conversation” rows.

Closed refusal turns show one plain-text card with the agent’s existing ACP refusal text. A refusal without text shows a generic refusal message.

### Conversation launch options

Use **New conversation with options** beside the `+` button to override the ego profile, workspace or executable for one conversation. Empty fields use Settings defaults. The header shows the resolved values. The conversation keeps these values and its peer identity after a restart; other chats keep their existing launch configuration. The plain `+` action creates a chat with the global defaults.

A custom conversation runs its own ego ACP process. Its mail identity is separate from daily chats, even when both use the same workspace.

When you scroll up and more conversation content arrives, **New text below** appears as a compact badge floating at the bottom-right of the transcript, above the composer, without reserving a row. Select it to return to the bottom and resume following. Reaching the bottom by hand clears it too. Search jumps do not show the badge until later content arrives. Disable **New text indicator** in **Settings → Appearance → Chat**, or mobile Settings. This preference is saved on the current client and applies to terminal Chat too.

AI Chat replies use plain chat prose without the terminal answer rail. Literal Markdown blockquotes keep their normal quote style.

Use **Pin repository** beside the composer to keep that conversation's repository context while you navigate elsewhere. **Pinned repository** identifies the frozen context. Unpin to follow the repository on screen again. The name uses the repository or group color, with the normal light-theme text adjustment. This does not move or filter conversation tabs. The existing pin icon still parks your draft; it is a separate action.

Middle-click a conversation tab to close it through the same path as its close button. The last conversation tab stays open. Press Escape from the composer or transcript to stop a running turn without clearing your draft. When Find is open, Escape closes Find first, including when focus has moved back to the composer. Desktop Stop and Send stay at the bottom of the input without growing with it. Park remains in the card header.

Repository names in linked worktree contexts use the registered parent repository color, including its inherited group color. Pinning a worktree keeps that color while you browse another repository.

## Telegram replies

After Telegram is configured in Settings, allowlisted phone text reaches the
latest-connected live AI Chat agent when no agent has explicitly registered.
With several live chats, connection order determines the fallback, not the
selected tab. An agent can call `telegram register` to take over the channel;
that registration has priority until it ends or another agent replaces it.
Registration lasts only while its MCP identity remains live. No PTY is needed.
Incoming phone text arrives through the normal peer inbox and TUIC wakes an idle
conversation. The no-agent reply appears only when neither a registered agent
nor a live AI Chat agent is available.

The phone shows a receipt reaction, then a working reaction once the mail is
read and a new turn starts. Assistant message chunks stream into the ephemeral
Telegram preview; thought chunks and tool arguments do not. Permissions show a
waiting reaction and ask you to respond in TUICommander. The preview is never
approval to publish anything.

`telegram begin` is optional. `telegram finish` supplies the exact final phone
answer and remains valid after the preview ends. If a turn started by TUIC's
peer-mail wake settles without finish, TUIC sends that turn's assistant text
once. Turns started from an unrelated conversation prompt do not auto-publish.
A failed turn sends one safe stopped notice. Stop on the phone targets only the
captured turn; a queued Stop does not interrupt unrelated work.

## Settings MCP startup boundary

The Settings owner can read and edit the configured Ego model/perimeter and base provider-auth configuration, with owner revisions and next-session effects. The AI Chat Settings button still opens the human login workflow. A schema-only assistant can also request typed `begin_login` and `login_status` from the configured Ego owner; you complete the exchange in its terminal. Terminal exit is not success: the owner reads credential presence, then the assistant refreshes provider state. Missing binary/capability is not permission to use a different executable or PTY fallback for ACP. See [startup workflows](../backend/settings-startup.md#ego).

### Discuss a Markdown selection

With a conversation open in AI Chat, select text in a Markdown file and choose **Send to AI Chat** beside the comment action. TUIC appends the file path, original source line range and quoted selected text to that conversation’s draft, then focuses the composer. It preserves your existing draft and attachments and does not send a message. This action is also available in the mobile Files Markdown view after you open a chat conversation.

Selections can include inline links and adjacent punctuation. Link destinations and reference identifiers do not count as visible occurrences when locating the source lines.

### Development backend mismatch

When a development frontend calls a command missing from the running backend,
AI Chat explains the **Backend/frontend version mismatch** instead of offering
Retry. Restart `make dev` (or `pnpm tauri dev`) when you are ready to end the
current backend session. Reloading the frontend does not rebuild the backend.
Release and remote-owner errors keep their existing recovery controls.

## Dictation in a detached chat

The detached AI Chat window owns its dictation input. Focus that window and hold
its configured push-to-talk key. On macOS, Fn starts in the focused chat window;
releasing it still stops that recording if focus moves away. The transcript goes
to the input and selection captured at recording start, not to the main window.
If the chat transcript has focus, dictation targets that window's composer.

AI Chat dictation inserts a draft for review. It does not submit the draft. If
the captured input is removed or edited before transcription completes, the
transcript is retained in Messages instead of overwriting the new draft. If the
model is missing, the setup action opens Voice settings in the main window.

## Confirm a detected ego executable

If no ego executable is configured, opening AI Chat checks the host's agent detection.
When it finds ego, the panel shows its path and **Use this**. Select **Use this** to
save that path through the normal settings writer and activate AI Chat. Detection
never saves a path automatically and never replaces a configured executable.
**Configure ego** still opens manual setup. If detection finds nothing or fails,
the existing inactive message and manual setup action remain available.

## Hands-free voice

Settings → Voice → Hands-free conversation lists attached AI Chat sessions in the Target picker, including sessions in detached panels. Select a chat and start the conversation to submit speech through its normal ACP prompt queue and allow replies through the voice tool. Closing or leaving the selected chat releases its voice binding.

## Master prompt

Settings → AI → AI Chat → Chat behavior has a multiline **Master prompt** field.
It adds instructions to new conversations without replacing TUIC launch rules. Empty
means no extra instructions. The limit is 8192 UTF-8 bytes. Existing conversations
keep their saved instructions, also after reopening. Smart Prompts, PR reviews,
changelog generation and story drafting do not use this field. Do not use it to store secrets.
