/**
 * Report Watcher — Activity Center integration for generated reports
 *
 * Watches PTY output for lines that reference markdown report files, e.g.:
 *   "Report saved to reports/coverage-2026-02-25.md"
 *   "Report: docs/review-auth-module.md"
 *
 * When a match is found, an Activity Center item is created with a link
 * to view the report content in the markdown panel.
 *
 * Adapt the REPORT_PATTERN regex and SECTION label to match whatever
 * output your tools produce.
 *
 * Capabilities required:
 *   - invoke:read_file  (read report content from disk)
 *   - ui:markdown        (open the markdown viewer panel)
 */

const PLUGIN_ID = "report-watcher";
const SECTION_ID = "reports";

// ---- Customize these to match your tool output ----

// Matches lines like:
//   "Report saved to path/to/file.md"
//   "Report: path/to/file.md"
//   "Generated: path/to/file.md"
const REPORT_PATTERN = /(?:Report saved to|Report:|Generated:)\s+(\S+\.md)/;

const DOCUMENT_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="currentColor">
  <path d="M4 1.75C4 .784 4.784 0 5.75 0h4.586c.464 0 .909.184 1.237.513l2.914 2.914c.329.328.513.773.513 1.237v9.586A1.75 1.75 0 0 1 13.25 16h-7.5A1.75 1.75 0 0 1 4 14.25V1.75zm1.75-.25a.25.25 0 0 0-.25.25v12.5c0 .138.112.25.25.25h7.5a.25.25 0 0 0 .25-.25V5h-2.75A1.75 1.75 0 0 1 9 3.25V.5H5.75zm5.75.56v2.19c0 .138.112.25.25.25h2.19L11.5.81z"/>
  <path d="M6.5 8a.75.75 0 0 1 .75-.75h3.5a.75.75 0 0 1 0 1.5h-3.5A.75.75 0 0 1 6.5 8zm.75 2.25a.75.75 0 0 0 0 1.5h3.5a.75.75 0 0 0 0-1.5h-3.5z"/>
</svg>`;

// ---- End customization ----

function stableItemId(filePath) {
  return `report:${filePath}`;
}

function buildContentUri(filePath, repoPath) {
  const params = new URLSearchParams({ file: filePath });
  if (repoPath) params.set("repo", repoPath);
  return `reports:detail?${params.toString()}`;
}

export default {
  id: PLUGIN_ID,

  onload(host) {
    host.registerSection({
      id: SECTION_ID,
      label: "REPORTS",
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
          host.log("warn", `Failed to load report: ${file}`, err);
          return null;
        }
      },
    });

    host.registerOutputWatcher({
      pattern: REPORT_PATTERN,
      onMatch(match, sessionId) {
        const filePath = match[1];
        const repoPath = host.getRepoPathForSession(sessionId);
        const fileName = filePath.split("/").pop() || filePath;

        host.addItem({
          id: stableItemId(filePath),
          pluginId: PLUGIN_ID,
          sectionId: SECTION_ID,
          title: fileName.replace(/\.md$/, ""),
          subtitle: `Report · ${repoPath ? repoPath.split("/").pop() : "unknown"}`,
          icon: DOCUMENT_SVG,
          dismissible: true,
          contentUri: buildContentUri(filePath, repoPath),
        });
      },
    });
  },

  onunload() {
    // All registrations are auto-disposed by the plugin registry
  },
};
