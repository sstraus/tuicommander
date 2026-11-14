# Design Mode v2

Status: Design proposal. Date: 2026-10-08.

## 1. Problem

Design Mode v1 is unusable:

1. Chrome `Overlay.setInspectMode` is always on while armed. It swallows every click; the page cannot be navigated (`design_mode/manager.rs` re-arms after each pick).
2. It starts from any agent terminal tab (`TabBar.tsx:382-389`, `useCommandPaletteActions.ts:141-152`).
3. A grab lands in the agent's input line with no review step, no per-pick comment, no choice of agent.
4. Nothing closes the loop after the agent edits the code.
5. It needs a URL (`Dev Server URL` in Repo Settings); otherwise `about:blank`.
6. **The "controlled by automated test software" banner has a cause in our code.** `browser.rs:104-110` launches through `chromey` and appends `.args([...])` without `disable_default_args()`. chromey then prepends `DEFAULT_ARGS` (`chromey-2.58.2/src/browser.rs:1578-1608`), which include `--enable-automation`, `--disable-extensions`, `--disable-blink-features=AutomationControlled`, `--disable-sync`, `--use-mock-keychain`. The banner is the effect of `--enable-automation`; `--disable-extensions` would also block any extension in that profile. The automation flag causes the banner in supported Chromium browsers.
   **Launch update:** the v1 browser launch no longer passes `--enable-automation` or `--disable-extensions`. This was a prerequisite for loading the extension in the TUIC-launched browser. The v1 launch still uses CDP.

Target: Design Mode starts only from a web page, never from a terminal tab. Picks go to a sidebar: collected data, a comment, the target agent (same hand-off as Markdown "Send to agent"). When the agent has made the change the page reloads. The user can always reach a page, also when no URL was printed.

## 2. Decisions and prior art already on record (not re-proposed)

| Source | Fact | Consequence |
|---|---|---|
| Earlier design-mode analysis §6 | Embedded CDP screencast rejected: TUIC would redraw frames and forward pointer, keyboard, clipboard, IME; Edge Tools jank (vscode-edge-devtools#931). Never prototyped. | E is not the primary path. |
| Hidden-tab screencast probe | `Page.startScreencast` emitted no frame in 5 s for a hidden tab. | E is also fragile when the source tab is backgrounded. |
| Original Design Mode design | Rejected: Tauri child webview (`add_child`, unstable): three engines, only WebView2 speaks CDP, no WebKit screenshot API, child is a native view ABOVE the HTML (palette/modals/toasts under it). Rejected: porting a guest-overlay design. | N stays rejected. I re-checked the facts in tauri source (§4.4) and found nothing new in its favour. |
| `design_mode/browser.rs` `DEFERRED (2026-09-23)` | Main-target inspect cannot reach cross-origin OOPIFs. | An extension content script can (`all_frames`, §5.2). |

## 3. Prior-art findings (what to copy, what not)

One existing design-mode implementation was read at source level. It embeds a real Chromium window in each pane. This section keeps the design lessons only.

### 3.1 Embedded browser

The reference tool builds one embedded browser view per tab. A screencast serves only its mobile and headless clients. It has an embedded real browser; TUIC (Tauri) has none. The design cannot be ported as is; a real external browser plus an extension is the closest equivalent.

### 3.2 Picking is armed on demand, one pick per arm

A guest script runs only while a pick is armed; a full-viewport hit-catcher calls `stopPropagation()`/`preventDefault()`. Capture-phase listeners on `window` may still see the click. One operation per tab, 120 s timeout, Esc cancels. The payload holds HTML, computed CSS, a cropped screenshot, the source file and line from dev source maps, and the React component name.

Lesson: the v1 swallow is a consequence of using Chrome's inspector. An in-page overlay armed only during a pick leaves the page navigable. An extension content script is the same mechanism in a normal browser.

### 3.3 Sidebar, comment, hand-off

- A card per pick, with a comment textarea and an intent toggle.
- A tray with edit/delete/copy/clear and a Send menu.
- The same send menu serves editor/Markdown review notes and browser picks. One send path, several producers.
- Page text is bounded to 2048 chars and fenced with a backtick-run-aware fence.

### 3.4 Reaching a page without a URL

Address bar with history; a new-tab omnibox over tabs, files, URLs and agents. Port discovery attributes a listener to a worktree by process `cwd`, then by a command line that contains the worktree path; the deepest path wins. URLs that dev servers print in the PTY are ANSI-stripped and validated against a live listener pid. No config-file parsing for the port.

### 3.5 Reload

The reference relies on dev-server HMR plus manual reload; it has no agent-idle-triggered reload. The idle-triggered reload below is TUIC's own design.

### 3.6 Copy / do not copy

| Copy | Do not copy |
|---|---|
| Arm per pick; Esc/timeout; page navigable otherwise. | Embedded browser view / embedded pane. |
| Port attribution by process cwd then command line (deepest wins); PTY-URL hint validated against a live listener. | An all-worktree ports panel. |
| Tray with edit/delete + one shared Send path. | Markup/drawing tools, intent taxonomy: YAGNI. |
| Bounded, fenced page text in the prompt. | Search-engine fallback in the address bar. |

## 4. Candidates

Constraints: the page is a cross-origin dev server; picking must happen inside it; the page stays fully usable; no automation banner; the user's own Chrome should be usable.

| # | Option | Pick in cross-origin frames | Banner | Screenshot | Reload | Own Chrome/Edge | Main cost |
|---|---|---|---|---|---|---|---|
| **X-A** | **Browser extension (MV3): content script picks, side panel is the sidebar, talks to TUIC; no CDP** | Yes (`all_frames`) | None (no automation flag, no debugging port) | `chrome.tabs.captureVisibleTab` + crop | `chrome.tabs.reload` | Yes | Extension distribution, TUIC-extension channel, per-browser quirks |
| X-B | Extension for UI/trigger only; TUIC CDP backend keeps capture and extraction | Main frame only (as v1) | Present unless `disable_default_args` fixes it; debugging port stays open | CDP | CDP | Only in the dedicated profile | Two channels (CDP + extension), keeps `manager.rs`/`browser.rs` |
| V1 | Chrome + `Overlay.setInspectMode` (today) | No | Yes | CDP | manual | No | Unusable (§1) |
| N | Tauri child webview | Yes via init script | n/a | No WebKit API | `Webview::reload` | n/a | z-order, `unstable`, 3 engines, rejected before |
| E | CDP screencast in a TUIC pane | Yes | Yes | CDP | CDP | No | Input forwarding, CPU, hidden-tab frames |
| C | iframe (+proxy) | No | n/a | n/a | yes | n/a | Cannot inspect cross-origin |
| P | TUIC plugin (`docs/plugins.md`) alone | No | n/a | n/a | no | n/a | A plugin cannot reach into a browser page |

### 4.1 Extension, full (X-A) versus extension + CDP (X-B)

- X-A drops the debugging port and `--enable-automation` altogether. Without CDP the browser is an ordinary browser: logins, other extensions, password manager and 2FA work, and the user's own Chrome/Edge can be used.
- X-A removes the reasons CDP was needed: extraction (`extract.js` is plain DOM code; run it in a `world: "MAIN"` content script so React `_debugStack`/Vue/Svelte hints are visible), screenshot (`captureVisibleTab`), hover highlight (own overlay in a closed Shadow DOM), source maps (script URLs go to TUIC; `source.rs` keeps its loopback-only guard).
- X-B keeps `DevToolsActivePort`, `--remote-debugging-port` and two protocols. Its only advantage is reuse of `manager.rs` (1233 lines, whose arm/inspect/capture parts become dead anyway). Rejected unless S0 shows X-A cannot capture something; I identified no such case.

### 4.2 TUIC plugin versus extension: which fits

TUIC plugins are JS modules in the TUIC webview (`docs/plugins.md`: Activity Center, panels, sidebar panels, output watchers, capability-gated invoke). They cannot instrument a page in an external browser, hold a native-messaging session, or survive a closed TUIC window. The sidebar must sit next to the page, where picks happen, so it is the extension's side panel. TUIC-side logic (agent list, idle-gate send, reload trigger, dev-server discovery) belongs to a core Rust module (`design_mode`, reduced). Verdict: **extension for page side + sidebar; core module for TUIC side; no plugin in v2.0.** A read-only TUIC plugin panel mirroring pending picks is possible later; not needed.

### 4.3 Support facts for the extension route (checked online, 2026-10-08)

- Chrome 137+ branded builds no longer honour `--load-extension`; it still works on Chromium and Chrome for Testing; `--disable-features=DisableLoadExtensionCommandLineSwitch` is a community workaround, not documented, may vanish ([chromium-extensions PSA](https://groups.google.com/a/chromium.org/g/chromium-extensions/c/1-g8EFx2BBY/m/S0ET5wPjCAAJ), [botasaurus#251](https://github.com/omkarcloud/botasaurus/issues/251)). CDP `Extensions.loadUnpacked` needs `--remote-debugging-pipe` and `--enable-unsafe-extension-debugging`, not `--remote-debugging-port` ([bitcrowd](https://bitcrowd.dev/loading-chrome-extensions-for-development-in-2025/)). Programmatic loading in branded Chrome therefore drags the automation machinery back in. Supported route: "Load unpacked" once in `chrome://extensions` (persistence across restarts not confirmed by the sources; S0) or the Chrome Web Store.
- `chrome.sidePanel`: Chrome 114+, `open()` needs a user gesture, global or per-tab panels ([Chrome docs](https://developer.chrome.com/docs/extensions/reference/api/sidePanel)). Edge supports it under the name "sidebar", with known behavioural differences (panel not re-shown on tab switch back; `sidePanel.open()` reports) ([Microsoft docs](https://learn.microsoft.com/en-us/microsoft-edge/extensions/developer-guide/sidebar)). Not verified on Edge 154 here: S0.
- Not verified: whether `--load-extension` works on Edge 154. S0.

### 4.4 Why N stays rejected (re-checked in source)

`Window::add_child` needs `feature = "unstable"` (`tauri-2.11.5/src/window/mod.rs:1127`); `Cargo.toml:179` enables only `protocol-asset`. Init scripts for all frames and `eval_with_callback` exist (`src/webview/mod.rs:868,927,1929`), so picking would work, but the child is a native view above the DOM and there is no WebKit screenshot API. The earlier rejection holds.

## 5. Recommended architecture (X-A)

```
 Browser (dedicated profile or user's Chrome/Edge)           TUICommander (Rust core + frontend)
 +---------------------------------------------+             +---------------------------------------+
 | page (dev server, any origin, any frame)    |             | design_mode (reduced)                 |
 |   content script (MAIN world, all_frames)   |             |   sanitise/format  payload.rs         |
 |   overlay + extract.js + HMR socket probe   |             |   source maps      source.rs          |
 |        ^ postMessage / runtime.sendMessage  |             |   dev-server discovery                |
 | service worker -- native messaging ---------+-- stdio --->|   send via enqueue_agent_command      |
 |   tabs.reload, captureVisibleTab            |  host bin   |   reload trigger (agent busy->idle)   |
 | side panel (sidebar UI)                     | (tuic-bridge|                                       |
 +---------------------------------------------+  + mcp.sock)+---------------------------------------+
```

### 5.1 Channel to TUIC: native messaging (decided, Q2); loopback HTTP is the fallback

TUIC has no always-on TCP listener. The local API is a Unix socket `<config_dir>/mcp.sock` with no auth (`docs/api/http-api.md`); the TCP listener exists only with Remote access enabled, with Basic Auth or `?token=`; every request checks Host and Origin and rejects foreign origins (`request_boundary.rs`, `cors()` at 187). A browser extension cannot use the Unix socket.

- **Native messaging.** The browser starts a host binary over stdio; the binary relays to `mcp.sock` (the shipped `tuic-bridge-*` sidecar relays the socket for MCP, but is not directly usable: it frames by newline and rejects extra arguments, `src-tauri/crates/tuic-bridge/src/main.rs:902-906,986-994`, whereas native messaging uses 4-byte length framing and passes the extension origin as an argument. Needs a length-framed adapter with an explicit instance handshake; never guess sockets). No port, no token on the wire, pinned by `allowed_origins: ["chrome-extension://<id>/"]` in the host manifest, so only our extension ID can launch it. Pages cannot reach it. No new network trust boundary. Costs: the host manifest is installed per browser (Chrome, Edge, Brave, Chromium each have a `NativeMessagingHosts` directory); its `path` goes stale when the app moves; the per-instance socket (`--instance`) must be selectable. For a custom profile, Chromium reads `<user-data-dir>/NativeMessagingHosts` (from memory of Chromium's `DIR_USER_NATIVE_MESSAGING`; unverified, S0). An open native port keeps the MV3 service worker alive (from memory; S0).
- **Loopback HTTP/SSE with pairing token (earlier proposal).** Needs a new listener on `127.0.0.1:<random>`, Origin allowlist `chrome-extension://<fixed id>` (fixed through the manifest `key`), bearer token delivered by a file in the extension directory TUIC materialises, rotated per start. Any local process that reads the token can call it; the service worker sleeps after ~30 s unless SSE traffic keeps it alive. It is a new trust boundary that native messaging avoids. Not chosen. Keep as fallback only if S0 finds native messaging unreliable in Edge.

Messages (extension to TUIC): `hello{repo hint, version}`, `list_agents`, `list_dev_servers`, `pick{payload}`, `send{pickIds, comments, sessionId}`. TUIC to extension: `agents`, `dev_servers`, `pick_ack`, `reload{tabId}`, `agent_state`.

### 5.2 Content script (page side)

- Two scripts: an **ISOLATED-world** picker/bridge (overlay, extraction orchestration, messaging; not subject to the page CSP, immune to page monkey-patching) and a minimal **MAIN-world** hint script (React/Vue/Svelte fields, WebSocket wrapper for the HMR probe). `all_frames: true`, `match_origin_as_fallback: true`, `run_at: document_start`; origin grants are still needed for sandboxed or opaque frames. Matches only `http://localhost/*`, `http://127.0.0.1/*`, `http://[::1]/*`, `https://localhost/*`; other dev hostnames (`*.local`, `*.test`) through `optional_host_permissions`, granted per host. The extension never touches general browsing in the user's own Chrome.
- Overlay: closed Shadow DOM root; the hit-catcher exists only while armed; hover box and label; Esc cancels; a sub-frame pick relays to the top frame by `postMessage` so one overlay shows.
- Capture needs `activeTab` or broad host access; frame-coordinate transforms for sub-frame picks, tab/document races and source-map replacement are S0/S1 items.
- Extraction: current `extract.js` (`tuicExtract.call(el)`): selector, DOM path, nearby text, HTML snippet, selected computed styles, referenced CSS tokens, rect, React/Vue/Svelte source hints (`extract.js:95-121`). Today it only ran through `Runtime.callFunctionOn`; running it as a normal function is a small adaptation checked in S1.
- Screenshot: service worker `chrome.tabs.captureVisibleTab` (needs `activeTab` from the arm gesture or host permission), cropped with `OffscreenCanvas` by the element rect × devicePixelRatio. Failure or oversize omits the image, as today.
- HMR probe: wrap `WebSocket` at `document_start`; record sockets matching Vite (`vite-hmr` sub-protocol), webpack/Next (`webpack-hmr`), Parcel; count messages. Exposed to the service worker for §8.

### 5.3 Side panel (sidebar)

`side_panel.default_path`; permissions `sidePanel`, `activeTab`, `scripting`, `tabs`, `nativeMessaging`, `storage`. Content:

1. Header: repo, connection state to TUIC, **Pick** (one-shot), **Multi-pick**, **Stop**.
2. Cards, one per pick: element label, selector, source file:line, thumbnail, collapsible raw data (DOM path, snippet, styles, tokens), comment textarea, delete.
3. Footer: agent select (§7) + **Send to agent** (disabled without comment or agent).
4. Empty state / top section: **dev servers** (§6).

Global panel with per-tab state keyed by `tabId`; Edge's "not re-shown on tab switch" quirk is an S0 check.

## 6. Entry points and the no-URL case

### 6.1 Entry points (no terminal-tab item)

- In the browser: the extension toolbar icon opens the side panel (one click; `sidePanel.open()` needs a gesture, so TUIC cannot auto-open it on launch).
- In TUIC, repo-scoped, not terminal-scoped:
  - Command Palette / repo menu **Open in Design Mode browser** (launches or focuses the dedicated profile at the best dev-server URL).
  - Toolbar button on URL tabs (`PluginPanel` url tabs opened through MCP `ui-tab`, `useAppInit.ts:739`): **Open in Design Mode browser** for that URL. This is the "TUIC command in the web preview tab".
  - Click on a `localhost` link in a terminal can offer the same action (setting, default off).
- Remove: terminal tab context item, palette item on the active terminal, the `D` badge (§12).

### 6.2 No URL printed: decision

Chosen: **dev-server discovery served by TUIC, shown in the extension side panel, plus the browser's own address bar** (always present: the universal floor, nothing to build). Candidates TUIC returns for a repo, in order:

1. Explicit **Dev Server URL** in Repo Settings (`repoSettings.ts:38`), if set, labelled "configured". Explicit beats heuristics only for ordering; a fresh validated PTY URL on a different port is shown next to it (Vite increments busy ports). Candidates are deduplicated, never auto-opened when ambiguous, and monorepo apps / exact worktrees are distinguished. URL ordering: the configured URL is first. A fresh PTY URL on a different port is shown beside it with a label, so a stale setting cannot send the user to the wrong port unnoticed.
2. **Listening loopback TCP ports whose process cwd (or command line) is inside the repo or one of its worktrees**, deepest path wins. macOS/Linux: `lsof -nP -iTCP -sTCP:LISTEN -Fpcn`, then `lsof -a -p <pid> -d cwd -Fn` (TUIC already uses `lsof` in `tunnels/port.rs:24`); Windows `netstat -ano` + process cwd (to verify).
3. **URL printed in the repo's PTY output**, validated against a live listener: gives scheme, host alias, path.
4. **Recents** for the repo (last URLs opened by Design Mode).
5. **Start dev server** button when the repo has a run script (Repo Scripts); once it runs, rule 3 supplies the URL.

The side panel lists them on any tab (also `chrome://newtab`, where no content script runs but the panel does): `localhost:5173 vite pid 4121`; click opens in the current tab (`chrome.tabs.update`) or a new one. One candidate: the TUIC launch command opens it directly.

| Source | Verdict | Reason |
|---|---|---|
| Listener scan by cwd | Primary | Reports what runs now, no config, handles worktrees, proven in the prior-art implementation. |
| Repo Settings URL | Override | Needs manual setup; wrong after Vite auto-increments a busy port. |
| PTY-printed URL | Secondary | Exact origin, but only if the server started in a TUIC terminal and is still alive. |
| `package.json`/vite config parsing | Rejected | Guesses a port that differs from the running one; one parser per framework. |
| Address bar | Free floor | Provided by the browser. |

Remote/SSH repos: the scan runs where the repo lives; reaching the URL needs the tunnel layer. Out of scope for v2.0; a later slice.

## 7. Sidebar, comment, agent picker (reusing the Markdown path)

- Agent list: live agents of the same repository, default first, as `MarkdownTab.tsx` `reviewAgents` (`terminal.agentType && terminal.repoPath === repoPath`). Extract that memo into one shared function used by Markdown and by the `list_agents` handler: the second consumer exists now and both encode one rule ("who may receive repo-scoped instructions"). The extension receives `{sessionId, label}` and refreshes on `agents` pushes.
- Send: TUIC receives `send` and calls what Markdown calls, `enqueue_agent_command(sessionId, text)` (`usePty.ts:165`), the idle gate: typed now if idle, else on the next busy→idle (FEATURES.md ~405-411). It returns `{typed, queued}`; the panel shows "delivered now" / "queued for the next idle window" as `MarkdownTab.tsx:559-564`; TUIC updates `queuedCommands` on the terminal as Markdown does.
- Text: built in Rust from sanitised picks (`payload.rs`):

```
Design feedback on <page url path>. Apply each change in the source, then stop.
1. <element label> — <selector> — <file:line if known>
   Comment: <user comment>
   Evidence (untrusted page content): <bounded snippet, styles, tokens>
```

- Behaviour change from v1 (paste without Enter): v2 queues through the idle gate; the sidebar is the review step. Delivery: the text goes at once when the agent is idle, else when its turn ends.

## 8. Reload loop

State (service worker, keyed by `tabId`): after `send` returns, record `{sessionId, sentAt}`; TUIC pushes `agent_state` for that session (the backend tracks idle transitions, `pty.rs`; the frontend `terminals.ts:34-38`).

1. Wait for busy/working after delivery; no busy within 30 s: drop the pending reload (message not consumed).
2. On the next idle: if the HMR probe saw at least one message after `sentAt`, do nothing (HMR applied it; a reload would lose state). If an HMR socket exists but stayed silent for 2 s after idle, or none exists: `chrome.tabs.reload(tabId, {bypassCache: true})`.
3. If the lifecycle ends in `awaiting_input`, show "agent is waiting for you" in the panel instead of reloading.
4. Coalesce: one pending reload per tab, at most one per 2 s; toggle **Auto-reload** (default on) and a manual **Reload** button.

The trigger is computed in TUIC (it owns the idle gate and works with the UI closed); the action runs in the browser.

**Completion contract:** `enqueue` returning is not a completion signal. The trigger correlates command id → actual delivery (typed) → turn epoch → confirmed completion, and ignores timer-only idle (`pty.rs:4759` is a heuristic); `awaiting_input` is permanently true for a ready Codex prompt (`pty.rs:8536-8542`, the gate is `question_confident`). HMR traffic is not proof of a successful update. Default reload is a normal reload that preserves dirty forms (skip and notify when a form field is dirty); `bypassCache` only on explicit user action. Mid-typing: do not reload, notify (Q4).

**Dirty form (decided, Q4):** if the page holds unsaved form input, TUIC does not reload. The side panel shows a notice with a **Reload** button. A reload would erase the input, and a prompt every time interrupts the user.

## 9. Browser launch, install, the banner

- **Browser choice.** The user chooses the browser. Default: TUIC opens an Edge or Chrome window with its own profile per repository. Option: the user's everyday Edge or Chrome with the extension installed there. Safari is not in v2.0 and is not planned: the extension targets Chrome and Edge, and TUIC opens Edge or Chrome whatever the system default is. Safari would need a signed macOS app, a sidebar in a separate window and a different channel (sources: [Apple, messaging in a Safari web extension](https://developer.apple.com/documentation/safariservices/messaging-between-the-app-and-javascript-in-a-safari-web-extension), [side panel support across browsers](https://mv3-extension.com/uiux-patterns-interactive-components/side-panel-devtools-interfaces/side-panel-support-across-browsers/), [w3c/webextensions #517](https://github.com/w3c/webextensions/issues/517)).
- **Dedicated profile (default).** TUIC launches one browser per repo with `--user-data-dir=<config>/design-mode/<repo-key>`, as a plain process: no `chromey` launch, no `--remote-debugging-*`, no `--enable-automation`. If chromey stays for anything, call `.disable_default_args()`. No banner.
- **Extension install.** TUIC materialises `<config>/design-mode/<repo-key>/<browser>/tuic-design-extension` and passes it through `--load-extension` on cold launches. Edge 155 loads it automatically and replaces stale installs from another folder. A running profile must be closed first because Chromium ignores forwarded load flags; TUIC reports an error and does not stop that browser. A single discovered server opens directly; otherwise, the landing page explains how to choose or start one. Branded Chrome 137+ reports a clear unsupported-autoload error; the CDP pipe alternative is not implemented, and Chrome is unverified locally because it is not installed. The fixed manifest key keeps the extension ID stable. Store distribution remains S7.
- **Native messaging host manifest** written by TUIC for the chosen browser, `path` pointing at the bundled bridge, refreshed at each TUIC start if the app path changed.
- **User's own Chrome/Edge (optional, Q9).** Same extension, host manifest in that browser's directory; no dedicated profile.

## 10. PWA / mobile

The browser always opens on the host (as v1). A browser/PWA client of TUIC sees "Design Mode runs on the host machine" and a button that starts it there (`docs/user-guide/design-mode.md`). Picks and sends happen between the host's browser and the host's TUIC core; PWA clients see nothing of it (later option: the same `design-pick` event can drive a read-only list). No mobile promise.

## 11. Security: the page is untrusted input

1. Pages never talk to TUIC. The extension is the only client. Native messaging pins the extension ID; the host binary relays to the Unix socket (no port).
2. The content script is injected only on loopback hosts by default; other hosts need an explicit user grant (`optional_host_permissions`). The MAIN-world script talks to the extension through a per-arm random nonce so a page cannot forge a pick into the sidebar (a page can read MAIN world; the nonce limits forgery to data the page already controls).
3. All page-derived text is sanitised in Rust before any agent sees it (`payload.rs`: `strip_controls`, `clamp`, `safe_url`, `safe_snippet`, `MAX_PROMPT_BYTES` 8 KiB, secret redaction, delimiter encoding). The extension sends raw bounded fields; TUIC owns the prompt. The comment is user text from the side panel (trusted); page evidence and comment sit in separate fenced regions so page text cannot forge a comment.
4. Prompt injection from page text remains a residual risk (same as v1); the header marks evidence as untrusted.
5. Source-map fetch stays in TUIC, loopback-only and origin-bound (`source.rs:51-57,141-143,190`).
6. Extension permissions minimal and listed; `nativeMessaging` is the only elevated one. `captureVisibleTab` only for the armed tab.
7. A developer-mode unpacked extension is a user trust decision; the folder is rewritten only by TUIC, with a version check on connect (`hello` carries the extension version; TUIC refuses a mismatched major).
8. Discovery shell-outs take only integers (pid, port); never a name or path from a process.

## 12. Removal of the terminal-tab entry and of the CDP backend

- Remove now: `TabBar.tsx:72-73,382-389,882,972`, `TabViews.tsx:92,164-173`, `useCommandPaletteActions.ts:7,26,141-152`, i18n `tabBar.*DesignMode*`, related tests (updated, not deleted), docs. `design_mode::start(session_id)` (derived from an agent session) becomes repo-scoped.
- After the extension works: `browser.rs`, the CDP parts of `manager.rs` (`Overlay`, `Debugger`, `DOM`, `Input`), the `chromey` Design Mode use, the "Attach browser automation to the same Chrome" guide section and `DevToolsActivePort` handling are dead. Keep `extract.js`, `payload.rs`, `source.rs`. **Migration:** build the extension. The CDP code stays until spike S0 proves that the extension finds source maps without CDP (today source maps come only from CDP, `manager.rs:481`). Removal is slice S6, after source-map parity is verified.

## 13. Implementation slices and validation

| # | Slice | Validation |
|---|---|---|
| S0 | **Spike (throwaway)**: minimal MV3 extension with side panel + MAIN-world content script + native messaging host to `mcp.sock`. Answer: (a) `--load-extension` on Edge 154, and "Load unpacked" persistence; (b) `chrome.sidePanel` on Edge 154; (c) `<user-data-dir>/NativeMessagingHosts` honoured in a custom profile; (d) service worker kept alive by the native port; (e) pick inside a cross-origin iframe; (f) `captureVisibleTab` crop; (g) `extract.js` as a plain function; (h) HMR probe on a Vite page. | Written pass/fail table per item with browser version; decision gate: X-A, HTTP fallback (§5.1), or X-B. |
| S1 | Extension: overlay, pick, extraction, side panel cards, comment; TUIC-side `pick` handler reusing `payload.rs`/`source.rs`. | Rust tests: page text with a fence, control characters or a `javascript:` URL cannot break out of the evidence block; Vitest/jsdom tests for card state and the arm/disarm contract. |
| S2 | Native messaging host + install/pairing: manifest writer, fixed extension ID, version handshake, per-instance socket choice. | Rust tests for manifest content (`allowed_origins`, path) and refusal of a mismatched version; manual per-browser check. |
| S3 | Agent picker + Send via `enqueue_agent_command`; shared `reviewAgents`. | Vitest: only same-repo agents, default first, busy agent queues (`typed=false`), failed send keeps cards. |
| S4 | Dev-server discovery + side-panel list + launch command + Start dev server. | Rust tests with `lsof` output fixtures **recorded on this Mac** (not hand-written): two worktrees (deepest match), two servers, dead listener, a process whose command line merely contains a path-like substring. |
| S5 | Reload loop (TUIC `agent_state` + HMR probe + `tabs.reload`). | State-machine test with fake lifecycle events: busy→idle reloads once; HMR message after send skips; no busy within 30 s drops; queued sends coalesce. Named bugs: double reload, reload during HMR, no reload after a queued send. |
| S6 | Remove terminal-tab entry, drop the `--enable-automation` launcher path, delete CDP backend only after S0 proves source maps without CDP, docs (`design-mode.md`, FEATURES, tauri-commands, http-api, sync-matrix), i18n. | `rg` shows no `startDesignMode` in TabBar/palette; docs build; retain regression coverage. |
| S7 | Distribution: unlisted Web Store / Edge Add-ons listing after the pilot, update channel. | Store review outcome; outside the first release. |

## 14. Design decisions

**Interaction and transport:**

- Q2 Channel: native messaging. The browser starts a small TUIC program that forwards the messages, and no network port is opened.
- Q3 Send: queued through the idle gate, like the Markdown review notes. The text goes at once when the agent is idle, else when its turn ends.
- Q6 Install: "Load unpacked" once per profile during the pilot, then an unlisted store listing.
- Q8 Remote/SSH repos: a later slice, not in v2.0.
- Q9 Browser: the user chooses. Default: TUIC opens an Edge or Chrome window with its own profile per repository. Option: the user's everyday Edge or Chrome with the extension installed there. No Safari.

**Migration and URL selection:**

- Q1 + Q5, primary route and the CDP code: build the extension. The CDP code (`browser.rs`, the CDP parts of `manager.rs`) stays until spike S0 proves source maps without CDP. Removal in S6 follows source-map parity verification.
- Q4 Reload while a form holds unsaved input: no reload; a notice with a **Reload** button.
- Q7 URL order: the configured URL first; a fresh PTY URL beside it, labelled.

S0 may change a design choice if browser behavior invalidates an assumption.

## 15. Design risk review

| # | Risk | Design response |
|---|---|---|
| 1 | Rigid ranking is wrong: keep explicit URL labelled, prefer fresh validated PTY URL over bare ports on Vite increment; dedupe; distinguish worktrees/monorepo apps; Docker ownership cannot prove repo identity; include wildcard/IPv6; cache/batch bounded `lsof`, measure macOS cost; never auto-open ambiguous. | §6.2 amended; S4 gains an `lsof` cost measurement and a Docker/wildcard/IPv6 fixture. |
| 2 | No-CDP direction is right, but MAIN alone is not enough: MAIN inherits page CSP, early injection cannot stop later API replacement, `all_frames` needs origin grants, closed shadow roots are reachable from extension context, CDP keeps debugger script/source-map events (`manager.rs:481`) and offscreen capture; a MAIN nonce is not authentication. | §5.2 split into ISOLATED picker/bridge + minimal MAIN hints; nonce demoted to anti-confusion only (security §11.2); S0 adds source-map discovery without CDP and a CSP-strict test page. |
| 3 | Native messaging is right. Registration dirs per browser/OS; Edge documents custom user-data placement; `connectNative` keeps workers alive but state must persist; existing bridge is incompatible (newline framing, arg rejection, `main.rs:902,986`). | §5.1 amended: new length-framed adapter, explicit instance handshake; HTTP only if installer constraints dominate. S0(c)(d) stay. |
| 4 | Unpacked is acceptable for a pilot, once per profile; prefer unlisted store distribution; policy/self-hosting is not a universal consumer route. Edge 154 untested. | §9 and Q6: unlisted store listing moves from S7 "later" to the target distribution after the pilot. |
| 5 | Reload heuristic is wrong: correlate command id → delivery → turn epoch → confirmed completion; `awaiting_input` includes ready Codex (`pty.rs:8540`); timer idle is heuristic (`:4759`); HMR traffic ≠ successful update; prefer normal reload, preserve dirty forms. | §8 rewritten as above; S5 validation gains: Codex ready prompt must not count as completion, timer idle without a delivered turn must not reload, dirty form is preserved. |
| Missing | `captureVisibleTab` needs `activeTab`/all_urls; tab/document races; frame-coordinate transforms; extension updates; source-map replacement. | Added to §5.2 and S0/S1. |

## 16. Terminal entry behavior

This decision overrides sections 6, 7 and 12: keep the terminal-tab entry and the
armed `D` badge. Starting from terminal X selects X in the sidebar; the user can
switch to another live same-repository agent. Starting without a session uses
the shared repository recipient rule and selects the first agent. When the
selected agent exits, the panel explains this and offers the others; it does not
silently send feedback to a replacement.
