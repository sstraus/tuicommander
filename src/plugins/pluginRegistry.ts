import { urlResolverRegistry } from "../components/CommandPalette/urlPaste";
import type { ReviewThreadsPage } from "../components/PrDiffTab/reviewThreads";
import { startWork } from "../hooks/git/startWork";
import { invoke, listen } from "../invoke";
import { activityStore } from "../stores/activityStore";
import { agentConfigsStore } from "../stores/agentConfigs";
import { appLogger } from "../stores/appLogger";
import { contextMenuActionsStore } from "../stores/contextMenuActionsStore";
import { editorTabsStore } from "../stores/editorTabs";
import { keybindingsStore } from "../stores/keybindings";
import { mdTabsStore } from "../stores/mdTabs";
import { notificationsStore } from "../stores/notifications";
import { pluginStore } from "../stores/pluginStore";
import { prNotificationsStore } from "../stores/prNotifications";
import { repoSettingsStore } from "../stores/repoSettings";
import { locateFile, repositoriesStore } from "../stores/repositories";
import { sidebarPluginStore } from "../stores/sidebarPluginStore";
import { statusBarTicker } from "../stores/statusBarTicker";
import { terminalsStore } from "../stores/terminals";
import { workItemLinksStore } from "../stores/workItemLinks";
import type { WorkItemRef } from "../types/work-items.gen";
import type { GitRemote } from "../types/workItemLinks";
import { randomId, randomUuid } from "../utils/randomId";
import { sanitizeSvgIcon } from "../utils/sanitizeSvg";
import { getShellFamily, sendCommand } from "../utils/sendCommand";
import { dashboardRegistry } from "./dashboardRegistry";
import { fileIconRegistry } from "./fileIconRegistry";
import { filePreviewRegistry } from "./filePreviewRegistry";
import { markdownProviderRegistry } from "./markdownProviderRegistry";
import { guardPluginAsyncCallback, guardPluginCallback, guardPluginPredicate } from "./pluginCallbackGuard";
import type {
	ActivityItem,
	ArtifactEntry,
	Disposable,
	FileIconProvider,
	FsChangeEvent,
	GitHubPrRequest,
	HttpFetchOptions,
	HttpResponse,
	MarkdownProvider,
	NotificationSound,
	OpenPanelOptions,
	OutputWatcher,
	PanelHandle,
	PluginCapability,
	PluginHost,
	PrNotificationSnapshot,
	RepoListEntry,
	RepoSettingsSnapshot,
	RepoSnapshot,
	StateChangeEvent,
	TerminalAction,
	TerminalStateSnapshot,
	TuiPlugin,
} from "./types";
import { INVOKE_WHITELIST, NOTIFICATION_SOUNDS, PluginCapabilityError } from "./types";
import { workItemProviderRegistry } from "./workItemProviderRegistry";

/**
 * Central plugin lifecycle manager.
 *
 * Responsibilities:
 * - Calls plugin.onload(host) on register, plugin.onunload() on unregister
 * - Auto-disposes all plugin registrations (sections, watchers, providers) on unregister
 * - Dispatches raw PTY lines to registered OutputWatchers
 * - Dispatches structured Tauri events to registered typed handlers
 * - Provides tiered API surface to plugins (Tier 1-4)
 */
function createPluginRegistry() {
	// Active plugin → its aggregated Disposable (wraps all sub-registrations)
	const plugins = new Map<
		string,
		{
			plugin: TuiPlugin;
			disposable: Disposable;
			agentTypes: readonly string[];
			controller: AbortController;
			started: boolean;
			backendReady?: Promise<unknown>;
		}
	>();
	// A replacement cannot acquire backend ownership before prior teardown finishes.
	const teardowns = new Map<string, Promise<void>>();

	// Plugin command handlers: action name (`plugin:<id>:<cmd>`) → run callback.
	// Populated via host.registerCommand(), consulted by the global keydown
	// dispatcher when a combo resolves to a plugin-namespaced action.
	const pluginCommandHandlers = new Map<string, () => void | Promise<void>>();

	// Panel message bridge: tabId → onMessage callback from plugin
	const panelMessageHandlers = new Map<string, (data: unknown) => void>();
	// Panel message bridge: tabId → send function (set by PluginPanel component)
	const panelSendChannels = new Map<string, (data: unknown, transfer?: Transferable[]) => void>();
	// Panel visibility: tabId → is its tab the one on screen (set by PluginPanel).
	// A plugin that rebuilds its board on a filesystem event has no other way to
	// tell that it is rendering into a tab behind display:none.
	const panelVisibility = new Map<string, boolean>();
	// Panel visibility: tabId → onVisibilityChange callback from plugin
	const panelVisibilityHandlers = new Map<string, (visible: boolean) => void>();
	// Panel lifetime: tabId → onClose callback from plugin. A hidden panel comes
	// back; a closed one never does, and a plugin that cannot tell them apart
	// keeps a dead handle and whatever the panel owned — a watch, a timer.
	const panelCloseHandlers = new Map<string, () => void>();

	// A panel dies whenever its tab leaves the store — through the plugin's own
	// close(), the × on the tab, a middle-click or the context menu. Subscribing
	// to the store covers all of them at once, so no close path can forget.
	mdTabsStore.onPluginPanelClosed((tabId) => {
		// Deleted before it runs: the callback may reach back into the registry,
		// and a second close of the same tab must be a no-op.
		const onClose = panelCloseHandlers.get(tabId);
		panelMessageHandlers.delete(tabId);
		panelSendChannels.delete(tabId);
		panelVisibility.delete(tabId);
		panelVisibilityHandlers.delete(tabId);
		panelCloseHandlers.delete(tabId);
		onClose?.();
	});

	// Global watcher list — all watchers from all plugins, tagged with pluginId.
	// `id` is the handle Rust reports matches under; `inRust` says whether the
	// backend compiled this watcher's pattern (see syncOutputWatchers).
	const outputWatchers: Array<{ id: string; pluginId: string; watcher: OutputWatcher; inRust: boolean }> = [];
	let nextWatcherId = 0;
	// Identifies this frontend to the backend. A desktop window and a browser tab
	// hold independent watcher sets, and watcher ids are per-frontend counters —
	// without this they would collide and one client would fire the other's.
	const clientId = randomId("c");
	// Orders the *mutations*, not the replies: two syncs can be in flight and the
	// backend must ignore the older one. The same counter guards the replies here.
	let watcherSyncSeq = 0;

	/**
	 * Push the watcher set to Rust, which assembles PTY lines on the reader
	 * thread and only wakes the WebView for a line that hit. Patterns the `regex`
	 * crate cannot express (lookaround, backreferences) come back as rejected;
	 * Rust then keeps shipping every line and those watchers are matched here.
	 */
	function syncOutputWatchers(): void {
		const specs = outputWatchers.map(({ id, watcher }) => ({
			id,
			pattern: watcher.pattern.source,
			flags: watcher.pattern.flags,
		}));
		const seq = ++watcherSyncSeq;
		invoke<{ applied: boolean; rejected: string[] }>("set_plugin_output_watchers", {
			clientId,
			seq,
			watchers: specs,
		})
			.then((outcome) => {
				if (seq !== watcherSyncSeq) return;
				// `applied: false` means the backend refused this sync as stale, so
				// its rejected list describes a set nobody holds. Only a genuine
				// reply may decide who matches what — guessing wrong costs a
				// watcher that never fires again.
				if (!outcome?.applied || !Array.isArray(outcome.rejected)) {
					for (const entry of outputWatchers) entry.inRust = false;
					return;
				}
				const rejectedIds = new Set(outcome.rejected);
				for (const entry of outputWatchers) {
					entry.inRust = !rejectedIds.has(entry.id);
				}
				for (const id of rejectedIds) {
					const entry = outputWatchers.find((w) => w.id === id);
					if (entry) {
						appLogger.debug(
							"plugin",
							`Watcher of "${entry.pluginId}" stays on the frontend: /${entry.watcher.pattern.source}/ is not expressible in the Rust regex crate`,
						);
					}
				}
			})
			.catch((err) => {
				if (seq !== watcherSyncSeq) return;
				for (const entry of outputWatchers) entry.inRust = false;
				appLogger.warn("plugin", `set_plugin_output_watchers failed: ${err}`);
			});
		scheduleWatcherHeartbeat();
	}

	/**
	 * Resend the set on a timer while any watcher exists.
	 *
	 * Rust is the only source of lines now, so a client it does not know about is
	 * blind — and two paths lead there with no local symptom: the sync failed
	 * (backend restarting, transport hiccup), or the client bound evicted this
	 * set to make room for a newer one, which reloading a browser tab does
	 * without any disconnect reaching the backend. Neither raises an event to
	 * recover from, so recovery is periodic. It doubles as the liveness signal
	 * eviction picks by, which is why it runs even when the set is unchanged.
	 */
	const WATCHER_HEARTBEAT_MS = 30_000;
	let watcherHeartbeat: ReturnType<typeof setInterval> | undefined;

	function scheduleWatcherHeartbeat(): void {
		const wanted = outputWatchers.length > 0;
		if (wanted && watcherHeartbeat === undefined) {
			watcherHeartbeat = setInterval(syncOutputWatchers, WATCHER_HEARTBEAT_MS);
		} else if (!wanted && watcherHeartbeat !== undefined) {
			// No watcher left: the backend holds a parked empty set and there is
			// nothing to keep alive.
			clearInterval(watcherHeartbeat);
			watcherHeartbeat = undefined;
		}
	}

	// Structured event handlers: type → list of { pluginId, handler }
	const structuredHandlers = new Map<
		string,
		Array<{ pluginId: string; handler: (payload: unknown, sessionId: string) => void }>
	>();

	// State change listeners for terminal/branch changes
	const stateChangeListeners: Array<{ pluginId: string; callback: (event: StateChangeEvent) => void }> = [];

	// -------------------------------------------------------------------------
	// Agent-type filtering
	// -------------------------------------------------------------------------

	// Fast lookup for paused plugins — avoids reactive store access in hot paths
	const pausedPlugins = new Set<string>();

	/** Returns true if a plugin is temporarily paused. */
	function isPluginPaused(pluginId: string): boolean {
		return pausedPlugins.has(pluginId);
	}

	/** Update the paused set (called from pluginStore.setPaused). */
	function setPluginPaused(pluginId: string, paused: boolean): void {
		if (paused) pausedPlugins.add(pluginId);
		else pausedPlugins.delete(pluginId);
	}

	/** Returns true if a plugin should receive events from a given session. */
	function pluginMatchesSession(pluginId: string, sessionId: string): boolean {
		const entry = plugins.get(pluginId);
		if (!entry || entry.agentTypes.length === 0) return true; // universal plugin
		const agentType = terminalsStore.getAgentTypeForSession(sessionId);
		return agentType !== null && entry.agentTypes.includes(agentType);
	}

	// -------------------------------------------------------------------------
	// Capability checking
	// -------------------------------------------------------------------------

	function requireCapability(
		pluginId: string,
		capabilities: ReadonlySet<string> | null,
		required: PluginCapability,
	): void {
		// null = built-in plugin, no restrictions
		if (capabilities === null) return;
		if (!capabilities.has(required)) {
			throw new PluginCapabilityError(pluginId, required);
		}
	}

	// -------------------------------------------------------------------------
	// Build the PluginHost surface for a given plugin
	// -------------------------------------------------------------------------

	/**
	 * Build a PluginHost for a plugin.
	 * @param pluginId - The plugin's unique ID
	 * @param disposables - Mutable array to track disposables for auto-cleanup
	 * @param capabilities - Set of declared capabilities, or null for built-in plugins (unrestricted)
	 */
	function buildHost(
		pluginId: string,
		disposables: Disposable[],
		capabilities: ReadonlySet<string> | null = null,
		isActive: () => boolean = () => true,
	): PluginHost {
		function track(d: Disposable): Disposable {
			// Wrap in idempotent guard: plugins may manually dispose() and then the
			// registry disposes again on unload — second call must be a no-op.
			let disposed = false;
			const safe: Disposable = {
				dispose() {
					if (disposed) return;
					disposed = true;
					d.dispose();
				},
			};
			if (isActive()) disposables.push(safe);
			else safe.dispose();
			return safe;
		}

		const httpCancels = new Set<() => void>();
		track({
			dispose: () => {
				for (const cancel of httpCancels) cancel();
				httpCancels.clear();
			},
		});
		const logger = pluginStore.getLogger(pluginId);
		const wrapActivityItemCallbacks = <T extends Partial<ActivityItem>>(item: T): T => {
			const wrapped = { ...item };
			if (wrapped.onClick) {
				wrapped.onClick = guardPluginCallback(pluginId, "activity item onClick", wrapped.onClick);
			}
			return wrapped;
		};

		return {
			// -- Tier 0: Logging --

			log(level, message, data) {
				logger.log(level, message, data);
				appLogger.push(level, "plugin", `[${pluginId}] ${message}`, data);
			},

			// -- Tier 1: Activity Center + watchers + providers --

			registerWorkItemProvider(provider) {
				return track(workItemProviderRegistry.register(provider));
			},

			settings(action) {
				return invoke("plugin_settings_action", { pluginId, action });
			},

			linkItem(repoPath, branch, link) {
				return workItemLinksStore.link(repoPath, branch, link);
			},

			unlinkItem(repoPath, branch) {
				return workItemLinksStore.unlink(repoPath, branch);
			},

			async startWork(request) {
				requireCapability(pluginId, capabilities, "pty:write");
				if (isPluginPaused(pluginId)) throw new Error(`Plugin ${pluginId} is paused`);
				return startWork(request);
			},

			registerUrlResolver(resolver) {
				return track(
					urlResolverRegistry.register(pluginId, (url, signal) =>
						isPluginPaused(pluginId) ? Promise.resolve(null) : resolver(url, signal),
					),
				);
			},

			registerSection(section) {
				return track(activityStore.registerSection({ ...section, pluginId }));
			},

			registerOutputWatcher(watcher: OutputWatcher): Disposable {
				// Optimistically ours until Rust confirms it compiled the pattern:
				// matching here is redundant work, never a missed line.
				const entry = { id: `w${nextWatcherId++}`, pluginId, watcher, inRust: false };
				outputWatchers.push(entry);
				syncOutputWatchers();
				return track({
					dispose() {
						const idx = outputWatchers.indexOf(entry);
						if (idx >= 0) {
							outputWatchers.splice(idx, 1);
							syncOutputWatchers();
						}
					},
				});
			},

			registerStructuredEventHandler(type, handler) {
				const list = structuredHandlers.get(type) ?? [];
				structuredHandlers.set(type, list);
				const entry = { pluginId, handler };
				list.push(entry);
				return track({
					dispose() {
						const list = structuredHandlers.get(type);
						if (!list) return;
						const idx = list.indexOf(entry);
						if (idx >= 0) list.splice(idx, 1);
					},
				});
			},

			registerMarkdownProvider(scheme: string, provider: MarkdownProvider): Disposable {
				return track(markdownProviderRegistry.register(scheme, provider));
			},

			registerFileIconProvider(provider: FileIconProvider): Disposable {
				requireCapability(pluginId, capabilities, "ui:file-icons");
				return track(fileIconRegistry.register(provider, pluginId));
			},

			registerFilePreview(options) {
				requireCapability(pluginId, capabilities, "ui:file-preview");
				return track(filePreviewRegistry.register(pluginId, options.extensions, options.onOpen));
			},

			addItem(item) {
				if (item.icon) item.icon = sanitizeSvgIcon(item.icon);
				activityStore.addItem(wrapActivityItemCallbacks(item));
			},

			removeItem(id) {
				activityStore.removeItem(id);
			},

			updateItem(id, updates) {
				if (updates.icon) updates.icon = sanitizeSvgIcon(updates.icon);
				activityStore.updateItem(id, wrapActivityItemCallbacks(updates));
			},

			// -- Tier 2: Read-only app state --

			getActiveRepo(): RepoSnapshot | null {
				const repo = repositoriesStore.getActive();
				if (!repo) return null;
				const workspace = repo.activeWorkspaceId ? repo.workspaces[repo.activeWorkspaceId] : null;
				return {
					path: repo.path,
					displayName: repo.displayName,
					// The plugin contract is the BRANCH, and it stays that — a plugin
					// renders a branch name, it never indexes our workspace map. Reading
					// it off the workspace rather than inferring it from the id preserves
					// the plugin contract if workspace identity changes independently.
					activeBranch: workspace?.branchName ?? null,
					worktreePath: workspace?.worktreePath ?? null,
				};
			},

			getRepos(): RepoListEntry[] {
				// Plugins should not see parked repos — they're considered dormant
				// from the SDK's point of view. (#1358-caf5)
				return repositoriesStore.getActivePaths().map((path) => {
					const repo = repositoriesStore.get(path);
					return { path, displayName: repo?.displayName ?? path };
				});
			},

			getActiveTerminalSessionId(): string | null {
				const terminal = terminalsStore.getActive();
				return terminal?.sessionId ?? null;
			},

			getRepoPathForSession(sessionId: string): string | null {
				const termId = terminalsStore.getTerminalForSession(sessionId);
				if (!termId) return null;
				return repositoriesStore.getRepoPathForTerminal(termId);
			},

			getSessionCwd(sessionId: string): string | null {
				const termId = terminalsStore.getTerminalForSession(sessionId);
				if (!termId) return null;
				const terminal = terminalsStore.get(termId);
				return terminal?.cwd ?? null;
			},

			async getClaudeProjectDir(repoPath: string): Promise<string | null> {
				requireCapability(pluginId, capabilities, "fs:read");
				const claudeConfigDir = agentConfigsStore.getDefaultConfig("claude")?.env?.CLAUDE_CONFIG_DIR ?? null;
				return invoke<string | null>("claude_project_dir", { cwd: repoPath, claudeConfigDir });
			},

			getActiveRepoPath(): string | null {
				return repositoriesStore.state.activeRepoPath;
			},

			getPrNotifications(): PrNotificationSnapshot[] {
				return prNotificationsStore.getActive().map((n) => ({
					id: n.id,
					repoPath: n.repoPath,
					branch: n.branch,
					prNumber: n.prNumber,
					title: n.title,
					type: n.type,
				}));
			},

			getSettings(repoPath: string): RepoSettingsSnapshot | null {
				// Unknown repo means unregistered: a settings entry is optional.
				if (!repositoriesStore.get(repoPath)) return null;
				const effective = repoSettingsStore.getEffective(repoPath);
				if (!effective) return null;
				return {
					path: effective.path,
					displayName: effective.displayName,
					baseBranch: effective.baseBranch,
					color: effective.color,
				};
			},

			getTerminalState(): TerminalStateSnapshot | null {
				const terminal = terminalsStore.getActive();
				if (!terminal) return null;
				const repoPath = this.getRepoPathForSession(terminal.sessionId ?? "");
				return {
					sessionId: terminal.sessionId,
					shellState: terminal.shellState,
					agentType: terminal.agentType,
					agentActive: terminal.agentType !== null,
					awaitingInput: terminal.awaitingInput,
					repoPath,
				};
			},

			onStateChange(callback: (event: StateChangeEvent) => void): Disposable {
				const entry = { pluginId, callback };
				stateChangeListeners.push(entry);
				return track({
					dispose() {
						const idx = stateChangeListeners.indexOf(entry);
						if (idx >= 0) stateChangeListeners.splice(idx, 1);
					},
				});
			},

			// -- Tier 2b: Git read (capability-gated) --

			async getGitBranches(repoPath: string): Promise<Array<{ name: string; isCurrent: boolean }>> {
				requireCapability(pluginId, capabilities, "git:read");
				const raw = await invoke<Array<{ name: string; is_current: boolean }>>("get_git_branches", { path: repoPath });
				return (raw ?? []).map((b) => ({ name: b.name, isCurrent: b.is_current }));
			},

			async getRecentCommits(
				repoPath: string,
				count?: number,
			): Promise<Array<{ hash: string; message: string; author: string; date: string }>> {
				requireCapability(pluginId, capabilities, "git:read");
				const raw = await invoke<Array<{ hash: string; message: string; author: string; date: string }>>(
					"get_recent_commits",
					{ path: repoPath, count: count ?? 20 },
				);
				return raw ?? [];
			},

			async getGitDiff(repoPath: string, scope?: "staged" | "unstaged"): Promise<string> {
				requireCapability(pluginId, capabilities, "git:read");
				const raw = await invoke<string>("get_git_diff", { path: repoPath, scope: scope ?? null });
				return raw ?? "";
			},

			async getRemotes(repoPath: string): Promise<GitRemote[]> {
				requireCapability(pluginId, capabilities, "git:read");
				return (await invoke<GitRemote[]>("get_git_remotes", { path: repoPath })) ?? [];
			},

			// -- Tier 3: Write actions (capability-gated) --

			registerTerminalAction(action: TerminalAction): Disposable {
				requireCapability(pluginId, capabilities, "ui:context-menu");
				// Wrap handler in a stale-plugin guard: after unregister, invocations are no-ops
				const guardedAction: TerminalAction = {
					...action,
					action: (ctx) => {
						if (!plugins.has(pluginId)) return;
						guardPluginCallback(pluginId, "terminal action", action.action)(ctx);
					},
					disabled: guardPluginPredicate(pluginId, "terminal action disabled", action.disabled, true),
				};
				return track(contextMenuActionsStore.registerAction(pluginId, guardedAction));
			},

			registerContextMenuAction(action: import("../stores/contextMenuActionsStore").ContextMenuAction): Disposable {
				requireCapability(pluginId, capabilities, "ui:context-menu");
				const guarded: import("../stores/contextMenuActionsStore").ContextMenuAction = {
					...action,
					action: (ctx) => {
						if (!plugins.has(pluginId)) return;
						guardPluginCallback(pluginId, "context menu action", action.action)(ctx);
					},
					disabled: guardPluginPredicate(pluginId, "context menu action disabled", action.disabled, true),
				};
				return track(contextMenuActionsStore.registerContextAction(pluginId, guarded));
			},

			registerSidebarPanel(options: import("../stores/sidebarPluginStore").SidebarPanelOptions) {
				requireCapability(pluginId, capabilities, "ui:sidebar");
				if (options.icon) options.icon = sanitizeSvgIcon(options.icon);
				const handle = sidebarPluginStore.registerPanel(pluginId, options);
				// Track dispose for auto-cleanup, but return the full handle
				track({ dispose: () => handle.dispose() });
				return handle;
			},

			async writePty(sessionId: string, data: string): Promise<void> {
				if (isPluginPaused(pluginId)) return;
				requireCapability(pluginId, capabilities, "pty:write");
				await invoke("write_pty", { sessionId, data });
			},

			async sendAgentInputIfEmpty(sessionId: string, text: string): Promise<boolean> {
				if (isPluginPaused(pluginId)) return false;
				requireCapability(pluginId, capabilities, "pty:write");
				return invoke<boolean>("send_agent_input_if_empty", { sessionId, text });
			},

			async sendAgentInput(sessionId: string, text: string): Promise<void> {
				if (isPluginPaused(pluginId)) return;
				requireCapability(pluginId, capabilities, "pty:write");
				const agentType = terminalsStore.getAgentTypeForSession(sessionId);
				if (!agentType) {
					appLogger.warn(
						"plugin",
						`[${pluginId}] sendAgentInput blocked — no active agent in session ${sessionId.slice(0, 8)}`,
					);
					return;
				}
				const shellFamily = await getShellFamily(sessionId);
				await sendCommand(
					(data) => invoke("write_pty", { sessionId, data }),
					text,
					agentType,
					shellFamily,
					true,
					sessionId,
				);
			},

			async readSessionOutput(sessionId: string, maxLines?: number): Promise<string> {
				requireCapability(pluginId, capabilities, "pty:read");
				return invoke<string>("plugin_read_session_output", {
					sessionId,
					maxLines: maxLines ?? null,
					pluginId,
				});
			},

			registerDashboard(options): Disposable {
				return track(
					dashboardRegistry.register({
						pluginId,
						label: options.label ?? "Dashboard",
						icon: options.icon,
						open: guardPluginAsyncCallback(pluginId, "dashboard open", options.open),
					}),
				);
			},

			registerCommand(options): Disposable {
				const actionName = `plugin:${pluginId}:${options.id}`;
				keybindingsStore.registerDynamicAction({
					action: actionName,
					label: options.title,
					pluginId,
					defaultKey: options.defaultShortcut,
				});
				pluginCommandHandlers.set(actionName, guardPluginAsyncCallback(pluginId, "command run", options.run));
				return track({
					dispose() {
						keybindingsStore.unregisterDynamicAction(actionName);
						pluginCommandHandlers.delete(actionName);
					},
				});
			},

			openMarkdownPanel(title: string, contentUri: string): void {
				requireCapability(pluginId, capabilities, "ui:markdown");
				mdTabsStore.addVirtual(title, contentUri);
			},

			openMarkdownFile(absolutePath: string): void {
				requireCapability(pluginId, capabilities, "ui:markdown");
				// The file names its own repo. Passing "" left the tab unowned, so it
				// showed up in every repo; passing the active repo would have been worse
				// still — it would have claimed a file belonging to a different one.
				const { repoPath, fsRoot, filePath } = locateFile(absolutePath);
				mdTabsStore.add(repoPath, filePath, fsRoot || undefined);
			},

			openMarkdownFileBackground(absolutePath: string): boolean {
				requireCapability(pluginId, capabilities, "ui:markdown");
				const { repoPath, fsRoot, filePath } = locateFile(absolutePath);
				if (!repoPath) return false;
				mdTabsStore.addFileBackground(repoPath, filePath, fsRoot || undefined);
				return true;
			},

			async playNotificationSound(sound?: NotificationSound): Promise<void> {
				requireCapability(pluginId, capabilities, "ui:sound");
				const resolved: NotificationSound = NOTIFICATION_SOUNDS.includes(sound as NotificationSound)
					? (sound as NotificationSound)
					: "info";
				if (sound !== undefined && resolved === "info") {
					appLogger.warn(
						"plugin",
						`[${pluginId}] playNotificationSound: unknown sound "${sound}", defaulting to "info"`,
					);
				}
				await notificationsStore.play(resolved);
			},

			// -- Tier 3b: Filesystem operations --

			async readFile(absolutePath: string): Promise<string> {
				requireCapability(pluginId, capabilities, "fs:read");
				return invoke<string>("plugin_read_file", { path: absolutePath, pluginId });
			},

			async readFiles(absolutePaths: string[]): Promise<(string | null)[]> {
				requireCapability(pluginId, capabilities, "fs:read");
				if (absolutePaths.length === 0) return [];
				return invoke<(string | null)[]>("plugin_read_files", { paths: absolutePaths, pluginId });
			},

			async readFileBase64(absolutePath: string, options?: { maxBytes?: number }): Promise<string> {
				requireCapability(pluginId, capabilities, "fs:read");
				return invoke<string>("plugin_read_file_base64", {
					path: absolutePath,
					maxBytes: options?.maxBytes,
					pluginId,
				});
			},

			async fileSize(absolutePath: string): Promise<number> {
				requireCapability(pluginId, capabilities, "fs:read");
				return invoke<number>("plugin_file_size", { path: absolutePath, pluginId });
			},
			async readFileRange(absolutePath: string, offset: number, length: number): Promise<string> {
				requireCapability(pluginId, capabilities, "fs:read");
				if (
					!Number.isSafeInteger(offset) ||
					offset < 0 ||
					!Number.isSafeInteger(length) ||
					length < 0 ||
					length > 8 * 1024 * 1024
				) {
					throw new Error("Invalid binary range: non-negative safe integers, length at most 8 MiB required");
				}
				return invoke<string>("plugin_read_file_range", { path: absolutePath, offset, length, pluginId });
			},
			async readFileTail(absolutePath: string, maxBytes: number): Promise<string> {
				requireCapability(pluginId, capabilities, "fs:read");
				return invoke<string>("plugin_read_file_tail", { path: absolutePath, maxBytes, pluginId });
			},

			async listDirectory(path: string, pattern?: string, options?: { sortBy?: "name" | "mtime" }): Promise<string[]> {
				requireCapability(pluginId, capabilities, "fs:list");
				return invoke<string[]>("plugin_list_directory", {
					path,
					pattern: pattern ?? null,
					sortBy: options?.sortBy ?? null,
					pluginId,
				});
			},

			async writeFile(absolutePath: string, content: string): Promise<void> {
				requireCapability(pluginId, capabilities, "fs:write");
				await invoke("plugin_write_file", { path: absolutePath, content, pluginId });
			},

			async writeFileBase64(absolutePath: string, content: string, options?: { maxBytes?: number }): Promise<void> {
				requireCapability(pluginId, capabilities, "fs:write");
				await invoke("plugin_write_file_base64", {
					path: absolutePath,
					content,
					maxBytes: options?.maxBytes,
					pluginId,
				});
			},

			async renamePath(from: string, to: string): Promise<void> {
				requireCapability(pluginId, capabilities, "fs:rename");
				await invoke("plugin_rename_path", { from, to, pluginId });
			},

			async scanBuildArtifacts(repoPaths: string[], options?: { forceRefresh?: boolean }): Promise<ArtifactEntry[]> {
				requireCapability(pluginId, capabilities, "fs:scan");
				return invoke<ArtifactEntry[]>("scan_build_artifacts", {
					repoPaths,
					pluginId,
					forceRefresh: options?.forceRefresh ?? false,
				});
			},

			async deleteBuildArtifact(path: string, repoPaths: string[]): Promise<void> {
				requireCapability(pluginId, capabilities, "fs:delete");
				await invoke("delete_build_artifact", { path, repoPaths, pluginId });
			},

			async trimBuildArtifact(path: string, repoPaths: string[]): Promise<number> {
				requireCapability(pluginId, capabilities, "fs:delete");
				return await invoke<number>("trim_build_artifact", { path, repoPaths, pluginId });
			},

			async watchPath(
				path: string,
				callback: (events: FsChangeEvent[]) => void,
				options?: { recursive?: boolean; debounceMs?: number },
			): Promise<Disposable> {
				requireCapability(pluginId, capabilities, "fs:watch");
				const watchId = await invoke<string>("plugin_watch_path", {
					path,
					pluginId,
					recursive: options?.recursive ?? false,
					debounceMs: options?.debounceMs ?? 300,
				});
				// Keyed on the watch, not the plugin: a plugin with K watches used to
				// get every change delivered to all K callbacks, and each one had no
				// way to tell whose path it was.
				const eventName = `plugin-fs-change-${watchId}`;
				const unlisten = await listen<FsChangeEvent[]>(eventName, (event) => {
					try {
						callback(event.payload);
					} catch (err) {
						const msg = err instanceof Error ? err.message : String(err);
						pluginStore.getLogger(pluginId).error(`watchPath callback threw: ${msg}`, err);
					}
				});
				const disposable: Disposable = {
					dispose() {
						unlisten();
						invoke("plugin_unwatch", { watchId, pluginId }).catch(() => {
							// Watcher may already be cleaned up on plugin unload
						});
					},
				};
				return track(disposable);
			},

			// -- Tier 3c: Status bar ticker --

			setTicker(options) {
				requireCapability(pluginId, capabilities, "ui:ticker");
				statusBarTicker.addMessage({
					id: options.id,
					pluginId,
					text: options.text,
					label: options.label,
					icon: options.icon ? sanitizeSvgIcon(options.icon) : undefined,
					priority: options.priority ?? 0,
					ttlMs: options.ttlMs ?? 60_000,
					onClick: options.onClick ? guardPluginCallback(pluginId, "ticker onClick", options.onClick) : undefined,
				});
			},

			clearTicker(id: string) {
				requireCapability(pluginId, capabilities, "ui:ticker");
				statusBarTicker.removeMessage(id, pluginId);
			},

			// -- Tier 3d: Panel UI --

			openPanel(options: OpenPanelOptions): PanelHandle {
				requireCapability(pluginId, capabilities, "ui:panel");
				const tabId = mdTabsStore.addPluginPanel(pluginId, options.id, options.title, options.html);
				track({ dispose: () => mdTabsStore.remove(tabId) });
				// Register message handler for this panel
				if (options.onMessage) {
					panelMessageHandlers.set(tabId, options.onMessage);
				}
				if (options.onVisibilityChange) {
					panelVisibilityHandlers.set(
						tabId,
						guardPluginCallback(pluginId, "panel onVisibilityChange", options.onVisibilityChange),
					);
				}
				if (options.onClose) {
					panelCloseHandlers.set(tabId, guardPluginCallback(pluginId, "panel onClose", options.onClose));
				}
				// addPluginPanel activates the tab, so the panel starts on screen. The
				// component confirms it a tick later; seeding avoids a window where
				// isVisible() lies to the plugin that just opened the panel.
				panelVisibility.set(tabId, true);
				return {
					tabId,
					update(html: string) {
						return mdTabsStore.updatePluginPanel(tabId, html);
					},
					isVisible() {
						return panelVisibility.get(tabId) ?? false;
					},
					close() {
						mdTabsStore.remove(tabId);
					},
					send(data: unknown, transfer?: Transferable[]) {
						const sender = panelSendChannels.get(tabId);
						if (sender) sender(data, transfer);
					},
				};
			},

			openEditorTab(filePath, repoPath, opts) {
				requireCapability(pluginId, capabilities, "ui:panel");
				editorTabsStore.add(repoPath, filePath, opts?.line, { fsRoot: opts?.fsRoot ?? repoPath });
			},

			// -- Tier 3e: Credential access --

			async readCredential(serviceName: string): Promise<string | null> {
				requireCapability(pluginId, capabilities, "credentials:read");

				return invoke<string | null>("plugin_read_credential", {
					serviceName,
					pluginId,
				});
			},

			// -- Tier 3f: HTTP requests --

			async httpFetch(url: string, options?: HttpFetchOptions): Promise<HttpResponse> {
				requireCapability(pluginId, capabilities, "net:http");
				const signal = options?.signal;
				if (signal?.aborted) throw new DOMException("Request cancelled", "AbortError");
				const requestId = randomUuid();
				const cancel = () => {
					void invoke("plugin_http_cancel", { pluginId, requestId }).catch(() => {});
				};
				httpCancels.add(cancel);
				signal?.addEventListener("abort", cancel, { once: true });
				try {
					const response = await invoke<HttpResponse>("plugin_http_fetch", {
						url,
						method: options?.method ?? null,
						headers: options?.headers ?? null,
						body: options?.body ?? null,
						account: options?.account ?? null,
						pluginId,
						requestId,
					});
					if (signal?.aborted) throw new DOMException("Request cancelled", "AbortError");
					return response;
				} catch (error) {
					if (signal?.aborted) throw new DOMException("Request cancelled", "AbortError");
					throw error;
				} finally {
					httpCancels.delete(cancel);
					signal?.removeEventListener("abort", cancel);
				}
			},

			async githubReviewThreads(
				account: string,
				reference: WorkItemRef,
				after: string | null,
			): Promise<ReviewThreadsPage> {
				requireCapability(pluginId, capabilities, "net:http");
				if (reference.providerId !== "github" || reference.accountId !== account)
					throw new Error("GitHub account mismatch");
				return invoke<ReviewThreadsPage>("plugin_github_action", {
					pluginId,
					account,
					reference,
					request: { action: "threads", after },
				});
			},

			async githubPrAction(account: string, reference: WorkItemRef, request: GitHubPrRequest): Promise<unknown> {
				requireCapability(pluginId, capabilities, "net:http");
				if (reference.providerId !== "github" || reference.accountId !== account)
					throw new Error("GitHub account mismatch");
				return invoke("plugin_github_action", { pluginId, account, reference, request });
			},

			// -- Tier 3g: CLI execution --

			async execCli(binary: string, args: string[], cwd?: string, options?: { timeoutMs?: number }): Promise<string> {
				requireCapability(pluginId, capabilities, "exec:cli");
				const timeoutMs = options?.timeoutMs;
				// JSON turns NaN and Infinity into null, which the backend reads as "use the default".
				if (timeoutMs !== undefined && !Number.isSafeInteger(timeoutMs))
					throw new Error(`execCli timeoutMs must be a whole number of milliseconds, got ${timeoutMs}`);
				return invoke<string>("plugin_exec_cli", {
					binary,
					args,
					cwd: cwd ?? null,
					pluginId,
					timeoutMs: timeoutMs ?? null,
				});
			},

			// -- Tier 4: Scoped Tauri invoke --

			async invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
				if (!INVOKE_WHITELIST.includes(cmd)) {
					throw new Error(`Plugin "${pluginId}": command "${cmd}" is not in the invoke whitelist`);
				}
				// Check capability for scoped invoke commands
				const NO_CAP_COMMANDS = ["read_plugin_data", "write_plugin_data", "delete_plugin_data"];
				const CAP_OVERRIDES: Record<string, PluginCapability> = {
					get_input_buffer_content: "pty:read",
				};
				if (capabilities !== null && !NO_CAP_COMMANDS.includes(cmd)) {
					const capKey = CAP_OVERRIDES[cmd] ?? (`invoke:${cmd}` as PluginCapability);
					requireCapability(pluginId, capabilities, capKey);
				}
				return invoke<T>(cmd, args);
			},
		};
	}

	// -------------------------------------------------------------------------
	// Lifecycle
	// -------------------------------------------------------------------------

	/**
	 * Register a plugin.
	 * @param plugin - The plugin to register
	 * @param capabilities - Optional set of declared capabilities for external plugins.
	 *   Pass null or omit for built-in plugins (unrestricted access).
	 * @param agentTypes - Optional list of agent types this plugin targets.
	 *   Empty or omit for universal plugins.
	 */
	async function register(plugin: TuiPlugin, capabilities?: string[], agentTypes?: string[]): Promise<boolean> {
		const previous = plugins.has(plugin.id) ? unregister(plugin.id) : teardowns.get(plugin.id);
		const disposables: Disposable[] = [];
		const controller = new AbortController();
		const entry = {
			plugin,
			controller,
			started: false,
			agentTypes: agentTypes ?? [],
			backendReady: undefined as Promise<unknown> | undefined,
			disposable: {
				dispose() {
					for (const d of disposables.splice(0)) {
						try {
							d.dispose();
						} catch {
							/* cleanup best-effort */
						}
					}
				},
			},
		};
		// Install ownership before the first await, including backend registration.
		plugins.set(plugin.id, entry);
		pluginStore.updatePlugin(plugin.id, { loaded: false, error: null });
		const current = () => plugins.get(plugin.id) === entry && !controller.signal.aborted;
		const rawHost = buildHost(plugin.id, disposables, capabilities ? new Set(capabilities) : null, current);
		// Check BEFORE calling the real host: disposing a late registration afterward
		// could already have replaced a current panel, command or provider.
		const host = new Proxy(rawHost, {
			get(target, property: keyof PluginHost) {
				const method = target[property];
				if (typeof method !== "function") return method;
				return (...args: unknown[]) => {
					controller.signal.throwIfAborted();
					return Reflect.apply(method, target, args);
				};
			},
		});
		const pluginLogger = pluginStore.getLogger(plugin.id);
		if (previous) await previous;
		if (!current()) return false;

		let phase = "Rust registration";
		try {
			if (capabilities) {
				entry.backendReady = invoke("register_loaded_plugin", {
					pluginId: plugin.id,
					capabilities: [...capabilities],
				});
				await entry.backendReady;
				entry.backendReady = undefined;
				if (!current()) return false;
			}
			phase = "onload";
			entry.started = true;
			const startup = plugin.onload(host, controller.signal);
			if (startup) {
				if (!current()) {
					void startup.catch(() => {});
					return false;
				}
				// Stop waiting for obsolete plugin work; the guarded host rejects late
				// calls even when the plugin ignores the cooperative abort signal.
				let cancel!: () => void;
				const cancelled = new Promise<void>((resolve) => {
					cancel = resolve;
				});
				controller.signal.addEventListener("abort", cancel, { once: true });
				try {
					await Promise.race([startup, cancelled]);
				} finally {
					controller.signal.removeEventListener("abort", cancel);
					cancel();
				}
			}
			if (!current()) return false;
			pluginStore.updatePlugin(plugin.id, { loaded: true, error: null });
			return true;
		} catch (err) {
			if (!current()) return false;
			const msg = err instanceof Error ? err.message : String(err);
			appLogger.error("plugin", `Plugin "${plugin.id}" ${phase} failed: ${msg}`, err);
			pluginLogger.error(`${phase} failed: ${msg}`, err);
			pluginStore.updatePlugin(plugin.id, { loaded: false, error: msg });
			await unregister(plugin.id);
			return false;
		}
	}

	function unregister(id: string): Promise<void> {
		const entry = plugins.get(id);
		if (!entry) return teardowns.get(id) ?? Promise.resolve();
		plugins.delete(id);
		try {
			if (entry.started) entry.plugin.onunload();
		} catch (err) {
			const msg = err instanceof Error ? err.message : String(err);
			appLogger.error("plugin", `Plugin "${id}" onunload failed: ${msg}`, err);
			pluginStore.getLogger(id).error(`onunload failed: ${msg}`, err);
		}
		entry.controller.abort();
		entry.disposable.dispose();
		statusBarTicker.removeAllForPlugin(id);
		const previous = teardowns.get(id);
		const removeBackend = () => invoke<void>("unregister_loaded_plugin", { pluginId: id });
		const disposed = (
			previous || entry.backendReady
				? Promise.all([previous, entry.backendReady])
						.catch(() => {})
						.then(removeBackend)
				: removeBackend()
		).catch((e) => appLogger.debug("plugin", `unregister_loaded_plugin cleanup failed for "${id}": ${e}`));
		teardowns.set(id, disposed);
		void disposed.then(() => {
			if (teardowns.get(id) === disposed) teardowns.delete(id);
		});
		pluginStore.updatePlugin(id, { loaded: false });
		return disposed;
	}

	// -------------------------------------------------------------------------
	// Dispatch
	// -------------------------------------------------------------------------

	/** Run one watcher against a line and defer its callback. */
	function fireWatcher(
		entry: { pluginId: string; watcher: OutputWatcher },
		cleanLine: string,
		sessionId: string,
	): void {
		const { pluginId, watcher } = entry;
		if (isPluginPaused(pluginId)) return;
		if (!pluginMatchesSession(pluginId, sessionId)) return;
		const { pattern, onMatch } = watcher;
		// Reset global regex state before each test to avoid position carry-over
		if (pattern.global) pattern.lastIndex = 0;
		const match = pattern.exec(cleanLine);
		if (!match) return;
		queueMicrotask(() => {
			try {
				onMatch(match, sessionId);
			} catch (err) {
				const msg = err instanceof Error ? err.message : String(err);
				appLogger.error("plugin", `Plugin "${pluginId}" output watcher threw: ${msg}`, err);
				pluginStore.getLogger(pluginId).error(`OutputWatcher threw: ${msg}`, err);
			}
		});
	}

	/**
	 * Run a clean PTY line through the watchers Rust is NOT matching. Regex
	 * matching is synchronous (cheap); onMatch callbacks are deferred via
	 * queueMicrotask so a slow handler cannot stall the event loop.
	 *
	 * `alreadyFired` holds the ids Rust matched on this same line — a watcher
	 * whose sync reply has not landed yet is still `inRust: false` here, and
	 * without the skip it would fire twice for one line.
	 */
	function dispatchLine(cleanLine: string, sessionId: string, alreadyFired?: ReadonlySet<string>): void {
		for (const entry of outputWatchers) {
			if (entry.inRust) continue; // already matched on the reader thread
			if (alreadyFired?.has(entry.id)) continue;
			fireWatcher(entry, cleanLine, sessionId);
		}
	}

	/**
	 * Lines assembled by the Rust reader. Each carries the cleaned text Rust
	 * matched on, so re-running the real `RegExp` here reproduces the same
	 * decision and yields the `RegExpExecArray` the plugin API promises, plus
	 * the qualified ids Rust already flagged.
	 *
	 * Rust sends every line only while some pattern could not be compiled;
	 * otherwise it sends the matches alone.
	 */
	function handleWatcherLines(sessionId: string, lines: ReadonlyArray<{ text: string; matched_ids: string[] }>): void {
		const prefix = `${clientId}/`;
		for (const { text, matched_ids } of lines) {
			const fired = new Set<string>();
			for (const qualified of matched_ids ?? []) {
				// Another frontend's watchers travel on the same event.
				if (!qualified.startsWith(prefix)) continue;
				const id = qualified.slice(prefix.length);
				const entry = outputWatchers.find((w) => w.id === id);
				if (!entry) continue;
				fired.add(id);
				fireWatcher(entry, text, sessionId);
			}
			dispatchLine(text, sessionId, fired);
		}
	}

	/**
	 * Dispatch a structured Tauri event to all registered handlers for the type.
	 * Handlers are deferred via queueMicrotask to avoid blocking the event loop.
	 */
	function dispatchStructuredEvent(type: string, payload: unknown, sessionId: string): void {
		const handlers = structuredHandlers.get(type);
		if (!handlers) return;
		for (const { pluginId, handler } of handlers) {
			if (isPluginPaused(pluginId)) continue;
			if (!pluginMatchesSession(pluginId, sessionId)) continue;
			queueMicrotask(() => {
				try {
					handler(payload, sessionId);
				} catch (err) {
					const msg = err instanceof Error ? err.message : String(err);
					appLogger.error("plugin", `Plugin "${pluginId}" structured handler "${type}" threw: ${msg}`, err);
					pluginStore.getLogger(pluginId).error(`Structured handler "${type}" threw: ${msg}`, err);
				}
			});
		}
	}

	/** Notify plugins that a PTY session is gone. */
	function removeSession(sessionId: string): void {
		dispatchStructuredEvent("session-closed", {}, sessionId);
	}

	// Reentrancy guard for notifyStateChange: if a listener callback triggers
	// another state change synchronously (e.g. writes to a reactive store that
	// a caller effect tracks), naive dispatch would recurse and freeze the main
	// thread. We detect re-entry, defer nested events to a microtask, and log
	// the offending plugin so the root cause is visible.
	let dispatching = false;
	const pendingEvents: StateChangeEvent[] = [];

	function dispatchNow(event: StateChangeEvent): void {
		for (const { pluginId, callback } of stateChangeListeners) {
			if (isPluginPaused(pluginId)) continue;
			if (event.sessionId && !pluginMatchesSession(pluginId, event.sessionId)) continue;
			try {
				callback(event);
			} catch (err) {
				const msg = err instanceof Error ? err.message : String(err);
				appLogger.error("plugin", `State change listener threw: ${msg}`, err);
			}
		}
	}

	/** Notify all state change listeners of a terminal/branch state change */
	function notifyStateChange(event: StateChangeEvent): void {
		if (dispatching) {
			appLogger.warn(
				"plugin",
				`notifyStateChange re-entered while dispatching (type=${event.type}); deferring to microtask`,
			);
			pendingEvents.push(event);
			return;
		}
		dispatching = true;
		try {
			dispatchNow(event);
		} finally {
			dispatching = false;
		}
		if (pendingEvents.length > 0) {
			const drain = pendingEvents.splice(0, pendingEvents.length);
			queueMicrotask(() => {
				for (const e of drain) notifyStateChange(e);
			});
		}
	}

	/** Remove all plugins and registrations (for testing). */
	async function clear(): Promise<void> {
		const pending = [...plugins.keys()].map((id) => unregister(id));
		stateChangeListeners.length = 0;
		panelMessageHandlers.clear();
		panelSendChannels.clear();
		panelVisibility.clear();
		panelVisibilityHandlers.clear();
		panelCloseHandlers.clear();
		await Promise.all([...pending, ...teardowns.values()]);
	}

	// -------------------------------------------------------------------------
	// Panel message bridge (used by PluginPanel component)
	// -------------------------------------------------------------------------

	/** Route a message from an iframe to the registered onMessage handler */
	function handlePanelMessage(tabId: string, data: unknown): void {
		const handler = panelMessageHandlers.get(tabId);
		if (handler) {
			try {
				handler(data);
			} catch (err) {
				const msg = err instanceof Error ? err.message : String(err);
				appLogger.error("plugin", `Panel message handler for tab "${tabId}" threw: ${msg}`, err);
			}
		}
	}

	/** Register a send channel for a panel (called by PluginPanel component on mount) */
	function registerPanelSendChannel(tabId: string, sender: (data: unknown, transfer?: Transferable[]) => void): void {
		panelSendChannels.set(tabId, sender);
	}

	/** Unregister a send channel (called by PluginPanel component on cleanup) */
	function unregisterPanelSendChannel(tabId: string): void {
		panelSendChannels.delete(tabId);
	}

	/**
	 * Record whether a panel is the tab on screen (called by PluginPanel).
	 *
	 * Only a real change reaches the plugin: the component re-reports on every
	 * effect run, and a plugin that rebuilds its board on this callback must not
	 * be woken by a repeat of what it already knows. A tab already closed is
	 * ignored — its entry is gone and must not come back.
	 */
	function setPanelVisible(tabId: string, visible: boolean): void {
		if (!panelVisibility.has(tabId) || panelVisibility.get(tabId) === visible) return;
		panelVisibility.set(tabId, visible);
		panelVisibilityHandlers.get(tabId)?.(visible);
	}

	/**
	 * Invoke a plugin command by its namespaced action name.
	 * Returns true if a handler was found and called.
	 * Used by the global keybinding dispatcher in App.tsx.
	 */
	function invokePluginCommand(actionName: string): boolean {
		const handler = pluginCommandHandlers.get(actionName);
		if (!handler) return false;
		try {
			const result = handler();
			if (result instanceof Promise) {
				result.catch((err) => {
					appLogger.error("plugin", `Plugin command "${actionName}" failed`, err);
				});
			}
		} catch (err) {
			appLogger.error("plugin", `Plugin command "${actionName}" threw`, err);
		}
		return true;
	}

	return {
		register,
		unregister,
		dispatchLine,
		handleWatcherLines,
		dispatchStructuredEvent,
		notifyStateChange,
		removeSession,
		clear,
		setPluginPaused,
		handlePanelMessage,
		registerPanelSendChannel,
		unregisterPanelSendChannel,
		setPanelVisible,
		invokePluginCommand,
	};
}

export const pluginRegistry = createPluginRegistry();
