/* @refresh reload */
import { lazy } from "solid-js";
import { ErrorBoundary, render } from "solid-js/web";
import App from "./App";
import { CrashScreen } from "./components/CrashScreen/CrashScreen";
import { initDebugGlobals } from "./debugGlobals";
import { appLogger } from "./stores/appLogger";
import { handleExternalLinkClick } from "./utils/externalLinkClick";
import { startFrontendHeartbeat } from "./utils/frontendHeartbeat";
import "./global.css";
import "./styles.css";

// Expose debug globals for MCP eval_js introspection (works in dev and release)
initDebugGlobals();

// Prove to the backend that this thread runs. Its silence is what tells the
// diagnostics thread the WebView is blocked — see frontend_liveness.rs.
startFrontendHeartbeat();

// Global error handlers — capture uncaught errors and unhandled promise rejections
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

const FloatingTerminal = lazy(() => import("./FloatingTerminal").then((m) => ({ default: m.FloatingTerminal })));

const root = document.getElementById("app");

if (!root) {
	throw new Error("Root element #app not found");
}

/** Route based on URL hash: #/floating renders the detached terminal window */
const isFloatingWindow = window.location.hash.startsWith("#/floating");

render(
	() => (
		<ErrorBoundary fallback={(err) => <CrashScreen title="TUICommander crashed" error={err} />}>
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

// Prevent the webview from navigating to dropped files (causes white screen
// in browser fallback mode). In Tauri mode with dragDropEnabled: true the
// OS-level handler emits via `onDragDropEvent` — webview drag events still
// fire here though, so suppression is still needed to block default navigation.
document.addEventListener("dragover", (e) => {
	e.preventDefault();
});
document.addEventListener("drop", (e) => {
	e.preventDefault();
});

// Register Tauri drag-drop listener + modifier key tracking (fire-and-forget).
import("./stores/dragDrop").then(({ initDragDrop }) => initDragDrop());

// Intercept external link clicks and open them in the system browser.
document.addEventListener("click", handleExternalLinkClick);

if (import.meta.env.DEV) {
	if ("__TAURI_INTERNALS__" in window) {
		import("./dev/backendSkew").then(({ startBackendSkewWarning }) => startBackendSkewWarning());
	}
	import("./dev/simulator");
}
