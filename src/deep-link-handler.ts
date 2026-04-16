import { onOpenUrl } from "@tauri-apps/plugin-deep-link";
import { invoke } from "./invoke";
import { isTauri } from "./transport";
import { appLogger } from "./stores/appLogger";
import { pluginStore } from "./stores/pluginStore";
import { repositoriesStore } from "./stores/repositories";

/** Callbacks provided by App.tsx to control UI navigation */
export interface DeepLinkCallbacks {
  openSettings: (tab?: string) => void;
  /** Show an in-app confirmation dialog — replaces native browser confirm() */
  confirm: (title: string, message: string) => Promise<boolean>;
  /** Show an in-app error notification — replaces native browser alert() */
  onInstallError: (message: string) => void;
}

/** Parse a tuic:// URL into a command, path segments, and parameters */
function parseDeepLink(urlString: string): { command: string; pathSegments: string[]; params: URLSearchParams } | null {
  try {
    const url = new URL(urlString);
    if (url.protocol !== "tuic:") return null;
    // URL hostname is the command (tuic://install-plugin?url=... or tuic://cmd/ui/toast?title=Hello)
    const command = url.hostname;
    // pathname segments after the leading slash, e.g. "/ui/toast" → ["ui", "toast"]
    const pathSegments = url.pathname.split("/").filter(Boolean);
    return { command, pathSegments, params: url.searchParams };
  } catch {
    return null;
  }
}

/** Commands that require user confirmation before execution */
const DESTRUCTIVE_COMMANDS = new Set([
  "agent/spawn",
  "session/create",
  "session/input",
  "session/kill",
  "session/close",
]);

/** Commands that are blocked entirely via deep link */
const BLOCKED_COMMANDS = new Set([
  "config/save",
  "debug/invoke_js",
]);

/** Handle a single deep link URL. Exported for tests. */
export async function handleDeepLink(urlString: string, callbacks: DeepLinkCallbacks): Promise<void> {
  const parsed = parseDeepLink(urlString);
  if (!parsed) {
    appLogger.warn("app", `Unrecognised deep link URL: ${urlString}`);
    return;
  }

  const { command, pathSegments, params } = parsed;

  switch (command) {
    case "install-plugin": {
      const url = params.get("url");
      if (!url) {
        appLogger.warn("app", "Deep link install-plugin: missing url parameter");
        return;
      }
      // Security: HTTPS only
      if (!url.startsWith("https://")) {
        appLogger.warn("app", "Deep link install-plugin: only HTTPS URLs are allowed");
        return;
      }
      // Confirmation dialog before downloading
      const proceed = await callbacks.confirm(
        "Install plugin?",
        `Install plugin from:\n${url}\n\nThis will download and install a plugin.`,
      );
      if (!proceed) return;

      try {
        await pluginStore.installFromUrl(url);
        // Open plugins tab so user can see the result
        callbacks.openSettings("plugins");
      } catch (err) {
        appLogger.error("plugin", "DeepLink: install-plugin failed", err);
        callbacks.onInstallError(`Plugin installation failed: ${err}`);
      }
      break;
    }

    case "open-repo": {
      const path = params.get("path");
      if (!path) {
        appLogger.warn("app", "Deep link open-repo: missing path parameter");
        return;
      }
      // Security: only allow repos already in the repo list
      if (!(path in repositoriesStore.state.repositories)) {
        appLogger.warn("app", `Deep link open-repo: path not in repo list: ${path}`);
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

    case "oauth-callback": {
      // OAuth 2.1 authorization code response from the upstream MCP server's
      // authorization server. Extract code + state and hand them to the
      // backend, which exchanges the code, persists the tokens, and resumes
      // the upstream connection.
      const code = params.get("code");
      const oauthState = params.get("state");
      const authError = params.get("error");

      if (authError) {
        const description = params.get("error_description") ?? "";
        appLogger.error(
          "app",
          `OAuth callback returned error: ${authError}${description ? ` (${description})` : ""}`,
        );
        callbacks.onInstallError(
          `OAuth authorization failed: ${authError}${description ? ` — ${description}` : ""}`,
        );
        return;
      }

      if (!code || !oauthState) {
        appLogger.warn("app", "Deep link oauth-callback: missing code or state parameter");
        return;
      }

      try {
        await invoke("mcp_oauth_callback", { code, oauthState });
        appLogger.info("app", "OAuth callback completed");
      } catch (err) {
        appLogger.error("app", "OAuth callback invoke failed", err);
        callbacks.onInstallError(`OAuth callback failed: ${err}`);
      }
      break;
    }

    case "cmd": {
      // Gateway: tuic://cmd/{tool}/{action}?{params}
      if (pathSegments.length < 2) {
        appLogger.warn("app", `Deep link cmd: requires tuic://cmd/{tool}/{action}, got: ${urlString}`);
        return;
      }
      const [tool, action] = pathSegments;
      const cmdKey = `${tool}/${action}`;

      // Blocked commands — never execute
      if (BLOCKED_COMMANDS.has(cmdKey)) {
        appLogger.warn("app", `Deep link cmd: blocked command: ${cmdKey}`);
        return;
      }

      // Destructive commands — require confirmation
      if (DESTRUCTIVE_COMMANDS.has(cmdKey)) {
        const proceed = await callbacks.confirm(
          "Execute command?",
          `Allow deep link to run:\n${tool} → ${action}\n\nThis action may modify sessions or spawn processes.`,
        );
        if (!proceed) return;
      }

      // Convert URLSearchParams to a plain object for the Tauri command
      const cmdParams: Record<string, string> = {};
      params.forEach((value, key) => {
        cmdParams[key] = value;
      });

      try {
        const result = await invoke("deep_link_mcp_call", {
          tool,
          action,
          params: cmdParams,
        });
        appLogger.info("app", `Deep link cmd ${cmdKey} result`, result);
      } catch (err) {
        appLogger.error("app", `Deep link cmd ${cmdKey} failed`, err);
      }
      break;
    }

    default:
      appLogger.warn("app", `Deep link unknown command: ${command}`);
  }
}

/** Initialise the deep link listener. Call once from App.tsx onMount. */
export function initDeepLinkHandler(callbacks: DeepLinkCallbacks): void {
  if (!isTauri()) return;

  onOpenUrl((urls: string[]) => {
    for (const url of urls) {
      handleDeepLink(url, callbacks).catch((err) =>
        appLogger.error("app", "Deep link handler error", err),
      );
    }
  }).catch((err) => {
    appLogger.error("app", "DeepLink: Failed to register handler", err);
  });
}
