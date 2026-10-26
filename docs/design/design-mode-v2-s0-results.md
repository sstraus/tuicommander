# Design Mode v2 S0 results

Date: 2026-10-08. Plan: `docs/design/design-mode-v2.md` (§4, §5, §9, §13 S0, §14).
Native macOS evidence only; Windows/Linux and branded Google Chrome were not tested.

**Current S0 acceptance update (2026-10-24):** the owned-CFT acceptance scope
for story 1672 AC1 is now verified. Round 16's fixed-build continuation below
proves native-folder installation and enabled persistence after a no-load-flag
restart. Round 12 proves real side-panel tab return, trusted top/frame picking,
one-shot/Escape/Stop chronology and a nonempty top PNG crop through the real
native-host installation. The crop uses the explicitly granted all-sites capture
permission, not a newly proved temporary activeTab grant. The original Edge,
echo-host and initial blocked findings below are historical evidence, not a
claim that these later CFT checks are still blocked. Windows/Linux, branded
Chrome and a fresh Edge-only replay remain untested.

**Gate recommendation: retain X-A with native messaging, provisionally.** Edge
loaded the extension, resolved the custom-profile host, and kept the worker's
native port alive. There is no measured channel failure justifying the HTTP
fallback. Source-map discovery worked on this Vite fixture without CDP, so there
is no measured reason to select X-B. **S0 is not fully passed:** GUI persistence,
panel gestures/tab switching, trusted picking, and successful screenshot cropping
remain blocked. Keep the product CDP backend until those checks pass; this spike
authorises no deletion or product rollout.

The native host is an **echo-only stdio adapter**, permitted by the spike brief.
It does not relay to `mcp.sock`; instance handshake and real read-only socket relay
remain unproven. The spike telemetry collector is unrelated to §5.1's product
HTTP fallback.

The raw event log redacts the ephemeral fixture Vite WebSocket token; timestamps,
nonces and event content are preserved. The final MAIN probe omits URL query
strings to avoid recording that token in subsequent runs.

Evidence paths below are relative to `spike/design-mode-v2-s0/`. The exact Edge
argv and server commands are in [launch.json](../../spike/design-mode-v2-s0/evidence/launch.json).

| Item | Browser and exact version | Result | Command / step | Evidence and limits |
|---|---|---|---|---|
| (a) Flag load and Load unpacked persistence | Microsoft Edge **154.0.4258.62**; GUI attempt: Google Chrome for Testing **149.0.7827.54** | **PASS flag load; BLOCKED GUI persistence** | Edge launch from `evidence/launch.json`, with `--load-extension=<extension>` and dedicated `--user-data-dir`. Via wrapper: `chrome://extensions`, Developer mode, Load unpacked. | `events.jsonl`: Edge `worker_boot`, content injection, native reply. Restart with the same flag also loaded it. Native folder chooser is outside agent-browser's CDP tree; MacControl lacks Accessibility permission. No directory selected and no no-flag restart of a GUI-installed extension: persistence is unproven. |
| (b) Side panel, gesture, tab switch | Microsoft Edge **154.0.4258.62** | **PASS API presence; BLOCKED interaction** | Worker reads `typeof chrome.sidePanel.open`, calls `setPanelBehavior({openPanelOnActionClick:true})`; extension includes panel + Pick button. | `worker_boot.sidePanel="function"`, no `panel_error` observed. API availability is not proof of opening. Native Edge gesture and tab return could not be driven with available permissions. CFT has no loaded extension because its folder chooser is blocked. |
| (c) Custom-profile host registration | Microsoft Edge **154.0.4258.62** | **PASS (echo)** | `python3 spike/design-mode-v2-s0/setup.py ~/Gits/.tmp/dm2-s0/edge-profile`; worker `connectNative('dev.tuic.dm2_s0')`. | `native_reply` echoes nonce and origin `chrome-extension://jnonmjddknoldgkhmfialhdimebopeop/`. The only spike host manifest is `<user-data-dir>/NativeMessagingHosts/dev.tuic.dm2_s0.json`; no global/user browser registration written. Real TUIC relay is untested. |
| (d) Native port keeps worker alive | Microsoft Edge **154.0.4258.62** | **PASS for measured 75 s interval** | Host replies immediately, then sleeps 75 s before another reply. No Edge CDP/worker inspector attached; no periodic extension timer. | Same worker nonce `a12a134f-b8dd-4f37-bddb-da277705b684` receives echo at `1791462407001` and delayed reply at `1791462482000` (74.999 s). Last intervening HMR message at `1791462432495`, leaving 49.505 s without extension event traffic before delayed reply. Does not prove crash recovery or indefinite lifetime. |
| (e) Cross-origin iframe picker | Microsoft Edge **154.0.4258.62** | **PASS synthetic protocol probe; BLOCKED trusted/visual pick** | Top page `http://127.0.0.1:19340/`, iframe `http://localhost:19340/frame.html?s0pick=1`; ISOLATED fixture probe arms and dispatches one click through the picker handler. | `pick` from frameId **2**, URL on localhost, selector `#target`, text `Frame target`; following `picker_stopped.armed=false`. Different hostnames make different origins. The background frame returned a zero rect: no coordinate/crop success is claimed. Real pointer hit testing, overlay rendering, Escape and frame transforms remain unverified. |
| (f) captureVisibleTab + crop | Microsoft Edge **154.0.4258.62** | **FAIL without gesture (expected); BLOCKED successful crop** | Worker attempts capture at page DOMContentLoaded with localhost host permissions and declared activeTab, before any extension action gesture. | `capture_error`: `Either the '<all_urls>' or 'activeTab' permission is required.` Declaring activeTab or granting localhost host access is insufficient here; the action must grant activeTab. No successful capture or PNG crop exists. Do not broaden permissions merely to bypass this check. |
| (g) extract.js as plain function | Microsoft Edge **154.0.4258.62** | **PASS DOM extraction; framework hints separate** | Byte-copy product `extract.js`; content script calls `tuicExtract.call(target)`. | `plain_extract` records selector, paths, snippet, styles and top-level rect `{x:32,y:102.015625,width:180,height:60}`. Source is empty in ISOLATED; MAIN `main_hint.source="fixture/app.ts"` separately finds the page-world Vue-shaped expando. The expando is an explicit probe fixture, not evidence of a full Vue application. |
| (h) Vite HMR probe | Microsoft Edge **154.0.4258.62** | **PASS** | MAIN wraps WebSocket at document_start. Append a comment to `fixture/app.ts` after load. | `hmr` records `connected` and actual `js-update` for `/app.ts` in top frame and iframe at `1791462432495`. This proves observation, not successful application of a product UI change or reload policy. |

## Round 2 permission recheck (2026-10-08)

**BLOCKED before browser launch.** Boss reported that MacControl already has its
permissions. The new live checks still refuse capture. The exact help and error
responses are preserved in
[maccontrol-round2.txt](../../spike/design-mode-v2-s0/evidence/maccontrol-round2.txt).

- Screen state was checked first: `ioreg -n Root -d1` reports
  `"IOConsoleLocked" = No`, with an active, logged-in console session.
- `maccontrol.help({})` succeeds and returns its normal usage instructions.
- `maccontrol.capture_screen({})` and `maccontrol.list_windows({})` both return
  `isError: true` with the exact text:
  `Screen Recording permission required. Grant access in System Settings > Privacy & Security > Screen Recording`.
- `maccontrol.processes({name:"mcpmaccontrol"})` identifies two running processes
  of `/Applications/MCPMacControl.app/Contents/MacOS/mcpmaccontrol`: PID **51489**
  (server; `--server` in `ps`) and PID **34336** (no server flag). `codesign`
  identifies the app as **com.sstraus.mcpmaccontrol**, with an ad-hoc signature.
  This is the installed app to inspect in Privacy & Security > Screen Recording.
  **Neither the error nor the available TCC logs identifies the denied process:**
  a targeted 10-minute TCC log query returned no matching attribution record.
  The exact responsible process and why the existing grant is ineffective remain
  unknown; no permission reset or process restart was attempted.

Items **(a), (b), (f)**, and the optional trusted cross-origin **(e)** remain
blocked with their round-1 results unchanged. A current Screen Recording refusal
is established; a current Accessibility refusal is **not** established, because
round 2 did not attempt input after the capture prerequisite failed. No Edge
profile, everyday browser data, credentials, or permission settings were touched.
This does not change the provisional X-A recommendation or establish an extension
capability failure. Windows/Linux remain untested.

## Additional gate checks

- **Strict CSP:** top page sets `script-src 'self'`, with no unsafe-inline/eval,
  and self-only connections plus its explicit Vite WebSocket. ISOLATED extraction
  and MAIN expando/HMR reports both arrived. This covers the supplied strict-CSP
  fixture, not every CSP/framework combination.
- **Source maps without CDP:** worker enumerates `document.scripts` and fetches
  `/app.ts` and `/@vite/client`; both return inline base64 source-map comments,
  recorded as `source_map.found=true`. This establishes discovery on Vite only,
  not complete source-to-element resolution, dynamically imported modules,
  header-only maps, or all frameworks. Keep those S1/S6 limitations explicit.
- **No automation banner mechanism on Edge:** exact argv has neither
  `--enable-automation` nor a debugging flag. Edge reports `webdriver:false`.
  Visual absence of the banner was not captured; it is not marked visually passed.
- **GUI evidence:** [CFT extensions screenshot](../../spike/design-mode-v2-s0/evidence/cft-extensions.png)
  and [attempt log](../../spike/design-mode-v2-s0/evidence/gui-attempts.txt).
  MacControl reports both Screen Recording and Accessibility permissions missing.
  `ioreg` returned no locked=true entry and the CFT screenshot succeeded; these
  failures are permission failures, not evidence of a locked display.
- **CDP disclosure:** used only through the mandated stealth wrapper for the
  extensions-page GUI attempt because MacControl was unavailable. Edge's capability
  measurements used no CDP. Wrapper CFT UA Chrome/149 matches its 149 binary and
  its required webdriver check passed. Everyday profiles and authentication stores
  were never accessed.

## Validation and remaining work

`python3 spike/design-mode-v2-s0/verify.py` passed fragmented, multiple Unicode
frames, and rejection of oversized/truncated frames; output is in
`evidence/framing.txt`. `node --check` passed worker, picker and MAIN scripts.
`git diff --check` passed. Biome matched zero files because spike paths are
outside its configured scope; this is not claimed as a formatting pass.
The instruction-link validator failed on the pre-existing CLAUDE.md violation.
Owned Edge/CFT/Vite/collector processes were stopped and spike native-host
registrations removed; dedicated profiles/logs remain under ~/Gits/.tmp/dm2-s0/.
No product compilation or application suite was run: all code is throwaway and
outside product imports. No desktop TUIC was launched.

To close S0, Boss or a permitted GUI runner must select the unpacked directory in
the **throwaway profile**, verify a restart without the load flag, open the panel
with its action, switch tabs and return, perform trusted top/frame picks, and
capture/crop after activeTab is granted. The manual sequence is in the spike
README. No missing GUI permission proves native messaging unreliable; keep HTTP
fallback and X-B contingent on a reproducible capability failure.

Out of scope: pre-existing `AGENTS.md` modification and `CLAUDE.md` instruction-link
violation were preserved. No unrelated story was changed or closed.

## Round 3: CFT without MacControl (2026-10-10)

CFT **149.0.7827.54**, dedicated profile under `~/Gits/.tmp/dm2-s0/1672-profile`,
loaded the unpacked extension with its flag. CDP was used only as the verification
harness; product native messaging and extraction do not use it. The named stealth
wrapper attached to this owned browser and verified `navigator.webdriver` is
undefined. No desktop TUIC was built or launched.

- **PASS side panel rendering:** a trusted CDP pointer click on a temporary
  extension-document button opened the actual browser sidebar. Its document
  measured 360 × 1231; see `evidence/cft-panel-0.png`. This proves CFT's gesture
  path, not Edge's toolbar or tab-return behaviour.
- **PASS trusted top and cross-origin frame picks:** CDP pointer events reached
  the one-shot picker, producing top-frame and frame-7 payloads with nonzero
  rectangles. See `evidence/round3-events.jsonl` and
  `evidence/cft-trusted-pick.png`. The earlier synthetic zero-rectangle limit no
  longer blocks implementation of the isolated picker.
- **Not passed:** capture still requires the browser action's activeTab grant;
  opening a panel from an extension document does not grant it. No successful
  crop is claimed. Product must report capture errors and retain useful DOM
  evidence, without broadening host permissions.
- **Native Edge checks remain:** GUI Load unpacked persistence without the flag,
  toolbar activeTab capture, and sidebar return after tab switching. These need
  native browser UI input; they do not block building S1-S4 on the measured CFT
  capability. The existing CDP backend stays intact.

Decision: proceed with X-A and native messaging. S2 must prove the real socket
relay and explicit-instance handshake; the spike echo is not that proof.

## Round 4 prerequisite audit (2026-10-14)

The managed `design-mode` peer tried to recheck the capture prerequisite through
TUIC's discovered `maccontrol__capture_screen` tool. Schema retrieval failed
before the capture call:

```text
invalid MCP candidate: invalid collapsed tool schema: unknown field `annotations`, expected one of `name`, `description`, `inputSchema`
```

This establishes a tool-schema discovery failure, **not** a current Screen
Recording refusal or a browser capability failure. No capture, browser launch,
profile change, desktop restart, or permission change was performed. Items (a),
(b), (e), and (f) retain their earlier evidence and limits; no new S0 pass is
claimed.

The source audit found the product autoload and agent browser command work
already integrated on main at `15e04143b4`. Automatic cold-launch extension
loading replaces manual product onboarding, but does not prove the spike's
historical no-flag GUI-install persistence check. Keep that distinction in the
final test evidence.

For the coordinated live test, use the throwaway profile and the spike README:

1. Recheck capture through a working MacControl schema, then record its actual
   response. Do not infer permission status from this discovery error.
2. Load unpacked in that profile, close its browser, and restart without the load
   flag. Record whether the extension remains enabled: item (a).
3. Open the sidebar through the native extension action, switch tabs, and return:
   item (b).
4. Make trusted top-level and cross-origin frame picks, and verify the picker
   stops after one pick and Escape cancels it: item (e).
5. After the extension action grants activeTab, capture the top-level target and
   retain the successful PNG crop with its rectangle: item (f).

Separately, product checks for stories 1834-f6c4 and 1724-fe7e still need Boss's
entry-point/pick delivery, visible-tab screenshot and sidebar history, and
Claude Code/Codex/ego tool-list refresh on Arm/Stop. Source and isolated protocol
tests are not evidence that these installed-browser checks passed.

### Product-pilot continuation (2026-10-10)

After the S2 host install, the product sidebar completed a real native `hello`
against the selected `mcp.sock` and pinned instance. CFT disabled the unpacked
extension on runtime reload while Developer mode was off
(`unsupportedDeveloperExtension=true`). A trusted click on Developer mode in
`chrome://extensions` enabled it immediately. The pilot setup instructions
already require this switch. The native host's framing/manifest/version tests and
repository-recipient test passed (4/4).

A live session observation also established that `is_remote=true` describes
sessions created through the API, including local PTYs. It is not a remote-daemon
ownership signal. Recipient isolation uses `connection_id` instead. The running
pre-v2 TUIC returns HTTP 404 for `/design-mode/extension`; the product sidebar
prints the exact endpoint and restart requirement while retaining cards. Native
Edge checks remain outstanding.

## Round 5 recheck (2026-10-17)

- `ioreg -n Root -d1`: `IOConsoleLocked = No` (13:01 CEST).
- `screencapture -x` from the managed shell succeeded (6.3 MB PNG, exit 0): no Screen Recording denial for the shell.
- The MacControl tool is not exposed to this session (ToolSearch "maccontrol": no match), so the MacControl-specific prompt could not be re-read.
- Trusted GUI input is unavailable: `osascript` System Events fails with error -1743 (not authorized to send Apple events). `cliclick` exists but Accessibility was not probed, to avoid a permission dialog on the Boss screen.
- Result: S0 items (a), (b), (e), (f) stay BLOCKED on trusted input. No new S0 pass is claimed.


## Round 6: isolated web and MCP checks (2026-10-22)

**MacControl is not a prerequisite for these checks.** Boss's correction applies:
TUIC's browser frontend can exercise its shared Tauri/HTTP backend, and an owned
Chrome test instance may use CDP as a harness. The product launcher must still
omit CDP and automation flags. No daily app restart or daily browser profile was
used. These results do not close S0 or the screenshot/client-refresh criteria.

### Setup and evidence

- Worktree: `fix/design-mode-web-checks`, starting at `0d02ef11470f`.
- Built and started with `make agent-test AGENT_TEST_INSTANCE=agent-test-design
  AGENT_TEST_PORT=19885`, after its matching dry run. This target runs its native
  build through build-slot/mbx. The isolated backend reported instance
  `ffd92cb1-5bdd-4de0-bf17-c33f918a184d`; the explicit socket is under
  `~/Gits/.tmp/tuic-at/8244dd0e0d93010a/`.
- The authenticated TUIC browser frontend loaded at `http://127.0.0.1:19885/`
  through the named stealth-wrapper/agent-browser session `design-web-ui`.
  `tuic-web-snapshot.txt` contains its real sidebar and terminal controls.
- Chrome for Testing **149.0.7827.54**, real product extension **2.0.0**, fixed ID
  `eefbgjmmpaimpeaflioojmdipmfchefo`; dedicated profile
  `~/Gits/.tmp/design-web/profile`. The freshly built native host installed an
  explicit socket/instance binding. Its sidebar reported that exact socket and
  the isolated Claude PTY recipient `3b946888-932f-434e-afb0-c595de039544`.
- The product Arm path launched Edge in the isolated agent-test instance only.
  Its own Edge processes were stopped before the harness used CFT. Browser cache
  and native-host files for CFT stayed under the Gits test profile. Product launch
  and agent-test state also used the explicitly selected isolated instance's
  application-support directory, never the daily configuration directory. The debug
  credential-store initialization also selected
  `~/.tuicommander-dev/instances/agent-test-design/` (startup log); no credential
  store was read or copied by the harness.
- The fixture was served from this worktree's `.tmp/design-web-page` on **19886**.
  Its top origin was `127.0.0.1`; its iframe origin was `localhost`. Native host,
  socket, DOM picks and MCP replies were real, not mock browser state.
- Raw evidence and harnesses: `~/Gits/.tmp/design-web/`. Build/start logs:
  `~/Gits/.tmp/results/design-web-{start,instance}.log`. No application suites,
  Clippy, mutation runs or rb tests were needed for this documentation/evidence
  change. The native build is not claimed as a test-suite pass.

### Per-criterion results

| Story / criterion | Non-native verification path | Measured result and remaining scope |
|---|---|---|
| 1724 AC4: sidebar who/what/when | Actual extension side panel attached to the isolated backend; MCP `ui design_screenshot`; expand its **Agent actions** details | **PASS DOM observation.** `history-dom-expanded.txt` shows `10:21:43 AM · 3b946888-932f-434e-afb0-c595de039544 · screenshot · failed`. `history-after.json` retains the same real action record. A failed browser action is correctly visible, not hidden. This is not a visual screenshot pass. |
| 1724 AC5: routing tests plus matching screenshot reality | Existing recorded routing/handler GREEN evidence, plus a real MCP navigation to the worktree fixture and an extension screenshot request | Navigation returned the requested URL (`design_navigate-headed.json`). Screenshot returned Chrome's `Either the '<all_urls>' or 'activeTab' permission is required.` No matching PNG was produced; the mixed criterion stays open. A later navigation during harness reconnect returned the production ten-second `command_timeout`; it is retained, not counted as successful delivery. |
| 1724 AC6: session-scoped discovery and Claude/Codex/ego refresh | MCP initialize, persistent SSE, tools/list before and after Arm/Stop, cached call after Stop, and separate installed-client probes | **Protocol PASS, client refresh unproved.** Arm and Stop each emitted `notifications/tools/list_changed`. Stop succeeded; the cached `ui design_screenshot` call then refused `Design Mode not armed for this session`. `tools-armed.json` and `tools-stopped.json` are identical under the later accepted #2067 grouping: `ui` always lists `design_*` actions and there is no standalone `design_mode` tool. The old tool-add/remove wording needs reconciliation with #2067, not another implementation of the removed tool. |
| 1724 AC19: visual action history | Screenshot of the real extension panel, not a static mock or the orchestrator WebView | **OPEN.** DOM history is proved above, but `Page.captureScreenshot` timed out at the harness's eight-second bound (`history-screenshot-error.txt`). No PNG is claimed. |
| 1724 AC20: visual screenshot correspondence | Compare the MCP screenshot with a harness screenshot of the same reviewed fixture | **OPEN.** The extension capture lacks `activeTab`; the independent harness capture also timed out. Neither half of the comparison is replaced with a fixture image. |
| 1724 AC21: three-client Arm/Stop refresh | Actual installed clients with invocation-scoped MCP configuration and isolated client homes; no model request or credentials copied | **OPEN.** Claude Code **2.1.306** reached its initial theme/onboarding screen; Codex **0.163.0** reached its sign-in screen; ego **0.1.2** reached model selection with isolated providers not signed in. These bounded probes did not establish a connected, ready MCP client, so they prove neither refresh support nor lack of support. The server-side cached-call fallback is verified independently, not attributed to any client. Transcripts: `{claude,codex,ego}-client.txt`; Claude startup log: `claude-debug.log`. |
| 1672 AC1 / S0(a): load and persistence | Test-profile flag load; `Extensions.loadUnpacked`; restart the same profile without `--load-extension`; `Extensions.getExtensions` | Flag/API loading worked. After each no-load-flag restart the test API returned `extensions: []` (`persistence-result.json`). This is **not** a successful persistent GUI installation, and does not prove that Chrome's actual **Load unpacked** chooser installation fails. That separate install/persistence step remains open. |
| 1672 AC1 / S0(b): side panel and tab return | `Target.getTargets` with `type: tab`, then `Extensions.triggerAction` using that **tab** target (not the page target) | The actual `panel.html` target opened and its DOM contained the real native connection and recipient. No OS toolbar click was needed to open it. Tab-return restoration and rendered screenshots were not proved. A page target is rejected with `Action can only be triggered on a tab target`; that is a harness-addressing distinction, not a product failure. |
| 1672 AC1 / S0(e): trusted top/frame picks and cancellation | Reload owned fixture pages after extension installation, then real `Input.dispatchMouseEvent` in top/frame targets; panel's real arm/stop path | **Partial PASS.** Persisted real picks contain `#target` at `{x:44,y:216.3125,width:278.828125,height:76}` and cross-origin `#frame-target` at `{x:8,y:8,width:291.265625,height:78}` (`picks-result.json`). No hand-written pick storage was injected. Escape and Stop inputs were exercised, but one-shot/cancellation ordering was not independently asserted; do not close that part. Reload after extension installation is required to inject content scripts into already open pages. |
| 1672 AC1 / S0(f): captureVisibleTab and crop | Invoke the browser action through the test API, then the real capture and crop path, without broadening permissions | **OPEN.** `Extensions.triggerAction` opened the panel but did not grant usable `activeTab` in this run, even on the selected fixture tab. Direct extension capture and the top pick both returned the precise Chrome permission error. Cross-origin picks explicitly retained DOM evidence and omitted their image. No successful crop exists. |

### What still needs a browser-chrome gesture, and what does not

The exact unproved gesture is invoking the extension's real browser action so
Chrome grants `activeTab` to the reviewed tab. Opening the panel from an
extension document, or calling the tested `Extensions.triggerAction`, did not
establish that grant. Likewise, the original GUI **Load unpacked** persistence
step has not been replaced by a successful persistent test-API install. These
are browser-chrome installation/permission checks, not reasons to block MCP
notifications, sidebar DOM checks or ordinary TUIC web controls on MacControl.
No claim is made that every possible browser API alternative has been exhausted.
Toolbar pinning itself is not a product acceptance criterion.

During the capture attempts, `ioreg` reported `IOConsoleLocked = Yes`. Both the
headed browser and the tested `--headless=new` configuration timed out on
`Page.captureScreenshot`; `fromSurface:false` and a later software-rendering
configuration also failed to produce a PNG. The concurrent lock is recorded as
context, not proved as the sole cause. No Accessibility/Screen Recording request,
permission reset, unlock attempt or permission broadening was performed.

The next verification pass needs a rendering-capable owned browser session, a
working browser-action grant, and ready MCP clients configured for the isolated
socket. It does **not** require restarting Boss's daily TUIC, installing into his
everyday browser, or waiting for MacControl before trying the web/MCP checks.


### Cleanup and validation

Design Mode was explicitly stopped, the owned PTY was deleted, all three remaining
named agent-browser daemons were closed, and the recorded Chrome/fixture processes
were terminated. `make agent-test-stop AGENT_TEST_INSTANCE=agent-test-design
AGENT_TEST_PORT=19885` stopped the isolated desktop. The final recorded-PID check
found none running and the isolated desktop PID file was absent. Raw test-profile
state and transcripts remain as evidence; no daily process was restarted.
Instruction-link validation and `git diff --check` passed. The formatting helper
was run against the branch change range; this report changes Markdown only.

## Round 7: catalogue reconciliation and capture authority (2026-10-22)

This round uses production HEAD `c67550fedb009640a118be019cfaa5d54cdfbfd2`.
It changes evidence and an explicit RED probe only, not production permissions or
browser behavior. The shared main-checkout story store remains authoritative.

### Catalogue contract

Approved #2067 (`96c4f5778`) removed standalone `design_mode` and moved its actions
onto the always-listed `ui` tool. Story 1724 AC6 was rejected as superseded, with
that reason preserved. Its checked replacement retains the observable contract:
`ui design_*` remains listed, Arm and Stop emit `notifications/tools/list_changed`,
and a cached action after Stop refuses `Design Mode not armed for this session`.
Round 6 retained all three observations. Actual-client refresh is still a separate
criterion; no scope was removed to close the story.

### Screenshot product defect and decision boundary

Authoritative sources inspected on 2026-10-22:

- [Chrome tabs.captureVisibleTab](https://developer.chrome.com/docs/extensions/reference/api/tabs#method-captureVisibleTab):
  “In order to call this method, the extension must have either the `<all_urls>`
  permission or the `activeTab` permission.”
- [Chrome activeTab](https://developer.chrome.com/docs/extensions/develop/concepts/activeTab):
  temporary access follows user invocation (action, context menu, command shortcut
  or omnibox acceptance), not merely an `activeTab` manifest declaration. Access
  is revoked on cross-origin navigation or tab closure.

The tabs page's general host-permission section also lists captureVisibleTab as
an example of host access. That generic prose does not establish that a narrow
localhost grant satisfies the method-specific contract. The method-specific
requirement agrees with the actual Chrome 149 denial recorded in Round 6.

Production call sites:

- `browserCommands.js` accepts static localhost access, checks the reviewed tab
  is active, then directly calls `chrome.tabs.captureVisibleTab`. It does not
  obtain a capture grant or convert this denial into an actionable typed error.
- `worker.js` calls that function for native commands after the armed-owner check.
  Its separate pick-crop path also calls captureVisibleTab; it retains DOM
  evidence and reports `Screenshot unavailable: …` on denial.
- `panel.js` requests per-host or HTTP/HTTPS-wide optional origins from explicit
  grant buttons. Its Pick button sends a worker message; the code does not
  establish that this is a Chrome browser-action invocation granting activeTab.
- The manifest declares `activeTab` and static localhost hosts, but no
  `<all_urls>` host grant. The ordinary product launcher supplies no user gesture.

**Confirmed defect, narrow claim:** a reviewed localhost page admitted by Design
Mode can still reject agent screenshots for missing capture authority. Arming and
static host admission are not sufficient. The recorded failure is a permission
failure, independent of whether a later PNG would render while the screen is
locked. No claim is made that capture after a real toolbar gesture also fails.

The smallest permission-preserving correction is an actionable typed refusal
that asks the user to invoke the Design Mode browser action on the reviewed tab,
plus documented re-grant behavior after cross-origin navigation. This does not
make unattended capture work. If unattended capture is required, Boss must first
approve an explicit broader capture-permission flow. A required `<all_urls>`
manifest grant would broaden install-time access; an optional grant would keep
consent explicit but still needs a verified Chrome flow. Neither was implemented
or requested in this round. CDP/debugger is not proposed as a product workaround.

### One extension-boundary RED

`scripts/design-mode-capture-permission.red.mjs` drives the production
`browserCommand` entry point with the actual recorded Chrome rejection at its
external API boundary. It asserts a typed `capture_permission_required` error
with browser-action recovery guidance. It does **not** mock a successful PNG,
model Chrome's grant rules, or prove a live screenshot. This narrower regression
protects the permission-preserving error correction, not an unapproved broad
permission implementation.

The fixture `scripts/fixtures/design-mode-chrome149-capture-denial.json` is the
unchanged parsed Round 6 MCP response from `design_screenshot-headed.json`, not
invented external behavior. SHA-256:
`b5b57855824a0cdd6e04d14613100c72061832f1e2dfecc735485462f832ae3d`.

One authorized pre-fix run:

```sh
/Users/stefano.straus/Gits/personal/orchestrator/tools/build/build-slot.sh \
  scripts/with-test-tmp.sh node --test scripts/design-mode-capture-permission.red.mjs
```

**Expected RED:** one test failed in 99.7 ms. Actual error was Chrome's raw
`Either the '<all_urls>' or 'activeTab' permission is required.`, not the expected
typed actionable refusal. Log: `~/Gits/.tmp/results/design-web-round2-red.log`.
The `.red.mjs` suffix keeps this explicitly invoked diagnostic outside routine
suite discovery while the decision is pending. No GREEN or production fix is
claimed. Adjacent inspected cases: hidden reviewed tab already refuses before
capture; cross-origin navigation requires renewed activeTab; frame picks retain
DOM without a screenshot. No new live visual claims were made for these cases.

### Real installed MCP clients

The isolated agent-test backend started with the authorized instance/port preset
and a matching dry run. Fresh instance ID:
`7857b87a-a5d9-440a-b9ff-60be68803ef5`. A throwaway Claude PTY supplied the live
owner identity `8c4af753-931e-4a0f-801f-dae7347af44c`. Clients inherited their
existing authentication environment; no credential file/keychain was read,
copied or printed, and no model prompt was submitted.

Invocation-only MCP files/overrides ran a stdio observation relay around the
worktree's `tuic-bridge` with the explicit isolated socket. The relay recorded
JSON-RPC methods, IDs and tool names only, not credential or model payloads.
Evidence: `~/Gits/.tmp/design-web/round2/{claude,codex,ego}-wire.jsonl` and the
per-client event/transcript files. Results:

| Client | Actual observation | Limit / fallback |
|---|---|---|
| Claude Code 2.1.306 | Initial tools/list, then each Arm/Stop notification immediately followed by a fresh tools/list; all three replies list the same nine grouped tools | Refresh **proved** for this bounded connected run. It used server/discover and subscriptions/listen, rather than a legacy initialize handshake. |
| Codex 0.163.0 | Legacy initialize, initial tools/list, both Arm/Stop notifications delivered; no later tools/list through the end of its 24-second run | Automatic refresh **not observed**, not a claim that Codex can never refresh. Cached grouped UI action remains usable and is refused server-side while stopped (Round 6). The top-level per-run mcp_servers override merged with ambient servers: startup showed eight servers, not a strict one-server environment. No tool/model operation was submitted to them. |
| ego 0.1.2 | First invocation file lacked required transport; corrected strict stdio file reached server/discover and tools/list, then exited with `invalid MCP candidate: registry publication failed` | Registry admission failed before Arm/Stop. No ego refresh or client fallback pass is claimed. Corrected invocation used inherited environment names and a Gits-local session directory; no empty authentication home. Root cause of registry rejection remains unknown. |

Claude and Codex Arm/Stop calls succeeded against the isolated instance. Each
browser launch was preceded by one ioreg check reporting unlocked; only newly
observed exact product Edge PIDs were stopped. A later pre-visual check reported
`IOConsoleLocked = Yes`, so screenshot/history/crop visual work was skipped,
without rendering retries or lock polling. S0 persistence, tab return, successful
crop and independently asserted cancellation ordering remain open.

### Cleanup and remaining gates

The client harness terminated/reaped its recorded client process groups and
recorded product Edge PIDs. At final cleanup the backend was already unavailable:
both socket cleanup requests failed to connect, and matching agent-test-stop
reported no running supervisor. The runtime log records the owned PTY's
`close_requested` and process exit; the cause of the backend shutdown is not
established here. All recorded client, bridge, owner, Edge and supervisor PIDs
were checked and absent; the desktop PID file was absent. No daily TUIC process
was stopped or restarted by this round.

Both stories remain open. Live matching screenshot and S0 evidence are still
missing; ego refresh is blocked by registry admission. Permission broadening and
any resulting production implementation require Boss's decision first.
Adversarial-TDD's admitted SKILL instructions were read, but its requested README
resource was unavailable (`no such file or directory`); no hidden review gate is
claimed. Independent critic, GREEN, full suites and mutation were not run.
Risk: **medium**, because the diagnosis concerns browser permissions and MCP
client lifecycle; the committed changes themselves do not change production.
Locations under `~/Gits/.tmp/` contain raw local evidence, not portable fixtures.
Verified static checks and story-close refusal output are recorded in the round's
terminal report.


## Round 8: approved optional all-sites capture (2026-10-22)

Boss selected option C in beta2-asks item 6. The shared 1724 worklog records the
decision: `<all_urls>` belongs in `optional_host_permissions`, never mandatory
`host_permissions`. The new **Let agents capture any site** button requests that
permission from an explicit user click. Existing consent avoids another request;
a declined prompt preserves temporary browser-action capture guidance.

Agent screenshots and pick crops now share the Chrome capture call and translate
the recorded missing-permission denial into `capture_permission_required`, naming
the option and the temporary browser-action alternative. Other capture failures
remain unchanged. The capture-only grant is not treated as eval consent and does
not register remote page-error scripts. Existing explicit eval/host grant buttons
are unchanged. Chrome still controls the real permission state.

### RED and targeted GREEN

- Pre-fix boundary RED at `4cb624a933bc0952a0bffb5d01676c97b9e12395`:
  `build-slot.sh scripts/with-test-tmp.sh node --test scripts/design-mode-capture-permission.red.mjs`
  failed on the missing typed option guidance. Log:
  `~/Gits/.tmp/results/design-optionc-red-retry.log`. The first foreground attempt
  timed out while queued (no test ran); the event-driven retry was admitted after
  160 seconds and the actual test failed as expected in 214 ms.
- The extended script has an opt-in real-Chrome test. With
  `TUIC_DESIGN_CDP=http://127.0.0.1:57323` against the pre-fix installed extension,
  capture without a browser-action gesture failed with the recorded Chrome
  permission denial; the real granted origins contained only static localhost
  entries. Log: `~/Gits/.tmp/results/design-optionc-live-red.log`. This was an
  actual Chrome API call, not a fake grant model or storage injection.
- Targeted GREEN after implementation: the recorded-denial boundary test passed;
  the live-Chrome case was explicitly skipped without its endpoint environment.
  Browser commands, host errors, worker and panel Vitest files passed **12 tests
  in four files**, 1.17 s. `pnpm exec tsc --noEmit` passed in the same admitted
  command. Log: `~/Gits/.tmp/results/design-optionc-green.log`. The scheduler
  admitted the command after 60 seconds. No full suite, mutation or Clippy ran.
- Adjacent protections cover hidden reviewed tabs, capture consent not becoming
  eval consent, capture-only origins not injecting remote error scripts, consent
  refusal guidance, and avoiding another permission request once already granted.

### Live consent boundary — deliberately not GREEN yet

The authorized isolated agent-test-design backend on 19885 started after its
matching dry run. Instance ID: `d3237151-2cce-4247-82ac-e086c35a5576`.
Chrome for Testing 149 uses the owned profile
`~/Gits/.tmp/design-web/profile`, fixture port 19886, and the real native host.
The changed unpacked extension sources were copied into that owned extension
folder while its recorded Chrome process was stopped, then Chrome was restarted.
This verifies extension source behavior, not a newly rebuilt embedded installer.
The production launcher still does not request a debugging port; CDP is used only
by this owned test harness.

Before the visual step, ioreg reported unlocked. The new option rendered in the
real extension document; its screenshot is retained at
`~/Gits/.tmp/design-web/optionc/option.png`. A trusted CDP pointer click on the
actual button initiated Chrome's optional permission request. Its promise remains
pending and `<all_urls>` is not yet granted. MacControl could not inspect the
browser-chrome consent prompt: **Screen Recording permission required. Grant
access in System Settings > Privacy & Security > Screen Recording**. No alternate
transport was used to bypass that OS block, and no permission state was fabricated.

The coordinator explicitly parked the consent click with Boss and authorized
leaving this test window open. The exact required action is to accept the genuine
Chrome all-sites permission prompt. After that click, rerun the script's live
case with `TUIC_DESIGN_CDP` pointing at the owned Chrome port and
`TUIC_DESIGN_CAPTURE` naming a Gits-local PNG path, then complete the matching
MCP/browser screenshot comparison. No post-grant screenshot success, crop,
revocation behavior or matching-image acceptance is claimed yet.

Recorded retained runtime ownership: Chrome PID 46762, fixture PID 39420, plus
the isolated agent-test supervisor recorded in
`~/Gits/.tmp/results/design-optionc-supervisor.pid`. Harness metadata is in
`~/Gits/.tmp/design-web/optionc/pids.json`; verify ownership again before stopping.
Chrome and the fixture are intentionally retained for Boss's pending click. At the
final PID check, supervisor 89047 and the desktop PID file were already absent;
backend shutdown attribution remains unknown. Resume the authorized isolated
backend before the later MCP comparison. The matching stop target remains `make agent-test-stop
AGENT_TEST_INSTANCE=agent-test-design AGENT_TEST_PORT=19885`.

The feature and consent flow require extension reload/reinstall from a rebuilt
native bridge before use in a normal installation. This round does not restart
Boss's daily app. Story 1724 remains open pending the actual consent and live
post-grant checks; prior S0 and ego-refresh gaps are not silently marked complete.


## Round 9: real consent GREEN; rebuilt installer mismatch (2026-10-22)

Boss accepted the genuine Chrome permission prompt. The actual extension document
reported `<all_urls>` in `chrome.permissions.getAll()` and displayed **All-sites
capture enabled. Agent eval still needs a separate grant.** No CDP permission
mutation, browser-action invocation, fake screenshot or storage grant was used.

The opt-in live boundary run passed both tests, with no skips, in 268.4 ms (the
Chrome case took 148.3 ms). It called the production browser-command module inside
the real extension document against the actual reviewed fixture and saved the PNG
at `~/Gits/.tmp/design-web/optionc/granted.png`. The admitted command used
`TUIC_DESIGN_CDP=http://127.0.0.1:58096` and `TUIC_DESIGN_CAPTURE` with that path.
Log: `~/Gits/.tmp/results/design-optionc-live-green-retry.log`. The first foreground
attempt timed out in the build queue without running tests; the event-driven retry
was admitted after 20 seconds. The display was unlocked before the visual step.
This proves post-consent Chrome capture without another browser-action gesture;
it does **not** prove the MCP transport or matching-image acceptance.

### Packaging provenance blocks the MCP comparison

The next normal isolated `make agent-test` build completed in 2m 24s after the
frontend build. Its bridge artifact had SHA256
`3ac72e610dc670bc9d8b79aca1b4d91d0a003293f8590ed45d953e54deabbe76`.
The new backend instance was `5604a56c-3450-4091-9bde-623644137fb4` on 19885.
A fresh throwaway Claude session was detected as a live Claude agent and armed
through the real product endpoint. The newly launched isolated Edge PID was
recorded from the exact executable/profile prefix and stopped before using the
owned Chrome test profile.

After stopping the owned Chrome, that bridge's real `--design-install` command
installed assets which did not match this worktree. Six files differed: manifest,
panel HTML/JS, worker, host errors and browser commands. The installed manifest
omitted optional `<all_urls>` and the installed panel omitted `grant-capture`.
The current source contains both. No source files were copied over this mismatch
to obtain a passing MCP result. Evidence is in
`~/Gits/.tmp/design-web/optionc/final-installed-assets.json`,
`rebuilt-manifest.json` and `rebuilt-panel.html`.

With those installed assets, the real panel snapshot returned `{ok:true}` without
a result; actual MCP `ui design_navigate` and `ui design_screenshot` returned
`no_reviewed_tab`. The saved browser-page PNG therefore has no successful MCP image
to compare. Neither a matching image nor successful action history is claimed.
The panel/document evidence and RPC responses use the `final-*` prefix in that
same evidence folder. This is not another consent failure: the installer has
replaced the tested extension assets. The root cause of that artifact mismatch is
not established. `mbx explain --last`, run from `src-tauri`, reported 913 hits and
no bridge miss; the build log records a bridge compilation line. No cache was
deleted, no clean build was run, and no cache bypass was attempted. The coordinator
was asked to authorize a bridge-only rebuild diagnosis or assign the cache owner.

Story 1724 matching-image acceptance remains open, along with its prior client
refresh gap. S0 persistence, tab return, crop and cancellation chronology also
remain open. Round 8's pending-consent status is historical and is superseded by
the real consent and boundary GREEN above; it must not be used to claim a still
pending Allow click.


## Round 10: critic fixes and bridge-only diagnosis (2026-10-22)

- Restored critic probe `1625eed38` before production edits. The targeted test
  failed with `promise resolved "{ errors: [] }" instead of rejecting` at
  `033609f6d`; log `~/Gits/.tmp/results/design-critic-f1-red.log`.
- Remote error reads now require an explicit host origin, using the same consent
  matcher as eval. Static localhost reads and explicit host grants remain valid.
  The capture button directly requests permission without a redundant `contains`.
  The recorded Chrome-denial regression moved into ordinary Vitest; the separate
  Node/CDP test script was removed. This spike record remains tracked.
- Host/eval buttons now inspect the actual granted origins after request success.
  If Chrome has not added separate eval consent, the panel explains the required
  revoke/grant/capture order rather than claiming a grant. Live verification of
  this sequence is still pending; its unit protection is not Chrome evidence.
- Targeted GREEN: 15 tests in four files, 1.18 s; TypeScript passed. Scheduler wait
  was 460 s. Log: `~/Gits/.tmp/results/design-critic-followup-green-build.log`.
- Authorized bridge-only build after touching `native_assets.rs` took 11.07 s.
  Cargo dep-info includes every embedded extension file: no missing
  `rerun-if-changed` was demonstrated. An include-only HTML marker was rebuilt and
  removed correctly in two further bridge-only builds. The temporary marker is
  restored away. That probe did not reproduce a general included-file cache bug.
- A second normal multi-package agent-test build again produced the exact old
  bridge hash from Round 9. The bridge-only rebuilt binary
  `8676b0fcb32518c72dc8cfaa6ed8dd7a508e6573b52c583e14a047204a6dce07`
  installed assets equal to all current source files. The repeated multi-package
  versus bridge-only discrepancy is reproducible here; its cache-layer cause is
  not yet isolated. The include probe and copied dep-info are retained under
  `~/Gits/.tmp/design-web/optionc/bridge-*` and `include-*`.
- Opening `panel.html` as a regular browser tab is not a real sidebar: its sender
  has a tab identity and the worker deliberately refuses panel-only operations.
  The genuine browser-action side panel fixed `no_reviewed_tab`; real MCP
  navigation then succeeded. A matching screenshot is still not available.
- MCP still returned the raw denial after file replacement, although a document
  fetch read the fixed source; a retained worker module is an inference. An owned
  extension reload was followed by Chrome signal 11 (`SEGV_ACCERR`), recorded in
  `optionc/chrome.log`; causality beyond that sequence is unknown. A controlled
  restart recovered the owned browser. No daily browser/app was touched.
- The old installer had removed the literal `<all_urls>` grant. The correctly
  reinstalled extension now needs another genuine Allow click. The real capture
  button was clicked with an unlocked display, but the request remains pending;
  no consent state was fabricated. Current owned Chrome PID 15598 and fixture PID
  5969 remain open for that prompt. The isolated instance is
  `8d2404b1-6f8f-4e8a-b45a-7fb317ce894e`, supervisor 92408, desktop PID 3459,
  owner `fd35d011-8234-4ef6-9bf8-31392ce9d596`. After Allow, verify MCP screenshot
  matching and the real host/eval click before claiming either acceptance.


## Round 11: matching actual MCP screenshot and host/eval check (2026-10-22)

The earlier pending-consent state is superseded. After the provider interruption,
Chrome had no page targets; a new owned extension document was opened and the real
capture button clicked. Chrome returned success and `getAll()` then included
literal `<all_urls>`. No permission was written by the harness. The profile also
retained broad HTTP/HTTPS origins from the older-manifest round, so this is not a
fresh-profile capture-only consent experiment.

A real **Allow eval on this host** click subsequently added `*://127.0.0.1/*` to
Chrome's origins. The panel reported the host grant. Production eval returned
`Design web verification`; the actual MCP `ui design_eval` did too. Evidence:
`optionc/critic-host-click.json` and `optionc/accepted-design_eval.json`. Thus this
observed Chrome request added an origin rather than reproducing the critic's
hypothesized no-op. The panel's no-op protection remains covered by unit tests;
no failing real Chrome no-op is claimed.

After re-arming the expired mode, the actual initialized owner MCP connection
successfully navigated the fixture and requested a screenshot over the real native
host. A separate browser-page screenshot matched exactly: **1628 × 1596 pixels,
zero differing pixels, identical PNG SHA256**
`31875551b7f884bb03bdd7e46b6baac01c7eeb2874dd703863e7f3f866ce3ca6`.
The decoded comparison used browser `createImageBitmap`/`OffscreenCanvas`; no new
image dependency was installed after Python PIL was found unavailable. Files:
`optionc/accepted-mcp.png`, `accepted-browser.png`, `accepted-comparison.json`,
and the actual RPC responses `accepted-design_navigate.json` and
`accepted-design_screenshot.json`. The page text contains the recorded fixture
token `DESIGN-WEB-20261022`.

The real sidebar history displays owner
`fd35d011-8234-4ef6-9bf8-31392ce9d596`, navigate at 14:03:12 and screenshot at
14:03:13, both done. Its actual screenshot is `optionc/accepted-panel.png`; DOM
and snapshot evidence are retained alongside it. The matching screenshots and
history were presented in TUIC's **Design Mode screenshot evidence** tab, backed
by `optionc/accepted-visual-evidence.md`. Each visual step checked ioreg once and
ran unlocked. No OS Screen Recording permission bypass was used.

1724 AC5/19/20 now have installed-extension reality evidence, together with the
previous recorded routing/handler GREEN. This does not close its remaining ego
refresh gap or S0's persistence/tab-return/crop/cancellation chronology. The
normal multi-package bridge artifact mismatch also remains a recorded build
provenance issue; the live run used the verified package-only rebuilt installer,
not copied extension source files.

Cleanup: mode stopped, throwaway owner deleted, and matching agent-test stop used.
Recorded Chrome 15598, fixture 5969, desktop 3459, supervisor 92408, owner 4792
and isolated Edge 5521/99846 were all absent; desktop PID file was absent.

## Round 12: live S0 crop, tab return and cancellation (2026-10-22)

This round used the real installed MV3 extension and native host in the owned
Chrome for Testing 149.0.7827.54 profile, with isolated `agent-test-design` on
19885. The owner was `120212b6-478c-476d-9060-50ea93d8e48f`; instance
`7ca9d54e-38bf-4e32-8aef-5d6411c3b936`. The normal agent-test bridge installer
wrote all 16 installed asset files byte-identical to current source in this run.
The earlier stale-bridge observation was not reproduced here, but its cause is
still unknown. No extension files were copied from source to mask provenance.

Evidence is under `~/Gits/.tmp/design-web/s0/`; harnesses are under the sibling
`harness/` directory. Each visual/input phase checked ioreg and ran unlocked.
Browser-owned CDP input was used, not OS screenshot/input automation. The product
launcher was unchanged and did not receive CDP flags. No draft, permission,
action-history or image state was manufactured.

| S0 item | Observed result | Evidence |
| --- | --- | --- |
| (a) install/persistence | Flag loading works. After a browser-UI directory-drop attempt and a graceful restart with **no `--load-extension`**, `Extensions.getExtensions` was empty both immediately and after startup settled. A second drop attempt without flag loading produced the browser's `Error: No dragged path`. This does not prove a genuine native chooser install fails. | `no-load-argv.json`, `persistence-result.json`, `persistence-settled.json`, `drag-events.json`, `drag-noflag-before.json`, `drag-noflag-immediate.json` |
| (b) side panel/tab return | The actual browser-action side panel showed the real native-host connection and owner. A second real tab had zero picks. Returning to the original tab restored both cards and the comment entered with trusted browser input: `S0 retained comment 20261022`. Snapshot and rendered textarea were independently checked. | `continuation.json`, `other-tab-panel.png`, `returned-panel.png` |
| (e) trusted top/frame picks | Trusted page input selected `#target`. A later trusted out-of-process, cross-origin iframe pick selected `#frame-target`, retained a nonzero DOM rectangle and returned the intentional frame-screenshot omission notice. | `chronology.json`, `continuation.json`, `frame-panel.png` |
| (f) real top-pick crop | The worker returned an actual PNG crop, **728 x 191**, with 139048 nontransparent pixels and 1059 distinct pixel colors. It was decoded through `createImageBitmap` and `OffscreenCanvas`; it was not a placeholder. This uses the existing real all-sites capture grant, not a newly proved temporary `activeTab` grant. | `top-crop.png`, `crop-decode.json`, `one-shot-panel.png` |

### Independent cancellation chronology

The scratch page button increments a visible counter through its ordinary
`onclick` handler. The first picking press left that counter at **0** and added
one pick; the next ordinary press advanced it to **1**, with the pick count still
**1**. This proves consumption of the picking gesture and normal browsing after
one-shot completion.

An initial harness attempt sent Escape before asynchronous arming had completed;
the next press made another real pick. It is not a product cancellation defect.
The corrected continuation waits for the actual picker overlay before sending
Escape, and for its removal before the next click. With two existing picks,
Escape then an ordinary click advanced the counter **1 -> 2**, keeping two picks.
Multi-pick, Stop, confirmed overlay removal and an ordinary click advanced it
**2 -> 3**, still keeping two picks. The frame pick subsequently raised the pick
count to three without advancing the top-page counter. No drafts were reset to
make these assertions pass. The successful continuation log is
`~/Gits/.tmp/results/design-s0-continuation.log`.

The first browser setup also raced restored tabs from the owned profile; the
corrected setup closed those owned tabs and opened one uniquely addressed test
window. An immediate post-reload Runtime evaluation timed out in an earlier
harness attempt. The final checks use an already-loaded document and explicit
state boundaries. These harness failures are retained; no production fix or
regression-test claim is based on them.

### Remaining browser-chrome installation boundary

The installed browser's own CDP protocol describes `Extensions.loadUnpacked` as
similar to the `--load-extension` flag; it exposes a path and an incognito option,
not a persistent-install option (`extensions-protocol.json`). The installed
`chrome://extensions` script was inspected through `Debugger.getScriptSource`
(`installed-chrome-extensions.js`): normal Load unpacked calls the browser's
`developerPrivate.loadUnpacked`; directory-drop uses `useDraggedPath:true`.
Trusted CDP drag events reached that real directory-drop path with the actual
extension directory, but the browser rejected it as **No dragged path**. The
harness did not substitute a fake drag registry, a retry token, browser profile
preferences or installation state.

The remaining unproved step is a genuine browser-native **Load unpacked folder
selection**, followed by a no-load-flag restart. It needs an authorized native
folder-selection surface. The existing OS automation permission block was not
bypassed, and no native chooser was left open. This is one identified remaining
route, not a claim that every possible browser installation API was exhausted.
S0 AC1 remains unchecked for that persistence requirement; tab return, crop and
cancellation no longer depend on native GUI access. No product defect was
established in this round, so no production code or application tests changed.

### Runtime and cleanup

The first readiness controller reported healthy startup, but its app and
supervisor were gone before session creation (curl exit 7); attribution is
unknown. A second run retained a detached monitor and the supervisor exit status,
and stayed healthy for the checks. Both agent-test builds used mbx after doctor
reported zero failures; the second build was a runtime-recovery attempt, not an
extra test gate. Logs are `design-s0-instance.log` and `s0/instance-retry.log`.

Mode was stopped, the throwaway owner deleted, Chrome closed gracefully, the
fixture stopped and the matching `make agent-test-stop` used. Recorded app,
owner, Edge, Chrome, fixture and supervisor/monitor PIDs were all absent; the
desktop PID file was absent (`cleanup-api.json`, `cleanup-pids.json`). No daily
application restart, reload, configuration or session change was made.

## Round 13: bounded native-folder-input attempt (2026-10-22)

At the coordinator's request, an owned CFT149 launch opened
`chrome://extensions/` without the load flag, using only the existing scratch
profile. Developer mode was already on from Round 12. The console was unlocked.
Before native input, `maccontrol__list_windows` with
`{app:"Google Chrome for Testing"}` returned exactly:

> Screen Recording permission required. Grant access in System Settings > Privacy & Security > Screen Recording

The attempt stopped at that refusal, before pressing Load unpacked or opening a
folder chooser. No key input, capture retry or alternative OS-input transport
was used. This is a current Screen Recording refusal, not an inferred
Accessibility failure. The required next step is the MacControl Screen Recording
grant, then the owned Chrome Load unpacked folder selection and no-load-flag
restart. AC1 remains unchecked; Round 12's other live evidence is unchanged.

The owned Chrome was closed through its own browser connection and its exact
recorded PID was absent afterward. No backend, fixture server or owner session
was started in this round. Evidence: `~/Gits/.tmp/design-web/s0-native/`
(`argv.json`, `pid.json`, `refusal.json`, `cleanup.json`).

## Round 14: native input reached; MacControl upstream stopped (2026-10-23)

The Screen Recording refusal from Round 13 was not reproduced. MacControl
`list_windows` succeeded, and a **window-only** screenshot of the owned CFT149
Extensions window succeeded. Native focus and a click on **Load unpacked** also
returned success. No current Accessibility or Screen Recording denial was
reported. The console was unlocked before both input phases.

The browser was Chrome for Testing **149.0.7827.54**, PID **35188**, using only
`~/Gits/.tmp/design-web/profile`, with no `--load-extension` flag. Its native
window ID was **13734**; another Chrome window, **12608**, was not touched.
Browser DOM/window geometry identified the Load unpacked button before the
native click. The screenshot was scoped to window 13734, never the full screen.

The next MacControl batch requested Cmd+Shift+G, a 400 ms wait, path typing and
Enter. It failed with exactly:

> MCP execution failed: Mcp error: -32000: TUICommander IPC request failed: read: response timed out

No completed-action list was returned, so the folder chooser state and the
number of executed actions are unknown. The requested path was also incorrect:
`~/Gits/.tmp/design-web/extension` does not exist; the recorded browser argv
identifies the actual installed directory as
`~/Gits/.tmp/design-web/profile/tuic-design-extension`. This harness mistake is
retained, not counted as an installation result. No successful folder selection
or extension installation is claimed.

A subsequent MacControl `list_windows` call returned:

> Upstream 'maccontrol' process is not running

This is a new upstream availability blocker, **not** a permission refusal or a
browser capability failure. No alternate native-input transport or upstream
restart was attempted. Restore the MacControl upstream, then repeat the owned
chooser flow with the actual installed directory and verify a no-load-flag
restart. AC1 remains unchecked; Round 12's tab-return, crop and cancellation
results are unchanged.

Raw evidence and the bounded browser inspection/cleanup harnesses are under
`~/Gits/.tmp/design-web/s0-native-round14/`: `argv.json`, `pid.json`,
`observations.json`, `browser-version.json`, and `cleanup.json`. The owned
browser was closed through its own browser connection; PID 35188 was absent
afterward. No backend, fixture server or owner session was started. No Boss
window, daily TUIC endpoint, permission setting or product source was changed.
This round changes documentation only; no application build or test suite ran.


## Round 15: single-key MacControl call timed out (2026-10-23)

After the coordinator reported the upstream reconnected, the retry used one
native action per call and no wait actions. Owned CFT149 PID **87700**, scratch
profile unchanged, opened Extensions without a load flag. MacControl identified
its window as **13901**; window 12608 remained untouched. The actual extension
folder `~/Gits/.tmp/design-web/profile/tuic-design-extension` was verified before
input. The console was unlocked.

The first Load unpacked click stopped with `user_active` and no completed
actions. A later separate click succeeded. The very next input call was exactly:

```json
{"actions":[{"type":"key","app":"Google Chrome for Testing","window_id":13901,"key":"cmd+shift+g"}]}
```

That single-key call returned:

> MCP execution failed: Mcp error: -32000: TUICommander IPC request failed: read: response timed out

Host timestamps bound the call at approximately **14 seconds**, from 14:24:53
to 14:25:07 CEST; this is not a separately instrumented duration. A subsequent
`list_windows({app:"Google Chrome for Testing"})` returned the same IPC timeout
at 14:25:22, approximately 15 seconds later. No path was typed, no Select action
was sent, and no further native-input retry was made. This does not establish
that the upstream process exited, but it establishes that even a single key
without an explicit wait failed to return. No new permission denial was seen.

AC1 remains unchecked. Browser-native folder selection and no-load restart
persistence remain unproved. Raw observations, argv/PID, browser version and
cleanup are in `~/Gits/.tmp/design-web/s0-native-round15/`. The owned browser
closed through its own connection and PID 87700 was absent afterward. No backend,
fixture, owner session, daily TUIC endpoint or alternate OS input was used.
Documentation only; no application build or test suite ran.


## Round 16: chooser opens, both key targets time out (2026-10-24)

The coordinator reported the fixed MacControl build installed and its Screen
Recording/Accessibility grants renewed. This attempt still found a native-key
transport blocker; it did not report a permission denial. The console was
unlocked. Only the same owned CFT149 scratch profile was launched, without a
load flag (PID **52530**). No backend or fixture was needed.

MacControl listed owned Extensions window **14990**, and the native Load
unpacked click succeeded. The next call contained only this action, no wait:

```json
{"actions":[{"type":"key","app":"Google Chrome for Testing","window_id":14990,"key":"cmd+shift+g"}]}
```

It returned exactly:

> MCP execution failed: Mcp error: -32000: TUICommander IPC request failed: read: response timed out

Host timestamps bound the call at approximately 15 seconds (09:50:18–09:50:33
CEST), not a separately instrumented duration. Unlike Round 15, the subsequent
`list_windows` **succeeded**, identifying the browser-native **Open** chooser as
window **15013**. Therefore this timeout is not evidence that the upstream died.
A separate single-key call explicitly targeted that chooser rather than its
parent window:

```json
{"actions":[{"type":"key","app":"Google Chrome for Testing","window_id":15013,"key":"cmd+shift+g"}]}
```

That call returned the same IPC timeout, approximately 18 seconds later
(09:50:48–09:51:06 CEST, host timestamps). No path was typed and no Select action
was sent. Native input stopped after these two distinct window targets; no
alternative OS-input transport or permission change was attempted. The other
CFT window **14155**, Device Activation, was not touched or captured.

AC1 remains unchecked: native chooser selection and the following no-load-flag
persistence check are unproved. Round 12's side-panel tab return, trusted picks,
actual top crop and cancellation chronology remain valid and were not rerun.
Raw argv/PID, exact action lists, timing bounds and cleanup are under
`~/Gits/.tmp/design-web/s0-native-round16/`. `owned-chooser.png` is a **window-only**
capture of 15013 retained for diagnosing the blocker, not an installation pass.
The owned browser closed through its own connection; PID 52530 was absent
subsequently. No daily TUIC endpoint, Boss window, backend, owner session or
fixture server was used. Documentation only; no application build or suite ran.


### Round 16 fixed-build continuation: persistent native install PASS (2026-10-24)

After the coordinator confirmed the old MacControl upstream was stopped, this
continuation completed the previously blocked folder selection. The console was
unlocked; only the existing owned CFT149 profile was launched (PID **89686**).
No TUIC backend, fixture or owner session was started.

The first native click did not establish an Open chooser. The browser harness
then clicked the **actual Load unpacked button** in `chrome://extensions` and
MacControl listed the resulting native **Open** window **15454**. This is the
browser's normal `developerPrivate.loadUnpacked` UI, not
`Extensions.loadUnpacked`, a flag load, or a fabricated install-state write.

Direct key targeting of chooser 15454 returned `failed to focus window: focus:
selected window is no longer available (it moved, resized or closed)`, even
after a fresh scoped screenshot. Targeting its owning Extensions window
**15418** succeeded: Cmd+Shift+G, paste the actual absolute directory
`~/Gits/.tmp/design-web/profile/tuic-design-extension`, Enter to accept the path,
then a **separate Enter** to select the directory. Each call contained one
action and no explicit wait. Several calls reported `user_active` **after**
reporting the named action completed; an earlier typing attempt stopped after
one character and was superseded by the complete paste. No incomplete typing
is counted as installation evidence. The successful result below independently
confirms the selected directory.

Before closing the browser, its read-only extension inventory reported:

- ID `eefbgjmmpaimpeaflioojmdipmfchefo`;
- name **TUICommander Design Mode**, version **2.0.0**;
- actual path `~/Gits/.tmp/design-web/profile/tuic-design-extension`;
- **enabled: true**.

The browser closed gracefully through its own connection and PID 89686 was
verified absent. A new process, PID **1540**, started with the same profile and
**no `--load-extension` flag**. The same read-only inventory again returned the
same ID, name, version, path and **enabled: true**. No loading/install API or
profile preferences were used between launch and observation. This proves the
missing native-install/no-load-restart persistence requirement, in contrast to
the empty inventory after the earlier flag/test-API installs.

The decisive argv, before/after inventories and PID cleanup are committed in
[round16-persistence.json](../../spike/design-mode-v2-s0/evidence/round16-persistence.json).
Raw harnesses and responses remain under
`~/Gits/.tmp/design-web/s0-native-round16-fixed/`; `chooser.png`, `installed.png`
and `after-restart.png` capture only the owned chooser/Extensions windows. The
unrelated CFT window **14155**, TUICommander Mobile, was never targeted or
captured. Restarted browser 1540 also closed gracefully and was verified absent.
No task-created runtime process remains.

**AC1 result:** (a) native installation/persistence now PASS; (b), (e), and (f)
retain Round 12's actual side-panel/tab-return, trusted pick/cancellation and
nonempty-crop evidence. Those checks were not rerun for this documentation-only
continuation. The PNG uses the real all-sites capture grant; no new activeTab
claim, cross-platform claim, Edge-only replay or application-suite pass is made.
No product code, browser permissions, Boss window or daily TUIC endpoint changed.
