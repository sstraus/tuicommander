/**
 * Shared types for the TUI Commander plugin system.
 *
 * Plugins are trusted first-party TypeScript modules compiled with the app.
 * Each plugin implements TuiPlugin and registers capabilities via PluginHost.
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
// Plugin host (the API surface exposed to plugins)
// ---------------------------------------------------------------------------

/**
 * The API that the plugin registry exposes to each loaded plugin.
 * Plugins call these methods in onload() and store the returned Disposables
 * for cleanup in onunload().
 */
export interface PluginHost {
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
