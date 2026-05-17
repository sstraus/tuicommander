import { emitTo } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { type Component, createEffect, createSignal, onCleanup, onMount, Show } from "solid-js";
import { Terminal } from "./components/Terminal";
import { IconReattach } from "./components/ui/PanelWindowControls";
import { isMacOS } from "./platform";
import { appLogger } from "./stores/appLogger";
import { settingsStore } from "./stores/settings";
import { terminalsStore } from "./stores/terminals";
import { applyAppTheme, applyFontFamily, listenForThemeChanges, loadThemes, themesLoaded } from "./themes";

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
			<div
				style={{
					display: "flex",
					"align-items": "center",
					"justify-content": "center",
					height: "100vh",
					color: "#f44",
					background: "#1e1e1e",
					"font-family": "monospace",
					"font-size": "14px",
					padding: "24px",
				}}
			>
				Missing sessionId or tabId — cannot attach to terminal.
			</div>
		);
	}

	onMount(async () => {
		// Remove the splash screen immediately (shared index.html has a #splash div
		// that is normally removed by useAppInit in the main window).
		document.getElementById("splash")?.remove();

		// Bootstrap settings and themes so theme and fonts are available
		await settingsStore
			.hydrate()
			.catch((e) => appLogger.warn("settings", "Failed to hydrate floating terminal settings", { error: String(e) }));
		await loadThemes();
		void listenForThemeChanges();

		// Set window title
		try {
			await getCurrentWebviewWindow().setTitle(name);
		} catch {
			/* ignore in tests */
		}

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

	// Apply theme to the floating window (guard: skip until loadThemes populates the Map)
	createEffect(() => {
		if (themesLoaded()) applyAppTheme(settingsStore.state.theme);
	});

	// Sync --font-mono CSS variable when font selection changes
	createEffect(() => applyFontFamily(settingsStore.state.font));

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
			} catch {
				/* main window may already be gone */
			}
			getCurrentWebviewWindow()
				.close()
				.catch(() => {});
		}, 1500);
	};

	// Notify main window on close so it can reattach the tab
	onMount(() => {
		const win = getCurrentWebviewWindow();
		let unlistenClose: (() => void) | undefined;

		win
			.onCloseRequested(async () => {
				await emitTo("main", "reattach-terminal", { tabId, sessionId });
			})
			.then((unlisten) => {
				unlistenClose = unlisten;
			})
			.catch((e) => appLogger.error("terminal", "Failed to close floating terminal", { error: String(e) }));

		onCleanup(() => unlistenClose?.());
	});

	const terminal = () => terminalsStore.get(tabId);
	const isBusy = () => terminalsStore.isBusy(tabId);
	const shellState = () => terminal()?.shellState;
	const awaitingInput = () => terminal()?.awaitingInput;

	const statusColor = () => {
		if (awaitingInput()) return "var(--warning, #d29922)";
		if (isBusy()) return "var(--activity, #58a6ff)";
		if (shellState() === "idle") return "var(--success, #3fb950)";
		if (shellState() === "exited") return "var(--text-muted, #666)";
		return "var(--text-secondary, #848d97)";
	};

	const statusLabel = () => {
		if (awaitingInput() === "question") return "Waiting for input";
		if (awaitingInput() === "error") return "Error";
		if (awaitingInput()) return "Awaiting input";
		if (isBusy()) return "Running";
		if (shellState() === "idle") return "Idle";
		if (shellState() === "exited") return "Exited";
		return "";
	};

	return (
		<div
			style={{
				width: "100%",
				height: "100vh",
				display: "flex",
				"flex-direction": "column",
				background: "var(--bg-primary, #1e1e1e)",
				overflow: "hidden",
			}}
		>
			<Show when={ready()}>
				<div
					style={{
						display: "flex",
						"align-items": "center",
						gap: "6px",
						padding: "3px 12px",
						"font-size": "11px",
						"font-family": "var(--font-mono, monospace)",
						color: "var(--text-secondary, #848d97)",
						background: "var(--bg-secondary, #161b22)",
						"border-bottom": "1px solid var(--border, #30363d)",
						"-webkit-app-region": "drag",
						"min-height": "20px",
					}}
				>
					<span style={{ color: statusColor(), "font-size": "8px" }}>●</span>
					<span style={{ color: statusColor() }}>{statusLabel()}</span>
					<span style={{ flex: "1" }} />
					<button
						type="button"
						onClick={async () => {
							try {
								await emitTo("main", "reattach-terminal", { tabId, sessionId });
							} catch {
								/* main window may already be gone */
							}
							getCurrentWebviewWindow()
								.close()
								.catch(() => {});
						}}
						title="Bring back to main window"
						style={{
							"-webkit-app-region": "no-drag",
							background: "none",
							border: "1px solid var(--border, #30363d)",
							"border-radius": "3px",
							color: "var(--text-secondary, #848d97)",
							padding: "2px 4px",
							cursor: "pointer",
							display: "inline-flex",
							"align-items": "center",
						}}
					>
						<IconReattach />
					</button>
				</div>
				<div style={{ flex: "1", "min-height": "0" }}>
					<Terminal id={tabId} alwaysVisible onFocus={() => {}} onSessionExit={handleSessionExit} />
				</div>
			</Show>
			<Show when={error()}>
				<div
					style={{
						position: "absolute",
						bottom: "8px",
						left: "0",
						right: "0",
						"text-align": "center",
						color: "#848d97",
						"font-family": "monospace",
						"font-size": "12px",
					}}
				>
					{error()} — closing...
				</div>
			</Show>
		</div>
	);
};
