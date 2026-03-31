import { invoke, listen } from "../invoke";
import { mdTabsStore } from "../stores/mdTabs";
import { appLogger } from "../stores/appLogger";
import { stripFrontmatter, extractPlanMetadata } from "../utils/frontmatter";
import type { DirEntry } from "../types/fs";
import type { MarkdownProvider, PluginHost, TuiPlugin } from "./types";

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const PLUGIN_ID = "plan";

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
  private unlistenDirChanged: (() => void) | null = null;
  private watchedPlansDir: string | null = null;

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

      const ownerRepo = host.getActiveRepoPath();

      appLogger.info("plugin", `[plan] event: raw="${rawPath}" abs="${absolutePath}" cwd="${cwd}" ownerRepo="${ownerRepo}"`);

      // Skip plans with unresolved relative paths (no cwd, not absolute)
      if (!absolutePath.startsWith("/")) {
        appLogger.warn("plugin", `[plan] SKIPPED: cannot resolve relative path "${rawPath}" (cwd=${cwd})`);
        return;
      }

      const isNew = !this.plans.has(absolutePath);
      this.addPlan(absolutePath);
      appLogger.info("plugin", `[plan] isNew=${isNew} ownerRepo=${ownerRepo} plans.size=${this.plans.size}`);

      if (isNew && ownerRepo) {
        const tabId = mdTabsStore.addVirtualBackground(displayName(absolutePath), contentUri(absolutePath), ownerRepo);
        appLogger.info("plugin", `[plan] addVirtualBackground result: tabId=${tabId}`);
      }
    });

    host.registerMarkdownProvider("plan", planMarkdownProvider);

    const activeRepo = host.getActiveRepoPath();
    if (activeRepo) {
      this.scanPlansDirectory(activeRepo);
      this.watchPlansDir(activeRepo);
    }
  }

  /** Start a file system watcher on <repo>/plans/ so new plans are detected immediately. */
  private watchPlansDir(repoPath: string): void {
    const plansDir = `${repoPath.replace(/\/$/, "")}/plans`;
    this.watchedPlansDir = plansDir;

    invoke("start_dir_watcher", { path: plansDir }).catch(() => {
      // plans/ directory may not exist — that's fine
    });

    listen<{ dir_path: string }>("dir-changed", (event) => {
      if (event.payload.dir_path !== this.watchedPlansDir) return;
      appLogger.info("plugin", "[plan] dir-changed detected, re-scanning plans/");
      this.rescanAndOpenNew(repoPath);
    }).then((unlisten) => {
      this.unlistenDirChanged = unlisten;
    }).catch((err) => {
      appLogger.warn("plugin", "[plan] Failed to register dir-changed listener", err);
    });
  }

  /** Re-scan plans/ and auto-open any new plans as background tabs. */
  private rescanAndOpenNew(repoPath: string): void {
    const root = repoPath.replace(/\/$/, "");
    invoke<DirEntry[]>("list_directory", { repoPath, subdir: "plans" })
      .then((entries) => {
        const mdFiles = entries.filter((e) => !e.is_dir && e.name.endsWith(".md"));
        for (const entry of mdFiles) {
          const absolutePath = `${root}/${entry.path}`;
          if (!this.plans.has(absolutePath)) {
            this.addPlan(absolutePath);
            mdTabsStore.addVirtualBackground(displayName(absolutePath), contentUri(absolutePath), repoPath);
            appLogger.info("plugin", `[plan] new plan detected via watcher: ${absolutePath}`);
          }
        }
      })
      .catch((err) => {
        appLogger.warn("plugin", "[plan] rescan after dir-changed failed", err);
      });
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

  /** Scan a repo's plans/ directory and add discovered plan files.
   *  Also checks for an active plan marker and auto-opens it as a background tab. */
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
      .then(() => this.openActivePlan(repoPath))
      .catch(() => {
        // plans/ directory may not exist — that's fine
        // Still try to open active plan even if plans/ doesn't exist
        this.openActivePlan(repoPath);
      });
  }

  /** Read .claude/active-plan.json and auto-open the active plan as a background tab.
   *  The marker file can be in <repo>/.claude/ or <repo>/src-tauri/.claude/ (Claude Code
   *  CWD varies). */
  private async openActivePlan(repoPath: string): Promise<void> {
    const root = repoPath.replace(/\/$/, "");
    const candidates = [
      `${root}/.claude/active-plan.json`,
      `${root}/src-tauri/.claude/active-plan.json`,
    ];

    for (const markerPath of candidates) {
      try {
        const raw = await invoke<string>("read_external_file", { path: markerPath });
        const parsed: unknown = JSON.parse(raw);
        if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) continue;
        const marker = parsed as { path?: string };
        if (!marker.path) continue;

        const planPath = marker.path.startsWith("/")
          ? marker.path
          : `${root}/${marker.path}`;

        if (!this.plans.has(planPath)) {
          this.addPlan(planPath);
        }
        // addVirtualBackground deduplicates internally — safe to call unconditionally
        const tabId = mdTabsStore.addVirtualBackground(displayName(planPath), contentUri(planPath), repoPath);
        if (tabId) {
          appLogger.info("plugin", `[plan] auto-opened active plan: ${planPath}`);
        }
        return; // Found and processed — stop searching candidates
      } catch {
        // Marker file not found at this path — try next candidate
      }
    }
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
      .catch((err) => {
        appLogger.warn("plugin", `[plan] Failed to enrich plan metadata: ${absolutePath}`, err);
      });
  }

  /** Get current plan entries (for testing). */
  getPlans(): Map<string, PlanEntry> {
    return this.plans;
  }

  onunload(): void {
    this.plans.clear();
    this.repoPath = null;
    this.unlistenDirChanged?.();
    this.unlistenDirChanged = null;
    if (this.watchedPlansDir) {
      invoke("stop_dir_watcher", { path: this.watchedPlansDir }).catch(() => {});
      this.watchedPlansDir = null;
    }
  }
}

const planPluginInstance = new PlanPlugin();
export const planPlugin: TuiPlugin = planPluginInstance;

/** Scan a repo's plans/ directory and populate the plans panel.
 *  Safe to call multiple times — uses stable IDs for dedup. */
export function scanPlans(repoPath: string): void {
  planPluginInstance.scanPlansDirectory(repoPath);
}
