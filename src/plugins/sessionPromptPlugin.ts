import type { MarkdownProvider, PluginHost, TuiPlugin } from "./types";

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const SECTION_ID = "session-prompts";
const PLUGIN_ID = "session-prompts";

// Terminal prompt SVG icon (chat bubble)
const ICON_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="currentColor">
  <path d="M1 3.5A1.5 1.5 0 0 1 2.5 2h11A1.5 1.5 0 0 1 15 3.5v7A1.5 1.5 0 0 1 13.5 12H9.373l-2.62 1.81A.75.75 0 0 1 5.6 13.2V12H2.5A1.5 1.5 0 0 1 1 10.5v-7Zm1.5-.5a.5.5 0 0 0-.5.5v7a.5.5 0 0 0 .5.5H6.35a.75.75 0 0 1 .75.75v.83l1.81-1.25a.75.75 0 0 1 .427-.133H13.5a.5.5 0 0 0 .5-.5v-7a.5.5 0 0 0-.5-.5h-11Z"/>
</svg>`;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface PromptEntry {
  id: string;
  sessionId: string;
  content: string;
  timestamp: number;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Truncate prompt text for the activity item title. */
function truncateTitle(content: string, maxLen = 60): string {
  const oneLine = content.replace(/\n/g, " ").trim();
  if (oneLine.length <= maxLen) return oneLine;
  return oneLine.slice(0, maxLen - 1) + "\u2026";
}

/** Format a timestamp as a compact time string. */
function formatTime(ts: number): string {
  const d = new Date(ts);
  return d.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
}

/** Stable item id for a prompt entry. */
function itemId(idx: number): string {
  return `prompt:${idx}`;
}

/** Build the contentUri for a prompt entry. */
function contentUri(idx: number): string {
  return `session-prompt:entry?idx=${idx}`;
}

// ---------------------------------------------------------------------------
// MarkdownProvider
// ---------------------------------------------------------------------------

/** Provides formatted markdown for a prompt entry. */
function makeMarkdownProvider(entries: PromptEntry[]): MarkdownProvider {
  return {
    provideContent(uri: URL): string | null {
      const idxStr = uri.searchParams.get("idx");
      if (idxStr === null) return null;
      const idx = parseInt(idxStr, 10);
      if (isNaN(idx) || idx < 0 || idx >= entries.length) return null;

      const entry = entries[idx];
      const time = new Date(entry.timestamp).toLocaleString();
      const lines = [
        `# User Prompt`,
        ``,
        `**Time:** ${time}  `,
        `**Session:** \`${entry.sessionId.slice(0, 8)}\``,
        ``,
        `---`,
        ``,
        "```",
        entry.content,
        "```",
      ];
      return lines.join("\n");
    },
  };
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

/** Max prompt items shown in the Activity Center bell dropdown. */
const MAX_PROMPT_ITEMS = 10;

class SessionPromptPlugin implements TuiPlugin {
  readonly id = PLUGIN_ID;
  private entries: PromptEntry[] = [];
  private nextIdx = 0;

  onload(host: PluginHost): void {
    host.registerSection({
      id: SECTION_ID,
      label: "USER PROMPTS",
      priority: 20, // Below plan (10)
      canDismissAll: true,
    });

    host.registerMarkdownProvider("session-prompt", makeMarkdownProvider(this.entries));

    host.registerStructuredEventHandler("user-input", (payload, sessionId) => {
      if (typeof payload !== "object" || payload === null) return;
      const { content } = payload as { content: string };
      if (typeof content !== "string" || content.trim().length === 0) return;

      const idx = this.nextIdx++;
      const entry: PromptEntry = {
        id: itemId(idx),
        sessionId,
        content,
        timestamp: Date.now(),
      };
      this.entries.push(entry);

      // Evict oldest entries beyond the limit
      while (this.entries.length > MAX_PROMPT_ITEMS) {
        const evicted = this.entries.shift()!;
        host.removeItem(evicted.id);
      }

      host.addItem({
        id: entry.id,
        pluginId: PLUGIN_ID,
        sectionId: SECTION_ID,
        title: truncateTitle(content),
        subtitle: formatTime(entry.timestamp),
        icon: ICON_SVG,
        dismissible: true,
        contentUri: contentUri(idx),
      });
    });
  }

  onunload(): void {
    this.entries = [];
    this.nextIdx = 0;
  }
}

export const sessionPromptPlugin: TuiPlugin = new SessionPromptPlugin();
