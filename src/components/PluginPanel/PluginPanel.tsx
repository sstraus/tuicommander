import { Component, createEffect, onCleanup } from "solid-js";
import type { PluginPanelTab } from "../../stores/mdTabs";

export interface PluginPanelProps {
  tab: PluginPanelTab;
  onClose?: () => void;
}

/**
 * Renders plugin HTML content in a sandboxed iframe.
 *
 * Security: `sandbox="allow-scripts"` without `allow-same-origin` ensures
 * the iframe cannot access Tauri IPC, the parent window, or same-origin
 * resources. Communication happens via postMessage bridge only.
 */
export const PluginPanel: Component<PluginPanelProps> = (props) => {
  let iframeRef: HTMLIFrameElement | undefined;

  // Handle messages from the iframe
  const handleMessage = (event: MessageEvent) => {
    // Only process messages from our iframe
    if (!iframeRef || event.source !== iframeRef.contentWindow) return;

    const data = event.data;
    if (!data || typeof data !== "object" || data.pluginId !== props.tab.pluginId) return;

    // Plugins can send messages back to the host via postMessage
    // For now, the only action is "close" to close the panel
    if (data.type === "close-panel") {
      props.onClose?.();
    }
  };

  window.addEventListener("message", handleMessage);
  onCleanup(() => window.removeEventListener("message", handleMessage));

  // Update iframe content when HTML changes
  createEffect(() => {
    const html = props.tab.html;
    if (iframeRef) {
      iframeRef.srcdoc = html;
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
        srcdoc={props.tab.html}
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
