import { convertFileSrc } from "@tauri-apps/api/core";
import { openPath } from "@tauri-apps/plugin-opener";
import { type Component, createEffect, createSignal, Match, onCleanup, Show, Switch } from "solid-js";
import { useRepository } from "../../hooks/useRepository";
import { invoke } from "../../invoke";
import { shortenHomePath } from "../../platform";
import { appLogger } from "../../stores/appLogger";
import { editorTabsStore } from "../../stores/editorTabs";
import { type HtmlPreviewTab as HtmlPreviewTabData, mdTabsStore } from "../../stores/mdTabs";
import { repositoriesStore } from "../../stores/repositories";
import { attachIframeKeyForwarder } from "../../utils/iframeKeyForwarder";
import { IFRAME_SCROLLBAR_STYLE, IFRAME_SEARCH_BRIDGE_SCRIPT } from "../../utils/iframeSearch";
import { isAbsolutePath, joinPath } from "../../utils/pathUtils";
import { buildSearchPattern, type SearchOptions } from "../shared/DomSearchEngine";
import e from "../shared/editor-header.module.css";
import { SearchBar } from "../shared/SearchBar";
import s from "./HtmlPreviewTab.module.css";

export interface HtmlPreviewTabProps {
	tab: HtmlPreviewTabData;
	onClose?: () => void;
}

// ---------------------------------------------------------------------------
// File type detection
// ---------------------------------------------------------------------------

type PreviewKind = "html" | "pdf" | "image" | "video" | "audio" | "text";

const IMAGE_EXTS = new Set(["png", "jpg", "jpeg", "gif", "webp", "svg", "avif", "ico", "bmp"]);
const VIDEO_EXTS = new Set(["mp4", "webm", "ogg", "mov"]);
const AUDIO_EXTS = new Set(["mp3", "wav", "flac", "aac", "ogg", "m4a"]);
const TEXT_EXTS = new Set(["txt", "json", "csv", "log", "xml", "yaml", "yml", "toml", "ini", "cfg", "conf"]);

function detectKind(fileName: string): PreviewKind {
	const ext = fileName.split(".").pop()?.toLowerCase() ?? "";
	if (ext === "html" || ext === "htm") return "html";
	if (ext === "pdf") return "pdf";
	if (IMAGE_EXTS.has(ext)) return "image";
	if (VIDEO_EXTS.has(ext)) return "video";
	if (AUDIO_EXTS.has(ext)) return "audio";
	if (TEXT_EXTS.has(ext)) return "text";
	return "text"; // fallback: render as plain text
}

/** Resolve full absolute path from tab data */
function absolutePath(tab: HtmlPreviewTabData): string {
	const root = tab.fsRoot || tab.repoPath;
	return isAbsolutePath(tab.filePath) ? tab.filePath : joinPath(root, tab.filePath);
}

export const HtmlPreviewTab: Component<HtmlPreviewTabProps> = (props) => {
	const [content, setContent] = createSignal("");
	const [loading, setLoading] = createSignal(false);
	const [error, setError] = createSignal<string | null>(null);
	const [reloadKey, setReloadKey] = createSignal(0);
	const [searchVisible, setSearchVisible] = createSignal(false);
	const [matchIndex, setMatchIndex] = createSignal(-1);
	const [matchCount, setMatchCount] = createSignal(0);
	const repo = useRepository();
	let wrapperRef: HTMLDivElement | undefined;
	let iframeRef: HTMLIFrameElement | undefined;
	let cleanupKeyForwarder: (() => void) | undefined;
	// Last search posted to the iframe, re-sent after a reload re-creates its DOM.
	let lastSearch: { term: string; opts: SearchOptions } | null = null;

	/** Post a search command into the (same-origin) preview iframe. */
	const postToIframe = (msg: Record<string, unknown>) => iframeRef?.contentWindow?.postMessage(msg, "*");

	const sendSearch = (term: string, opts: SearchOptions) => {
		const pattern = term ? buildSearchPattern(term, opts) : null;
		postToIframe({ type: "tuic:search", source: pattern?.source ?? "", flags: pattern?.flags ?? "gi" });
	};

	const handleIframeLoad = (ev: Event) => {
		cleanupKeyForwarder?.();
		const iframe = ev.target as HTMLIFrameElement;
		iframeRef = iframe;
		cleanupKeyForwarder = attachIframeKeyForwarder(iframe);
		// A reload rebuilds the iframe DOM and wipes its marks — re-run any active search.
		if (searchVisible() && lastSearch) sendSearch(lastSearch.term, lastSearch.opts);
	};

	const reloadIframe = () => {
		if (kind() === "html") {
			setReloadKey((k) => k + 1);
		} else if (iframeRef) {
			const cur = iframeRef.src;
			iframeRef.src = cur;
		}
	};

	const handleMessage = (event: MessageEvent) => {
		if (!iframeRef || event.source !== iframeRef.contentWindow) return;
		const data = event.data;
		if (!data || typeof data !== "object") return;
		switch (data.type) {
			case "tuic:reload-request":
				reloadIframe();
				break;
			case "tuic:search-open":
				setSearchVisible(true);
				break;
			case "tuic:search-result":
				setMatchCount(typeof data.count === "number" ? data.count : 0);
				setMatchIndex(typeof data.index === "number" ? data.index : -1);
				break;
		}
	};

	onCleanup(() => cleanupKeyForwarder?.());

	// Register the iframe→parent message bridge in the component owner scope
	// (not a JSX IIFE, which the renderer may hoist as static and skip) so the
	// listener and its teardown are reliably paired with mount/unmount.
	window.addEventListener("message", handleMessage);
	onCleanup(() => window.removeEventListener("message", handleMessage));

	const focusWrapper = () => requestAnimationFrame(() => wrapperRef?.focus({ preventScroll: true }));

	createEffect(() => {
		if (mdTabsStore.state.activeId === props.tab.id) focusWrapper();
	});

	// Expose openSearch so the global find shortcut (App.findInTerminal) can open
	// the SearchBar when the wrapper — not the iframe — holds focus.
	createEffect(() => {
		mdTabsStore.setHandle(props.tab.id, { openSearch: () => setSearchVisible(true) });
		onCleanup(() => mdTabsStore.clearHandle(props.tab.id));
	});

	const handleSearch = (term: string, opts: SearchOptions) => {
		lastSearch = { term, opts };
		sendSearch(term, opts);
		if (!term) {
			setMatchCount(0);
			setMatchIndex(-1);
		}
	};
	const handleSearchNext = () => postToIframe({ type: "tuic:search-next" });
	const handleSearchPrev = () => postToIframe({ type: "tuic:search-prev" });
	const handleSearchClose = () => {
		postToIframe({ type: "tuic:search-close" });
		setSearchVisible(false);
		setMatchCount(0);
		setMatchIndex(-1);
		lastSearch = null;
		focusWrapper();
	};

	const kind = () => detectKind(props.tab.fileName);

	/** Asset URL for binary files (PDF, images, video, audio), cache-busted via repo revision */
	const assetUrl = () => {
		const rev = props.tab.repoPath ? repositoriesStore.getRevision(props.tab.repoPath) : 0;
		return `${convertFileSrc(absolutePath(props.tab))}?v=${rev}`;
	};

	/** Read file content — used for HTML and text previews */
	const readFileContent = async (fsRoot: string | undefined, filePath: string): Promise<string> => {
		if (isAbsolutePath(filePath)) {
			return await invoke<string>("read_external_file", { path: filePath });
		}
		return fsRoot
			? await repo.readFile(fsRoot, filePath)
			: await invoke<string>("read_external_file", { path: filePath });
	};

	// Load content for HTML (srcdoc with search/reload injection) and plain-text previews
	createEffect(() => {
		const { repoPath, filePath, fsRoot } = props.tab;
		void (repoPath ? repositoriesStore.getRevision(repoPath) : 0);
		void reloadKey();
		const k = kind();

		if (!filePath || (k !== "text" && k !== "html")) {
			setContent("");
			return;
		}

		if (!content()) setLoading(true);
		setError(null);

		(async () => {
			try {
				let fileContent = await readFileContent(fsRoot || repoPath, filePath);
				if (k === "html") {
					const absPath = absolutePath(props.tab);
					const dirPath = absPath.substring(0, absPath.lastIndexOf("/") + 1);
					const baseTag = `<base href="${convertFileSrc(dirPath)}">`;
					fileContent = fileContent.includes("<head")
						? fileContent.replace(/<head([^>]*)>/i, `<head$1>${baseTag}`)
						: `${baseTag}${fileContent}`;
					const headClose = fileContent.indexOf("</head>");
					if (headClose >= 0) {
						fileContent =
							fileContent.slice(0, headClose) +
							IFRAME_SEARCH_BRIDGE_SCRIPT +
							IFRAME_SCROLLBAR_STYLE +
							fileContent.slice(headClose);
					} else {
						fileContent = IFRAME_SEARCH_BRIDGE_SCRIPT + IFRAME_SCROLLBAR_STYLE + fileContent;
					}
				}
				setContent(fileContent);
			} catch (err) {
				const msg = err instanceof Error ? err.message : String(err);
				appLogger.error("app", "File preview: read failed", { repoPath, filePath, error: msg });
				setError(msg);
				setContent("");
			} finally {
				setLoading(false);
			}
		})();
	});

	const handleOpenExternal = () => {
		openPath(absolutePath(props.tab)).catch((err) =>
			appLogger.error("app", "Failed to open file externally", { path: absolutePath(props.tab), error: String(err) }),
		);
	};

	const handleEdit = () => {
		const { fsRoot, repoPath, filePath } = props.tab;
		editorTabsStore.add(fsRoot || repoPath, filePath);
	};

	const displayPath = () => {
		const { fsRoot, repoPath, filePath } = props.tab;
		const root = fsRoot || repoPath;
		return isAbsolutePath(filePath) ? shortenHomePath(filePath) : joinPath(shortenHomePath(root), filePath);
	};

	return (
		<div class={s.wrapper} ref={wrapperRef} tabIndex={-1}>
			<div class={e.header}>
				<span class={e.filename} title={displayPath()}>
					{props.tab.fileName}
				</span>
				<button class={e.btn} onClick={handleEdit} title="Edit source">
					<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
						<path d="M11.13 1.47a1.5 1.5 0 0 1 2.12 0l1.28 1.28a1.5 1.5 0 0 1 0 2.12L5.9 13.5a1 1 0 0 1-.5.27l-3.5.87a.5.5 0 0 1-.6-.6l.87-3.5a1 1 0 0 1 .27-.5L11.13 1.47ZM12.2 2.53l-8.46 8.47-.58 2.34 2.34-.58 8.47-8.46-1.77-1.77Z" />
					</svg>
				</button>
				<button class={e.btn} onClick={handleOpenExternal} title="Open externally">
					<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
						<path d="M9 2h5v5l-2-2-3 3-2-2 3-3zm-3 7l-3 3 2 2H0V9l2 2 3-3z" />
					</svg>
				</button>
			</div>
			<Show when={kind() === "html"}>
				<SearchBar
					visible={searchVisible()}
					onSearch={handleSearch}
					onNext={handleSearchNext}
					onPrev={handleSearchPrev}
					onClose={handleSearchClose}
					matchIndex={matchIndex()}
					matchCount={matchCount()}
				/>
			</Show>
			<Show when={loading()}>
				<div style={{ padding: "24px", color: "var(--fg-muted)" }}>Loading...</div>
			</Show>
			<Show when={error()}>
				<div style={{ padding: "24px", color: "var(--error)" }}>{error()}</div>
			</Show>
			<Show when={!loading() && !error()}>
				<Switch>
					<Match when={kind() === "html"}>
						<Show when={content()} keyed>
							<iframe
								class={s.iframe}
								sandbox="allow-scripts allow-same-origin"
								srcdoc={content()}
								title={props.tab.fileName}
								onLoad={handleIframeLoad}
							/>
						</Show>
					</Match>
					<Match when={kind() === "pdf"}>
						<iframe class={s.iframe} src={assetUrl()} title={props.tab.fileName} onLoad={handleIframeLoad} />
					</Match>
					<Match when={kind() === "image"}>
						<div class={s.mediaContainer}>
							<img class={s.image} src={assetUrl()} alt={props.tab.fileName} />
						</div>
					</Match>
					<Match when={kind() === "video"}>
						<div class={s.mediaContainer}>
							<video class={s.media} src={assetUrl()} controls />
						</div>
					</Match>
					<Match when={kind() === "audio"}>
						<div class={s.mediaContainer}>
							<audio src={assetUrl()} controls />
						</div>
					</Match>
					<Match when={kind() === "text"}>
						<pre class={s.textContent}>{content()}</pre>
					</Match>
				</Switch>
			</Show>
		</div>
	);
};

export default HtmlPreviewTab;
