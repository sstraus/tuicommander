# Native question parity audit — 2021-30cd

Audit date: 2026-10-19. Claude is the reference, not an assumed all-pass baseline. This report records retained live evidence, not subsequent fixes. No production code changed.

## Evidence key and limits

- **F** = `~/Gits/.tmp/results/parity-2021-final/`: permissions, mobile Chat, desktop DOM/store and network captures.
- **R** = `~/Gits/.tmp/results/live-restart/`: native multiple-choice round trips, mobile DOM/API and cleanup. Its `parity-matrix.md` identifies runtime `c281128313` and mobile viewport 390×844.
- **Q** = native multiple-choice question; **P** = enforced permission for harmless `printf`.
- **Unknown** is missing proof, not success. Store state is not a visual badge. Native output after a mobile answer does not prove desktop delivery. List navigation is not an answer surface.

These are local retained artifacts, not portable links or committed fixtures. Some JSON files encode another JSON string. `F/permission-states.json` is empty; use `F/permissions.json`. All agents use PTY, including ego CLI; this is not ACP AI Chat. No second desktop instance was used.

## Shared detection and native state

| Agent | Q detection and native options | P detection and native options | Evidence |
| --- | --- | --- | --- |
| Claude (reference) | Imperative `Pick a colour` is confidently awaiting but has empty question text and no exported choices (2074). Question-mark title later exports Red/Green/Blue and native text/chat alternatives. | Confident awaiting; native Yes/No correctly exported. | R/parity-all-states.json; R/claude-output.json; F/claude-choice-chat-dom.txt; F/permissions.json |
| Codex | Real /plan question; exported Red/Green/Blue/None of the above. | Native three-choice approval remains open but REST and desktop store say idle with no choices (2075). | R/codex-output.json; R/parity-all-states.json; F/permissions.json; F/desktop-dom-and-store.json |
| Grok | Real Ask; exported Red/Green/Blue/z custom. | Confident awaiting; all five native scope/approval choices exported. | R/grok-output.json; R/parity-all-states.json; F/permissions.json |
| ego | Native selector; exported Red/Green/Blue. | Confident awaiting; native y/a/n/e/d exported. | R/ego-question-state.json; F/permissions.json |

## Agent × surface × capability

Detection below uses the shared API state above. The badge column distinguishes DOM evidence from store-only evidence. Q and P answer claims remain separate.

| Agent | Surface | Badge / state evidence | Options | Answer delivery | Artifacts / gap |
| --- | --- | --- | --- | --- | --- |
| Claude | Desktop terminal | P store awaiting; final imperative Q badge unknown | Native Q/P output captured; canvas pixels unknown | Desktop Q/P unknown | R/claude-output.json; F/permissions.json; F/desktop-dom-and-store.json |
| Claude | Desktop Chat | Q store awaiting with Chat active | Q named choices absent; only Copy/compose controls | Named-choice answer unavailable; compose answer not tested | F/desktop-claude-open-question-chat.json; 2079 |
| Claude | Mobile list | P question in awaiting list; final imperative Q badge not separately captured | N/A | N/A | F/list-dom.txt; R/parity-matrix.md |
| Claude | Mobile CLI | P Input; Q detection lacks exported title | P Yes/No match native; imperative Q colour controls absent | Q generic Enter selects Red; CLI-specific P delivery unknown | R/claude-mobile-dom.txt; R/parity-after-states.json; F/claude-permission-cli-dom.txt; 2074 |
| Claude | Mobile Chat | Q/P Input | Q colour controls and native alternatives; P Yes/No | Q Blue confirmed in native output and Chat with awaiting retraction. P command completes after approval sequence; multi-tab delay affects timing. | F/claude-choice-chat-dom.txt; F/claude-chat-blue-after.json; F/claude-chat-blue-after-dom.txt; F/claude-permission-chat-single-tab-dom.txt; F/permission-native-after.json; 2078 |
| Codex | Desktop terminal | Q store awaiting; P store idle despite native dialog | Native Q/P output captured; canvas pixels unknown | Desktop Q/P unknown | R/desktop-parity-questions.json; F/desktop-dom-and-store.json; 2075 |
| Codex | Desktop Chat | Final session disabled by exact transcript-binding gate | Unavailable in captured session | Unknown; binding gate is not itself a confirmed defect | F/desktop-dom-and-store.json |
| Codex | Mobile list | Q Input/title; P absent from awaiting list | N/A | N/A | R/mobile-list-dom.txt; F/list-dom.txt; 2075 |
| Codex | Mobile CLI | Q Input; P Idle | Q four choices; P no choice overlay, generic keyboard remains | Q Green and awaiting false. P generic Enter approves once; native command confirms. | R/codex-mobile-dom.txt; R/codex-after-mobile-answer.json; R/codex-after-output.json; F/codex-before-enter-dom.txt; F/codex-after-enter-dom.txt; 2075 |
| Codex | Mobile Chat | Surface excluded | No Chat toggle | Unavailable | F/codex-permission-cli-dom.txt plus source exclusion; 2076 |
| Grok | Desktop terminal | Q/P store awaiting; pixels unknown | Native Q/P captured | Desktop Q/P unknown | R/desktop-parity-questions.json; F/desktop-dom-and-store.json; F/permissions.json |
| Grok | Desktop Chat | Completed P transcript captured, not pending dialog | Pending Q/P controls unknown live; shared renderer lacks choices | Desktop Q/P unknown | F/desktop-dom-and-store.json; common boundary 2079 |
| Grok | Mobile list | Q Input/title; final open P list not captured | N/A | N/A | R/mobile-list-dom.txt; F/list-dom.txt predates final Grok session |
| Grok | Mobile CLI | Q/P question detected | Q four choices; P five native choices | Q Green and awaiting false. P approval sequence followed by PARITY_GROK_OK/DONE and awaiting false. | R/grok-mobile-dom.txt; R/grok-after-mobile-answer.json; R/grok-after-output.json; F/grok-permission-cli-dom.txt; F/grok-after-permission-dom.txt; F/post-approval-states.json |
| Grok | Mobile Chat | Surface excluded | No Chat toggle | Unavailable | F/grok-permission-cli-dom.txt plus source exclusion; 2077 |
| ego | Desktop terminal | Q/P store awaiting; pixels unknown | Native Q/P captured | Desktop Q/P unknown | R/desktop-parity-questions.json; F/desktop-dom-and-store.json; F/permissions.json |
| ego | Desktop Chat | Transcript after original command; later progress approval still pending | No native choices in captured button list | Desktop Q/P unknown | F/desktop-dom-and-store.json; F/post-approval-states.json; common boundary 2079 |
| ego | Mobile list | P awaiting question shown; final Q badge not separately captured | N/A | N/A | F/list-dom.txt; R/parity-matrix.md |
| ego | Mobile CLI | Q Input and Open ego question | Q three choices; P CLI-specific capture absent | Q Green and awaiting retraction; P CLI-specific delivery unknown | R/ego-mobile-dom.txt; R/ego-open-dom.txt; R/ego-after-mobile-answer.json; R/2029-ego-native-output.json |
| ego | Mobile Chat | P Input and expanded question action | P y/a/n/e/d; Q Chat not captured | P command eventually executes after Chat tap when connection pressure releases. Later progress-tool approval is a different question, not stale printf approval. Q Chat unknown. | F/ego-permission-chat-dom.txt; F/permission-native-after.json; F/post-approval-states.json; 2078 |

## Confirmed gaps and source boundaries

Line references were checked in the main checkout during report preparation. A missing boundary is not automatically an isolated root cause.

| Story | Confirmed gap | Detection / rendering boundary |
| --- | --- | --- |
| 2074-b0dc | Claude imperative AskUserQuestion title yields no native title/options payload. | `src-tauri/crates/tuic-terminal/src/output_parser.rs:2554-2561` requires a title ending in `?`; called at `src-tauri/src/pty.rs:2724-2726`. Mobile controls require a choice payload at `src/mobile/components/CommandInput.tsx:455,528-529`. |
| 2075-aa2f | Codex native enforced approval becomes idle while still open. | `src-tauri/src/pty.rs:2723-2735` chooses the parser; `src-tauri/crates/tuic-terminal/src/output_parser.rs:2225-2229,2248-2259,2316-2321` handles footer/options/title. Root cause remains unisolated; do not assert a wrapped-label or transport cause. |
| 2076-8190 | Mobile excludes Codex Chat despite shared desktop support. | `src/mobile/components/useSessionChatView.tsx:24,36-45` accepts only Claude/ego; compare `src/stores/chatView.ts:61-72`. Preserve exact transcript-binding gates. |
| 2077-b132 | Mobile excludes Grok Chat despite shared desktop support. | Same mobile exclusion at `src/mobile/components/useSessionChatView.tsx:24,36-45`; shared support at `src/stores/chatView.ts:61-72`. |
| 2078-464b | Two same-origin mobile tabs exhaust observed HTTP/1.1 connections and delay ordinary state/answer requests. | `src/transport/ipc.ts:357-375` creates one EventSource per subscription. `F/network-requests.json` records six open streams across the two tabs, connection IDs 7673/7678/7649/7763 and 7762/7761, with later ordinary requests lacking responses at capture end. Tab closure releases queued work. This is not proof of faulty native key semantics. |
| 2079-78fa | Desktop terminal Chat omits native question controls that mobile Chat shows. | `src/components/Terminal/Terminal.tsx:1450-1458` passes terminal/session identity; `src/components/Terminal/TerminalChatView.tsx:43-49` mounts the transcript without question state. Compare mobile `src/mobile/components/CommandInput.tsx:528-529`. Same-session pending Claude desktop/mobile DOM confirms the gap; Grok/ego pending-dialog parity is source-shared, not independently live-proven. |

All six existing fix stories include authentic-evidence requirements, named RED regressions and batch GREEN criteria. They are follow-up fixes; this audit does not implement them. The earlier mobile session-binding fix belongs to 2071-174b, not this audit.

## Acceptance-criterion disposition

1. **Supported:** real Q sessions exist for all four in R; real native P dialogs for all four exist in F/permissions.json. Startup failures are not counted as successful sessions.
2. **Not fully supported:** the matrix above records the available evidence, but desktop answer delivery, several pending Chat cells, and some final list badges have no retained per-cell proof. Confirmed unavailable controls are gaps, not passes. A mobile round trip cannot satisfy a desktop round trip. Do not check this criterion or close the story on the current evidence.
3. **Supported by this report:** all agents/surfaces/capabilities are listed; unknown cells remain explicit; six confirmed gaps have source boundaries and story IDs.
4. **Supported:** existing stories 2074-b0dc through 2079-78fa contain RED-ready follow-up criteria. No fix was made here.
5. **Already checked:** the coordinator reports all four final parity sessions closed. R/parity-cleanup.json documents the earlier multiple-choice cleanup. This report starts no session and touches no Boss session.

One additional throwaway session cannot establish missing agent-specific desktop delivery for all four agents. Completion requires the coordinator to authorize and collect the missing evidence, or explicitly revise the audit's acceptance scope. No unavailable cell is silently waived.

## Isolated follow-up attempt (21:18–21:26 local time)

The coordinator authorized fresh disposable sessions to fill unknown cells, using a private headless backend and same-source Vite. The main-checkout build was refused by build-slot's protected-checkout gate. The coordinator then supplied the TUIC-created read-only build source `tuicommander__wt/chore-live-check-main`, revision `445fd7d9475638479f234a42f8d82e07acd8f44a`. Its native headless build succeeded through build-slot/mbx (2m55s build, preceded by 100s admission wait). No installed binary was replaced.

The backend used private port 22021; Vite used 22022. Backend HOME and temporary paths were under `~/Gits/.tmp/results/close-2021-live/`, not Boss's config. Backend and Vite HTTP probes returned 200. The mandated shared-browser wrapper initially created owned tab `59F685D750AB3C921392660F1334877A`, but navigation/eval then timed out. A full-budget recovery produced: `agent-browser --cdp 57162 --pin-tab --json open ... timed out after 60 seconds`. See `~/Gits/.tmp/results/close-2021-browser-recovery.log` and `close-2021-live/browser-transcript.txt`. No alternative browser transport was used to claim DOM evidence.

The four HTTP spawn receipts and `startup.json` are retained under `close-2021-live/`. Grok reached a native colour question. Claude stopped at its external-instruction import decision, Codex at an update notice, and ego exited with `read-only directory: cannot enforce a read-only directory beneath a writable root`. Startup gates are not question-parity passes. No permission or sandbox override was used to evade ego's rejection.

No new matrix cell is checked from this attempt. Three still-live owned PTYs were deleted; ego was already absent. `sessions-after-cleanup.json` is an empty list. The exact owned browser target was closed through its CDP cleanup endpoint; the shared Chrome process was left intact. The first named agent-browser daemon close timed out (`process-cleanup.json`); the final retry after exact target closure succeeded (`daemon-final-close.txt`). Backend and Vite received SIGTERM and were confirmed absent with `ps`. Criterion 2 remains open pending working browser automation and an approved ego launch configuration.


## Current-main retry by live-2021b (23:37–23:42 local time)

The retry fast-forwarded the assigned branch to current main `112ace3f25e67efe02ae8ff2d609e804ea3c5a5a`. A native headless build through build-slot/mbx passed in 2m03s; mbx doctor reported zero failures. The private backend used port 22121, the worktree Vite used port 22122, and backend HOME was `~/Gits/.tmp/results/live-2021b/home`. No installed binary was replaced. The mandated ab-stealth wrapper opened the private page and returned snapshot, eval and screenshot evidence successfully. This retry does not reproduce the earlier browser timeout.

Fresh ego PTY `f13269e2-a47f-4df5-a993-6700b5893c69` exited before the question with the exact error: `read-only directory: cannot enforce a read-only directory beneath a writable root`. TUIC supplies ego's attachment grant with `--read-only-dir` (`src-tauri/src/agent_hook_launch.rs:273,296`). In this retry, the backend-owned grant is beneath the isolated HOME inside `~/Gits`, which is a default writable root for ego. No policy override or alternate launch transport was used to bypass the rejection. An approved ego launch/configuration layout that preserves the read-only grant is required before the complete matrix can run under this isolated-HOME constraint.

`~/Gits/.tmp/results/live-2021b/startup.json` retains API state and native output for all four spawn receipts. Claude was at external-import consent, Codex at its composer, and Grok had started the audit prompt. These are startup observations, not passing question cells. The browser transcript and `startup.png` record the private page, not Boss's desktop app. No criterion was newly checked. Criterion 2 remains open; `story-close.sh --check` reported it unchecked and otherwise passed its format preflight. That preflight is not authorization to close an incomplete story.

Cleanup: the three live owned PTYs were deleted; ego was already absent. `sessions-after-cleanup.json` is `[]`. The owned browser tab and named daemon closed successfully; shared Chrome was left running. Backend PID 77320 and Vite PID 77321 received SIGTERM and a subsequent `ps` found neither. The retry's temporary Vite config was removed. No production code or capture corpus changed, and no Rust or frontend test suite ran.

### Approved root-narrowing retry (23:43–23:45 local time)

The coordinator authorized an isolated ego user config with its sole writable root set to the parity cwd. `EGO_HOME=~/Gits/.tmp/results/live-2021b/ego-home ego config set -- 'roots=[{path="/Users/stefano.straus/Gits/.tmp/results/live-2021b/scratch/ego",access="read-write"}]'` succeeded. `ego-effective.json` records the effective configuration. The fresh native ego launch accepted the read-only attachment directory, resolving the first launch rejection, but then exited with `no stored subscription credential: run ego auth login openai-codex`. Evidence: `r2-startup.json`. No credential was read, copied or linked from Boss's store.

A launch-scoped `-c roots=[{path="/Users/stefano.straus/Gits/.tmp/results/live-2021b/scratch/ego",access="read-write"}]` attempt, without changing Boss's saved configuration, failed validation before startup: `the command line: invalid config in flags layer: roots in project/local layers may only restrict read access` (`r3-startup.json`). Thus the tested installed CLI does not permit this alternative writable-root override. The available `ego auth login [--device] openai-codex` command requires a new login to the isolated store. No login was started. The remaining prerequisite is isolated-store authentication or an approved way to select a separate user configuration while using the existing credential provider; neither is a parity pass.

All retry sessions were deleted (`r2-sessions-after-cleanup.json` is `[]`), the owned `live-2021b-r2` browser tab and daemon closed, and backend PID 26557 plus Vite PID 26559 were stopped. No production code changed and criterion 2 remains unchecked.


## desk-2021 isolated desktop retry (2026-10-20)

**Criterion 2 remains unchecked.** This run resolves the ego login prerequisite and adds real round trips. It does not establish the complete question/permission matrix. A failed binding, a startup gate, an absent approval, or an API payload alone is not a surface pass.

### Runtime and evidence

Evidence root **D**: `~/Gits/.tmp/results/desk-2021/`. `browser-transcript.txt` retains the ordered browser commands and results; `snapshot-01.txt` through `snapshot-34.txt` split its DOM snapshots. JSON captures retain the authoritative `/sessions` collection in `sessions` and native output in `agents.<agent>.output`. The unused `agents.<agent>.session` field is empty because the attempted per-session GET is not a state endpoint; do not use that field as evidence.

- Source: `chore/desk-2021-cells`, pre-documentation HEAD `ded040a205b427d9963c466916a5ff5cea8b70a1`; the requested merge of `chore/live-2021-parity` reported already up to date.
- Authorized `make agent-test AGENT_TEST_INSTANCE=at-2021 AGENT_TEST_PORT=19921` built and launched the isolated native macOS app. `mbx doctor` reported zero failures. API calls used its printed Unix socket. Browser checks used its embedded frontend at `127.0.0.1:19921`, not Boss's app, not port 9876, and not a Vite server. These desktop-shell browser checks do not prove the native IPC path or native window pixels.
- The first launch exited with status 0 before browser capture; the initial browser navigation and subsequent socket/TCP probes failed with connection refused. Cause unknown. See `runtime-ended.json` and `agent-test.log`. A fresh retry ran until explicit cleanup.
- The mandated wrapper repeatedly reported **macOS screen is locked**. DOM snapshots and several clicks worked, but browser commands also timed out and the backend logged sleep/wake gaps. No screenshot is claimed. Canvas content and visual badge colour remain unverified. Desktop tab DOM classes `awaitingInput`/`awaitingQuestion` were retained in the transcript; a status bar reading Ready is not substituted for question state.
- `ego auth status` reported an existing openai-codex OAuth login. Ego used its default home and scratch cwd under D; no credential store was read or copied and no saved ego configuration changed. The agent-test upload grant is outside Gits and native ego accepted it. The isolated TUIC instance initially had no `ego_executable`; only its own config was set through `/config` to the installed absolute executable. After that, ego Chat opened successfully.
- Q receipts: Claude `4db5e9d2`, Codex `8be58af6`, Grok `e7fffde7`, ego `415d06bf`. P receipts are in `permission-receipts.json`. These are disposable PTY agents, not ACP conversations.

### Added per-cell evidence

Q = native colour question; P = enforced harmless printf approval. This table records this retry only; historical R/F evidence above remains separate. “Unknown” is deliberately not a pass. List rows are navigation surfaces, not answer controls.

| Agent | Surface | Detection / badge and options | Native answer evidence / remaining gap | D artifacts |
| --- | --- | --- | --- | --- |
| Claude | Desktop terminal | Q API awaiting; terminal canvas mounted; tab awaiting classes captured. Canvas options not pixel-verified. | Native Enter cleared awaiting, but retained output was still in PostToolUse hooks and does not prove a selected colour. P delivery unknown. | `final-questions.json`, `snapshot-34.txt`, `desktop-terminal-claude-after.json` |
| Claude | Desktop Chat | Q Red/Green/Blue and native text/chat alternatives; P Yes/No rendered in Native terminal question region. | Green Q confirmed in native output; P printf and PARITY_P_DONE confirmed after Yes tap. | `round2-api.json`, `snapshot-22.txt`, `snapshot-33.txt`, `desktop-chat-round2.json`, `permission-final.json` |
| Claude | Mobile list | Q Input shown. Initial narrow native viewport exported no title/options; explicit resize restored them. | Navigation only; no answer controls expected. P list not separately retained. | `questions-before.json`, `snapshot-06.txt`, `snapshot-12.txt`, `claude-mobile-chat-before.json` |
| Claude | Mobile CLI | Q Input; narrow native dialog showed generic Yes/No instead of all colour choices. | No new CLI-specific Q/P answer round trip. Earlier R/F evidence is not relabelled as this retry. | `snapshot-13.txt`, `questions-before.json` |
| Claude | Mobile Chat | After native resize, Q title and all five native choices rendered with Input header. | Blue tap confirmed by native User answered and PARITY_Q_DONE Blue. P not repeated here. | `snapshot-14.txt`, `claude-mobile-chat-before.json`, `desktop-before.json` |
| Codex | Desktop terminal | P API awaiting and terminal canvas mounted. Q native output retained. Canvas options not pixel-verified. | No terminal-specific answer round trip. | `desktop-before.json`, `permission-before.json`, `snapshot-31.txt` |
| Codex | Desktop Chat | Q four choices with separate hints; P three native approval options rendered. | Green Q confirmed in native output; P Yes tap executes printf and produces PARITY_P_DONE. | `snapshot-20.txt`, `snapshot-32.txt`, `desktop-chat-round2.json`, `permission-final.json` |
| Codex | Mobile list | First real /plan Q was visibly open in native output, but API/list reported Idle. Later repeated Q was correctly exported. | This first-question mismatch is not an all-pass detection result. P list not separately retained. | `questions-before.json`, `snapshot-06.txt`, `snapshot-09.txt`, `desktop-before.json` |
| Codex | Mobile CLI | First Q title was in native output but no choice controls rendered; generic keyboard remained. | No CLI-specific answer in this retry. | `snapshot-10.txt`, `questions-before.json` |
| Codex | Mobile Chat | Chat opens with exact transcript binding, but first Q had no choices or Input badge because API was idle. | Generic Enter from Chat selected Red; native Questions answered and PARITY_Q_DONE Red confirm delivery. P Chat not repeated. | `snapshot-11.txt`, `mobile-after.json` |
| Grok | Desktop terminal | Q API exports choices; canvas mounted and awaiting tab classes retained. | Trusted browser key 2 reached the native selector; Green confirmed. No fresh P approval: default-mode printf ran without a dialog. | `round2-api.json`, `snapshot-23.txt`, `grok-desktop-terminal-after.json`, `permission-before.json` |
| Grok | Desktop Chat | Native Q still open, but Chat closes with no Grok session is registered for this terminal. | Blocked by exact transcript binding; no Chat answer pass. P Chat unproven. | `snapshot-21.txt`, `snapshot-23.txt`, `desktop-chat-round2.json` |
| Grok | Mobile list | Q Input, title and awaiting list entry rendered. | Navigation only. Fresh printf produced no P dialog; not a permission pass. | `snapshot-06.txt`, `snapshot-09.txt`, `questions-before.json`, `permission-before.json` |
| Grok | Mobile CLI | Q Red/Green/Blue/z rendered with Input header. | Green tap confirmed in native output and awaiting retracted. | `snapshot-07.txt`, `grok-mobile-cli-after.json`, `mobile-after.json` |
| Grok | Mobile Chat | Toggle exists, but Chat closes with the same exact-binding error and returns to CLI. | Unavailable in this session; the subsequent Green tap is a CLI answer, not Chat delivery. | `snapshot-08.txt`, `questions-before.json`, `grok-mobile-cli-after.json` |
| ego | Desktop terminal | Q native selector/API payload retained; terminal canvas mounted. | No terminal-specific Q/P answer round trip. Later Q repeat failed with HTTP response body failed; no recovery/pass inferred. | `snapshot-16.txt`, `desktop-before.json`, `final-questions.json` |
| ego | Desktop Chat | After isolated TUIC executable configuration, Q transcript and Red/Green/Blue controls rendered. | Blue tap confirmed by native PARITY_Q_DONE Blue. P desktop Chat not exercised. | `snapshot-19.txt`, `desktop-before.json`, `desktop-chat-after.json` |
| ego | Mobile list | Q Input/title and P Input/Allow run printf question rendered. | Navigation only. | `snapshot-02.txt`, `snapshot-24.txt`, `snapshot-27.txt`, `questions-before.json`, `permission-startup.json` |
| ego | Mobile CLI | Q and P header actions expand the correct native options. | Green Q and one-time y approval both reach ego; printf exits 0 and prints marker. Later progress-tool approval is distinct, not stale printf approval. | `snapshot-04.txt`, `snapshot-29.txt`, `ego-mobile-cli-after.json`, `permission-final.json` |
| ego | Mobile Chat | Transcript and expanded Q Red/Green/Blue controls render after isolated TUIC executable configuration. | Red tap confirmed in native PARITY_Q_DONE Red despite a transient provider retry. P Chat is historical F evidence only. | `snapshot-26.txt`, `grok-desktop-terminal-after.json`, `ego-mobile-chat-after.json` |

### Proven limits and follow-up boundaries

1. **Grok Chat cannot be audited in these sessions.** Both surfaces return the same binding error while native Q/API detection works. `src-tauri/src/chat_view/mod.rs:312-315` requires `grok_transcript`; absence fails closed. This is the proven layer, not proof that the registry, process lookup, or producer is defective. No guessed binding or fallback was installed.
2. **Initial detection is not uniformly successful.** The first Codex /plan dialog had API Idle and no choice payload; a repeat exported the complete payload. The initial narrow Claude viewport omitted choices; an explicit 41×128 resize restored them. The earlier failed states remain evidence, not waived startup noise. Root causes are not established by this audit.
3. **Pixel checks and remaining independent round trips are incomplete.** The locked screen prevented reliable canvas evidence; later ego stream failure and repeated browser/API timeouts limit continuation. Desktop terminal answers for Codex/ego, several independent permission answers, and Grok Chat are still not proven. No original acceptance scope was revised, so criterion 2 must not be checked from this run.
4. **No new enforced Grok permission was produced.** Explicit default permission mode still ran printf without approval. This does not verify approval detection or answer delivery; the historical F enforced-approval evidence remains the only such Grok evidence.

### Cleanup and validation

All eight retry sessions were explicitly deleted; `cleanup.json` retains responses and `sessions-after-cleanup.json` is `[]`. The first-launch receipts were already absent in `retry-startup.json`. `make agent-test-stop` requested shutdown; the supervisor removed its PID file and stopped (the supervising make reports Error 143 / exit 2 after the requested SIGTERM). No test instance remains.

The owned wrapper tab initially refused close with Cannot close the last tab. The named `ab-shared-desk-2021-81136` daemon then closed successfully; the exact owned target `B698ED61B198B1E387870653C3293E27` was closed through its CDP cleanup endpoint. Shared Chrome PID 81136 was left running. No Vite or headless backend was started. No Boss session, installed executable, production source or capture corpus changed. No Rust/Vitest/TypeScript suite ran; this is live audit evidence plus documentation checks, not a suite pass.

## desk-2021b completion audit (2026-10-21)

This section resolves the remaining evidence cells. Earlier tables are dated observations, not the final disposition. **The collection for criterion 2 is complete, with failures recorded below. This is not an all-pass parity result.** An unavailable Chat surface is a failed cell, not a successful answer and not an untested cell. The mobile list is a navigation surface: options and answer delivery are not applicable there.

### Runtime and retained evidence

Evidence root **B**: `~/Gits/.tmp/results/desk-2021b/`. Source and native build: `023ca2c650`, branch `chore/desk-2021-cells`. `make agent-test AGENT_TEST_DRY_RUN=1 AGENT_TEST_INSTANCE=at-2021b AGENT_TEST_PORT=19931` described the isolated launch; the matching start built through build-slot/mbx and reported ready. `agent-test.log` retains the build and launch. No installed binary was replaced.

The worktree Vite frontend ran on port 19932. Its HTTP/WS proxy connected to the test instance's printed Unix socket, not Boss's backend. Browser checks used the mandated ab-stealth wrapper with `AB_SESSION=desk-2021b`; one owned tab was reused for desktop and mobile. Desktop viewport was 1400×900 and mobile viewport was 390×844. Screenshots and DOM commands succeeded in this run. The first two navigations preceded Vite readiness and returned connection refused; subsequent navigation worked. No screen-lock failure or interrupted round trip is claimed for this run.

`browser-transcript.txt` retains ordered navigation, DOM, focus and trusted keyboard operations. `snapshot-01.txt` through `snapshot-24.txt` split its DOM snapshots. The JSON files contain the authoritative `/sessions` collection and per-agent native output. `receipts.json` identifies all ten owned PTYs. Agents used scratch directories named `parity-<agent>` with permission/retry suffixes under B. These are PTY conversations, not ACP AI Chat.

Default ego home and existing login were used; no credential store was read or copied. Only the isolated TUIC configuration was changed to select the installed ego executable. Claude permission sessions used a scratch `.claude/settings.json` with `permissions.ask=["Bash"]`. Grok initially executed printf without a dialog even with default permission mode, including subshell and absolute-path variants; these are not permission passes. A fresh Grok session with scratch `.claude/settings.local.json` containing ask rules produced the real five-option approval. Its own workspace trust prompt was answered before the audit command. No saved global permission rule was added and no always-allow option was selected.

### Final cell disposition

Each entry records API detection, badge/state, options and delivery, in that order. D, F and R refer to the earlier evidence roots. Existing proven cells are cited, not silently repeated or upgraded. A **FAIL** is a measured unavailable or incorrect capability. Native canvas screenshots are retained as evidence; they do not establish the native Tauri IPC transport or Boss's desktop window.

| Agent | Surface | Q disposition and evidence | P disposition and evidence |
| --- | --- | --- | --- |
| Claude | Desktop terminal | API awaiting; awaiting tab classes captured on the repeat; native canvas captured; trusted Enter selects Red, native User answered and PARITY_Q_DONE Red. B `q-ready.json`, `q-badges.json`, `desktop-q-answers.json`, `completed-roundtrips.json`, `claude-desktop-terminal-q.png`, `claude-desktop-q-badge.png`, browser transcript. | Enforced native Yes/No captured; desktop canvas Enter executes printf and PARITY_P_DONE. Shared detection is also captured on the independent second approval, not inferred from command completion. B `mobile-p-before.json`, `desktop-p-answers.json`, `claude-desktop-terminal-p.png`, browser transcript; historical F desktop store awaiting. |
| Claude | Desktop Chat | API awaiting; Q controls and Green delivery already proven. D `round2-api.json`, `snapshot-22.txt`, `desktop-chat-round2.json`. | API awaiting; Yes/No and approval delivery already proven. D `snapshot-33.txt`, `permission-final.json`. |
| Claude | Mobile list | API awaiting and Input badge already proven. D `questions-before.json`, `snapshot-06.txt`, `snapshot-12.txt`. Options/delivery N/A. | API awaiting; Input and Do you want to proceed rendered. B `mobile-p-before.json`, `snapshot-11.txt`. Options/delivery N/A. |
| Claude | Mobile CLI | **FAIL** native colour controls in the captured narrow dialog; API awaiting but exported title/options missing, generic Enter selects Red. R `parity-all-states.json`, `claude-mobile-dom.txt`, `parity-after-states.json`; D `snapshot-13.txt`. | API awaiting; Input; native Yes/No buttons; tap 1 Yes executes second printf and PARITY_P2_DONE. B `mobile-p-before.json`, `snapshot-12.txt`, `mobile-claude-p-answer.json`, `completed-roundtrips.json`. |
| Claude | Mobile Chat | API awaiting; Input; colour controls; Blue confirmed in native output. D `snapshot-14.txt`, `claude-mobile-chat-before.json`, `desktop-before.json`. | API awaiting; Input; Yes/No; command completes after Chat approval. F `permissions.json`, `claude-permission-chat-single-tab-dom.txt`, `permission-native-after.json`. |
| Codex | Desktop terminal | API awaiting; awaiting tab classes captured on repeat; native canvas captured; trusted Enter selects Red and native PARITY_Q_DONE Red. B `q-ready.json`, `q-badges.json`, `desktop-q-answers.json`, `codex-desktop-terminal-q.png`, `codex-desktop-q-badge.png`, browser transcript. | API exports all three approval choices; native canvas captured; trusted Enter produces You approved, command output and PARITY_P_DONE. B `p-ready.json`, `desktop-p-answers.json`, `codex-desktop-terminal-p.png`, browser transcript. A nonawaiting tab class was also observed before answer; see live-update limit below. |
| Codex | Desktop Chat | API awaiting; four choices and Green delivery already proven. D `snapshot-20.txt`, `desktop-chat-round2.json`. | API awaiting; three approval choices and printf delivery already proven. D `snapshot-32.txt`, `permission-final.json`. |
| Codex | Mobile list | **FAIL** first native /plan question was Idle with no choices; repeat detection works. D `questions-before.json`, `snapshot-06.txt`, `snapshot-09.txt`, `desktop-before.json`. Options/delivery N/A. | API awaiting; Input and approval title rendered. B `mobile-p-before.json`, `snapshot-11.txt`, `snapshot-13.txt`. Options/delivery N/A. |
| Codex | Mobile CLI | API/controls on a later Q and Green answer are proven in R; the first-Q detection failure remains recorded, not waived. R `codex-mobile-dom.txt`, `codex-after-mobile-answer.json`, `codex-after-output.json`; D `snapshot-10.txt`. | Historical **FAIL** detection/overlay: Idle and no options while native approval was open; generic Enter still approves. F `permissions.json`, `codex-before-enter-dom.txt`, `codex-after-enter-dom.txt`. B `snapshot-14.txt` independently shows all three native options and Input on the later runtime state. |
| Codex | Mobile Chat | First-Q detection/controls **FAIL**; Chat itself opens with exact binding and generic Chat Enter reaches Red. D `questions-before.json`, `snapshot-11.txt`, `mobile-after.json`. | API awaiting; Input; Chat transcript and three matching approval buttons; tap 1 Yes executes second printf and PARITY_P2_DONE. B `mobile-p-before.json`, `snapshot-15.txt`, `codex-mobile-chat-p-after.json`, `completed-roundtrips.json`. |
| Grok | Desktop terminal | API awaiting; awaiting tab classes and native canvas retained in B `q-ready.json`, `final-evidence.json`, `grok-desktop-terminal-q.png`, browser transcript. Independent desktop key 2 → Green already proven in D `grok-desktop-terminal-after.json`, `snapshot-23.txt`. | API awaiting; five choices; awaiting tab classes after reload; native canvas captured. Trusted key 3 approves once; native output shows PARITY_GROK_P4_OK and PARITY_P4_DONE. B `grok-p4-waiting.json`, `grok-mobile-chat-p.json`, `grok-desktop-terminal-p.png`, `grok-desktop-terminal-p-after.json`, `completed-roundtrips.json`, browser transcript. |
| Grok | Desktop Chat | **FAIL** exact transcript binding; no Chat options or answer path. D `snapshot-21.txt`, `snapshot-23.txt`, `desktop-chat-round2.json`. B `snapshot-07.txt` also records disabled Chat during native Q. | API confidently awaiting; **FAIL** Chat is disabled with unbound-transcript title, so Chat options/delivery are unavailable. B `grok-p4-waiting.json`, `snapshot-21.txt`, browser transcript. Native options and delivery belong to terminal, not Chat. |
| Grok | Mobile list | API awaiting; Input/title already proven. D `questions-before.json`, `snapshot-06.txt`, `snapshot-09.txt`. Options/delivery N/A. | API awaiting; Input and Print the parity probe token rendered. B `grok-mobile-chat-p.json`, `snapshot-22.txt`. Options/delivery N/A. |
| Grok | Mobile CLI | API awaiting; Input; Red/Green/Blue/z; Green delivery already proven. D `snapshot-07.txt`, `grok-mobile-cli-after.json`, `mobile-after.json`. | API awaiting; Input; all five matching native choices captured in B `grok-mobile-chat-p.json`, `snapshot-23.txt`. Independent CLI approval delivery already proven in F `grok-permission-cli-dom.txt`, `grok-after-permission-dom.txt`, `post-approval-states.json`. |
| Grok | Mobile Chat | **FAIL** Chat closes with no Grok session is registered; no Chat options/delivery. D `snapshot-08.txt`, `questions-before.json`, `grok-mobile-cli-after.json`. Subsequent CLI answer is not Chat delivery. | API awaiting; Input; **FAIL** Chat closes with the same binding error; the five controls remaining after closure belong to CLI. B `grok-mobile-chat-p.json`, `snapshot-24.txt`. No Chat answer is claimed. |
| ego | Desktop terminal | API awaiting; awaiting tab classes; native selector canvas; trusted browser Enter selects Red and PARITY_Q_DONE Red. B `q-ready.json`, `snapshot-04.txt`, `ego-desktop-terminal-q.png`, `desktop-q-answers.json`, browser transcript. | API awaiting and tab awaiting classes, but **FAIL** initial question title/options payload when approval footer wraps. Native canvas and y still work: printf exits 0 with PARITY_P_OK. B `p-ready.json`, `snapshot-09.txt`, `ego-desktop-terminal-p.png`, `desktop-p-answers.json`. Later MCP approval is a different prompt, not stale printf state. |
| ego | Desktop Chat | API awaiting; colour controls and Blue delivery already proven after isolated executable selection. D `snapshot-19.txt`, `desktop-before.json`, `desktop-chat-after.json`. | API confidently awaiting with all five choices on shorter P2; tab awaiting classes present; **FAIL** desktop Chat disabled with unbound-transcript title. No desktop Chat option/answer path. B `ego-desktop-chat-p-unavailable.json`, `snapshot-16.txt`, `snapshot-20.txt`, `ego-desktop-chat-p-disabled.png`, browser transcript. Mobile Chat opens for this same pending PTY (`snapshot-19.txt`), so this is not an unauthenticated ego launch. |
| ego | Mobile list | API awaiting; Input/title already proven. D `questions-before.json`, `snapshot-02.txt`. Options/delivery N/A. | API awaiting; Input/title already proven in D `permission-startup.json`, `snapshot-27.txt`; B `snapshot-13.txt` also retains shorter P2. Options/delivery N/A. |
| ego | Mobile CLI | API awaiting; Input/header action expands all three choices; Green delivery already proven. D `snapshot-04.txt`, `ego-mobile-cli-after.json`. | API awaiting; Input/header action expands y/a/n/e/d; one-time y executes printf. D `snapshot-29.txt`, `permission-final.json`. |
| ego | Mobile Chat | API awaiting; Input/header action; Red/Green/Blue; Red delivery already proven. D `snapshot-26.txt`, `grok-desktop-terminal-after.json`, `ego-mobile-chat-after.json`. | API awaiting; Input/header action with y/a/n/e/d; Chat approval eventually executes printf. F `ego-permission-chat-dom.txt`, `permission-native-after.json`, `post-approval-states.json`. B `snapshot-19.txt` independently confirms Chat binds the shorter pending P2. |

### Failed cells and source boundaries

- **New, decisive parser gap:** ego's actual newline-bearing printf approval hard-wraps the a-option onto another physical row at 128 columns. `p-ready.json` retains native Allow/Risk/y-a/n-e-d but API has an empty question title and no `choice_prompt`. The shorter P2 exports all five choices. `src-tauri/crates/tuic-terminal/src/ego_question.rs:160-161` requires Risk at exactly the third-last nonblank row, and `:185` joins exactly two footer rows. The additional footer row violates that fixed shape. This audit makes no parser change; the coordinator must file the fix with authentic replay evidence.
- **New desktop/browser Chat availability gap:** ego P2 is confidently awaiting and opens in mobile Chat, but desktop Chat remains disabled, including after page reload and mobile binding. The disabled title is The agent session is not bound to a transcript yet. `src/stores/chatView.ts:70-72` gates on `agentSessionId` for ego; `src/hooks/useAppInit.ts:1124-1138` restores agent type and awaiting state but no ego session ID. This identifies the rendering gate and restore boundary, not a proven complete producer root cause. The coordinator must assess/file this separately; no guessed ID or fallback was installed.
- **Existing Grok binding limitation, now P-proven:** native P is detected and mobile CLI controls work, but mobile Chat closes and desktop Chat is disabled. `src-tauri/src/chat_view/mod.rs:312-315` requires the registered Grok transcript. This extends the earlier Q evidence to P; it does not establish why registration is absent.
- **Observed live-update mismatch, not attributed to sleep:** before reload, some desktop tab DOM classes remained shellBusy/shellIdle despite API awaiting; reload restored awaitingInput/awaitingQuestion. B retains both ordered class dumps. `src/hooks/useAgentPolling.ts:79-106` applies snapshot awaiting state; `:119-129` consumes state-change events; `src/hooks/useAppInit.ts:1134-1136` restores awaiting on mount. The report does not infer a new backend detection defect, a native IPC defect, or a sleep/wake root cause from that browser-only mismatch. No failed answer was hidden behind it.

The existing first Codex question, narrow Claude selector and Grok binding failures remain failures. Later successful cells do not erase them. No production implementation, fixture corpus, permission bypass or installed executable changed.

### Cleanup and criterion 2

All ten owned PTYs were deleted. `cleanup.json` retains responses and `sessions-after-cleanup.json` is `[]`. `make agent-test-stop AGENT_TEST_INSTANCE=at-2021b AGENT_TEST_PORT=19931` requested shutdown; its desktop PID file disappeared and supervisor/Vite PIDs were absent afterward. The owned browser tab and named `ab-shared-desk-2021b-9981` daemon closed successfully; shared Chrome was not stopped. The temporary Vite configuration was removed. No Boss session or window was used.

Criterion 2 can be checked for **evidence collection**, including failed cells and N/A list controls. It does not certify all capabilities as working, waive failed binding gates, prove native IPC/pixels, or close the follow-up fixes. No Rust, Vitest or TypeScript test suite was run for this documentation audit; the authorized native agent-test build and live UI/API round trips are the validation evidence.

## Awaiting badge investigation (2105-f1e5, 2026-10-21)

The earlier `desk-2021b` browser transcript records tab classes that disagree with the session API. It does not retain the event or store-update sequence needed to locate the loss. Its temporary `.tmp/parity-vite.config.ts` was removed after the audit. The observation alone does not prove either a native IPC defect or a proxy defect.

A fresh check used source revision `f9a028f990`, `make agent-test AGENT_TEST_INSTANCE=at-2105 AGENT_TEST_PORT=19945`, and a worktree Vite page on port 19946. The proxy used only the isolated instance's Unix socket. The required stealth wrapper controlled one owned browser tab. One real Claude PTY ran under `~/Gits/.tmp/results/fix-2105-evidence/scratch/parity-claude`. The native startup import-consent dialog was answered with its selected **No, disable external imports** option; no credential store was read. No Boss session or desktop window was inspected.

Before spawning Claude, browser diagnostics recorded an independent `session-state-changed` SSE stream, calls to the existing terminal store's `update` method, and tab class changes through `MutationObserver`. These diagnostics did not inject a lifecycle state or call the awaiting handler. The native question then produced this sequence without navigation or reload:

| Observation | Browser timestamp (Unix ms) |
| --- | --- |
| Tab becomes `shellBusy` | 1792555628227 |
| Independent SSE receives authoritative `awaiting_input=true` | 1792555638063 |
| Application writes `awaitingInput="question"` to `term-1` | 1792555638063 |
| Tab has `awaitingInput` and `awaitingQuestion`, without `shellBusy` | 1792555638066 |

The observed SSE-to-DOM interval was 3 ms. A later same-page API read and DOM read also showed `awaiting_input=true` and the awaiting classes together. This is a passing fresh transition, not a reproduction or a fix of the earlier mismatch. There was no independent API polling timestamp for the first instant that the backend became awaiting; the transition timing above uses its authoritative SSE snapshot. The capture does not establish native WebView pixels or IPC delivery.

The inspected path is `state.rs:4926-4975` (accumulator publishes a deduplicated snapshot to both transports), `mcp_http/sse_routes.rs:562-565` (shared payload), `useAgentPolling.ts:123-129,80-101` (session routing and store update), and `TabViews.tsx:133-135` (awaiting excludes the busy class). The proxy recorded successful HTTP 204 responses to `/events/types`, including the late subscription after the terminal appeared. `transport/sse.ts:132-168` shows why that route is relevant to a late consumer; its failure in the original audit is **not** established.

Evidence is retained under `~/Gits/.tmp/results/fix-2105-evidence/`: `transition-proof.json`, `api-awaiting.json`, `browser-transcript.txt`, `proxy.ndjson`, `claude-awaiting.png`, the launch/proxy/probe scripts, and cleanup receipts. The native agent-test build passed through build-slot/mbx. No Rust or frontend test suite ran, no regression test was invented for an unproven cause, and no product code or capture corpus changed. The original loss boundary remains unresolved; story criterion 1 must stay open until its event/store trace or an equivalent failing reproduction is available.

Cleanup deleted the owned Claude PTY (`sessions-after-cleanup.json` is `[]`), closed the owned browser tab and named daemon, stopped Vite, and requested `make agent-test-stop AGENT_TEST_INSTANCE=at-2105 AGENT_TEST_PORT=19945`. The shared Chrome and Boss's instance were left running.

### Recovered proxy configuration: harness cause established

The coordinator subsequently supplied the exact audit session transcript at `~/.ego/sessions/01a15c60-3644-75d1-b0da-7c1a0f1f7078/events.jsonl`. Only events containing the named configuration path were inspected. Event-file line 262 (record sequence 261) contains the original shell write of `.tmp/parity-vite.config.ts`; line 289 restarts Vite without changing the file; line 969 removes it during cleanup. No later edit of that path appears among those matching events. The recovered file is retained as `original-parity-vite.config.ts` in the evidence directory, SHA-256 `d13c33532ed762179b43a79de59ea324f64688ae876702c1aaa6ec0898f49c7c`.

**This resolves the earlier attribution gap: the audit proxy excludes the application's SSE requests.** The configuration's line 3 ends its route-root regex with `(?:/|$)`, not `(?:/|\\?|$)`. The application constructs `/events?types=...&stream_id=...` at `src/transport/sse.ts:76-78`. Vite tests that regex against the full `req.url`, including the query (`node_modules/vite/dist/node/chunks/node.js:19363-19367,19444-19448` in the installed dependency). Thus the SSE request is not sent to the Unix socket at all. It cannot deliver an awaiting transition to the tab store.

A Node probe decoded the actual JavaScript string literal from the recovered configuration and applied its regex, rather than transcribing a replacement pattern. `original-proxy-match.json` retains these results:

| Request | Original proxy matches |
| --- | --- |
| `/events` | Yes |
| `/events?types=session-state-changed&stream_id=probe` | **No** |
| `/events/types` | Yes |
| `/sessions` | Yes |
| `/sessions/<id>/stream?format=grid` | Yes |

This explains why API snapshot reads, terminal WebSockets, and reload-time hydration could work while live lifecycle pushes did not. It is a harness routing defect, not evidence of a product awaiting-state defect. The fresh check above used a query-aware boundary and recorded successful delivery through SSE, the store and DOM. No product code or regression test is warranted for this audit failure. The prior unresolved conclusion is superseded by this recovered configuration evidence, not by the passing run alone.

Story disposition: criterion 1 is proven by the recovered config and Vite matcher. The product-only RED condition does not apply. The live check retains concurrent API JSON and DOM state plus the 3 ms authoritative SSE-to-DOM interval; it does not claim a separately polled timestamp for the first API-true instant. The coordinator owns the final harness/wontfix classification and must not describe this documentation commit as a product fix.

Suggested memory: `$wiz:remember` can retain that Vite regex proxy contexts match the full request URL: route-root boundaries must accept `?` to forward queried SSE endpoints.
