# Design Mode

Design Mode collects elements and comments in a browser sidebar before sending feedback to an agent. Ordinary page interaction remains available until you explicitly start picking.

## Start Design Mode

1. Open an agent terminal in your repository. Choose **Start Design Mode** from its context menu or the Command Palette. **Choose Design Mode browser** offers Edge or Chrome; the palette also offers a command for each browser. Safari is unsupported.
2. TUIC opens a dedicated profile for this repository and browser. Edge loads the extension automatically from that profile’s own folder. A single discovered dev server opens directly; otherwise, a page explains how to choose or start one. No manual extension installation is required.
3. Click the TUICommander browser extension icon to open the sidebar. It lists configured and running dev servers. Choose one, enter a URL in the address bar, or use **Start dev server** if you have configured a **Run Script** in Repository Settings. Refresh rescans servers and agents.
4. Choose **Pick element**, then click an element. A card appears and normal browsing resumes. **Multi-pick** continues collection until **Stop**, Escape or the two-minute timeout. Each card has an independent comment and Delete button. Drafts survive worker suspension; closing their browser tab deletes them. A tab holds at most 32 picks.
5. Add comments, choose the receiving agent, and click **Send to agent**. The terminal that launched Design Mode is selected initially. Other live local agents in the same Git repository, including linked worktrees, are available. If your selection exits, the sidebar asks you to choose another; it never silently switches recipients.
6. Feedback is delivered immediately when the agent is idle, or queued for its next idle window. The sidebar reports which happened. Failed sends retain your cards; retries to the same recipient reuse the exact batch and delivery key. Choosing a different recipient rebuilds the batch from retained commented picks with a new delivery key. Only commented picks are sent.

The D badge and Stop action remain on the launching terminal. Browsers open on the backend host, including when TUIC is controlled from a browser/PWA client. No debugging port or automation banner is requested by the v2 launcher. Close an existing Design Mode browser window before starting again: Chromium ignores new extension flags on a running profile, and a cold launch also repairs an extension loaded from another repository’s folder. TUIC reports this condition and does not stop browser processes.

Branded Google Chrome 137+ ignores automatic extension loading. TUIC reports an error and asks you to choose Edge or Chromium; CDP pipe installation is not implemented. Branded Chrome is unverified on the development machine because it is not installed. See the [Chrome extension team announcement](https://groups.google.com/a/chromium.org/g/chromium-extensions/c/1-g8EFx2BBY/m/S0ET5wPjCAAJ).

## Page JavaScript errors

Design Mode records the current page's uncaught errors, unhandled promise rejections, `console.error` calls and failed `<script>`, `<link>` and `<img>` loads (a 404 module included), in every frame, from the start of the page load. Each record has a message, source URL, line and column, stack and time. The sidebar keeps the last 50 per tab, deduplicated by message and source, and clears them when the tab loads a new page.

When errors exist, the sidebar shows an **Errors** section with a count badge. Tick **Attach to feedback** to send them with your next batch, or use **Send errors only** to send them alone, without any comment. Sent errors are removed from the list. They are untrusted page content: Rust strips terminal controls and URL queries, redacts recognised secrets, bounds the text and fences it, exactly like picks. Failed `fetch` and XHR requests are not observed.

## Connection and evidence

The native host pins the extension identity, repository and exact TUIC instance. It relays a bounded set of operations over that instance’s local socket or named pipe. Reopen Design Mode after restarting TUIC to refresh its binding. Connection failures show the endpoint and reason. An older running backend can list agents but returns an explicit restart error for the new feedback/discovery route.

Cards retain selectors, DOM paths, snippets, selected styles, CSS tokens and available framework source hints. CSS tokens include references in shorthand declarations such as `padding: var(--gap)`, inline styles and adopted sheets in open shadow roots; unrelated theme tokens are excluded. Rust bounds and sanitises page evidence, redacts recognised secrets, strips terminal controls and fences it as untrusted content. Captured PNG crops are saved in `design-grabs` and sent as readable `[image: path]` references. Screenshot capture is optional: permission errors and oversized crops retain DOM evidence. Cross-origin frame picks currently retain DOM evidence without frame screenshots.

Discovery lists only the dev servers of the agent selected in the side panel (initially the session that launched Design Mode; the list refreshes when you pick another agent): the configured **Dev Server URL**, live listeners attributed by process directory or an absolute command path to that session’s worktree, and URLs that session printed in its own terminal. Other worktrees of the repository and other sessions are not listed. Live listeners must answer HTTP GET `/` successfully with a `text/html` Content-Type within 750 ms. TUIC itself is excluded. Probes do not follow redirects; the scan has a 3-second budget. Fresh PTY URLs are accepted only when their port has a verified live owned HTML listener. Multiple candidates are listed instead of automatically choosing one. Windows native host registration currently targets Edge and awaits Chrome/multi-profile validation. Windows discovery offers configured URLs and the address bar; listener attribution is not claimed there. Repo-local `.tuic.json` cannot silently supply executable scripts.

The pilot has browser evidence from Chrome for Testing. Native Edge Load unpacked persistence, toolbar-granted screenshot permissions and sidebar restoration still need an Edge check. Agents ask the browser to reload the reviewed tab with MCP `ui` `action: "design_reload"` (for static pages without HMR). The extension also tells TUICommander it is alive every 5 s: Design Mode stops by itself 60 s after the browser goes away. Everyday-profile installation is later work.

### Start from a link or coordinator

Right-click a terminal HTTP(S) link and choose **Start in Design Mode**, then **Edge** or **Chrome**. This always starts or rebinds to that terminal agent and opens the exact URL, even if another agent is already connected. The D badge follows the bound session.

For layout work, coordinators use MCP `ui` with `action: "design_start"`, `session_id` naming the live terminal agent, and `url` naming the HTTP(S) page. Optional `browser` selects `chrome` or `edge`. Use `action: "design_status"` for bindings and `action: "design_stop"` with the returned `repoPath` as `repo_path`. Browser launch happens on the host; an ACP coordinator does not become the terminal agent.

If the sidebar icon is hidden, open the Extensions (puzzle) menu and pin **TUICommander Design Mode**. You can also press **Option+Shift+D** on macOS or **Alt+Shift+D** on Windows/Linux. Browser choice currently lists Edge and Chrome; launch reports if the chosen browser is not installed.

### Agent browser commands

Agents can request `design_navigate`, `design_reload`, `design_screenshot`,
`design_errors`, and `design_eval` through `ui`. The native port returns a request-id-matched reply, with a ten-second
timeout. Open the sidebar on the page to select the reviewed tab. Screenshot fails
if that tab is not visible; it never captures a different active tab. PNG files are
stored in the instance's `design-grabs` directory.

For screenshots without repeated browser-action gestures, click **Let agents capture any site**
in the extension sidebar, then accept Chrome's optional all-sites permission prompt.
The extension does not request this permission at installation. Clicking the option
again after consent does not request another grant. You can revoke access in Chrome's extension settings. This grant also permits selected-element screenshot crops,
but does not enable agent eval or remote page-error injection. Chrome's grant is
extension-wide; TUIC still restricts commands to the armed session and reviewed tab.

Without this grant, invoke the Design Mode browser action on the reviewed tab for
temporary `activeTab` capture access. Cross-origin navigation or tab closure can
remove that access. A missing grant returns `capture_permission_required` and names
**Let agents capture any site**; DOM evidence remains available. Errors and eval values are bounded,
redacted evidence, not instructions. Navigation reports acceptance, not page-load completion.

Local HTTP(S) navigation is allowed by default. Remote navigation needs a host grant.
Use **Allow eval on this host** or **Allow agents on all sites** in the sidebar to
explicitly permit agent JavaScript. Default localhost inspection access alone does
not permit eval. Grants are Chrome extension permissions and can be revoked in
Chrome's extension settings. Eval runs in the page MAIN world without a debugger port.
The sidebar retains the last fifty agent actions with session, action, time and outcome.

The grouped `ui` tool always lists `design_*` actions. Discovery is not authorization:
dispatch requires the armed terminal session and repository. Lifecycle changes
emit `notifications/tools/list_changed`; clients that cache schemas may still show
actions while stopped, but dispatch refuses them with
`Design Mode not armed for this session`. Remote error capture starts on the next
page load after an explicit host/eval grant and is removed for future loads when
that grant is revoked. The capture-only all-sites option does not register remote
error scripts. Remote `design_errors` calls without a separate host grant refuse
instead of reporting an unobserved empty list.

Chrome can return success for a host permission request already covered by the
capture grant without adding separate eval consent. The sidebar checks the granted
origins and reports this case instead of claiming eval is enabled. If this occurs,
remove the all-sites capture grant in Chrome, grant the host (or all sites) first,
then enable capture again.
