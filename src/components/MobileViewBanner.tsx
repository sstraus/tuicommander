import { createSignal, Show } from "solid-js";
import { isTauri } from "../transport";

const DISMISS_KEY = "tuic-mobile-banner-dismissed";

/** Detect mobile user agent (phones only, not tablets) */
function isMobileUA(): boolean {
  if (typeof navigator === "undefined") return false;
  return /iPhone|Android.*Mobile|webOS|iPod/i.test(navigator.userAgent);
}

/**
 * Non-intrusive banner shown in the desktop UI when accessed from a phone browser.
 * Suggests switching to /mobile. Dismissible with localStorage persistence.
 * Only renders in browser mode (never inside Tauri webview).
 */
export function MobileViewBanner() {
  const shouldShow = !isTauri() && isMobileUA() && !localStorage.getItem(DISMISS_KEY);
  const [visible, setVisible] = createSignal(shouldShow);

  const dismiss = () => {
    setVisible(false);
    localStorage.setItem(DISMISS_KEY, "1");
  };

  return (
    <Show when={visible()}>
      <div style={{
        display: "flex",
        "align-items": "center",
        "justify-content": "center",
        gap: "12px",
        padding: "8px 16px",
        background: "var(--bg-tertiary)",
        "border-bottom": "1px solid var(--border)",
        "font-size": "13px",
        color: "var(--fg-secondary)",
        "flex-shrink": "0",
      }}>
        <span>Mobile view available for a better experience on this device.</span>
        <a
          href="/mobile"
          style={{
            color: "var(--accent)",
            "text-decoration": "none",
            "font-weight": "500",
          }}
        >
          Switch
        </a>
        <button
          onClick={dismiss}
          aria-label="Dismiss mobile banner"
          style={{
            background: "none",
            border: "none",
            color: "var(--fg-muted)",
            cursor: "pointer",
            padding: "2px 6px",
            "font-size": "16px",
            "line-height": "1",
          }}
        >
          &times;
        </button>
      </div>
    </Show>
  );
}
