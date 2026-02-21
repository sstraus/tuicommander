/**
 * Hello World Plugin — Minimal example (Tier 1 only)
 *
 * Watches for "hello" in PTY output and adds an activity item.
 * Demonstrates: registerSection, registerOutputWatcher, addItem.
 * No capabilities needed.
 */

const PLUGIN_ID = "hello-world";
const SECTION_ID = "hello";

const ICON = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="currentColor"><path d="M8 1a7 7 0 1 0 0 14A7 7 0 0 0 8 1zm0 12.5a5.5 5.5 0 1 1 0-11 5.5 5.5 0 0 1 0 11zM6 6.5a1 1 0 1 1-2 0 1 1 0 0 1 2 0zm6 0a1 1 0 1 1-2 0 1 1 0 0 1 2 0zm-1.5 3.25c0 .14-.11.25-.25.25h-4.5a.25.25 0 0 1-.25-.25C5.5 8.56 6.57 7.5 8 7.5s2.5 1.06 2.5 2.25z"/></svg>`;

let count = 0;

export default {
  id: PLUGIN_ID,

  onload(host) {
    host.registerSection({
      id: SECTION_ID,
      label: "HELLO WORLD",
      priority: 50,
      canDismissAll: true,
    });

    host.registerOutputWatcher({
      pattern: /hello\s+(\w+)/i,
      onMatch(match, sessionId) {
        count++;
        host.addItem({
          id: `hello:${count}`,
          pluginId: PLUGIN_ID,
          sectionId: SECTION_ID,
          title: `Hello ${match[1]}!`,
          subtitle: `Detected in session ${sessionId}`,
          icon: ICON,
          dismissible: true,
        });
      },
    });
  },

  onunload() {
    count = 0;
  },
};
