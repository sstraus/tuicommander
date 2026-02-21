/**
 * Auto Confirm Plugin — PTY write (Tier 3)
 *
 * Watches for [y/N] prompts and auto-responds with "y\n".
 * Demonstrates: registerOutputWatcher, writePty().
 * Capabilities: pty:write
 *
 * WARNING: Use with caution! This auto-confirms every [y/N] prompt.
 */

const PLUGIN_ID = "auto-confirm";
const SECTION_ID = "auto-confirm";

const ICON = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="currentColor"><path d="M13.78 4.22a.75.75 0 0 1 0 1.06l-7.25 7.25a.75.75 0 0 1-1.06 0L2.22 9.28a.75.75 0 0 1 1.06-1.06L6 10.94l6.72-6.72a.75.75 0 0 1 1.06 0z"/></svg>`;

let confirmCount = 0;

export default {
  id: PLUGIN_ID,

  onload(host) {
    host.registerSection({
      id: SECTION_ID,
      label: "AUTO CONFIRM",
      priority: 40,
      canDismissAll: true,
    });

    host.registerOutputWatcher({
      pattern: /\[y\/N\]|\[Y\/n\]|\(y\/n\)/i,
      onMatch(match, sessionId) {
        confirmCount++;
        // Send "y" followed by Enter
        host.writePty(sessionId, "y\n").catch((err) => {
          console.error("[auto-confirm] writePty failed:", err);
        });

        host.addItem({
          id: `confirm:${confirmCount}`,
          pluginId: PLUGIN_ID,
          sectionId: SECTION_ID,
          title: "Auto-confirmed",
          subtitle: match[0],
          icon: ICON,
          iconColor: "#3fb950",
          dismissible: true,
        });
      },
    });
  },

  onunload() {
    confirmCount = 0;
  },
};
