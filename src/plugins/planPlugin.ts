import { invoke } from "../invoke";
import { mdTabsStore } from "../stores/mdTabs";
import { appLogger } from "../stores/appLogger";
import { stripFrontmatter, extractPlanMetadata } from "../utils/frontmatter";
import type { DirEntry } from "../types/fs";
import type { MarkdownProvider, PluginHost, TuiPlugin, SidebarItem } from "./types";
import type { SidebarPanelHandle } from "../stores/sidebarPluginStore";

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

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
// Internal plan item tracking
// ---------------------------------------------------------------------------

interface PlanEntry {
  path: string;
  title: string;
  subtitle: string;
  metadata?: Record<string, string>;
}

// ---------------------------------------------------------------------------
// Plugin implementation
// ---------------------------------------------------------------------------

class PlanPlugin implements TuiPlugin {
  readonly id = PLUGIN_ID;
  private panelHandle: SidebarPanelHandle | null = null;
  private plans = new Map<string, PlanEntry>();

  onload(host: PluginHost): void {
    this.plans.clear();

    this.panelHandle = host.registerSidebarPanel({
      id: "active-plans",
      label: "ACTIVE PLANS",
      icon: ICON_SVG,
      priority: 10,
      collapsed: false,
    });

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

      const isNew = !this.plans.has(absolutePath);

      this.addPlan(absolutePath);

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

  /** Add or update a plan entry and refresh the sidebar panel. */
  private addPlan(absolutePath: string): void {
    if (!this.plans.has(absolutePath)) {
      this.plans.set(absolutePath, {
        path: absolutePath,
        title: displayName(absolutePath),
        subtitle: absolutePath,
      });
    }
    this.refreshPanel();
    this.enrichPlan(absolutePath);
  }

  /** Scan a repo's plans/ directory and add discovered plan files. */
  scanPlansDirectory(repoPath: string): void {
    invoke<DirEntry[]>("list_directory", { repoPath, subdir: "plans" })
      .then((entries) => {
        const mdFiles = entries.filter((e) => !e.is_dir && e.name.endsWith(".md"));
        for (const entry of mdFiles) {
          const absolutePath = `${repoPath.replace(/\/$/, "")}/${entry.path}`;
          this.addPlan(absolutePath);
        }
      })
      .catch(() => {
        // plans/ directory may not exist — that's fine
      });
  }

  /** Read a plan file and update the entry with extracted metadata. */
  private enrichPlan(absolutePath: string): void {
    invoke<string>("plugin_read_file", { path: absolutePath, pluginId: PLUGIN_ID })
      .then((raw) => {
        const meta = extractPlanMetadata(raw);
        const entry = this.plans.get(absolutePath);
        if (!entry) return;

        const updates: Record<string, string> = {};
        if (meta.status) updates.status = meta.status;
        if (meta.effort) updates.effort = meta.effort;
        if (meta.priority) updates.priority = meta.priority;
        if (meta.story) updates.story = meta.story;
        if (meta.created) updates.created = meta.created;

        entry.title = meta.title ?? displayName(absolutePath);
        entry.metadata = Object.keys(updates).length > 0 ? updates : undefined;
        this.refreshPanel();
      })
      .catch(() => {
        // File read failed — keep fallback title, no metadata
      });
  }

  /** Rebuild sidebar items from the internal plans map. */
  private refreshPanel(): void {
    if (!this.panelHandle) return;
    const items: SidebarItem[] = [];
    for (const [, entry] of this.plans) {
      const subtitle = entry.metadata?.status
        ? `${entry.metadata.status}${entry.metadata.effort ? ` · ${entry.metadata.effort}` : ""}`
        : entry.subtitle;
      items.push({
        id: `plan:${entry.path}`,
        label: entry.title,
        subtitle,
        icon: ICON_SVG,
        onClick: () => {
          mdTabsStore.addVirtual(entry.title, contentUri(entry.path));
        },
      });
    }
    this.panelHandle.setItems(items);
    this.panelHandle.setBadge(items.length > 0 ? String(items.length) : null);
  }

  /** Get current plan entries (for testing). */
  getPlans(): Map<string, PlanEntry> {
    return this.plans;
  }

  onunload(): void {
    this.panelHandle = null;
    this.plans.clear();
  }
}

const planPluginInstance = new PlanPlugin();
export const planPlugin: TuiPlugin = planPluginInstance;

/** Scan a repo's plans/ directory and populate the sidebar panel.
 *  Safe to call multiple times — uses stable IDs for dedup. */
export function scanPlans(repoPath: string): void {
  planPluginInstance.scanPlansDirectory(repoPath);
}
