import { invoke } from "../invoke";
import type { MarkdownProvider, PluginHost, TuiPlugin } from "./types";
import { repositoriesStore } from "../stores/repositories";
import { terminalsStore } from "../stores/terminals";

// ---------------------------------------------------------------------------
// Patterns (validated against actual stories-cli.js output, post-ANSI-strip)
// ---------------------------------------------------------------------------

// "✓ Updated: 324-9b46 ready → in_progress"
const STATUS_PATTERN = /✓ Updated: (\d+-[0-9a-f]{4}) \S+ → (\S+)/;

// "✓ Added worklog to 324-9b46: message"
const WORKLOG_PATTERN = /✓ Added worklog to (\d+-[0-9a-f]{4}):/;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const SECTION_ID = "stories";
const PLUGIN_ID = "wiz-stories";

// Bolt / lightning icon
const BOLT_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="currentColor">
  <path d="M9.504 1.132a1 1 0 0 1 .395 1.377L7.89 6H12a1 1 0 0 1 .765 1.636l-7 8a1 1 0 0 1-1.765-.877L6.11 10H2a1 1 0 0 1-.765-1.636l7-8a1 1 0 0 1 1.27-.232z"/>
</svg>`;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function stableItemId(storyId: string): string {
  return `stories:${storyId}`;
}

function buildContentUri(storyId: string, storiesDir: string | null): string {
  const params = new URLSearchParams({ id: storyId });
  if (storiesDir) params.set("dir", storiesDir);
  return `stories:detail?${params.toString()}`;
}

/**
 * Resolve the stories directory from the terminal session.
 * Walks repos → branches → terminals to find the matching repo path.
 */
function defaultGetStoriesDir(sessionId: string): string | null {
  // Find the terminal with this session
  const termId = terminalsStore.getIds().find(
    (id: string) => terminalsStore.get(id)?.sessionId === sessionId,
  );
  if (!termId) return null;

  // Find the repo that owns this terminal
  for (const repoPath of repositoriesStore.getPaths()) {
    const repo = repositoriesStore.get(repoPath);
    if (!repo) continue;
    for (const branch of Object.values(repo.branches)) {
      if (branch.terminals.includes(termId)) {
        return `${repoPath}/stories`;
      }
    }
  }
  return null;
}

// ---------------------------------------------------------------------------
// MarkdownProvider
// ---------------------------------------------------------------------------

function createStoriesMarkdownProvider(): MarkdownProvider {
  return {
    async provideContent(uri: URL): Promise<string | null> {
      const storyId = uri.searchParams.get("id");
      const dir = uri.searchParams.get("dir");
      if (!storyId || !dir) return null;

      try {
        const files = await invoke<{ path: string; git_status: string }[]>(
          "list_markdown_files",
          { path: dir },
        );
        const entry = files.find((f) => f.path.startsWith(`${storyId}-`));
        if (!entry) return null;

        return await invoke<string>("read_file", { path: dir, file: entry.path });
      } catch (err) {
        console.warn("[wizStoriesPlugin] Failed to load story content:", storyId, err);
        return null;
      }
    },
  };
}

// ---------------------------------------------------------------------------
// Plugin factory
// ---------------------------------------------------------------------------

/**
 * Create the wiz-stories plugin with an injectable storiesDir resolver.
 * The default resolver walks the repo/branch/terminal graph to find the
 * stories directory for the terminal that produced the PTY output.
 */
export function createWizStoriesPlugin(
  getStoriesDir: (sessionId: string) => string | null = defaultGetStoriesDir,
): TuiPlugin {
  return new WizStoriesPlugin(getStoriesDir);
}

class WizStoriesPlugin implements TuiPlugin {
  readonly id = PLUGIN_ID;

  private readonly getStoriesDir: (sessionId: string) => string | null;

  constructor(getStoriesDir: (sessionId: string) => string | null) {
    this.getStoriesDir = getStoriesDir;
  }

  onload(host: PluginHost): void {
    host.registerSection({
      id: SECTION_ID,
      label: "STORIES",
      priority: 20,
      canDismissAll: false,
    });

    host.registerMarkdownProvider(SECTION_ID, createStoriesMarkdownProvider());

    host.registerOutputWatcher({
      pattern: STATUS_PATTERN,
      onMatch: (match, sessionId) => {
        const storyId = match[1];
        const status = match[2];
        const storiesDir = this.getStoriesDir(sessionId);
        host.addItem({
          id: stableItemId(storyId),
          pluginId: PLUGIN_ID,
          sectionId: SECTION_ID,
          title: storyId,
          subtitle: `${storyId} · ${status}`,
          icon: BOLT_SVG,
          dismissible: true,
          contentUri: buildContentUri(storyId, storiesDir),
        });
      },
    });

    host.registerOutputWatcher({
      pattern: WORKLOG_PATTERN,
      onMatch: (match, sessionId) => {
        const storyId = match[1];
        const storiesDir = this.getStoriesDir(sessionId);
        // Add or refresh the item (addItem deduplicates by id)
        host.addItem({
          id: stableItemId(storyId),
          pluginId: PLUGIN_ID,
          sectionId: SECTION_ID,
          title: storyId,
          subtitle: `${storyId} · worklog`,
          icon: BOLT_SVG,
          dismissible: true,
          contentUri: buildContentUri(storyId, storiesDir),
        });
      },
    });
  }

  onunload(): void {
    // All registrations are auto-disposed by the plugin registry
  }
}

/** Default singleton using the standard store-based storiesDir resolver */
export const wizStoriesPlugin: TuiPlugin = createWizStoriesPlugin();
