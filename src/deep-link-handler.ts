import { onOpenUrl } from "@tauri-apps/plugin-deep-link";
import { isTauri } from "./transport";
import { invoke } from "./invoke";
import { repositoriesStore } from "./stores/repositories";

/** Callbacks provided by App.tsx to control UI navigation */
export interface DeepLinkCallbacks {
  openSettings: (tab?: string) => void;
}

/** Parse a tuic:// URL into a command and parameters */
function parseDeepLink(urlString: string): { command: string; params: URLSearchParams } | null {
  try {
    const url = new URL(urlString);
    if (url.protocol !== "tuic:") return null;
    // URL hostname is the command (tuic://install-plugin?url=...)
    const command = url.hostname;
    return { command, params: url.searchParams };
  } catch {
    return null;
  }
}

/** Handle a single deep link URL */
async function handleDeepLink(urlString: string, callbacks: DeepLinkCallbacks): Promise<void> {
  const parsed = parseDeepLink(urlString);
  if (!parsed) {
    console.warn("[DeepLink] Unrecognised URL:", urlString);
    return;
  }

  const { command, params } = parsed;

  switch (command) {
    case "install-plugin": {
      const url = params.get("url");
      if (!url) {
        console.warn("[DeepLink] install-plugin: missing url parameter");
        return;
      }
      // Security: HTTPS only
      if (!url.startsWith("https://")) {
        console.warn("[DeepLink] install-plugin: only HTTPS URLs are allowed");
        return;
      }
      // Confirmation dialog before downloading
      const proceed = confirm(`Install plugin from:\n${url}\n\nThis will download and install a plugin.`);
      if (!proceed) return;

      try {
        await invoke("install_plugin_from_url", { url });
        // Open plugins tab so user can see the result
        callbacks.openSettings("plugins");
      } catch (err) {
        console.error("[DeepLink] install-plugin failed:", err);
        alert(`Plugin installation failed: ${err}`);
      }
      break;
    }

    case "open-repo": {
      const path = params.get("path");
      if (!path) {
        console.warn("[DeepLink] open-repo: missing path parameter");
        return;
      }
      // Security: only allow repos already in the repo list
      if (!(path in repositoriesStore.state.repositories)) {
        console.warn("[DeepLink] open-repo: path not in repo list:", path);
        return;
      }
      repositoriesStore.setActive(path);
      break;
    }

    case "settings": {
      const tab = params.get("tab") ?? undefined;
      callbacks.openSettings(tab);
      break;
    }

    default:
      console.warn("[DeepLink] Unknown command:", command);
  }
}

/** Initialise the deep link listener. Call once from App.tsx onMount. */
export function initDeepLinkHandler(callbacks: DeepLinkCallbacks): void {
  if (!isTauri()) return;

  onOpenUrl((urls: string[]) => {
    for (const url of urls) {
      handleDeepLink(url, callbacks).catch((err) =>
        console.error("[DeepLink] Handler error:", err),
      );
    }
  }).catch((err) => {
    console.error("[DeepLink] Failed to register handler:", err);
  });
}
