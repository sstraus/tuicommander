/**
 * Claude Status Plugin — Agent-scoped example (Tier 1 only)
 *
 * Demonstrates the agentTypes manifest field: this plugin only receives
 * events from terminals running Claude Code. Output watchers are silently
 * skipped for plain shells, Gemini, Codex, or any other agent.
 *
 * Tracks:
 * - Usage limit warnings ("You've used X% of your weekly limit")
 * - Rate limit events (via structured event handler)
 */

const PLUGIN_ID = "claude-status";
const SECTION_ID = "claude-status";

const ICON = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="currentColor"><path d="M8 1.5a6.5 6.5 0 1 0 0 13 6.5 6.5 0 0 0 0-13zM0 8a8 8 0 1 1 16 0A8 8 0 0 1 0 8zm9-3a1 1 0 1 1-2 0 1 1 0 0 1 2 0zM8 7a.75.75 0 0 1 .75.75v3.5a.75.75 0 0 1-1.5 0v-3.5A.75.75 0 0 1 8 7z"/></svg>`;

let itemCount = 0;

export default {
  id: PLUGIN_ID,

  onload(host) {
    host.registerSection({
      id: SECTION_ID,
      label: "CLAUDE STATUS",
      priority: 25,
      canDismissAll: true,
    });

    // Watch for usage limit warnings in Claude's output.
    // Because agentTypes: ["claude"] is set in manifest.json, this watcher
    // will ONLY fire for terminals where Claude Code is the foreground process.
    host.registerOutputWatcher({
      pattern: /You[''\u2019]ve used (\d+)% of your (weekly|session) limit/,
      onMatch(match) {
        const percentage = parseInt(match[1], 10);
        const limitType = match[2];
        const level = percentage >= 80 ? "high" : percentage >= 50 ? "medium" : "low";

        host.addItem({
          id: `${PLUGIN_ID}:usage`,
          pluginId: PLUGIN_ID,
          sectionId: SECTION_ID,
          title: `${percentage}% ${limitType} limit used`,
          subtitle: level === "high" ? "Consider pausing to avoid hitting the cap" : undefined,
          icon: ICON,
          iconColor: level === "high" ? "var(--color-danger)" : "var(--fg-muted)",
          dismissible: true,
        });
      },
    });

    // Listen for rate-limit structured events from the Rust OutputParser.
    // Also agent-scoped — only fires for Claude terminals.
    host.registerStructuredEventHandler("rate-limit", (payload) => {
      itemCount++;
      host.addItem({
        id: `${PLUGIN_ID}:rate-${itemCount}`,
        pluginId: PLUGIN_ID,
        sectionId: SECTION_ID,
        title: "Rate limited",
        subtitle: `Pattern: ${payload.pattern_name}`,
        icon: ICON,
        iconColor: "var(--color-warning)",
        dismissible: true,
      });
    });
  },

  onunload() {
    itemCount = 0;
  },
};
