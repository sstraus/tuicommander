/**
 * CI Notifier Plugin — Sound + markdown panel (Tier 1 + Tier 3)
 *
 * Watches for CI failure patterns, plays notification sound, shows details
 * in a markdown panel.
 * Demonstrates: registerOutputWatcher, playNotificationSound(), openMarkdownPanel(),
 *               registerMarkdownProvider.
 * Capabilities: ui:sound, ui:markdown
 */

const PLUGIN_ID = "ci-notifier";
const SECTION_ID = "ci";

const ICON_FAIL = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="currentColor"><path d="M8 1a7 7 0 1 0 0 14A7 7 0 0 0 8 1zm3.22 9.16a.75.75 0 1 1-1.06 1.06L8 9.06l-2.16 2.16a.75.75 0 0 1-1.06-1.06L6.94 8 4.78 5.84a.75.75 0 0 1 1.06-1.06L8 6.94l2.16-2.16a.75.75 0 0 1 1.06 1.06L9.06 8l2.16 2.16z"/></svg>`;

const failures = [];

export default {
  id: PLUGIN_ID,

  onload(host) {
    host.registerSection({
      id: SECTION_ID,
      label: "CI STATUS",
      priority: 15,
      canDismissAll: true,
    });

    // Provide markdown content for failure details
    host.registerMarkdownProvider("ci", {
      provideContent(uri) {
        const idx = parseInt(uri.searchParams.get("idx") || "-1", 10);
        const failure = failures[idx];
        if (!failure) return null;
        return [
          `# CI Failure: ${failure.pipeline}`,
          "",
          `**Step:** ${failure.step}`,
          `**Time:** ${failure.time}`,
          "",
          "## Output",
          "```",
          failure.output,
          "```",
        ].join("\n");
      },
    });

    // Watch for CI failure patterns like "FAILED: step-name"
    host.registerOutputWatcher({
      pattern: /(?:FAILED|FAILURE|ERROR):\s*(.+)/i,
      onMatch(match, sessionId) {
        const step = match[1].trim();
        const idx = failures.length;
        failures.push({
          pipeline: "CI",
          step,
          time: new Date().toLocaleTimeString(),
          output: match[0],
        });

        host.addItem({
          id: `ci:fail:${idx}`,
          pluginId: PLUGIN_ID,
          sectionId: SECTION_ID,
          title: step,
          subtitle: "CI failure detected",
          icon: ICON_FAIL,
          iconColor: "#f85149",
          dismissible: true,
          contentUri: `ci:detail?idx=${idx}`,
        });

        // Play notification sound (async, fire and forget)
        host.playNotificationSound().catch(() => {});
      },
    });
  },

  onunload() {
    failures.length = 0;
  },
};
