import { Component } from "solid-js";
import { isTauri } from "../transport";
import { uiStore } from "../stores/ui";
import { appLogger } from "../stores/appLogger";

export interface DetachedPlaceholderProps {
  panel: string;
  windowLabel: string;
}

export const DetachedPlaceholder: Component<DetachedPlaceholderProps> = (props) => {
  const handleBringBack = async () => {
    if (!isTauri()) return;
    try {
      const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
      const detached = await WebviewWindow.getByLabel(props.windowLabel);
      if (detached) {
        await detached.destroy();
      }
    } catch (e) {
      appLogger.warn("detached-placeholder", "Failed to close detached window", { error: String(e) });
    }
    uiStore.setAiChatDetached(false);
  };

  return (
    <div style={{
      display: "flex",
      "flex-direction": "column",
      "align-items": "center",
      "justify-content": "center",
      gap: "12px",
      width: "300px",
      "min-width": "200px",
      height: "100%",
      background: "var(--bg-primary)",
      "border-left": "1px solid var(--border)",
      color: "var(--fg-secondary)",
      "font-size": "13px",
    }}>
      <svg width="24" height="24" viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.3">
        <path d="M8 2h4v4M8 6l4-4M6 3H3a1 1 0 00-1 1v7a1 1 0 001 1h7a1 1 0 001-1V8" stroke-linecap="round" stroke-linejoin="round" />
      </svg>
      <span>{props.panel} is in a separate window</span>
      <button
        onClick={handleBringBack}
        style={{
          padding: "4px 12px",
          background: "var(--bg-tertiary)",
          color: "var(--fg-primary)",
          border: "1px solid var(--border)",
          "border-radius": "4px",
          cursor: "pointer",
          "font-size": "12px",
        }}
      >
        Bring back
      </button>
    </div>
  );
};
