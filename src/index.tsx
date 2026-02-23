/* @refresh reload */
import { lazy } from "solid-js";
import { render, ErrorBoundary } from "solid-js/web";
import App from "./App";
import "./global.css";
import "./styles.css";
import "@xterm/xterm/css/xterm.css";

const FloatingTerminal = lazy(() => import("./FloatingTerminal").then((m) => ({ default: m.FloatingTerminal })));

const root = document.getElementById("app");

if (!root) {
  throw new Error("Root element #app not found");
}

/** Crash screen shown when a render error kills the app */
function CrashScreen(props: { error: Error }) {
  return (
    <div style={{
      padding: "24px",
      "font-family": "monospace",
      color: "#f44",
      background: "#1e1e1e",
      height: "100vh",
      overflow: "auto",
    }}>
      <h2 style={{ margin: "0 0 12px" }}>TUICommander crashed</h2>
      <pre style={{ "white-space": "pre-wrap", color: "#ccc" }}>{props.error.message}</pre>
      <pre style={{ "white-space": "pre-wrap", color: "#888", "font-size": "12px" }}>{props.error.stack}</pre>
      <button
        style={{
          "margin-top": "16px",
          padding: "8px 16px",
          background: "#333",
          color: "#fff",
          border: "1px solid #555",
          "border-radius": "4px",
          cursor: "pointer",
        }}
        onClick={() => location.reload()}
      >
        Reload
      </button>
    </div>
  );
}

/** Route based on URL hash: #/floating renders the detached terminal window */
const isFloatingWindow = window.location.hash.startsWith("#/floating");

render(
  () => (
    <ErrorBoundary fallback={(err) => <CrashScreen error={err} />}>
      {isFloatingWindow ? <FloatingTerminal /> : <App />}
    </ErrorBoundary>
  ),
  root,
);

// Splash screen is removed inside initApp() after store hydration completes.
// This prevents a flash of empty state (e.g. "Add Repository" button) before
// persisted data has loaded.

// Suppress the native webview context menu (macOS "Reload" button) in production.
// In dev mode, keep it available for debugging convenience.
if (!import.meta.env.DEV) {
  document.addEventListener("contextmenu", (e) => {
    e.preventDefault();
  });
}

// Intercept external link clicks and open them in the system browser.
// Without this, Tauri shows a scary "WARNING: This link could potentially be
// dangerous" navigation confirmation dialog.
document.addEventListener("click", (e) => {
  const anchor = (e.target as HTMLElement).closest("a[href]") as HTMLAnchorElement | null;
  if (!anchor) return;
  const href = anchor.href;
  if (!href) return;
  // Only intercept http/https links (not internal anchors, javascript:, etc.)
  if (href.startsWith("http://") || href.startsWith("https://")) {
    e.preventDefault();
    import("./utils/openUrl").then(({ handleOpenUrl }) => handleOpenUrl(href));
  }
});

if (import.meta.env.DEV) {
  import("./dev/simulator");
}
