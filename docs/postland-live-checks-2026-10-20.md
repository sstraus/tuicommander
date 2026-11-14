# Isolated post-landing checks — 2026-10-20

## Scope and isolation

Source: `chore/live-postland`, commit `a0bec09eea760f18ad113745d66cc8d8bbe166de`.
The native headless build passed through build-slot and mbx in 2m20s. `mbx doctor` reported zero failures. No Rust or frontend test suite ran.

The check used this worktree's `tuic-remote`, private port 22231, isolated HOME and temporary paths under `~/Gits/.tmp/results/live-postland/`, and worktree Vite on port 22232. The named browser session was `live-postland`. The Vite proxy used the isolated Unix socket for the full application router. Initial TCP-proxy captures are not evidence of complete desktop-router parity. No second desktop instance ran, and no request targeted production port 9876.

Local evidence directory: `~/Gits/.tmp/results/live-postland/`. These are retained local artifacts, not portable links. Screenshots were captured, but this agent did not perform subjective pixel approval.

## Results

| Story | Fresh result | Evidence and limit |
| --- | --- | --- |
| 2008-5317 | PASS: isolated restart recovery | `restart-exchanges.json` records 33 real exchanges, including suspension setup retries. The recorder suspended a real shell tab, stopped its own backend, restarted it, resumed the stable identity with a different PTY, and read the same pre-restart durable message. Project progress succeeded. Headerless inbox returned bounded missing-context metadata; missing-project progress was refused. Replacement migration and live-owner stranded-mail assertions passed. `restart.log` reports CAPTURE OK. Python exercised MCP, not ego's stdio consumer. |
| 2013-73f9 | BLOCKED: real ego hop measurement | Both native ego launch attempts exited before a usable turn. No before/after real-ego discovery count was measured. Server discovery exchanges in the restart recording do not substitute for that consumer measurement. |
| 2018-1885 | BLOCKED: live ego idle state | No usable ego composer was reached. An exited session is not an idle-session pass. |
| 2026-9178 | PASS in browser mode: Codex native question | Installed Codex reported 0.163.0, not the historical 0.162.1. `pre-proxy.json` and `last-states.json` export the clean question and Red/Green/Blue/None of the above. `codex-mobile.png` and the browser transcript show the 390×844 buttons. One Green tap produced the native answered question and retracted awaiting. A second question was retained for the focused 1400×900 desktop capture: `codex-pending-desktop.png`, `codex-pending-desktop-state.txt`, `codex-pending-native.json`, and `codex-grid.json`. Native Tauri delivery was not tested. |
| 2027-7ef3 | PASS in browser mode: Grok native question | `startup.json` and `after-startup.json` record the real 1.0.62 Ask and shared Red/Green/Blue/z options. `grok-desktop.png` captures the desktop terminal. `grok-mobile.png` and the browser transcript show 390×844 controls. One Green tap produced native `PARITY_Q_DONE Green` and awaiting retraction, retained in `current.json`. Native desktop Chat delivery was not tested. |
| 2028-6999 | BLOCKED: live ego selector title | No usable ego turn or selector was reached. |
| 2032-8dc3 | PARTIAL / BLOCKED: local Chat opens, transcript not proved | A real MCP-spawned local Claude completed `LIVE_POSTLAND_CHAT_OK`. The focused terminal enabled Chat and opened the transcript UI (`local-chat-open.png`). It showed no messages; `/chat-view` returned `not_bound: no Claude agent is bound to this terminal` (`local-chat-api-error.json`). The isolated backend and child use different HOME values, and the run did not establish the transcript-store binding. An explicit `CLAUDE_CONFIG_DIR` retry reached native login selection (`last-states.json`), so it was stopped without login. This is not a proven remote-classification regression or a transcript-content pass. |
| 2044-31f3 | PASS: caller tab placement; shell scope | A real shell tab declared this already-existing assigned worktree via caller-bound MCP. `declaration.json` records the backend receipt. `declaration-dom.txt`, the browser transcript and `declared-sidebar.png` show its Terminal tabs group immediately below `chore/live-postland`, with the main workspace empty. The actual shell cwd remained the root checkout. No branch or worktree was created or removed. This run proves shell-tab placement, not a fresh ego-driven worktree creation. |
| 1787-4778 | PARTIAL PASS: HTTP/MCP; IPC unavailable | After enabling settings access in the isolated app's normal config path, MCP changed notification volume to 0.25 and HTTP changed it to 0.35. Both transports returned identical complete snapshots after each write (`settings-parity.json`). Initial disabled-tool replies were setup observations, not parity defects. A real IPC check requires an authorized desktop instance; none was started. |
| 1789-796d | PARTIAL PASS: browser Settings sample | General and the owner-bound All settings `app.shell` inspector rendered on the real isolated backend. `settings-general-live.png`, `settings-inspector.png`, and their DOM captures retain control identities and Source: stored. This is not exhaustive catalogue parity, a native WebKit check, light/narrow/plugin coverage, or subjective visual approval. |

## Blocking prerequisite

`ego-startup-output.json` records the exact native refusal:

> read-only directory: cannot enforce a read-only directory beneath a writable root

The backend-owned attachment grant lies under the isolated HOME within `~/Gits`. No permission, sandbox, configuration, transport or attachment-grant override was used to bypass this refusal. The remaining ego checks need an approved layout that preserves the read-only grant and an authenticated provider available in that layout. No credential store was read or copied, and no login was started.

## Harness observations

The initial Vite proxy targeted the restricted TCP router. The final proxy targeted the full isolated Unix router; `/config/repositories` then returned 200. Settings MCP starts disabled and requires both its enable flag and removal of the denied capability. Those setup corrections did not change production code.

The first Codex prompt submission after `/plan` was refused as `agent_not_ready` despite the native empty composer. The retained receipt is `codex-prompt-exchanges.json`. The question was subsequently submitted through the frontend's existing `sendCommand` helper, not a raw text-plus-Enter write. This single observation does not establish a persistent submission defect.

## Completion

Each assigned completed story receives a fresh worklog entry with the scope above. No story is reopened and no missing live proof is claimed complete. All ten `story-close.sh --check` preflights passed without writes (`story-close-checks.json`). The existing completed criteria were not reinterpreted as fresh live proof. `fmt-changed.sh main...HEAD`, `git diff --check`, and instruction-link validation passed. `sessions-after-cleanup.json` is `[]`; the named browser tab and daemon closed; the recorded backend and final Vite process groups received SIGTERM, and `ps` found neither PID. The temporary Vite configuration was removed. No production implementation changed.

## Current-ego follow-up — live-postland2

This follow-up supersedes the three blocked ego cells and the initial blocking prerequisite above. Other cells retain their initial scope and limits.

### Executable identity and isolation

The previous harness explicitly selected the installed ego executable
(`live-postland/control.py`); its retained startup output reports `ego 0.1.2`.
The same executable still reports `ego 0.1.2` and has mtime 2026-10-19 11:54.
Its SHA-256 measured for this follow-up is
`b8eea72439cfd4da9b536358b76ddcca09117458524b0c32621a78a25c6e13cd`.
No hash or source revision was retained from the earlier invocation, so its
exact historical build revision cannot be proved from the version string.

At native build start, ego master was
`c9c74377092c3b37e34a6bc47e7baec676084650`, described as
`jev-before-removal-1522-gc9c74377`. Tracked source was clean; unrelated untracked
reports were left unchanged. With the mbx PATH shim active, `mbx doctor` reported
zero failures. `build-slot.sh cargo build -p ego-cli --bin ego` succeeded in
35.21s. The test binary was not substituted for the installed binary. The
actual selected executable was `<ego checkout>/target/debug/ego`,
version `0.1.2`, SHA-256
`12560feccf6d7aae2c7f282e28afd5cfe8fce3d8f5a5919d84899a4616eb706a`.
That hash was unchanged after the live checks. Master advanced to `08d0fbac`
during the run, after compilation; the source delta is one documentation file.
These checks identify the built `c9c74377` source, not a later rebuilt master.

TUIC source was `8d5bbbc602beca0262331bb646439a9d1a8e739a` on
`chore/live-postland2`. The worktree native headless build succeeded through
build-slot and mbx in 2m13s:
`cargo build --manifest-path src-tauri/Cargo.toml --bin tuic-remote --no-default-features`.
Both native builds used the task's explicit Mac live-binary exception. No test
suite, rb test, production desktop restart, or second desktop instance ran.

The backend used private port 22341, instance `live-postland2`, and isolated
HOME/config/temp paths under `~/Gits/.tmp/results/live-postland2/`. Worktree Vite
used 22342 and proxied to that backend's full Unix router. The named browser
session was `live-postland2`; desktop and `mobile.html` pages were exercised.
No request targeted port 9876. No repository was added or changed in the app.

### Startup prerequisite resolved

A fresh launch of the installed executable reproduces the exact old refusal:

> read-only directory: cannot enforce a read-only directory beneath a writable root

`installed-baseline-request.json`, `installed-baseline-spawn.json`, and
`installed-baseline-output.json` retain the request and result. The built master
instead prints the admitted read-only launch directory and reaches a real turn.
The grant remains nested under the inherited writable root, for example:

```text
~/Gits/.tmp/results/live-postland2/home/Library/Application Support/com.tuic.commander/instances/live-postland2/attachments/pty-82963ec9-62ee-47d5-84b7-18bd7f100bb7
```

The attachment flag was not removed, relocated, or bypassed. This is evidence
that current master resolves the observed 591 startup failure, not a new 591
defect. As before, `sandbox=off` explicitly leaves exec children unconfined; no
native read-only sandbox enforcement claim is made by this launch check.

The first rebuilt-client attempt with the existing user ego configuration then
stopped on an independent prerequisite: model capabilities cache schema 5 is
obsolete and schema 6 is required (`startup.json`). The shared user cache was
not changed. A fresh isolated `EGO_HOME` catalogue reported the subscription
provider unavailable without stored credentials. No login was attempted and no
credential store was copied or inspected. The remaining checks used the same
GPT-6.1 Sol model through `openrouter/openai/gpt-6.1-sol`, with the authorized
OpenRouter key passed only in process environment. A catalogue refresh in the
isolated home confirmed that model available. No secret value is in the saved
spawn requests. `--bare` excluded ambient configuration; the MCP run explicitly
attached the worktree bridge using `--mcp @.../mcp.json`, with `TUIC_SOCKET`
pointing only at this isolated backend. The first bare run without that explicit
attachment had no MCP search tool and is not discovery evidence.

### Fresh results

| Story | Follow-up result | Evidence and limit |
| --- | --- | --- |
| 2013-73f9 | PASS: current real-ego first-call path | `real-ego-tool-trace.json` records successful `search_tools` with limit 1 and budget 16384, followed directly by successful `call_tool(session/status)`, with no `get_tool_schema`. Search returned a complete sibling `top_schema` and catalogue generation `7866dae2f29e7126ee1887c3b2ccac171293db74100a23d914bb86ab6fded9e1`. Thus current discovery uses 1 hop, or 2 calls including the effect. The retained 2026-10-19 real old-client baseline in `live-2013/baseline.md` used 2 discovery hops, or 3 successful calls including the effect, plus its rejected shortcut. That is a historical comparator, not a fresh same-server A/B: today's installed client exits at the attachment prerequisite. `mcp-effect.json` corroborates successful server ingress. The two read-only MCP approvals were answered normally; no grant policy was disabled. |
| 2018-1885 | PASS: completed real turns are not Working or awaiting | A single mobile Green tap answered each native selector. Both clients printed `LIVE_POSTLAND2_DONE Green` and reached an empty composer. `answered.json`, `final-idle-after-progress.json`, the MCP session inventory, and browser transcript retain `shell_state=idle`, `awaiting_input=false`, and final `agent_state=completed` after the suggest marker. Mobile first showed Idle, then Finished, with 0 awaiting (`idle-mobile.png`, `final-idle-mobile.png`). This proves the ordinary completed-turn display; a pending background job was not introduced, so that specific variant was not rechecked. The MCP inventory is the live session state, not a native IPC claim. |
| 2028-6999 | FAIL for the broader clean-title live check; original diagnostic boundary not disproved | Options and answer delivery work, but the exported title is not only `Pick a colour?`. The first bare client included adjacent connection and intent lines (`first-turn.json`, `selector-mobile.png`). The explicit-MCP client included the preceding successful tool receipt (`mcp-effect.json`, `mcp-selector-mobile.png`): `✓ call_tool {"tool_name":"session","arguments":{"action":"status","session_id":"f2a8baa3"}} · 542ms · approval 17.7s Pick a colour?`. The real native question text and options are clean in the terminal. Shared `question_text` and `choice_prompt.title`, then the mobile question sheet, contain the extra receipt. This is a fresh adjacent-text case, not reproduction of the historical provider notice immediately above a selector. |

### Reproducible title evidence and additional observation

The explicit-MCP session is `82963ec9-62ee-47d5-84b7-18bd7f100bb7`; its ego
session is `01a157fb-bd7a-7099-9f07-d602ca639b45`. The complete fresh TCAP remains
local as `live-postland2/ego-selector-receipt-title.tcap`, SHA-256
`ec3ef01aa666221c3444c908a51a481fea80af4f9b075593a9d8a7b33b3b1059`.
It retains real geometry, output and answer input. No capture is committed, so
no replay golden was generated or claimed. The full native event log remains
in the isolated ego session store, and the selected tool-event projection is
in `real-ego-tool-trace.json`.

Decisive source evidence: `ego_question.rs::is_question_boundary` recognizes
blank rows, native `εgo:` diagnostics, composer and working chrome, but does
not recognize a successful tool receipt or TUIC connection/intent lines. The
selector scan joins those adjacent rows into its title. This explains the
observed shared-state contamination without attributing it to browser layout.
No parser fix, RED test, or story was created: this assignment authorizes only
a documentation commit, and the parent owns follow-up implementation scope.

OUT-OF-SCOPE: the second native default-mode MCP approval was visible while the
shared snapshot reported Working with `awaiting_input=false`; the mobile Yes/No
controls were absent. `second-approval.json`, the browser transcript, and the
same TCAP retain this. A raw single `y` key approved the already-authorized
read-only status call; no command text plus Enter was injected. The first
approval's mobile Yes control worked. This is not an idle-composer failure.

### Follow-up validation and cleanup

Only this report is committed. Instruction-link validation passed (2 pairs).
The existing checked live criteria 2013 item 4 and 2018 item 4 were confirmed
through `check-criterion`; no criterion was used to claim a clean 2028 title.
Screenshots and DOM snapshots are retained evidence, not subjective pixel
approval. No production code, dependency, capture corpus, installed executable,
or shared user configuration was changed.

Cleanup completed: capture disabled, both owned live sessions deleted, and the
isolated session inventory is empty. The recorded backend PID 45101 and Vite
PID 52663 received SIGTERM; final `ps` returned neither PID. The named pinned
browser tab and its named agent-browser session closed. The temporary Vite
configuration was removed. `cleanup.json` retains the receipts. Changed-file
format validation and `git diff --check` passed. All retained evidence paths
are local artifacts, not portable links.
