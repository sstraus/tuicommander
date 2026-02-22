import { Component, Show, onMount, onCleanup, createEffect, createSignal } from "solid-js";
import { Terminal } from "./components/Terminal";
import { settingsStore } from "./stores/settings";
import { terminalsStore } from "./stores/terminals";
import { applyAppTheme } from "./themes";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { emitTo } from "@tauri-apps/api/event";

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

  onMount(async () => {
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
  });

  // Apply theme to the floating window
  createEffect(() => applyAppTheme(settingsStore.state.theme));

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
          onFocus={() => {}}
        />
      </Show>
    </div>
  );
};
