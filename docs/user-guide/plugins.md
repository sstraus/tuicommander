# Plugins

TUICommander has an Obsidian-style plugin system. Plugins can watch terminal output, push notifications to the Activity Center, render markdown panels, control PTY sessions, and more.

## Installing Plugins

### From the Community Registry

1. Open **Settings** (`Cmd+,`) → **Plugins** tab → **Browse**
2. Browse available plugins — each shows name, description, and author
3. Click **Install** on any plugin
4. The plugin is downloaded and activated immediately

An "Update available" badge appears when a newer version exists in the registry.

Registry plugins include file viewers such as **DOCX Preview**, which opens Word `.docx`/`.dotx` files from the File Browser as an HTML preview tab; **XLSX Preview**, which opens Excel and OpenDocument spreadsheets as sortable tables; and **SQLite Viewer**, which browses schema and indexes, filters and paginates rows, explains read-only queries, copies CSV, and explicitly edits primary-key tables for `.db`, `.sqlite`, `.sqlite3`, `.db3`, and `.s3db` files. SQLite Viewer bundles its JavaScript and WebAssembly engine, makes no CDN request, and releases its live database engine when the viewer tab closes. Stock sql.js cannot open SQLCipher-encrypted databases; the viewer reports that case instead of prompting for an unusable key.

### From a ZIP File

1. Open **Settings** → **Plugins** → **Installed**
2. Click **Install from file...**
3. Select a `.zip` archive containing the plugin

### Via Deep Link

Click a link like `tuic://install-plugin?url=https://example.com/plugin.zip` — TUICommander shows a confirmation dialog, then downloads and installs the plugin. Only HTTPS URLs are accepted.

### Manual Installation

Copy the plugin directory to:

- **macOS:** `~/Library/Application Support/com.tuic.commander/plugins/my-plugin/`
- **Linux:** `~/.config/tuicommander/plugins/my-plugin/`
- **Windows:** `%APPDATA%/com.tuic.commander/plugins/my-plugin/`

A plugin directory contains at minimum `manifest.json` and `main.js`.

## Managing Plugins

Loaded configurable plugins publish individual settings through a scoped settings service. Saved legacy configuration is retained and validated on load. Changes schedule a plugin reload; a stale revision or invalid batch writes nothing. Secret settings show only whether a value is set. Disabling/unloading removes the plugin settings from discovery.

### Work-item provider data

GitHub Work, Jira and GitLab use Tasks-owned `data/work-items.json` for provider accounts. Configure these accounts from Tasks, not from a plugin Settings page. The reserved `data/config.json` requires a settings schema and must not be used for these accounts.

If an older GitHub Work installation reports `Configurable plugin requires a settings schema`, first install the app and plugin versions that use `work-items.json`. With the app stopped, back up the old `data/config.json` and move it to `data/work-items.json` in that plugin's installed directory. Do not overwrite an existing destination or change the file contents. Restart the app and check that the plugin loads. This is a one-time operator action; the app does not read the old file as a fallback.


### GitHub Work post-login verification

The account HTTP port, runtime catalog and account-scoped B6/B7 ports are already integrated. A recorded-fixture pass is not proof of desktop loading or a successful account login.

An operator can run a read-only check after login:

1. Start a **separate desktop verification instance** with `TUIC_APP_INSTANCE=github-work-check`. Do this only when you can release the global dictation shortcut and microphone. An agent must not start a second desktop app. Do not use the production instance or its sessions.
2. In that instance, install and enable GitHub Work. Complete GitHub device-flow login in Settings, with `repo read:project` consent. Configure the account and repository in Tasks. An old authorization without Projects access needs new consent.
3. Select an accessible issue URL, PR URL and Projects v2 node ID (`PVT_...`). The project must have a single-select Status field. Confirm the verification instance's HTTP port; it must not be `9876`.
4. Set these non-secret values once in your shell:

   ```sh
   export GH_WORK_CHECK_URL=http://127.0.0.1:9877
   export GH_WORK_CHECK_ACCOUNT=github.com
   export GH_WORK_CHECK_ISSUE=https://github.com/OWNER/REPO/issues/NUMBER
   export GH_WORK_CHECK_PR=https://github.com/OWNER/REPO/pull/NUMBER
   export GH_WORK_CHECK_PROJECT=PVT_REPLACE_WITH_PROJECT_NODE_ID
   ```

After login and setup, the check is one command from the checkout:

```sh
node scripts/github-work-live-check.mjs "$GH_WORK_CHECK_URL" "$GH_WORK_CHECK_ACCOUNT" "$GH_WORK_CHECK_ISSUE" "$GH_WORK_CHECK_PR" "$GH_WORK_CHECK_PROJECT" --isolated-desktop
```

The explicit flag confirms that the selected port belongs to the isolated desktop; the script cannot identify an instance from its port alone. The script refuses port `9876` and non-loopback URLs. It does not start an app, inspect credential stores, authenticate or change GitHub data. It checks the real desktop plugin loaded state, then imports a fresh installed module graph and uses temporary local registrations with the real Rust account ports. This is not a second activation of the installed plugin. It reads search, issue/PR detail, bounded comment pages, duplicate candidates, B6 threads, B7 action state and Projects board/roadmap data. It allows at most 24 port calls and waits at most 90 seconds for its result. The report contains check names and counts, not response bodies or credentials. Exit `0` means these reads passed; exit `1` identifies the failed stage; exit `2` means invalid arguments. A timeout does not cancel an in-flight backend read.

This probe does **not** prove Tasks rendering, the actual Tasks registry's unload cleanup, start-work delivery or live mutations. Those remain separate checks. Shut down only the verification instance when finished. Until the operator runs it, the account/desktop integration criterion remains unchecked.

### Registered-provider desktop E2E

For an authorized `agent-test-<name>` desktop, use the Unix socket printed by
`make agent-test AGENT_TEST_DRY_RUN=1`. Install GitHub Work through the desktop
installer. Configure exactly one account with repository
`sstraus/tuic-test-sandbox` in its `data/work-items.json`. Complete that instance's
Device Flow with the same account. Supply a sandbox issue with a comment and a
sandbox PR with a review thread:

```sh
node scripts/github-work-desktop-e2e.mjs "$GH_WORK_CHECK_SOCKET" sstraus 1 2 --isolated-desktop
```

This E2E uses the installed plugin's existing registration and actual Tasks UI.
It reads both items, opens their details, and pages PR comments into the B6
review thread. It also reads the B7 state through the production account-scoped
IPC port. It does not replace modules, register a test host, or mock HTTP.
It sends no GitHub mutations. Projects, start-work, unload cleanup, and B7
mutation behavior remain separate checks. The result contains check names, not
private response bodies. The outer result deadline is 450 seconds.

Append `--preflight-only` to check plugin load, sandbox configuration and Tasks
mounting before login. A preflight success is **not** authenticated E2E evidence.
The socket must be below `~/Gits/.tmp/tuic-at/`; there is no default-app target.
Stop the selected instance with the matching `make agent-test-stop` variables.

### Provider closure prerequisites

A private headless backend and a worktree Vite frontend isolate backend state,
but the normal browser loader skips user plugins before discovery. They do not
provide an equivalent loaded-user-plugin E2E. Do not work around this by faking
Tauri globals, manually substituting a host, or mocking authenticated HTTP. An
agent must not start an ordinary second desktop app to bypass this limitation.
An explicitly authorized `make agent-test` instance is the isolated desktop
verification path; use a unique `agent-test-<name>` instance and port.

The remaining checks need operator input:

- **GitHub Work integration:** complete device-flow login for a test account with
  `repo read:project`; configure its repository in Tasks; supply an accessible
  issue, PR and Projects v2 project. In an approved loaded-plugin runtime, run
  the read-only probe above and separately check the actual Tasks views.
- **GitHub create metadata E2E:** authorize one issue creation in a named scratch
  repository. With the installed plugin loaded, open Tasks → New task, select
  Issue, confirm repository labels/milestones/assignees load, then submit the
  authorized issue and read back its repository and metadata. Record its URL.
  Discovery alone and the read-only probe do not prove issue creation.
- **Jira recordings:** authorize a test site/account and read-only collection of
  issue transitions, comment pages and agile board configuration with a
  multi-status column. Keep the observed 429/Retry-After boundary unverified
  until a real response is available; do not force a rate limit. Account
  verification and post-write ADF read-back remain separate boundaries; the
  latter needs explicit scratch-project write consent. The Jira fixture README
  lists recorded and synthetic evidence separately.

Enter credentials only through the core account login. Do not send them in chat,
copy them from token stores, or commit them with fixtures. Until these prerequisites
and the real-runtime checks are satisfied, keep the corresponding integration,
recorded-fixture and create-E2E criteria unchecked.

### Settings → Plugins → Installed

The Installed tab lists all installed plugins:

- **Toggle switch** — Enable or disable a plugin. Disabled plugins are not loaded but remain installed.
- **Logs** — Click to expand the plugin's log viewer. Shows recent activity and errors (500-entry ring buffer).
- **Uninstall** — Remove the plugin directory (confirmation required).

Error count badges appear on plugins that have logged errors.

### Preinstalled External Plugins

Plan Tracker and Stories Ticker are ordinary external plugins. TUICommander installs them once when upgrading from the compiled-in implementations, after which they can be disabled, uninstalled, or updated like any other plugin. An explicit uninstall is permanent; later launches do not recreate the plugin.

External plugins load in the desktop app. Browser/PWA mode does not execute
locally installed plugin code, so these two integrations are unavailable there.

## How Plugins Work

Plugins interact with the app through a **PluginHost API** organized in 4 capability tiers:

| Tier | Access | Examples |
|------|--------|----------|
| **1** | Always available | Watch terminal output, add Activity Center items, provide markdown content |
| **2** | Always available | Read repository list, active branch, terminal sessions (read-only) |
| **3** | Requires capability | Send input to terminals (`pty:write` — raw `writePty` or agent-aware `sendAgentInput`), open markdown panels (`ui:markdown`), play sounds (`ui:sound`), read/list/watch files (`fs:read`, `fs:list`, `fs:watch`) |
| **4** | Requires capability | Invoke whitelisted Tauri commands (`invoke:read_file`, `invoke:list_markdown_files`) |

Capabilities are declared in the plugin's `manifest.json`. A plugin without `pty:write` cannot send input to your terminals.

## Activity Center

The toolbar bell icon is the Activity Center. Plugins contribute sections and items here:

- **Sections** — Grouped headings (e.g., "ACTIVE PLAN", "CI STATUS")
- **Items** — Individual notifications with icon, title, subtitle
- **Actions** — Click an item to open its detail (usually a markdown panel), or dismiss it

The bell counts active, dismissible notifications. Static plugin status rows and dashboard launchers stay visible without adding to the badge.

## Hot Reload

When you edit a plugin's files, TUICommander detects the change and automatically reloads the plugin — no restart needed. Save `main.js` and see changes in seconds.

## Example Plugins

TUICommander ships with example plugins in `examples/plugins/`:

| Plugin | What it does |
|--------|-------------|
| `hello-world` | Minimal example — watches terminal output, adds Activity Center items |
| `auto-confirm` | Auto-responds to Y/N prompts in terminal |
| `ci-notifier` | Sound notifications and markdown panels for CI events |
| `repo-dashboard` | Reads repo state, generates dynamic markdown summaries |
| `report-watcher` | Watches terminal for generated report files, shows them in Activity Center |

## Writing Your Own Plugin

See the [Plugin Authoring Guide](../plugins.md) for the full API reference, manifest format, capability details, structured event types, and testing patterns.

## Troubleshooting

| Problem | Fix |
|---------|-----|
| Plugin not appearing | Check that `manifest.json` exists and `id` matches the directory name |
| "Requires app version X.Y.Z" | Update TUICommander or lower `minAppVersion` in the manifest |
| "Requires capability X" | Add the capability to the `capabilities` array in `manifest.json` |
| Changes not taking effect | Save the file again to trigger hot reload, or restart the app |
| Plugin errors | Check Settings → Plugins → Logs for the plugin's error log |

Repository-aware plugins discard scans started before a repository switch or unload.
The MDKB status ticker refreshes when you select another repository.

Automatic retry and wakeup decisions are cancelled when the agent resumes work,
user input or a dialog changes its state, the session closes, or the plugin unloads
while the composer is being inspected.

CSV, DOCX and XLSX previews keep files from different repositories in separate
panels, even when their file names match. Reopening the same file reuses its panel.

The community catalogue declares each shipped plugin’s permissions, agent targets,
CLI binaries and minimum app version. Dashboard plugins need at least v1.0.3;
MDKB and RTK dashboard lifetime handling needs v1.7.6. SQLite’s panel bundle keeps
its ES2022 JavaScript target.

In the Tasks board, lane counts say how many cards are loaded. Select **More** in any lane to load the next board page across all lanes. Cards show assignees and labels. Select **Move…** for a destination menu; unavailable transitions are disabled. Use the arrow keys to navigate, Enter to move, or Escape to close. Scroll inside the menu to reach more destinations. After a move, page one refreshes while other loaded cards remain visible; reload to refresh later pages.

### Remembered Tasks views

Tasks restores your selected source for each repository. Each provider account keeps its own List, Board or Roadmap choice and list group, sort, preset, search and filters. These choices survive closing Tasks and restarting the app. If a provider stops accepting a saved sort, Tasks retries the last accepted sort, then the provider default.

### Tasks start-work template

Add `.tuic-start-work.md` beside `.tuic.json` in your repository root to customize
the brief sent by Tasks **Start work**. For example:

```markdown
Work on {{key}}: {{title}}
Source: {{url}}
Read the repository instructions before making changes.
```

The file is read on each action. The three placeholders are replaced once;
unknown placeholders remain literal. Without a template, Start work uses the
item key, title and body as before. A template replaces that entire brief.
Item control characters are removed before substitution. The delivered result
retains the 12,000-character cap and untrusted-content wrapper. Read errors are
shown before a worktree is created.

### Jira query errors

In Tasks search, prefix a query with `jql:` to require JQL, or `text:` for
literal text. Jira parse errors stay visible with the input unchanged. A previous
result page remains marked as stale until a corrected query succeeds. Issue keys
and saved-filter predicates such as `filter = 28712` keep their usual meaning.

Tasks small-list mode (#1851-7ade): when the provider reports at most 1,000 matches, Tasks collects the complete search/preset scope and uses the Rust projection for all-field sort, group and filter. A changed total, missing rows or a nonterminal cursor cannot establish completeness. Unknown totals and larger lists keep provider-supported server controls. No extra cache or persisted state is added.

GitHub Tasks boards (#1846-55d7): the optional provider `listBoards(signal)` port discovers the current user Projects v2 when Board opens. The existing picker selects a project; paging preserves project identities. Missing `read:project` stays visible instead of claiming there are no boards. Providers without discovery retain their configured boards. Discovery does not persist selections or write to GitHub.

Async plugin startup completes before the plugin is marked as loaded. Startup failures appear in the plugin error state and its log. Disabling or reloading a plugin cancels its startup lifetime and prevents late registrations from the old host.

Cancelling Jira URL resolution stops its in-flight HTTP requests. Plugins can also cancel their own HTTP requests; unloading a plugin cancels its active requests.

Jira and GitLab expose a default project in their schema-driven plugin settings page. Jira uses a project ID; GitLab accepts a project path or ID. A project saved for an account in Tasks setup takes precedence. The default applies after the plugin reloads; empty means no fallback project. Account bindings remain in `data/work-items.json`, while these defaults use the settings service in `data/config.json`.

### Same-version updates

An Update badge can also mean that installed payload files differ from the
catalogue at the same version. User settings in `data/` do not cause this badge.
The app computes installed identity when it discovers plugins; Refresh updates
the catalogue. Older catalogue entries without a payload digest use version-only
comparison. A newer locally installed version is not downgraded.
