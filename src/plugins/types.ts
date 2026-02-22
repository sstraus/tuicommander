/**
 * Shared types for the TUICommander plugin system.
 *
 * Plugins implement TuiPlugin and register capabilities via PluginHost.
 * Built-in plugins are compiled with the app; external plugins are loaded
 * at runtime from {config_dir}/plugins/ via the plugin:// URI protocol.
 */

// ---------------------------------------------------------------------------
// Disposable
// ---------------------------------------------------------------------------

/** Returned by every PluginHost.register*() call. Call dispose() in onunload(). */
export interface Disposable {
  dispose(): void;
}

// ---------------------------------------------------------------------------
// Activity items
// ---------------------------------------------------------------------------

/**
 * A section heading in the Activity Center bell dropdown.
 * Each plugin registers one section and contributes items into it.
 */
export interface ActivitySection {
  /** Unique section identifier (e.g. "plan", "stories") */
  id: string;
  /** Display label shown as section header (e.g. "ACTIVE PLAN", "STORIES") */
  label: string;
  /** Lower number = higher position in dropdown */
  priority: number;
  /** Whether the section shows a "Dismiss All" button */
  canDismissAll: boolean;
}

/**
 * A single entry in the Activity Center.
 * Contributed by plugins via PluginHost.addItem().
 */
export interface ActivityItem {
  /** Unique item identifier */
  id: string;
  /** ID of the plugin that owns this item */
  pluginId: string;
  /** Section this item belongs to */
  sectionId: string;
  /** Primary display text (larger, prominent) */
  title: string;
  /** Secondary display text (smaller, muted) */
  subtitle?: string;
  /**
   * Inline SVG string rendered via innerHTML.
   * SECURITY: MUST be a compile-time constant from trusted first-party code.
   * NEVER pass user-generated, PTY-derived, or externally-sourced strings here.
   */
  icon: string;
  /** CSS color for the icon (e.g. "var(--fg-muted)", "#3fb950") */
  iconColor?: string;
  /** Whether the user can dismiss this item individually */
  dismissible: boolean;
  /** Whether this item has been dismissed by the user */
  dismissed?: boolean;
  /** Unix timestamp when this item was created (set by store) */
  createdAt: number;
  /**
   * URI resolved by markdownProviderRegistry when user clicks the item.
   * Format: "scheme:path?key=value" — e.g. "plan:file?path=/foo/bar.md"
   * Mutually exclusive with onClick.
   */
  contentUri?: string;
  /**
   * Direct click handler (alternative to contentUri for simple actions).
   * Mutually exclusive with contentUri.
   */
  onClick?: () => void;
}

// ---------------------------------------------------------------------------
// Plugin output watchers
// ---------------------------------------------------------------------------

/**
 * Watches PTY output lines for a regex pattern.
 * Registered via PluginHost.registerOutputWatcher().
 *
 * IMPORTANT: onMatch must be synchronous and fast (< 1ms).
 * Schedule any async work (file reads, API calls) separately.
 */
export interface OutputWatcher {
  /** Pattern to test against each clean (ANSI-stripped) PTY line */
  pattern: RegExp;
  /**
   * Called synchronously when pattern matches.
   * @param match - The RegExpExecArray from pattern.exec(line)
   * @param sessionId - The PTY session that produced the line
   */
  onMatch: (match: RegExpExecArray, sessionId: string) => void;
}

// ---------------------------------------------------------------------------
// Markdown content providers
// ---------------------------------------------------------------------------

/**
 * Generates virtual markdown content for a URI scheme.
 * Registered via PluginHost.registerMarkdownProvider().
 *
 * The URI scheme corresponds to the scheme portion of ActivityItem.contentUri.
 * Example: for "plan:file?path=/foo/bar.md", scheme = "plan".
 */
export interface MarkdownProvider {
  /**
   * Generate and return markdown content for the given URI.
   * Return null to indicate the content is unavailable.
   */
  provideContent(uri: URL): string | null | Promise<string | null>;
}

// ---------------------------------------------------------------------------
// Read-only snapshot types (Tier 2 — always available)
// ---------------------------------------------------------------------------

/** Read-only snapshot of the active repository */
export interface RepoSnapshot {
  path: string;
  displayName: string;
  activeBranch: string | null;
  worktreePath: string | null;
}

/** Read-only snapshot of a registered repository */
export interface RepoListEntry {
  path: string;
  displayName: string;
}

/** Read-only snapshot of a PR notification */
export interface PrNotificationSnapshot {
  id: string;
  repoPath: string;
  branch: string;
  prNumber: number;
  title: string;
  type: string;
}

/** Read-only snapshot of effective repo settings */
export interface RepoSettingsSnapshot {
  path: string;
  displayName: string;
  baseBranch: string;
  color: string;
}

// ---------------------------------------------------------------------------
// Capabilities
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Filesystem change events
// ---------------------------------------------------------------------------

/** A filesystem change event emitted by the watcher. */
export interface FsChangeEvent {
  type: "create" | "modify" | "delete";
  path: string;
}

// ---------------------------------------------------------------------------
// Capabilities
// ---------------------------------------------------------------------------

/** Known capability strings for external plugins */
export type PluginCapability =
  | "pty:write"
  | "ui:markdown"
  | "ui:sound"
  | "invoke:read_file"
  | "invoke:list_markdown_files"
  | "fs:read"
  | "fs:list"
  | "fs:watch";

/** Error thrown when a plugin calls a method without the required capability */
export class PluginCapabilityError extends Error {
  constructor(pluginId: string, capability: PluginCapability) {
    super(`Plugin "${pluginId}" requires capability "${capability}" but did not declare it`);
    this.name = "PluginCapabilityError";
  }
}

// ---------------------------------------------------------------------------
// Plugin host (the API surface exposed to plugins)
// ---------------------------------------------------------------------------

/** Commands allowed via host.invoke() (Tier 4) */
export const INVOKE_WHITELIST: readonly string[] = [
  "read_file",
  "list_markdown_files",
  "read_plugin_data",
  "write_plugin_data",
  "delete_plugin_data",
];

/**
 * The API that the plugin registry exposes to each loaded plugin.
 * Plugins call these methods in onload() and store the returned Disposables
 * for cleanup in onunload().
 *
 * API Tiers:
 * - Tier 1: Activity Center + watchers + providers (always available)
 * - Tier 2: Read-only app state snapshots (always available)
 * - Tier 3: Write actions (capability-gated via manifest.json)
 * - Tier 4: Scoped Tauri invoke (whitelisted commands only)
 */
export interface PluginHost {
  // -- Tier 0: Logging (always available) --

  /** Write a message to this plugin's log (visible in Settings > Plugins). */
  log(level: "debug" | "info" | "warn" | "error", message: string, data?: unknown): void;

  // -- Tier 1: Activity Center + watchers + providers (always available) --

  /** Register a section in the Activity Center dropdown */
  registerSection(section: ActivitySection): Disposable;

  /** Register a PTY output line watcher */
  registerOutputWatcher(watcher: OutputWatcher): Disposable;

  /**
   * Register a handler for structured events from the Rust OutputParser.
   * The type corresponds to ParsedEvent.type (e.g. "plan-file", "rate-limit").
   */
  registerStructuredEventHandler(
    type: string,
    handler: (payload: unknown, sessionId: string) => void,
  ): Disposable;

  /** Register a markdown content provider for a URI scheme */
  registerMarkdownProvider(scheme: string, provider: MarkdownProvider): Disposable;

  /** Add an activity item to the store */
  addItem(item: Omit<ActivityItem, "createdAt">): void;

  /** Remove an activity item by ID */
  removeItem(id: string): void;

  /** Update fields on an existing activity item */
  updateItem(id: string, updates: Partial<Omit<ActivityItem, "id" | "pluginId" | "createdAt">>): void;

  // -- Tier 2: Read-only app state (always available) --

  /** Get the active repository snapshot, or null if none active */
  getActiveRepo(): RepoSnapshot | null;

  /** Get all registered repositories */
  getRepos(): RepoListEntry[];

  /** Get the active terminal's session ID, or null if none active */
  getActiveTerminalSessionId(): string | null;

  /** Get the repository path that owns a terminal session, or null if not found */
  getRepoPathForSession(sessionId: string): string | null;

  /** Get active (non-dismissed) PR notifications */
  getPrNotifications(): PrNotificationSnapshot[];

  /** Get effective settings for a repository */
  getSettings(repoPath: string): RepoSettingsSnapshot | null;

  // -- Tier 3: Write actions (capability-gated) --

  /** Send input to a terminal session. Requires "pty:write" capability. */
  writePty(sessionId: string, data: string): Promise<void>;

  /** Open a virtual markdown tab and show the panel. Requires "ui:markdown" capability. */
  openMarkdownPanel(title: string, contentUri: string): void;

  /** Play the notification sound. Requires "ui:sound" capability. */
  playNotificationSound(): Promise<void>;

  // -- Tier 3b: Filesystem operations (capability-gated) --

  /** Read a file as UTF-8 text. Path must be absolute and within $HOME. Requires "fs:read". */
  readFile(absolutePath: string): Promise<string>;

  /** List filenames in a directory, optionally filtered by glob. Requires "fs:list". */
  listDirectory(path: string, pattern?: string): Promise<string[]>;

  /**
   * Watch a path for filesystem changes. Requires "fs:watch".
   * @param path - Absolute path within $HOME
   * @param callback - Called with batched change events
   * @param options - recursive (default false), debounceMs (default 300)
   * @returns Disposable to stop watching
   */
  watchPath(
    path: string,
    callback: (events: FsChangeEvent[]) => void,
    options?: { recursive?: boolean; debounceMs?: number },
  ): Promise<Disposable>;

  // -- Tier 4: Scoped Tauri invoke (whitelisted commands only) --

  /** Invoke a whitelisted Tauri command. See INVOKE_WHITELIST for allowed commands. */
  invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T>;
}

// ---------------------------------------------------------------------------
// Plugin lifecycle
// ---------------------------------------------------------------------------

/**
 * Interface that every built-in plugin implements.
 *
 * Pattern (Obsidian-style):
 *   onload  — register capabilities, store returned Disposables
 *   onunload — dispose all registrations
 */
export interface TuiPlugin {
  /** Unique plugin identifier (e.g. "plan", "wiz-stories") */
  id: string;
  /**
   * Called once when the plugin is registered.
   * Register sections, watchers, providers here.
   */
  onload(host: PluginHost): void;
  /**
   * Called when the plugin is unregistered.
   * Must dispose all Disposables obtained in onload().
   */
  onunload(): void;
}
