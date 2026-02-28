import { Component, createEffect, onCleanup, onMount } from "solid-js";
import type { PluginPanelTab } from "../../stores/mdTabs";
import { pluginRegistry } from "../../plugins/pluginRegistry";

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

/** Inject theme CSS variables into HTML before </head> (or prepend if no </head>) */
function injectThemeVars(html: string): string {
  const themeStyle = extractThemeVars();
  if (!themeStyle) return html;
  const headClose = html.indexOf("</head>");
  if (headClose >= 0) {
    return html.slice(0, headClose) + themeStyle + html.slice(headClose);
  }
  return themeStyle + html;
}

/**
 * Renders plugin HTML content in a sandboxed iframe.
 *
 * Security: `sandbox="allow-scripts"` without `allow-same-origin` ensures
 * the iframe cannot access Tauri IPC, the parent window, or same-origin
 * resources. Communication happens via postMessage bridge only.
 *
 * Message bridge: Non-system messages from the iframe are routed to the
 * plugin's onMessage callback via pluginRegistry.handlePanelMessage().
 * The plugin can send messages back via panelHandle.send().
 */
export const PluginPanel: Component<PluginPanelProps> = (props) => {
  let iframeRef: HTMLIFrameElement | undefined;

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

    // Route all other messages to the plugin's onMessage handler
    pluginRegistry.handlePanelMessage(props.tab.id, data);
  };

  window.addEventListener("message", handleMessage);
  onCleanup(() => window.removeEventListener("message", handleMessage));

  // Register send channel so plugin can send messages to this iframe
  onMount(() => {
    const tabId = props.tab.id;
    pluginRegistry.registerPanelSendChannel(tabId, (data: unknown) => {
      if (iframeRef?.contentWindow) {
        iframeRef.contentWindow.postMessage(data, "*");
      }
    });
    onCleanup(() => pluginRegistry.unregisterPanelSendChannel(tabId));
  });

  // Update iframe content when HTML changes (with theme injection)
  createEffect(() => {
    const html = props.tab.html;
    if (iframeRef) {
      iframeRef.srcdoc = injectThemeVars(html);
    }
  });

  return (
    <div style={{
      width: "100%",
      height: "100%",
      display: "flex",
      "flex-direction": "column",
    }}>
      <iframe
        ref={iframeRef}
        sandbox="allow-scripts"
        srcdoc={injectThemeVars(props.tab.html)}
        style={{
          width: "100%",
          height: "100%",
          border: "none",
          background: "transparent",
        }}
      />
    </div>
  );
};

export default PluginPanel;
