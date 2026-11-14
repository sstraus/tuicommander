import { type Component, createEffect, createSignal, onCleanup, onMount, Show, untrack } from "solid-js";
import { invoke } from "../../invoke";
import { pluginRegistry } from "../../plugins/pluginRegistry";
import { appLogger } from "../../stores/appLogger";
import { editorTabsStore } from "../../stores/editorTabs";
import type { PluginPanelTab } from "../../stores/mdTabs";
import { mdTabsStore } from "../../stores/mdTabs";
import { repositoriesStore } from "../../stores/repositories";
import { settingsStore } from "../../stores/settings";
import { terminalsStore } from "../../stores/terminals";
import { toastsStore } from "../../stores/toasts";
import { themeGeneration } from "../../themes";
import { writeClipboard } from "../../utils/clipboard";
import { classifyFile } from "../../utils/filePreview";
import { attachIframeKeyForwarder } from "../../utils/iframeKeyForwarder";
import { IFRAME_SEARCH_SCRIPT } from "../../utils/iframeSearch";
import { handleOpenUrl } from "../../utils/openUrl";
import { assignTabToActiveGroup } from "../../utils/paneTabAssign";
import { ContextMenu, createContextMenu } from "../ContextMenu/ContextMenu";
import { PLUGIN_BASE_CSS } from "./pluginBaseStyles";
import { resolveTuicPath } from "./resolveTuicPath";
import { TUIC_SDK_SCRIPT, TUIC_SDK_VERSION } from "./tuicSdk";

export interface PluginPanelProps {
	tab: PluginPanelTab;
	onClose?: () => void;
	/**
	 * Whether this panel is on screen. Hidden panels unload their iframes so
	 * their page scripts cannot block the terminal's shared WebContent thread.
	 * Absent (detached windows, previews) means always visible.
	 */
	visible?: () => boolean;
}

/** The `:root` custom properties a plugin iframe is allowed to see. */
const THEME_VAR_PREFIXES = [
	"--bg-",
	"--fg-",
	"--border",
	"--accent",
	"--error",
	"--warning",
	"--success",
	"--ring-",
	"--text-",
];

interface ThemeSnapshot {
	generation: number;
	/** `<style>:root{…}</style>` for srcdoc injection, "" when nothing matched. */
	css: string;
	/** The same values keyed as `bgPrimary` for SDK delivery. */
	object: Record<string, string>;
}

let themeCache: ThemeSnapshot | undefined;

/**
 * The app's `:root` theme variables, in both shapes a plugin needs.
 *
 * Reading them means walking every rule of every stylesheet, and the answer
 * only changes when applyAppTheme rewrites the root properties — so the walk
 * happens once per theme generation and every panel shares the result. It used
 * to run twice per update per panel, once for each shape.
 *
 * The generation is read untracked on purpose: this runs inside the srcdoc
 * effect, and tracking it there would rebuild srcdoc — a full iframe reload —
 * on every theme switch. A live panel learns about theme changes over
 * postMessage instead.
 */
function themeSnapshot(): ThemeSnapshot {
	const generation = untrack(themeGeneration);
	if (themeCache?.generation === generation) return themeCache;

	const root = getComputedStyle(document.documentElement);
	const vars: string[] = [];
	const object: Record<string, string> = {};
	for (const sheet of document.styleSheets) {
		try {
			for (const rule of sheet.cssRules) {
				if (rule instanceof CSSStyleRule && rule.selectorText === ":root") {
					for (let i = 0; i < rule.style.length; i++) {
						const prop = rule.style[i];
						if (!THEME_VAR_PREFIXES.some((p) => prop.startsWith(p))) continue;
						const value = root.getPropertyValue(prop).trim();
						vars.push(`${prop}:${value}`);
						// Convert --bg-primary to bgPrimary for JS-friendly access
						object[prop.replace(/^--/, "").replace(/-([a-z])/g, (_, c) => c.toUpperCase())] = value;
					}
				}
			}
		} catch {
			// Cross-origin stylesheets cannot be read — skip silently
		}
	}

	themeCache = {
		generation,
		css: vars.length > 0 ? `<style>:root{${vars.join(";")}}</style>` : "",
		object,
	};
	return themeCache;
}

/** Extract theme vars as a plain object for SDK delivery */
function extractThemeObject(): Record<string, string> {
	return themeSnapshot().object;
}

/**
 * Inject theme CSS variables, the base stylesheet, and the SDK/search scripts
 * into HTML before </head> (or prepend if no </head>).
 *
 * `selfStyled` decides the base sheet and is derived from the tab's SOURCE, not
 * by sniffing the HTML. A plugin dashboard legitimately ships its own
 * supplementary `<style>` for layout yet still relies on PLUGIN_BASE_CSS for
 * typography/theme; content sniffing conflated the two and dropped the base
 * sheet, rendering dashboards serif-on-white.
 *
 *  - Plugin dashboards (SDK `addPluginPanel`) → selfStyled=false → the base
 *    sheet is forced on so the shared `.dashboard`/`.dash-*` classes render.
 *  - Design/preview tabs (MCP `ui action=tab html=…` / `file://`, #080) →
 *    selfStyled=true → the base sheet is omitted so a self-styled white page
 *    keeps its own background, color and typography.
 *
 * Theme vars and the SDK/search scripts are injected in both cases — they only
 * define CSS vars / add behavior.
 */
export function injectThemeVars(html: string, selfStyled: boolean): string {
	const themeStyle = themeSnapshot().css;
	const baseStyle = selfStyled ? "" : `<style id="tuic-base">${PLUGIN_BASE_CSS}</style>`;
	const injection = baseStyle + themeStyle + TUIC_SDK_SCRIPT + IFRAME_SEARCH_SCRIPT;
	const headClose = html.indexOf("</head>");
	if (headClose >= 0) {
		return html.slice(0, headClose) + injection + html.slice(headClose);
	}
	return injection + html;
}

/**
 * Renders plugin HTML content in a sandboxed iframe.
 *
 * Security: `sandbox="allow-scripts allow-same-origin"` — plugins run in
 * a desktop app where users install them voluntarily (same trust model as
 * VS Code extensions). `allow-same-origin` is required because WKWebView
 * inherits the parent's CSP into srcdoc iframes and CSP3 ignores
 * `unsafe-inline` when source-list entries are present, which silently
 * blocks all inline scripts without it.
 *
 * CSP: The parent's CSP includes 'unsafe-inline' in script-src to allow
 * inline scripts in srcdoc iframes (inherited CSP). This is safe because
 * the main app has no user-injected HTML content (desktop app), and plugin
 * code is further isolated by the sandbox attribute.
 *
 * Message bridge: Non-system messages from the iframe are routed to the
 * plugin's onMessage callback via pluginRegistry.handlePanelMessage().
 * The plugin can send messages back via panelHandle.send().
 */
export const PluginPanel: Component<PluginPanelProps> = (props) => {
	let iframeRef: HTMLIFrameElement | undefined;
	let containerRef: HTMLDivElement | undefined;
	let cleanupKeyForwarder: (() => void) | undefined;
	const menu = createContextMenu();

	/**
	 * Force a reload of the plugin iframe. URL-mode reassigns `src` to itself
	 * (which WebKit/Chromium treat as a navigation), srcdoc-mode bumps the
	 * `reloadKey` signal driving a keyed `<Show>` so Solid unmounts/remounts
	 * the iframe element — the most reliable way to re-parse srcdoc without
	 * races (previous setSrcdoc("") → rAF → setSrcdoc(x) approach could leave
	 * the iframe permanently blank if the two writes got coalesced).
	 */
	const reloadIframe = () => {
		if (!iframeRef) return;
		if (props.tab.url && !props.tab.url.startsWith("file://")) {
			const cur = iframeRef.src;
			iframeRef.src = cur;
		} else {
			setReloadKey((k) => k + 1);
		}
	};

	/** Open the reload context menu at page-coordinates. */
	const openReloadMenu = (pageX: number, pageY: number) => {
		menu.openAt(pageX, pageY);
	};

	/** Resolve a path (absolute or relative) to repo + relPath.
	 *
	 *  A relative path is resolved against THIS panel's repo when it has one;
	 *  only an unscoped panel falls back to the active repo. Resolving against
	 *  focus alone sent a scoped panel's own links into a foreign repo. */
	const resolvePathForSdk = (path: string) => {
		const repos = Object.keys(repositoriesStore.state.repositories);
		const base = props.tab.repoPath ?? repositoriesStore.state.activeRepoPath;
		return resolveTuicPath(path, repos, base);
	};

	/**
	 * Send the SDK init handshake to the URL-mode iframe.
	 *
	 * Two call sites (see docs/tuic-sdk.md §Timing Notes):
	 *  1. iframe `onLoad` — primary path for child pages with a synchronous
	 *     `<head>` listener.
	 *  2. In response to `tuic:sdk-request` from the child — fallback for
	 *     child pages whose listener registers asynchronously (ES modules,
	 *     frameworks that mount after DOMContentLoaded). Without this, the
	 *     onLoad message would fire before the listener exists and be lost.
	 */
	const sendToIframe = (data: Record<string, unknown>) => {
		iframeRef?.contentWindow?.postMessage(data, "*");
	};

	/** Inject SDK + search scripts into a URL-mode iframe (same-origin only) */
	const injectSdkIntoUrlIframe = () => {
		try {
			const doc = iframeRef?.contentDocument;
			if (doc && !doc.getElementById("tuic-sdk")) {
				const range = doc.createRange();
				range.selectNode(doc.head || doc.documentElement);
				const frag = range.createContextualFragment(TUIC_SDK_SCRIPT + IFRAME_SEARCH_SCRIPT);
				(doc.head || doc.documentElement).appendChild(frag);
			}
		} catch {
			// Cross-origin: cannot access contentDocument — fall back to postMessage handshake
		}
	};

	const installKeyForwarder = () => {
		cleanupKeyForwarder?.();
		cleanupKeyForwarder = undefined;
		if (iframeRef) {
			cleanupKeyForwarder = attachIframeKeyForwarder(iframeRef);
		}
	};

	const sendSdkInit = () => {
		injectSdkIntoUrlIframe();
		installKeyForwarder();
		sendToIframe({ type: "tuic:sdk-init", version: TUIC_SDK_VERSION });
		sendToIframe({ type: "tuic:repo-changed", repoPath: repositoriesStore.state.activeRepoPath ?? null });
		sendToIframe({ type: "tuic:theme-changed", theme: extractThemeObject() });
	};

	// URL-mode iframes are cross-origin by design. If after a load event
	// contentDocument becomes accessible, the iframe navigated to the
	// TUIC app origin (e.g. via a relative link) — reset to about:blank.
	const guardSameOriginNav = () => {
		if (!iframeRef || !props.tab.url || props.tab.url.startsWith("file://")) return;
		try {
			if (iframeRef.contentDocument) {
				// about:blank is same-origin too — it's our OWN reset target, not an
				// escape. Without this check the guard re-fires on the about:blank load
				// it just triggered, looping (reset → load → guard → reset → …).
				if (iframeRef.contentDocument.location?.href === "about:blank") return;
				appLogger.error("plugin", `Panel '${props.tab.id}' navigated to app origin — blocked`);
				iframeRef.src = "about:blank";
			}
		} catch {
			// Cross-origin — expected
		}
	};

	/** Handle tuic:* SDK messages from the iframe */
	const handleTuicMessage = (data: Record<string, unknown>) => {
		switch (data.type) {
			case "tuic:sdk-request": {
				// Fallback handshake: child page's listener was not ready when
				// iframe onLoad fired; it re-requests init. Respond idempotently.
				sendSdkInit();
				return;
			}
			case "tuic:open": {
				const path = typeof data.path === "string" ? data.path : "";
				if (!path) {
					appLogger.warn("plugin", "tuic:open missing path");
					return;
				}
				const resolved = resolvePathForSdk(path);
				if (!resolved) {
					appLogger.warn("plugin", `tuic:open cannot resolve path: ${path}`);
					return;
				}
				// Images and other media go to the preview tab: the markdown reader
				// reads UTF-8 text and fails on binary files.
				const tabId =
					classifyFile(resolved.relPath) === "preview"
						? mdTabsStore.addHtmlPreview(resolved.repoPath, resolved.relPath)
						: mdTabsStore.add(resolved.repoPath, resolved.relPath);
				if (data.pinned) mdTabsStore.setPinned(tabId, true);
				return;
			}
			case "tuic:open-url": {
				// URL-mode pages can post arbitrary messages without a user click.
				if (props.tab.url && !props.tab.url.startsWith("file://")) return;
				if (typeof data.url === "string") handleOpenUrl(data.url);
				return;
			}
			case "tuic:edit": {
				const path = typeof data.path === "string" ? data.path : "";
				if (!path) {
					appLogger.warn("plugin", "tuic:edit missing path");
					return;
				}
				const resolved = resolvePathForSdk(path);
				if (!resolved) {
					appLogger.warn("plugin", `tuic:edit cannot resolve path: ${path}`);
					return;
				}
				const line = typeof data.line === "number" ? data.line : 0;
				editorTabsStore.add(
					resolved.repoPath,
					resolved.relPath,
					line || undefined,
					resolved.external ? { externalEditable: true } : undefined,
				);
				return;
			}
			case "tuic:terminal": {
				const repoPath = typeof data.repoPath === "string" ? data.repoPath : "";
				if (!repoPath) {
					appLogger.warn("plugin", "tuic:terminal missing repoPath");
					return;
				}
				if (!(repoPath in repositoriesStore.state.repositories)) {
					appLogger.warn("plugin", `tuic:terminal repo not in repo list: ${repoPath}`);
					return;
				}
				const id = terminalsStore.add({
					sessionId: null,
					fontSize: settingsStore.state.defaultFontSize,
					name: terminalsStore.nextDefaultName(),
					cwd: repoPath,
					awaitingInput: null,
				});
				assignTabToActiveGroup(id, "terminal");
				terminalsStore.setActive(id);
				return;
			}
			case "tuic:toast": {
				const title = typeof data.title === "string" ? data.title : "";
				if (!title) {
					appLogger.warn("plugin", "tuic:toast missing title");
					return;
				}
				const message = typeof data.message === "string" ? data.message : "";
				const level = data.level === "warn" || data.level === "error" ? data.level : "info";
				const sound = data.sound === true;
				toastsStore.add(title, message, level, sound);
				return;
			}
			case "tuic:clipboard": {
				const text = typeof data.text === "string" ? data.text : "";
				writeClipboard(text).catch((err) => {
					appLogger.warn("plugin", `tuic:clipboard failed: ${err}`);
				});
				return;
			}
			case "tuic:get-file": {
				const path = typeof data.path === "string" ? data.path : "";
				const requestId = data.requestId;
				if (!path || requestId == null) {
					appLogger.warn("plugin", "tuic:get-file missing path or requestId");
					return;
				}
				const resolved = resolvePathForSdk(path);
				// fs_read_file is repo-scoped; external reads are not part of this surface.
				if (!resolved || resolved.external) {
					sendToIframe({ type: "tuic:get-file-result", requestId, error: `Cannot resolve path: ${path}` });
					return;
				}
				invoke<string>("fs_read_file", { repoPath: resolved.repoPath, file: resolved.relPath })
					.then((content) => sendToIframe({ type: "tuic:get-file-result", requestId, content }))
					.catch((err) => sendToIframe({ type: "tuic:get-file-result", requestId, error: String(err) }));
				return;
			}
			case "tuic:plugin-message": {
				pluginRegistry.handlePanelMessage(props.tab.id, data.payload);
				return;
			}
			case "tuic:reload-request": {
				reloadIframe();
				return;
			}
			case "tuic:context-menu": {
				// Iframe-local coordinates from the SDK → translate to page coords
				// via the iframe's current bounding rect.
				if (!iframeRef) return;
				const rect = iframeRef.getBoundingClientRect();
				const x = rect.left + (typeof data.x === "number" ? data.x : 0);
				const y = rect.top + (typeof data.y === "number" ? data.y : 0);
				openReloadMenu(x, y);
				return;
			}
			default:
				appLogger.warn("plugin", `Unknown tuic SDK command: ${data.type}`);
		}
	};

	// Handle messages from the iframe
	const handleMessage = (event: MessageEvent) => {
		// Only process messages from our iframe
		if (!iframeRef || event.source !== iframeRef.contentWindow) return;

		const data = event.data;
		if (!data || typeof data !== "object") return;

		// System message: close-panel (backward compatible)
		if (data.type === "close-panel" && data.pluginId === props.tab.pluginId) {
			props.onClose?.();
			return;
		}

		// TUIC SDK messages — handled by the host, never forwarded to plugins
		if (typeof data.type === "string" && data.type.startsWith("tuic:")) {
			handleTuicMessage(data);
			return;
		}

		// Route all other messages to the plugin's onMessage handler
		pluginRegistry.handlePanelMessage(props.tab.id, data);
	};

	// Register message listener and send channel on mount; clean up on unmount
	onMount(() => {
		window.addEventListener("message", handleMessage);
		onCleanup(() => {
			window.removeEventListener("message", handleMessage);
			cleanupKeyForwarder?.();
		});

		const tabId = props.tab.id;
		pluginRegistry.registerPanelSendChannel(tabId, (data: unknown, transfer?: Transferable[]) => {
			if (iframeRef?.contentWindow) {
				// srcdoc iframes have an opaque ("null") origin — use "*" but rely on
				// event.source === iframeRef.contentWindow check in handleMessage above
				// to ensure only our iframe receives the message.
				iframeRef.contentWindow.postMessage({ type: "tuic:host-message", payload: data }, "*", transfer ?? []);
			}
		});
		onCleanup(() => pluginRegistry.unregisterPanelSendChannel(tabId));
	});

	/** Whether this panel is the one the user is looking at. */
	const isVisible = () => props.visible?.() ?? true;

	// Let the owning plugin see what the host sees, so it can skip a render that
	// nobody would look at and catch up when the panel comes back.
	createEffect(() => pluginRegistry.setPanelVisible(props.tab.id, isVisible()));

	// Broadcast active repo changes to the iframe.
	//
	// Both effects below read their source BEFORE bailing out on visibility, so
	// the bail-out tracks it too: showing the panel re-runs the effect and hands
	// over the value current at that moment. A hidden panel therefore costs
	// nothing and still can never be left displaying a stale repo or theme — and
	// it is handed one value, not a replay of every change it slept through.
	createEffect(() => {
		const repoPath = repositoriesStore.state.activeRepoPath ?? null;
		if (!isVisible()) return;
		sendToIframe({ type: "tuic:repo-changed", repoPath });
	});

	// Broadcast theme changes to the iframe
	createEffect(() => {
		// Track the generation, not the theme name: it moves on a hot-reload of the
		// same theme too, and it moves *after* the new values are on the root — so
		// the snapshot this effect reads can never be the outgoing theme's.
		void themeGeneration();
		if (!isVisible()) return;
		sendToIframe({ type: "tuic:theme-changed", theme: extractThemeObject() });
	});

	const [srcdoc, setSrcdoc] = createSignal<string>("");
	/** Bumped on reload — drives keyed <Show> to remount the iframe element. */
	const [reloadKey, setReloadKey] = createSignal<number>(1);
	// Inline HTML mode: inject theme vars, base styles, and SDK into srcdoc.
	// URL mode: load directly via src= so the page keeps its own CSP
	// (srcdoc inherits the parent's Tauri CSP, which blocks external resources).
	// file:// URLs: read via IPC and convert to srcdoc (sandbox blocks file://).
	//
	// Writing srcdoc NAVIGATES the iframe: new document, new JS global, and the
	// scroll position, focus and in-page state of the old one are gone. Two
	// equality checks keep an unchanged render from paying that — the store skips
	// a `tabs[id].html` write to the same string, and this signal skips an
	// identical srcdoc — and PluginPanel.test.tsx locks both in.
	//
	// The host deliberately does NOT try to swap the body in place instead. It
	// cannot do that transparently: innerHTML never runs the <script> tags most
	// dashboards ship in their body, and it destroys the elements a head script
	// attached its listeners to. A plugin that wants an incremental update
	// already has the channel for it — panelHandle.send() to a listener in its
	// own page, which knows what changed and what to keep.
	createEffect(() => {
		const url = props.tab.url;
		if (url?.startsWith("file://")) {
			const filePath = decodeURIComponent(url.replace(/^file:\/\//, ""));
			const dirPath = filePath.substring(0, filePath.lastIndexOf("/") + 1);
			invoke<string>("read_external_file", { path: filePath })
				.then((content) => {
					const baseTag = `<base href="http://asset.localhost${dirPath}">`;
					const withBase = content.includes("<head")
						? content.replace(/<head([^>]*)>/i, `<head$1>${baseTag}`)
						: `${baseTag}${content}`;
					setSrcdoc(injectThemeVars(withBase, props.tab.selfStyled ?? false));
				})
				.catch((err) => {
					appLogger.error("plugin", `Failed to read file:// tab content: ${filePath}`, err);
					setSrcdoc(`<body style="color:#e55;padding:24px">Failed to load ${filePath}: ${err}</body>`);
				});
		} else if (!url) {
			setSrcdoc(injectThemeVars(props.tab.html, props.tab.selfStyled ?? false));
		}
	});

	const iframeStyle = {
		width: "100%",
		height: "100%",
		border: "none",
		background: "transparent",
	};

	return (
		<div
			ref={containerRef}
			data-focus-target="plugin-iframe"
			data-tab-id={props.tab.id}
			data-plugin-id={props.tab.pluginId}
			tabIndex={-1}
			style={{
				width: "100%",
				height: "100%",
				display: "flex",
				"flex-direction": "column",
			}}
			// Catches right-clicks on the iframe border / surrounding area; inside
			// the iframe the SDK forwards via postMessage (`tuic:context-menu`).
			onContextMenu={(e) => {
				e.preventDefault();
				openReloadMenu(e.clientX, e.clientY);
			}}
		>
			<Show when={props.visible?.() ?? true}>
				{props.tab.url && !props.tab.url.startsWith("file://") ? (
					<iframe
						ref={iframeRef}
						src={props.tab.url}
						sandbox="allow-scripts allow-same-origin"
						onLoad={() => {
							guardSameOriginNav();
							sendSdkInit();
						}}
						style={iframeStyle}
					/>
				) : (
					/* DO NOT remove allow-same-origin — WKWebView inherits the parent
           CSP into srcdoc iframes and CSP3 silently blocks ALL inline scripts
           when source-list entries coexist with 'unsafe-inline'. Without
           allow-same-origin every plugin's JS is dead (no D&D, no filters,
           no SDK). Plugins are user-installed, same trust as VS Code exts.

           Keyed <Show> on reloadKey() forces Solid to unmount/remount the
           iframe element on reload — avoids the srcdoc write races that
           can leave it blank. */
					<Show when={reloadKey()} keyed>
						<iframe
							ref={iframeRef}
							sandbox="allow-scripts allow-same-origin"
							srcdoc={srcdoc()}
							style={iframeStyle}
							onLoad={installKeyForwarder}
						/>
					</Show>
				)}
			</Show>
			<ContextMenu
				visible={menu.visible()}
				x={menu.position().x}
				y={menu.position().y}
				onClose={menu.close}
				items={[
					{ label: "Reload", shortcut: navigator.platform.includes("Mac") ? "⌘R" : "Ctrl+R", action: reloadIframe },
				]}
			/>
		</div>
	);
};

export default PluginPanel;
