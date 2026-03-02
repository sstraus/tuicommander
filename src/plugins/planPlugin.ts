import { invoke } from "../invoke";
import { repositoriesStore } from "../stores/repositories";
import { terminalsStore } from "../stores/terminals";
import { activityStore } from "../stores/activityStore";
import { mdTabsStore } from "../stores/mdTabs";
import { appLogger } from "../stores/appLogger";
import { parseFrontmatter } from "../utils/frontmatter";
import type { MarkdownProvider, PluginHost, TuiPlugin } from "./types";

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const SECTION_ID = "plan";
const PLUGIN_ID = "plan";

// Inline document SVG icon
const ICON_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="currentColor">
  <path fill-rule="evenodd" d="M3 1.5A1.5 1.5 0 0 1 4.5 0h4.379a1.5 1.5 0 0 1 1.06.44l3.122 3.12A1.5 1.5 0 0 1 13.5 4.622V13.5A1.5 1.5 0 0 1 12 15H4.5A1.5 1.5 0 0 1 3 13.5v-12Zm1.5-.5a.5.5 0 0 0-.5.5v12a.5.5 0 0 0 .5.5H12a.5.5 0 0 0 .5-.5V5h-2.25A1.25 1.25 0 0 1 9 3.75V1H4.5Zm5 .5v2.25c0 .138.112.25.25.25H12L9.5 1.5Z" clip-rule="evenodd"/>
  <path d="M5 8.25a.75.75 0 0 1 .75-.75h4.5a.75.75 0 0 1 0 1.5h-4.5A.75.75 0 0 1 5 8.25Zm0 2.5A.75.75 0 0 1 5.75 10h4.5a.75.75 0 0 1 0 1.5h-4.5A.75.75 0 0 1 5 10.75Z"/>
</svg>`;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Derive the display name from an absolute plan file path. */
function displayName(absolutePath: string): string {
  const base = absolutePath.split("/").pop() ?? absolutePath;
  return base.replace(/\.[^.]+$/, ""); // strip extension
}

/** Stable item id for a given plan path. */
function itemId(absolutePath: string): string {
  return `plan:${absolutePath}`;
}

/** Build the contentUri for a plan file path. */
function contentUri(absolutePath: string): string {
  return `plan:file?path=${encodeURIComponent(absolutePath)}`;
}

/** Get the CWD for a terminal session, or null if not found. */
function getSessionCwd(sessionId: string): string | null {
  for (const t of Object.values(terminalsStore.state.terminals)) {
    if (t.sessionId === sessionId && t.cwd) {
      return t.cwd;
    }
  }
  return null;
}

/** Resolve a plan path to absolute using the session's CWD. */
function resolvePath(path: string, sessionId: string): string {
  if (path.startsWith("/")) return path;
  const cwd = getSessionCwd(sessionId);
  if (!cwd) return path; // can't resolve without CWD
  return `${cwd.replace(/\/$/, "")}/${path}`;
}

/** Derive the repo path for a session by finding which registered repo the CWD belongs to. */
function deriveRepoPath(sessionId: string): string | null {
  const cwd = getSessionCwd(sessionId);
  if (!cwd) return null;
  for (const repoPath of repositoriesStore.getPaths()) {
    if (cwd.startsWith(repoPath)) return repoPath;
  }
  // No registered repo matches — use the CWD itself as a best-effort repo identifier
  return cwd;
}

// ---------------------------------------------------------------------------
// MarkdownProvider implementation
// ---------------------------------------------------------------------------

const planMarkdownProvider: MarkdownProvider = {
  async provideContent(uri: URL): Promise<string | null> {
    const rawPath = uri.searchParams.get("path");
    if (!rawPath || rawPath.includes("..")) return null;

    try {
      const raw = await invoke<string>("plugin_read_file", { path: rawPath, pluginId: PLUGIN_ID });
      const { content } = parseFrontmatter(raw);
      return content;
    } catch (err) {
      appLogger.warn("plugin", `Failed to read plan file: ${rawPath}`, err);
      return null;
    }
  },
};

// ---------------------------------------------------------------------------
// Plugin implementation
// ---------------------------------------------------------------------------

/** Max plan items shown in the Activity Center bell dropdown */
const MAX_PLAN_ITEMS = 3;

class PlanPlugin implements TuiPlugin {
  readonly id = PLUGIN_ID;
  /** Ordered list of active plan item IDs (oldest first) */
  private planItemIds: string[] = [];

  onload(host: PluginHost): void {
    host.registerSection({
      id: SECTION_ID,
      label: "ACTIVE PLAN",
      priority: 10,
      canDismissAll: false,
    });

    // Rebuild planItemIds from hydrated activityStore items
    const existingItems = activityStore.getForSection(SECTION_ID);
    this.planItemIds = existingItems
      .sort((a, b) => a.createdAt - b.createdAt)
      .map((i) => i.id);

    host.registerStructuredEventHandler("plan-file", (payload, sessionId) => {
      if (typeof payload !== "object" || payload === null || typeof (payload as Record<string, unknown>).path !== "string") return;
      const rawPath = (payload as { path: string }).path;

      // Resolve relative paths to absolute using session CWD
      const absolutePath = resolvePath(rawPath, sessionId);
      const repoPath = deriveRepoPath(sessionId);
      const id = itemId(absolutePath);

      // If already tracked, move it to the end (most recent)
      const existingIdx = this.planItemIds.indexOf(id);
      const isNew = existingIdx < 0;
      if (existingIdx >= 0) {
        this.planItemIds.splice(existingIdx, 1);
      }
      this.planItemIds.push(id);

      // Evict oldest items beyond the limit
      while (this.planItemIds.length > MAX_PLAN_ITEMS) {
        const evictId = this.planItemIds.shift()!;
        host.removeItem(evictId);
      }

      const title = displayName(absolutePath);
      const uri = contentUri(absolutePath);

      // Add or update (addItem deduplicates by id)
      host.addItem({
        id,
        pluginId: PLUGIN_ID,
        sectionId: SECTION_ID,
        title,
        subtitle: absolutePath,
        icon: ICON_SVG,
        dismissible: true,
        repoPath: repoPath ?? undefined,
        contentUri: uri,
      });

      // Auto-open new plans belonging to the active repo as a background tab
      if (isNew && repoPath && repoPath === repositoriesStore.state.activeRepoPath) {
        mdTabsStore.addVirtualBackground(title, uri);
      }
    });

    host.registerMarkdownProvider("plan", planMarkdownProvider);
  }

  onunload(): void {
    // All registrations are auto-disposed by the plugin registry
  }
}

export const planPlugin: TuiPlugin = new PlanPlugin();
