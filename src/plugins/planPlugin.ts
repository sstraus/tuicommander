import { invoke } from "../invoke";
import { mdTabsStore } from "../stores/mdTabs";
import { appLogger } from "../stores/appLogger";
import { stripFrontmatter, extractPlanMetadata } from "../utils/frontmatter";
import type { DirEntry } from "../types/fs";
import type { MarkdownProvider, PluginHost, TuiPlugin } from "./types";

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

/** Make a path relative to a repo root. */
function relativePath(absolutePath: string, repoPath: string | null): string {
  if (repoPath && absolutePath.startsWith(repoPath)) {
    return absolutePath.slice(repoPath.length).replace(/^\//, "");
  }
  return absolutePath;
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
  status: string | null;
  effort: string | null;
}

// ---------------------------------------------------------------------------
// All Plans panel HTML builder
// ---------------------------------------------------------------------------

function buildAllPlansHtml(plans: Map<string, PlanEntry>, repoPath: string | null): string {
  // Group by status
  const groups = new Map<string, PlanEntry[]>();
  for (const entry of plans.values()) {
    const status = entry.status?.toLowerCase().trim() ?? "unknown";
    if (!groups.has(status)) groups.set(status, []);
    groups.get(status)!.push(entry);
  }

  // Sort: active first, then draft, then completed, then rest
  const statusOrder = ["in_progress", "in progress", "approved", "designed", "draft", "completed", "parked", "unknown"];
  const sortedStatuses = [...groups.keys()].sort((a, b) => {
    const ai = statusOrder.indexOf(a);
    const bi = statusOrder.indexOf(b);
    return (ai === -1 ? 99 : ai) - (bi === -1 ? 99 : bi);
  });

  let rows = "";
  for (const status of sortedStatuses) {
    const entries = groups.get(status) ?? [];
    for (const entry of entries) {
      const relPath = relativePath(entry.path, repoPath);
      const badgeClass = status === "in_progress" || status === "in progress" ? "badge-accent"
        : status === "completed" ? "badge-success"
        : status === "draft" ? "badge-muted"
        : status === "parked" ? "badge-warning"
        : "badge-muted";
      rows += `<tr data-path="${entry.path}" style="cursor:pointer">
        <td>${entry.title}</td>
        <td><span class="${badgeClass}">${status}</span></td>
        <td>${entry.effort ?? "\u2014"}</td>
        <td style="color:var(--fg-muted);font-size:var(--font-2xs)">${relPath}</td>
      </tr>`;
    }
  }

  return `<!DOCTYPE html><html><head><style>
    body { padding: 12px; }
    h2 { margin: 0 0 12px; font-size: var(--font-md); }
    table { width: 100%; border-collapse: collapse; }
    th { text-align: left; font-size: var(--font-2xs); color: var(--fg-muted); text-transform: uppercase; letter-spacing: 0.04em; padding: 4px 8px; border-bottom: 1px solid var(--border); }
    td { padding: 6px 8px; font-size: var(--font-sm); }
    tr:hover { background: var(--bg-highlight); }
  </style></head><body>
    <h2>All Plans (${plans.size})</h2>
    <table>
      <tr><th>Title</th><th>Status</th><th>Effort</th><th>Path</th></tr>
      ${rows}
    </table>
    <script>
      document.querySelectorAll("tr[data-path]").forEach(tr => {
        tr.addEventListener("click", () => {
          window.parent.postMessage({ action: "open-plan", path: tr.dataset.path }, "*");
        });
      });
    </script>
  </body></html>`;
}

// ---------------------------------------------------------------------------
// Plugin implementation
// ---------------------------------------------------------------------------

class PlanPlugin implements TuiPlugin {
  readonly id = PLUGIN_ID;
  private plans = new Map<string, PlanEntry>();
  private repoPath: string | null = null;

  onload(host: PluginHost): void {
    this.plans.clear();
    this.repoPath = host.getActiveRepoPath();

    // Context menu + Command Palette: "View All Plans" opens a panel
    host.registerTerminalAction({
      id: "view-all-plans",
      label: "View All Plans",
      action: () => {
        const html = buildAllPlansHtml(this.plans, this.repoPath);
        host.openPanel({
          id: "all-plans",
          title: "All Plans",
          html,
          onMessage: (data) => {
            if (typeof data === "object" && data !== null && (data as Record<string, unknown>).action === "open-plan") {
              const path = (data as { path: string }).path;
              mdTabsStore.addVirtual(displayName(path), contentUri(path));
            }
          },
        });
      },
      disabled: () => this.plans.size === 0,
    });

    host.registerStructuredEventHandler("plan-file", (payload, sessionId) => {
      if (typeof payload !== "object" || payload === null || typeof (payload as Record<string, unknown>).path !== "string") return;
      const rawPath = (payload as { path: string }).path;

      const cwd = host.getSessionCwd(sessionId);
      const absolutePath = rawPath.startsWith("/") ? rawPath
        : cwd ? `${cwd.replace(/\/$/, "")}/${rawPath}` : rawPath;

      const activeRepo = host.getActiveRepoPath();
      if (activeRepo !== null) {
        if (!cwd || !cwd.startsWith(activeRepo)) return;
      }

      const isNew = !this.plans.has(absolutePath);
      this.addPlan(absolutePath);

      if (isNew && activeRepo) {
        mdTabsStore.addVirtualBackground(displayName(absolutePath), contentUri(absolutePath), activeRepo);
      }
    });

    host.registerMarkdownProvider("plan", planMarkdownProvider);

    const activeRepo = host.getActiveRepoPath();
    if (activeRepo) {
      this.scanPlansDirectory(activeRepo);
    }
  }

  /** Add or update a plan entry, then enrich with file metadata. */
  private addPlan(absolutePath: string): void {
    if (!this.plans.has(absolutePath)) {
      this.plans.set(absolutePath, {
        path: absolutePath,
        title: displayName(absolutePath),
        status: null,
        effort: null,
      });
    }
    this.enrichPlan(absolutePath);
  }

  /** Scan a repo's plans/ directory and add discovered plan files. */
  scanPlansDirectory(repoPath: string): void {
    this.repoPath = repoPath;
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
        entry.title = meta.title ?? displayName(absolutePath);
        entry.status = meta.status ?? null;
        entry.effort = meta.effort ?? null;
      })
      .catch(() => {
        // File read failed — keep fallback title
      });
  }

  /** Get current plan entries (for testing). */
  getPlans(): Map<string, PlanEntry> {
    return this.plans;
  }

  onunload(): void {
    this.plans.clear();
    this.repoPath = null;
  }
}

const planPluginInstance = new PlanPlugin();
export const planPlugin: TuiPlugin = planPluginInstance;

/** Scan a repo's plans/ directory and populate the plans panel.
 *  Safe to call multiple times — uses stable IDs for dedup. */
export function scanPlans(repoPath: string): void {
  planPluginInstance.scanPlansDirectory(repoPath);
}
