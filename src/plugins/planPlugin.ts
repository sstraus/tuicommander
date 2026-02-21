import { invoke } from "../invoke";
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

    // Split absolute path into repo_path (dirname) + file (basename)
    // so it satisfies read_file_impl's within-repo security constraint.
    const lastSlash = rawPath.lastIndexOf("/");
    const dirPath = lastSlash > 0 ? rawPath.slice(0, lastSlash) : rawPath;
    const fileName = lastSlash >= 0 ? rawPath.slice(lastSlash + 1) : rawPath;

    try {
      return await invoke<string>("read_file", { path: dirPath, file: fileName });
    } catch (err) {
      console.warn("[planPlugin] Failed to read plan file:", rawPath, err);
      return null;
    }
  },
};

// ---------------------------------------------------------------------------
// Plugin implementation
// ---------------------------------------------------------------------------

class PlanPlugin implements TuiPlugin {
  readonly id = PLUGIN_ID;

  onload(host: PluginHost): void {
    host.registerSection({
      id: SECTION_ID,
      label: "ACTIVE PLAN",
      priority: 10,
      canDismissAll: false,
    });

    host.registerStructuredEventHandler("plan-file", (payload, _sessionId) => {
      if (typeof payload !== "object" || payload === null || typeof (payload as Record<string, unknown>).path !== "string") return;
      const { path } = payload as { path: string };
      const id = itemId(path);

      // Add or update (addItem deduplicates by id)
      host.addItem({
        id,
        pluginId: PLUGIN_ID,
        sectionId: SECTION_ID,
        title: displayName(path),
        subtitle: path,
        icon: ICON_SVG,
        dismissible: true,
        contentUri: contentUri(path),
      });
    });

    host.registerMarkdownProvider("plan", planMarkdownProvider);
  }

  onunload(): void {
    // All registrations are auto-disposed by the plugin registry
  }
}

export const planPlugin: TuiPlugin = new PlanPlugin();
