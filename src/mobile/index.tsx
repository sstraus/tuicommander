/* @refresh reload */
import { render, ErrorBoundary } from "solid-js/web";
import MobileApp from "./MobileApp";
import { appLogger } from "../stores/appLogger";
import "../global.css";

// Global error handlers
window.addEventListener("error", (event) => {
  appLogger.error("app", `Uncaught: ${event.message}`, {
    filename: event.filename,
    lineno: event.lineno,
    colno: event.colno,
  });
});

window.addEventListener("unhandledrejection", (event) => {
  const reason = event.reason instanceof Error ? event.reason.message : String(event.reason);
  const stack = event.reason instanceof Error ? event.reason.stack : undefined;
  appLogger.error("app", `Unhandled rejection: ${reason}`, stack ? { stack } : undefined);
});

const root = document.getElementById("mobile-app");

if (!root) {
  throw new Error("Root element #mobile-app not found");
}

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
      <h2 style={{ margin: "0 0 12px" }}>TUICommander Mobile crashed</h2>
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
      <MobileApp />
    </ErrorBoundary>
  ),
  root,
);
