/**
 * Wiz Stories Plugin — Tracks story status changes and worklogs
 *
 * Watches PTY output for stories-cli patterns:
 *   "✓ Updated: 324-9b46 ready → in_progress"
 *   "✓ Added worklog to 324-9b46: message"
 *
 * Creates Activity Center items with links to story markdown content.
 *
 * Capabilities required:
 *   - invoke:list_markdown_files (find story .md files)
 *   - invoke:read_file (read story content)
 *   - ui:markdown (open markdown panel)
 */

const PLUGIN_ID = "wiz-stories";
const SECTION_ID = "stories";

// "✓ Updated: 324-9b46 ready → in_progress"
const STATUS_PATTERN = /✓ Updated: (\d+-[0-9a-f]{4}) \S+ → (\S+)/;

// "✓ Added worklog to 324-9b46: message"
const WORKLOG_PATTERN = /✓ Added worklog to (\d+-[0-9a-f]{4}):/;

const BOLT_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="currentColor">
  <path d="M9.504 1.132a1 1 0 0 1 .395 1.377L7.89 6H12a1 1 0 0 1 .765 1.636l-7 8a1 1 0 0 1-1.765-.877L6.11 10H2a1 1 0 0 1-.765-1.636l7-8a1 1 0 0 1 1.27-.232z"/>
</svg>`;

function stableItemId(storyId) {
  return `stories:${storyId}`;
}

function buildContentUri(storyId, storiesDir) {
  const params = new URLSearchParams({ id: storyId });
  if (storiesDir) params.set("dir", storiesDir);
  return `stories:detail?${params.toString()}`;
}

function getStoriesDir(host, sessionId) {
  const repoPath = host.getRepoPathForSession(sessionId);
  return repoPath ? `${repoPath}/stories` : null;
}

export default {
  id: PLUGIN_ID,

  onload(host) {
    host.registerSection({
      id: SECTION_ID,
      label: "STORIES",
      priority: 20,
      canDismissAll: false,
    });

    host.registerMarkdownProvider(SECTION_ID, {
      async provideContent(uri) {
        const storyId = uri.searchParams.get("id");
        const dir = uri.searchParams.get("dir");
        if (!storyId || !dir) return null;

        try {
          const files = await host.invoke("list_markdown_files", { path: dir });
          const entry = files.find((f) => f.path.startsWith(`${storyId}-`));
          if (!entry) return null;
          return await host.invoke("read_file", { path: dir, file: entry.path });
        } catch (err) {
          host.log("warn", `Failed to load story content: ${storyId}`, err);
          return null;
        }
      },
    });

    host.registerOutputWatcher({
      pattern: STATUS_PATTERN,
      onMatch(match, sessionId) {
        const storyId = match[1];
        const status = match[2];
        const storiesDir = getStoriesDir(host, sessionId);
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
      onMatch(match, sessionId) {
        const storyId = match[1];
        const storiesDir = getStoriesDir(host, sessionId);
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
  },

  onunload() {
    // All registrations are auto-disposed by the plugin registry
  },
};
