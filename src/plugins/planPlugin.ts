import { invoke } from "../invoke";
import { activityStore } from "../stores/activityStore";
import { mdTabsStore } from "../stores/mdTabs";
import { appLogger } from "../stores/appLogger";
import { stripFrontmatter, extractPlanMetadata } from "../utils/frontmatter";
import type { DirEntry } from "../types/fs";
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

// ---------------------------------------------------------------------------
// MarkdownProvider implementation
// ---------------------------------------------------------------------------

const planMarkdownProvider: MarkdownProvider = {
  async provideContent(uri: URL): Promise<string | null> {
    const rawPath = uri.searchParams.get("path");
    if (!rawPath || rawPath.includes("..")) return null;

    try {
      const raw = await invoke<string>("plugin_read_file", { path: rawPath, pluginId: PLUGIN_ID });
      return stripFrontmatter(raw);
    } catch (err) {
      appLogger.warn("plugin", `Failed to read plan file: ${rawPath}`, err);
      return null;
    }
  },
};

// ---------------------------------------------------------------------------
// Plugin implementation
// ---------------------------------------------------------------------------

class PlanPlugin implements TuiPlugin {
  readonly id = PLUGIN_ID;
  private host: PluginHost | null = null;

  onload(host: PluginHost): void {
    this.host = host;

    host.registerSection({
      id: SECTION_ID,
      label: "ACTIVE PLAN",
      priority: 10,
      canDismissAll: false,
    });

    // Enrich any hydrated items that have no metadata yet
    const existingItems = activityStore.getForSection(SECTION_ID);
    for (const item of existingItems) {
      if (!item.metadata) {
        const path = item.subtitle; // subtitle holds the absolute path
        if (path) this.enrichItem(item.id, path, host);
      }
    }

    host.registerStructuredEventHandler("plan-file", (payload, sessionId) => {
      if (typeof payload !== "object" || payload === null || typeof (payload as Record<string, unknown>).path !== "string") return;
      const rawPath = (payload as { path: string }).path;

      // Resolve relative paths to absolute using session CWD via PluginHost
      const cwd = host.getSessionCwd(sessionId);
      const absolutePath = rawPath.startsWith("/") ? rawPath
        : cwd ? `${cwd.replace(/\/$/, "")}/${rawPath}` : rawPath;

      // Only show plans from terminals belonging to the active repo
      const activeRepo = host.getActiveRepoPath();
      if (activeRepo !== null) {
        if (!cwd || !cwd.startsWith(activeRepo)) return;
      }

      const repoPath = activeRepo ?? cwd ?? undefined;
      const isNew = !activityStore.getForSection(SECTION_ID).some((i) => i.id === itemId(absolutePath));

      this.addPlanItem(absolutePath, repoPath, host);

      // Auto-open new plans belonging to the active repo as a background tab
      if (isNew && activeRepo) {
        mdTabsStore.addVirtualBackground(displayName(absolutePath), contentUri(absolutePath), activeRepo);
      }
    });

    host.registerMarkdownProvider("plan", planMarkdownProvider);

    // Scan plans/ directory for the active repo on startup
    const activeRepo = host.getActiveRepoPath();
    if (activeRepo) {
      this.scanPlansDirectory(activeRepo);
    }
  }

  /** Add or update a plan item in the activity store, then enrich with file metadata. */
  private addPlanItem(absolutePath: string, repoPath: string | undefined, host: PluginHost): void {
    const id = itemId(absolutePath);
    host.addItem({
      id,
      pluginId: PLUGIN_ID,
      sectionId: SECTION_ID,
      title: displayName(absolutePath),
      subtitle: absolutePath,
      icon: ICON_SVG,
      dismissible: true,
      repoPath,
      contentUri: contentUri(absolutePath),
    });
    this.enrichItem(id, absolutePath, host);
  }

  /** Scan a repo's plans/ directory and add discovered plan files. */
  scanPlansDirectory(repoPath: string): void {
    invoke<DirEntry[]>("list_directory", { repoPath, subdir: "plans" })
      .then((entries) => {
        const mdFiles = entries.filter((e) => !e.is_dir && e.name.endsWith(".md"));
        for (const entry of mdFiles) {
          const absolutePath = `${repoPath.replace(/\/$/, "")}/${entry.path}`;
          if (this.host) {
            this.addPlanItem(absolutePath, repoPath, this.host);
          }
        }
      })
      .catch(() => {
        // plans/ directory may not exist — that's fine
      });
  }

  /** Read a plan file and update the ActivityItem with extracted metadata. */
  private enrichItem(itemId: string, absolutePath: string, host: PluginHost): void {
    invoke<string>("plugin_read_file", { path: absolutePath, pluginId: PLUGIN_ID })
      .then((raw) => {
        const meta = extractPlanMetadata(raw);
        const updates: Record<string, string> = {};
        if (meta.status) updates.status = meta.status;
        if (meta.effort) updates.effort = meta.effort;
        if (meta.priority) updates.priority = meta.priority;
        if (meta.story) updates.story = meta.story;
        if (meta.created) updates.created = meta.created;

        host.updateItem(itemId, {
          title: meta.title ?? displayName(absolutePath),
          metadata: Object.keys(updates).length > 0 ? updates : undefined,
        });
      })
      .catch(() => {
        // File read failed — keep fallback title, no metadata
      });
  }

  onunload(): void {
    this.host = null;
  }
}

const planPluginInstance = new PlanPlugin();
export const planPlugin: TuiPlugin = planPluginInstance;

/** Scan a repo's plans/ directory and populate the PlanPanel.
 *  Safe to call multiple times — uses stable IDs for dedup. */
export function scanPlans(repoPath: string): void {
  planPluginInstance.scanPlansDirectory(repoPath);
}
