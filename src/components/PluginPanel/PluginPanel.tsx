import { Component, createEffect, createSignal, onCleanup, onMount } from "solid-js";
import type { PluginPanelTab } from "../../stores/mdTabs";
import { pluginRegistry } from "../../plugins/pluginRegistry";
import { PLUGIN_BASE_CSS } from "./pluginBaseStyles";
import { TUIC_SDK_SCRIPT, TUIC_SDK_VERSION } from "./tuicSdk";
import { repositoriesStore } from "../../stores/repositories";
import { mdTabsStore } from "../../stores/mdTabs";
import { editorTabsStore } from "../../stores/editorTabs";
import { terminalsStore } from "../../stores/terminals";
import { settingsStore } from "../../stores/settings";
import { appLogger } from "../../stores/appLogger";
import { assignTabToActiveGroup } from "../../utils/paneTabAssign";
import { resolveTuicPath } from "./resolveTuicPath";

export interface PluginPanelProps {
  tab: PluginPanelTab;
  onClose?: () => void;
}

/**
 * Extract CSS custom properties from the app's :root for injection into iframe.
 * Only includes --bg-*, --fg-*, --border*, --accent*, --error*, --warning*, --success* vars.
 */
function extractThemeVars(): string {
  const root = getComputedStyle(document.documentElement);
  const vars: string[] = [];
  const prefixes = ["--bg-", "--fg-", "--border", "--accent", "--error", "--warning", "--success", "--ring-", "--text-"];
  for (const sheet of document.styleSheets) {
    try {
      for (const rule of sheet.cssRules) {
        if (rule instanceof CSSStyleRule && rule.selectorText === ":root") {
          for (let i = 0; i < rule.style.length; i++) {
            const prop = rule.style[i];
            if (prefixes.some((p) => prop.startsWith(p))) {
              vars.push(`${prop}:${root.getPropertyValue(prop).trim()}`);
            }
          }
        }
      }
    } catch {
      // Cross-origin stylesheets cannot be read — skip silently
    }
  }
  return vars.length > 0 ? `<style>:root{${vars.join(";")}}</style>` : "";
}


/** Inject theme CSS variables and base stylesheet into HTML before </head> (or prepend if no </head>) */
function injectThemeVars(html: string): string {
  const themeStyle = extractThemeVars();
  const baseStyle = `<style id="tuic-base">${PLUGIN_BASE_CSS}</style>`;
  const injection = baseStyle + themeStyle + TUIC_SDK_SCRIPT;
  const headClose = html.indexOf("</head>");
  if (headClose >= 0) {
    return html.slice(0, headClose) + injection + html.slice(headClose);
  }
  return injection + html;
}

/**
 * Renders plugin HTML content in a sandboxed iframe.
 *
 * Security: `sandbox="allow-scripts"` without `allow-same-origin` ensures
 * the iframe cannot access Tauri IPC, the parent window, or same-origin
 * resources. Communication happens via postMessage bridge only.
 *
 * CSP: The parent's CSP includes 'unsafe-inline' in script-src to allow
 * inline scripts in srcdoc iframes (inherited CSP). This is safe because
 * the main app has no user-injected HTML content (desktop app), and plugin
 * code is further isolated by the sandbox attribute.
 *
 * Message bridge: Non-system messages from the iframe are routed to the
 * plugin's onMessage callback via pluginRegistry.handlePanelMessage().
 * The plugin can send messages back via panelHandle.send().
 */
export const PluginPanel: Component<PluginPanelProps> = (props) => {
  let iframeRef: HTMLIFrameElement | undefined;

  /** Resolve a path (absolute or relative) to repo + relPath */
  const resolvePathForSdk = (path: string) => {
    const repos = Object.keys(repositoriesStore.state.repositories);
    return resolveTuicPath(path, repos, repositoriesStore.state.activeRepoPath);
  };

  /**
   * Send the SDK init handshake to the URL-mode iframe.
   *
   * Two call sites (see docs/tuic-sdk.md §Timing Notes):
   *  1. iframe `onLoad` — primary path for child pages with a synchronous
   *     `<head>` listener.
   *  2. In response to `tuic:sdk-request` from the child — fallback for
   *     child pages whose listener registers asynchronously (ES modules,
   *     frameworks that mount after DOMContentLoaded). Without this, the
   *     onLoad message would fire before the listener exists and be lost.
   */
  const sendSdkInit = () => {
    iframeRef?.contentWindow?.postMessage(
      { type: "tuic:sdk-init", version: TUIC_SDK_VERSION },
      "*",
    );
  };

  /** Handle tuic:* SDK messages from the iframe */
  const handleTuicMessage = (data: Record<string, unknown>) => {
    switch (data.type) {
      case "tuic:sdk-request": {
        // Fallback handshake: child page's listener was not ready when
        // iframe onLoad fired; it re-requests init. Respond idempotently.
        sendSdkInit();
        return;
      }
      case "tuic:open": {
        const path = typeof data.path === "string" ? data.path : "";
        if (!path) {
          appLogger.warn("plugin", "tuic:open missing path");
          return;
        }
        const resolved = resolvePathForSdk(path);
        if (!resolved) {
          appLogger.warn("plugin", `tuic:open cannot resolve path: ${path}`);
          return;
        }
        const tabId = mdTabsStore.add(resolved.repoPath, resolved.relPath);
        if (data.pinned) mdTabsStore.setPinned(tabId, true);
        return;
      }
      case "tuic:edit": {
        const path = typeof data.path === "string" ? data.path : "";
        if (!path) {
          appLogger.warn("plugin", "tuic:edit missing path");
          return;
        }
        const resolved = resolvePathForSdk(path);
        if (!resolved) {
          appLogger.warn("plugin", `tuic:edit cannot resolve path: ${path}`);
          return;
        }
        const line = typeof data.line === "number" ? data.line : 0;
        editorTabsStore.add(resolved.repoPath, resolved.relPath, line || undefined);
        return;
      }
      case "tuic:terminal": {
        const repoPath = typeof data.repoPath === "string" ? data.repoPath : "";
        if (!repoPath) {
          appLogger.warn("plugin", "tuic:terminal missing repoPath");
          return;
        }
        if (!(repoPath in repositoriesStore.state.repositories)) {
          appLogger.warn("plugin", `tuic:terminal repo not in repo list: ${repoPath}`);
          return;
        }
        const count = terminalsStore.getCount();
        const id = terminalsStore.add({
          sessionId: null,
          fontSize: settingsStore.state.defaultFontSize,
          name: `Terminal ${count + 1}`,
          cwd: repoPath,
          awaitingInput: null,
        });
        assignTabToActiveGroup(id, "terminal");
        terminalsStore.setActive(id);
        return;
      }
      default:
        appLogger.warn("plugin", `Unknown tuic SDK command: ${data.type}`);
    }
  };

  // Handle messages from the iframe
  const handleMessage = (event: MessageEvent) => {
    // Only process messages from our iframe
    if (!iframeRef || event.source !== iframeRef.contentWindow) return;

    const data = event.data;
    if (!data || typeof data !== "object") return;

    // System message: close-panel (backward compatible)
    if (data.type === "close-panel" && data.pluginId === props.tab.pluginId) {
      props.onClose?.();
      return;
    }

    // TUIC SDK messages — handled by the host, never forwarded to plugins
    if (typeof data.type === "string" && data.type.startsWith("tuic:")) {
      handleTuicMessage(data);
      return;
    }

    // Route all other messages to the plugin's onMessage handler
    pluginRegistry.handlePanelMessage(props.tab.id, data);
  };

  // Register message listener and send channel on mount; clean up on unmount
  onMount(() => {
    window.addEventListener("message", handleMessage);
    onCleanup(() => window.removeEventListener("message", handleMessage));

    const tabId = props.tab.id;
    pluginRegistry.registerPanelSendChannel(tabId, (data: unknown) => {
      if (iframeRef?.contentWindow) {
        // srcdoc iframes have an opaque ("null") origin — use "*" but rely on
        // event.source === iframeRef.contentWindow check in handleMessage above
        // to ensure only our iframe receives the message.
        iframeRef.contentWindow.postMessage(data, "*");
      }
    });
    onCleanup(() => pluginRegistry.unregisterPanelSendChannel(tabId));
  });

  const [srcdoc, setSrcdoc] = createSignal<string>("");

  // Inline HTML mode: inject theme vars, base styles, and SDK into srcdoc.
  // URL mode: load directly via src= so the page keeps its own CSP
  // (srcdoc inherits the parent's Tauri CSP, which blocks external resources).
  createEffect(() => {
    if (!props.tab.url) {
      setSrcdoc(injectThemeVars(props.tab.html));
    }
  });

  const iframeStyle = {
    width: "100%",
    height: "100%",
    border: "none",
    background: "transparent",
  };

  return (
    <div style={{
      width: "100%",
      height: "100%",
      display: "flex",
      "flex-direction": "column",
    }}>
      {props.tab.url ? (
        <iframe
          ref={iframeRef}
          src={props.tab.url}
          sandbox="allow-scripts allow-same-origin"
          onLoad={sendSdkInit}
          style={iframeStyle}
        />
      ) : (
        <iframe
          ref={iframeRef}
          sandbox="allow-scripts"
          srcdoc={srcdoc()}
          style={iframeStyle}
        />
      )}
    </div>
  );
};

export default PluginPanel;
