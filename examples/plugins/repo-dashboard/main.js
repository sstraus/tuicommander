/**
 * Repo Dashboard Plugin — Read-only state (Tier 2)
 *
 * Uses read-only host methods to build a multi-repo status overview
 * displayed as a markdown panel.
 * Demonstrates: getActiveRepo(), getRepos(), getPrNotifications(),
 *               registerMarkdownProvider, addItem.
 * No capabilities needed (Tier 2 is always available).
 */

const PLUGIN_ID = "repo-dashboard";
const SECTION_ID = "dashboard";

const ICON = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="currentColor"><path d="M2 2.5A2.5 2.5 0 0 1 4.5 0h8.75a.75.75 0 0 1 .75.75v12.5a.75.75 0 0 1-.75.75h-2.5a.75.75 0 0 1 0-1.5h1.75v-2h-8a1 1 0 0 0-.714 1.7.75.75 0 0 1-1.072 1.05A2.495 2.495 0 0 1 2 11.5v-9zm10.5-1h-8a1 1 0 0 0-1 1v6.708A2.486 2.486 0 0 1 4.5 9h8V1.5zM5 12.25v3.25a.25.25 0 0 0 .4.2l1.45-1.087a.25.25 0 0 1 .3 0L8.6 15.7a.25.25 0 0 0 .4-.2v-3.25a.25.25 0 0 0-.25-.25h-3.5a.25.25 0 0 0-.25.25z"/></svg>`;

/** Reference to host for use in markdown provider */
let hostRef = null;

export default {
  id: PLUGIN_ID,

  onload(host) {
    hostRef = host;

    host.registerSection({
      id: SECTION_ID,
      label: "DASHBOARD",
      priority: 45,
      canDismissAll: false,
    });

    // Register markdown provider that generates the dashboard dynamically
    host.registerMarkdownProvider("dashboard", {
      provideContent() {
        if (!hostRef) return null;

        const repos = hostRef.getRepos();
        const active = hostRef.getActiveRepo();
        const prs = hostRef.getPrNotifications();

        const lines = ["# Repository Dashboard", ""];

        // Active repo
        if (active) {
          lines.push(`## Active: ${active.displayName}`);
          lines.push(`- **Branch:** ${active.activeBranch || "detached"}`);
          if (active.worktreePath) {
            lines.push(`- **Worktree:** ${active.worktreePath}`);
          }
          lines.push("");
        } else {
          lines.push("*No active repository*", "");
        }

        // All repos
        lines.push(`## Repositories (${repos.length})`);
        if (repos.length === 0) {
          lines.push("*No repositories registered*");
        } else {
          for (const repo of repos) {
            const isActive = active && repo.path === active.path ? " **(active)**" : "";
            lines.push(`- ${repo.displayName}${isActive}`);
          }
        }
        lines.push("");

        // PR notifications
        lines.push(`## PR Notifications (${prs.length})`);
        if (prs.length === 0) {
          lines.push("*No active PR notifications*");
        } else {
          lines.push("| PR | Branch | Type |");
          lines.push("|---|---|---|");
          for (const pr of prs) {
            lines.push(`| #${pr.prNumber} ${pr.title} | ${pr.branch} | ${pr.type} |`);
          }
        }

        return lines.join("\n");
      },
    });

    // Add a persistent item that opens the dashboard
    host.addItem({
      id: "dashboard:overview",
      pluginId: PLUGIN_ID,
      sectionId: SECTION_ID,
      title: "Repo Dashboard",
      subtitle: "View repository overview",
      icon: ICON,
      dismissible: false,
      contentUri: "dashboard:overview",
    });
  },

  onunload() {
    hostRef = null;
  },
};
