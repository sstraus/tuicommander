/* @refresh reload */
import { render, ErrorBoundary } from "solid-js/web";
import App from "./App";
import "./styles.css";
import "@xterm/xterm/css/xterm.css";

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
      <h2 style={{ margin: "0 0 12px" }}>TUI Commander crashed</h2>
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

render(
  () => (
    <ErrorBoundary fallback={(err) => <CrashScreen error={err} />}>
      <App />
    </ErrorBoundary>
  ),
  root,
);

if (import.meta.env.DEV) {
  import("./dev/simulator");
}
