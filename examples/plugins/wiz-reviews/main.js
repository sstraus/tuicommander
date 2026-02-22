/**
 * Wiz Reviews Plugin — Tracks code review output
 *
 * Watches PTY output for review file patterns:
 *   "Review saved to reviews/review-*.md"
 *   "📋 Review report: reviews/review-*.md"
 *
 * Creates Activity Center items that link to the review markdown content.
 *
 * Capabilities required:
 *   - invoke:read_file (read review content)
 *   - ui:markdown (open markdown panel)
 */

const PLUGIN_ID = "wiz-reviews";
const SECTION_ID = "reviews";

// Matches: "Review saved to reviews/review-something.md"
// or: "📋 Review report: reviews/review-something.md"
const REVIEW_PATTERN = /(?:Review saved to|📋 Review report:)\s+(reviews\/review-[^\s]+\.md)/;

const CLIPBOARD_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="currentColor">
  <path d="M5.75 1a.75.75 0 0 0-.75.75v.5c0 .414.336.75.75.75h4.5a.75.75 0 0 0 .75-.75v-.5a.75.75 0 0 0-.75-.75h-4.5z"/>
  <path fill-rule="evenodd" d="M3.5 3.25a.75.75 0 0 1 .75-.75h.5a.75.75 0 0 1 0 1.5H4.5v9.5h7V4h-.25a.75.75 0 0 1 0-1.5h.5a.75.75 0 0 1 .75.75v10.5a.75.75 0 0 1-.75.75h-8a.75.75 0 0 1-.75-.75V3.25z" clip-rule="evenodd"/>
</svg>`;

let count = 0;

function stableItemId(filePath) {
  return `review:${filePath}`;
}

function buildContentUri(filePath, repoPath) {
  const params = new URLSearchParams({ file: filePath });
  if (repoPath) params.set("repo", repoPath);
  return `reviews:detail?${params.toString()}`;
}

export default {
  id: PLUGIN_ID,

  onload(host) {
    host.registerSection({
      id: SECTION_ID,
      label: "REVIEWS",
      priority: 25,
      canDismissAll: true,
    });

    host.registerMarkdownProvider(SECTION_ID, {
      async provideContent(uri) {
        const file = uri.searchParams.get("file");
        const repo = uri.searchParams.get("repo");
        if (!file || !repo) return null;

        try {
          return await host.invoke("read_file", { path: repo, file });
        } catch (err) {
          host.log("warn", `Failed to load review content: ${file}`, err);
          return null;
        }
      },
    });

    host.registerOutputWatcher({
      pattern: REVIEW_PATTERN,
      onMatch(match, sessionId) {
        const filePath = match[1];
        const repoPath = host.getRepoPathForSession(sessionId);
        const fileName = filePath.split("/").pop() || filePath;
        count++;

        host.addItem({
          id: stableItemId(filePath),
          pluginId: PLUGIN_ID,
          sectionId: SECTION_ID,
          title: fileName.replace(/\.md$/, ""),
          subtitle: `Code review · ${repoPath ? repoPath.split("/").pop() : "unknown"}`,
          icon: CLIPBOARD_SVG,
          dismissible: true,
          contentUri: buildContentUri(filePath, repoPath),
        });
      },
    });
  },

  onunload() {
    count = 0;
  },
};
