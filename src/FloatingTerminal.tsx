import { Component, Show, onMount, onCleanup, createEffect, createSignal } from "solid-js";
import { Terminal } from "./components/Terminal";
import { settingsStore } from "./stores/settings";
import { terminalsStore } from "./stores/terminals";
import { applyAppTheme } from "./themes";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { emitTo } from "@tauri-apps/api/event";
import { isMacOS } from "./platform";

const MIN_FONT_SIZE = 8;
const MAX_FONT_SIZE = 32;
const FONT_STEP = 2;

/** Parse URL hash params: #/floating?sessionId=...&tabId=...&name=... */
function getHashParams(): { sessionId: string; tabId: string; name: string } {
  const hash = window.location.hash;
  const queryPart = hash.split("?")[1] || "";
  const params = new URLSearchParams(queryPart);
  return {
    sessionId: params.get("sessionId") || "",
    tabId: params.get("tabId") || "",
    name: decodeURIComponent(params.get("name") || "Terminal"),
  };
}

/**
 * Minimal app rendered inside a floating (detached) terminal window.
 * Connects to an existing PTY session by sessionId — the PTY stays alive in Rust.
 */
export const FloatingTerminal: Component = () => {
  const { sessionId, tabId, name } = getHashParams();
  const [ready, setReady] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  // Validate required params before doing anything
  if (!sessionId || !tabId) {
    return (
      <div style={{
        display: "flex",
        "align-items": "center",
        "justify-content": "center",
        height: "100vh",
        color: "#f44",
        background: "#1e1e1e",
        "font-family": "monospace",
        "font-size": "14px",
        padding: "24px",
      }}>
        Missing sessionId or tabId — cannot attach to terminal.
      </div>
    );
  }

  onMount(async () => {
    // Remove the splash screen immediately (shared index.html has a #splash div
    // that is normally removed by useAppInit in the main window).
    document.getElementById("splash")?.remove();

    // Bootstrap settings so theme and fonts are available
    await settingsStore.hydrate().catch(() => {});

    // Set window title
    try {
      await getCurrentWebviewWindow().setTitle(name);
    } catch { /* ignore in tests */ }

    // Register terminal with the original tabId so Terminal component reconnects to the existing PTY
    terminalsStore.register(tabId, {
      sessionId,
      fontSize: settingsStore.state.defaultFontSize,
      name,
      cwd: null,
      awaitingInput: null,
    });
    terminalsStore.setActive(tabId);

    setReady(true);

    // Deferred fit: the Tauri window may not have stable layout when Terminal
    // first mounts.  xterm won't repaint until it receives new data, so we
    // force a fit() after the window has settled.
    setTimeout(() => {
      terminalsStore.get(tabId)?.ref?.fit();
    }, 150);
  });

  // Apply theme to the floating window
  createEffect(() => applyAppTheme(settingsStore.state.theme));

  // Keyboard shortcuts: zoom (Cmd/Ctrl +/-/0) and close (Cmd/Ctrl+W)
  onMount(() => {
    const handler = (e: KeyboardEvent) => {
      const mod = isMacOS() ? e.metaKey : e.ctrlKey;
      if (!mod) return;

      switch (e.key) {
        case "=":
        case "+": {
          e.preventDefault();
          const current = terminalsStore.get(tabId)?.fontSize ?? settingsStore.state.defaultFontSize;
          terminalsStore.setFontSize(tabId, Math.min(MAX_FONT_SIZE, current + FONT_STEP));
          break;
        }
        case "-": {
          e.preventDefault();
          const current = terminalsStore.get(tabId)?.fontSize ?? settingsStore.state.defaultFontSize;
          terminalsStore.setFontSize(tabId, Math.max(MIN_FONT_SIZE, current - FONT_STEP));
          break;
        }
        case "0": {
          e.preventDefault();
          terminalsStore.setFontSize(tabId, settingsStore.state.defaultFontSize);
          break;
        }
        case "w":
        case "W": {
          e.preventDefault();
          getCurrentWebviewWindow().close();
          break;
        }
      }
    };

    document.addEventListener("keydown", handler);
    onCleanup(() => document.removeEventListener("keydown", handler));
  });

  // Auto-close when PTY session exits
  const handleSessionExit = () => {
    setError("Session ended");
    // Brief delay so the user sees "[Process exited]" before the window closes
    setTimeout(async () => {
      try {
        await emitTo("main", "reattach-terminal", { tabId, sessionId });
      } catch { /* main window may already be gone */ }
      getCurrentWebviewWindow().close().catch(() => {});
    }, 1500);
  };

  // Notify main window on close so it can reattach the tab
  onMount(() => {
    const win = getCurrentWebviewWindow();
    let unlistenClose: (() => void) | undefined;

    win.onCloseRequested(async () => {
      await emitTo("main", "reattach-terminal", { tabId, sessionId });
    }).then((unlisten) => {
      unlistenClose = unlisten;
    }).catch(() => {});

    onCleanup(() => unlistenClose?.());
  });

  return (
    <div style={{
      width: "100%",
      height: "100vh",
      background: "var(--bg-primary, #1e1e1e)",
      overflow: "hidden",
    }}>
      <Show when={ready()}>
        <Terminal
          id={tabId}
          alwaysVisible
          onFocus={() => {}}
          onSessionExit={handleSessionExit}
        />
      </Show>
      <Show when={error()}>
        <div style={{
          position: "absolute",
          bottom: "8px",
          left: "0",
          right: "0",
          "text-align": "center",
          color: "#848d97",
          "font-family": "monospace",
          "font-size": "12px",
        }}>
          {error()} — closing...
        </div>
      </Show>
    </div>
  );
};
