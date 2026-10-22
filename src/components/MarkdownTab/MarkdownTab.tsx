import { type Component, createEffect, createMemo, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { reviewAgents as repositoryReviewAgents } from "../../../browser-extension/design-mode/reviewAgents.js";
import { usePty } from "../../hooks/usePty";
import { useRepository } from "../../hooks/useRepository";
import { t } from "../../i18n";
import { invoke } from "../../invoke";
import { markdownProviderRegistry } from "../../plugins/markdownProviderRegistry";
import { appLogger } from "../../stores/appLogger";
import { diffTabsStore } from "../../stores/diffTabs";
import { editorTabsStore } from "../../stores/editorTabs";
import { type FileTab, type MdTabData, mdTabsStore } from "../../stores/mdTabs";
import { repositoriesStore } from "../../stores/repositories";
import { terminalsStore } from "../../stores/terminals";
import { toastsStore } from "../../stores/toasts";
import { uiStore } from "../../stores/ui";
import { copyPathToClipboard } from "../../utils/clipboard";
import { openFileAction } from "../../utils/filePreview";
import { buildMarkdownReviewPrompt } from "../../utils/markdownReviewPrompt";
import { isAbsolutePath, joinPath, normalizeSep, pathDirname } from "../../utils/pathUtils";
import { consumePendingHeading, setPendingHeading } from "../../utils/pendingHeadings";
import {
	insertTweakBlockComment,
	insertTweakComment,
	OverlappingCommentError,
	parseTweakComments,
	removeTweakComment,
	type TweakComment,
	toggleCheckbox,
	updateTweakComment,
} from "../../utils/tweakComments";
import { ContextMenu, createContextMenu } from "../ContextMenu";
import type { SearchOptions } from "../shared/DomSearchEngine";
import { DomSearchEngine } from "../shared/DomSearchEngine";
import { DomSearchOverview } from "../shared/DomSearchOverview";
import e from "../shared/editor-header.module.css";
import { createSearchVisibility, SearchBar } from "../shared/SearchBar";
import { ContentRenderer } from "../ui/ContentRenderer";
import { Select } from "../ui/Select";
import { CommentOverlay } from "./CommentOverlay";
import { LiveMarkdownEditor } from "./LiveMarkdownEditor";
import { liveModeSupported } from "./liveMarkdown";
import s from "./MarkdownTab.module.css";

export interface MarkdownTabProps {
	tab: MdTabData;
	onClose?: () => void;
	reloadToken?: () => number;
	onOpenFileLink?: (target: MarkdownFileLink) => void;
}

/** Public handle exposed via ref for external callers (e.g. App.tsx keybinding) */
export interface MarkdownTabHandle {
	openSearch: () => void;
}

/** True when the read failed because the file no longer exists on disk. Absolute
 *  paths go through `read_external_file`, which throws (rather than returning "")
 *  for a deleted file — an expected, non-warn-worthy state for a stale tab. */
function isMissingFileError(msg: string): boolean {
	return msg.includes("No such file") || msg.includes("os error 2");
}

type MarkdownLinkTarget =
	| { kind: "heading"; anchor: string }
	| {
			kind: "file";
			absolute_path: string;
			open_path: string;
			is_directory: boolean;
			same_document: boolean;
			anchor?: string | null;
			line?: number | null;
	  }
	| { kind: "missing"; path: string }
	| { kind: "blocked"; reason: string };

export type MarkdownFileLink = Extract<MarkdownLinkTarget, { kind: "file" }>;

function headingSlug(text: string): string {
	return text
		.trim()
		.toLowerCase()
		.replace(/[^\p{L}\p{N}\s-]/gu, "")
		.replace(/\s+/g, "-");
}

export const MarkdownTab: Component<MarkdownTabProps> = (props) => {
	const [content, setContent] = createSignal("");
	const [loading, setLoading] = createSignal(false);
	const [error, setError] = createSignal<string | null>(null);
	const {
		visible: searchVisible,
		focusToken: searchFocusToken,
		open: openSearchBar,
		close: closeSearchBar,
	} = createSearchVisibility();
	const [matchIndex, setMatchIndex] = createSignal(-1);
	const [matchCount, setMatchCount] = createSignal(0);
	const [overviewFractions, setOverviewFractions] = createSignal<number[]>([]);
	const [selectedAgentSession, setSelectedAgentSession] = createSignal("");
	const [sendingChanges, setSendingChanges] = createSignal(false);
	// True once this tab wrote the file (checkbox, tweak, live edit) and no agent has been told yet.
	const [editedSinceSend, setEditedSinceSend] = createSignal(false);
	const [sentCommentIds, setSentCommentIds] = createSignal<ReadonlySet<string>>(new Set());
	const [scrollEl, setScrollEl] = createSignal<HTMLElement>();
	const repo = useRepository();
	const pty = usePty();
	const contextMenu = createContextMenu();
	let wrapperRef: HTMLDivElement | undefined;
	let contentRef: HTMLDivElement | undefined;
	// Reactive signal so CommentOverlay mounts only after the rendered element exists.
	const [overlayContentEl, setOverlayContentEl] = createSignal<HTMLDivElement | undefined>();
	let engine: DomSearchEngine | undefined;
	let lastSearchTerm = "";
	let lastSearchOpts: SearchOptions = { caseSensitive: false, regex: false, wholeWord: false };
	let searchDebounceTimer: ReturnType<typeof setTimeout> | undefined;

	// Expose openSearch for external callers
	const handle: MarkdownTabHandle = {
		openSearch: () => {
			openSearchBar();
		},
	};

	// Register handle on the mdTabsStore so App.tsx can access it
	createEffect(() => {
		mdTabsStore.setHandle(props.tab.id, handle);
		onCleanup(() => mdTabsStore.clearHandle(props.tab.id));
	});

	// When this tab is active, focus the wrapper so wheel events route by cursor
	// position rather than following xterm's retained textarea focus.
	let focusFrame: number | undefined;
	const focusWrapper = () => {
		if (focusFrame !== undefined) cancelAnimationFrame(focusFrame);
		focusFrame = requestAnimationFrame(() => {
			focusFrame = undefined;
			wrapperRef?.focus({ preventScroll: true });
		});
	};
	onCleanup(() => {
		if (focusFrame !== undefined) cancelAnimationFrame(focusFrame);
	});

	onMount(() => {
		if (mdTabsStore.state.activeId === props.tab.id) focusWrapper();
	});

	createEffect(() => {
		if (mdTabsStore.state.activeId === props.tab.id) focusWrapper();
	});

	/** Read file content — uses repo-scoped read for relative paths, external read for absolute.
	 *  Absolute paths bypass the repo security check since they're already fully qualified. */
	const readFileContent = async (fsRoot: string | undefined, filePath: string): Promise<string> => {
		// Absolute paths: always use read_external_file (no repo constraint).
		// This avoids "Access denied" when the file is outside the tab's repoPath
		// (e.g. file from a different repo opened via terminal link click).
		if (isAbsolutePath(filePath)) {
			return await invoke<string>("read_external_file", { path: filePath });
		}
		return fsRoot
			? await repo.readFile(fsRoot, filePath)
			: await invoke<string>("read_external_file", { path: filePath });
	};

	createEffect(() => {
		const tab = props.tab;

		if (tab.type === "file") {
			const { repoPath, filePath, fsRoot } = tab as FileTab;
			// Track revisions by repo path (keyed by the repo root, not the worktree)
			void (repoPath ? repositoriesStore.getRevision(repoPath) : 0);

			if (!filePath) {
				setContent("");
				return;
			}

			if (!content()) setLoading(true);
			setError(null);

			(async () => {
				try {
					const fileContent = await readFileContent(fsRoot || repoPath, filePath);
					if (!fileContent) {
						// Empty result = genuinely-empty file OR a deleted/missing file
						// (repo.readFile returns "" for both, no throw). Neither is
						// warn-worthy: empty content renders fine and a deleted file is
						// expected — matches the focus-reload effect's silent handling
						// below. This effect re-runs on every repo-revision bump, so a
						// stale tab on a deleted file would otherwise spam warnings.
						appLogger.debug("app", "readFileContent returned empty", { repoPath, filePath, fsRoot });
					}
					setContent(fileContent);
				} catch (err) {
					const msg = err instanceof Error ? err.message : String(err);
					// A deleted file is expected for a stale tab and this effect re-runs on
					// every repo-revision bump, so an ERROR log here spams. Match the
					// focus-reload effect below: silent for missing files, log anything else.
					if (!isMissingFileError(msg)) {
						appLogger.error("app", "readFileContent failed", { repoPath, filePath, fsRoot, error: msg });
					}
					setError(msg);
					setContent("");
				} finally {
					setLoading(false);
				}
			})();
		} else if (tab.type === "virtual") {
			const { contentUri } = tab;

			setLoading(true);
			setError(null);

			(async () => {
				try {
					const result = await markdownProviderRegistry.resolve(contentUri);
					if (result === null) {
						setError("Content unavailable");
						setContent("");
					} else {
						setContent(result);
					}
				} catch (err) {
					setError(String(err));
					setContent("");
				} finally {
					setLoading(false);
				}
			})();
		} else {
			setContent("");
			setLoading(false);
		}
	});

	// Re-apply search when content changes
	createEffect(() => {
		// Subscribe to content changes
		content();
		if (!searchVisible() || !lastSearchTerm) return;
		// Wait for DOM to settle after innerHTML update
		requestAnimationFrame(() => rerunSearch());
	});

	// Reload file content when this tab gains focus or the repo revision bumps
	// (catches external edits both on tab switch and while the tab is already active).
	createEffect(() => {
		const isActive = mdTabsStore.state.activeId === props.tab.id;
		if (!isActive) return;
		props.reloadToken?.();
		const tab = props.tab;
		if (tab.type !== "file" || !tab.filePath) return;
		const { filePath, fsRoot, repoPath } = tab as FileTab;
		const root = fsRoot || repoPath;

		// Subscribe to repo revision so the effect re-runs on external file changes.
		void (repoPath ? repositoriesStore.getRevision(repoPath) : 0);

		// Capture current content before the async read to avoid a stale compare.
		const currentContent = content();
		let cancelled = false;
		onCleanup(() => {
			cancelled = true;
		});

		(async () => {
			try {
				const diskContent = await readFileContent(root, filePath);
				if (!cancelled && diskContent !== currentContent) setContent(diskContent);
			} catch (err) {
				if (cancelled) return;
				const msg = err instanceof Error ? err.message : String(err);
				// Silently ignore expected errors (file deleted); log anything else.
				if (!isMissingFileError(msg)) {
					appLogger.warn("app", "focus-reload readFileContent failed", { repoPath, filePath, error: msg });
				}
			}
		})();
	});

	/** Run search with current term and options */
	function rerunSearch() {
		if (!contentRef) return;
		if (!engine) engine = new DomSearchEngine(contentRef);
		const count = engine.search(lastSearchTerm, lastSearchOpts);
		setMatchCount(count);
		setMatchIndex(count > 0 ? 0 : -1);
		const el = scrollEl();
		setOverviewFractions(el && count > 0 ? engine.matchFractions(el) : []);
	}

	const handleSearch = (term: string, opts: SearchOptions) => {
		lastSearchTerm = term;
		lastSearchOpts = opts;

		clearTimeout(searchDebounceTimer);
		if (!term) {
			engine?.clear();
			setMatchCount(0);
			setMatchIndex(-1);
			setOverviewFractions([]);
			return;
		}

		searchDebounceTimer = setTimeout(() => {
			rerunSearch();
		}, 150);
	};

	const handleSearchNext = () => {
		if (!engine || matchCount() === 0) return;
		const idx = engine.next();
		setMatchIndex(idx);
	};

	const handleSearchPrev = () => {
		if (!engine || matchCount() === 0) return;
		const idx = engine.prev();
		setMatchIndex(idx);
	};

	const handleSearchClose = () => {
		engine?.clear();
		closeSearchBar();
		setMatchCount(0);
		setMatchIndex(-1);
		setOverviewFractions([]);
		focusWrapper();
	};

	const scrollToHeading = (anchor: string) => {
		const wanted = anchor.toLowerCase();
		const heading = Array.from(contentRef?.querySelectorAll<HTMLElement>("h1,h2,h3,h4,h5,h6") ?? []).find(
			(el) => el.id.toLowerCase() === wanted || headingSlug(el.textContent ?? "") === wanted,
		);
		heading?.scrollIntoView({ block: "start" });
	};

	createEffect(() => {
		const tab = props.tab;
		const body = content();
		if (tab.type !== "file" || !body || loading() || mdTabsStore.state.activeId !== tab.id) return;
		const root = tab.fsRoot || tab.repoPath;
		const absolute = normalizeSep(isAbsolutePath(tab.filePath) ? tab.filePath : joinPath(root, tab.filePath));
		const anchor = consumePendingHeading(absolute);
		if (!anchor) return;
		requestAnimationFrame(() => scrollToHeading(anchor));
	});

	const handleMdLink = async (href: string) => {
		const tab = props.tab;
		if (tab.type !== "file") return;
		const ft = tab as FileTab;
		const root = ft.fsRoot || ft.repoPath;
		try {
			const resolved = await invoke<MarkdownLinkTarget>("resolve_markdown_link", {
				root,
				currentFile: ft.filePath,
				href,
			});
			if (resolved.kind === "heading") {
				scrollToHeading(resolved.anchor);
				return;
			}
			if (resolved.kind === "missing") {
				toastsStore.add("File not found", `File not found: ${resolved.path}`, "error");
				return;
			}
			if (resolved.kind === "blocked") {
				appLogger.debug("app", "Blocked Markdown link", { href, reason: resolved.reason });
				toastsStore.add("Could not open link", resolved.reason, "error");
				return;
			}
			if (resolved.anchor && resolved.same_document && resolved.line == null) {
				scrollToHeading(resolved.anchor);
				return;
			}
			if (props.onOpenFileLink) {
				props.onOpenFileLink(resolved);
				return;
			}
			if (resolved.is_directory) {
				uiStore.setFileBrowserExternalRoot(resolved.absolute_path);
				uiStore.setFileBrowserPanelVisible(true);
				return;
			}
			setPendingHeading(root, resolved);
			openFileAction(resolved.open_path, ft.repoPath, ft.fsRoot, resolved.line);
		} catch (err) {
			appLogger.error("app", "Markdown link target lookup failed", { href, error: String(err) });
			toastsStore.add("Could not open link", href, "error");
		}
	};

	/** Write the updated markdown source back to disk and refresh displayed content. */
	const writeTweakedSource = async (updatedContent: string): Promise<boolean> => {
		const tab = props.tab;
		if (tab.type !== "file") return false;
		const ft = tab as FileTab;
		const root = ft.fsRoot || ft.repoPath;

		try {
			if (isAbsolutePath(ft.filePath)) {
				await invoke<void>("write_external_file", { path: ft.filePath, content: updatedContent });
			} else if (root) {
				await invoke<void>("write_file", { repoPath: root, file: ft.filePath, content: updatedContent });
			} else {
				appLogger.error("app", "writeTweakedSource: cannot resolve write target", { filePath: ft.filePath });
				toastsStore.add("Couldn't save Markdown file", "The file path could not be resolved.", "error");
				return false;
			}
			setContent(updatedContent);
			setEditedSinceSend(true);
			return true;
		} catch (err) {
			appLogger.error("app", "writeTweakedSource: write failed", err);
			toastsStore.add("Couldn't save Markdown file", err instanceof Error ? err.message : String(err), "error");
			return false;
		}
	};

	const handleTweakSave = async (comment: TweakComment, occurrenceIndex: number) => {
		const current = content();
		// If the id already exists in the source, it's an edit; otherwise it's a new insert.
		const isExisting =
			current.includes(`<!--tweak:begin:${comment.id}-->`) ||
			current.includes(`<!--tweak:block:${comment.id} `) ||
			current.includes(`<!--tweak:item:${comment.id} `);
		try {
			const updated = isExisting
				? updateTweakComment(current, comment.id, comment.comment)
				: insertTweakComment(current, comment, occurrenceIndex);
			return await writeTweakedSource(updated);
		} catch (err) {
			appLogger.error("app", `handleTweakSave failed: ${err instanceof Error ? err.message : String(err)}`);
			if (err instanceof OverlappingCommentError) {
				toastsStore.add(
					t("markdownTab.commentOverlap", "Couldn't add comment"),
					t(
						"markdownTab.commentOverlapMsg",
						"That text already has a comment. Click the highlight to edit it, or select different text.",
					),
					"error",
				);
			} else {
				toastsStore.add(
					t("markdownTab.commentAnchorFailed", "Couldn't add comment"),
					t(
						"markdownTab.commentAnchorFailedMsg",
						"The selected text couldn't be located in the file source. Try selecting within a single formatted span.",
					),
					"error",
				);
			}
			return false;
		}
	};

	const handleTweakBlockSave = async (comment: TweakComment, range: { start: number; end: number }) => {
		try {
			// `comment.highlighted` is the block source when the popover opened; the
			// insert refuses the range if the file changed under it since then.
			const updated = insertTweakBlockComment(content(), comment, range);
			return await writeTweakedSource(updated);
		} catch (err) {
			appLogger.error("app", `handleTweakBlockSave failed: ${err instanceof Error ? err.message : String(err)}`);
			toastsStore.add(
				t("markdownTab.commentAnchorFailed", "Couldn't add comment"),
				t("markdownTab.commentBlockChanged", "The Markdown block changed before the comment was saved. Try again."),
				"error",
			);
			return false;
		}
	};

	const handleCheckboxToggle = async (sourceLine: number, mark: " " | "x" | "~", sourceCol?: number) => {
		const updated = toggleCheckbox(content(), sourceLine, mark, sourceCol);
		await writeTweakedSource(updated);
	};

	const handleTweakDelete = async (id: string) => {
		try {
			const updated = removeTweakComment(content(), id);
			return await writeTweakedSource(updated);
		} catch (err) {
			appLogger.error("app", "handleTweakDelete failed", err);
			toastsStore.add("Couldn't delete comment", err instanceof Error ? err.message : String(err), "error");
			return false;
		}
	};

	const [liveMode, setLiveMode] = createSignal(false);
	const [liveDirty, setLiveDirty] = createSignal(false);
	const liveAvailable = () => props.tab.type === "file" && liveModeSupported(content());
	const toggleLive = () => {
		if (liveMode() && liveDirty() && !window.confirm(t("markdownTab.liveDiscard", "Discard unsaved changes?"))) return;
		setLiveDirty(false);
		setLiveMode(!liveMode());
	};

	const handleEdit = () => {
		const tab = props.tab;
		if (tab.type === "file") {
			const ft = tab as FileTab;
			editorTabsStore.add(ft.fsRoot || ft.repoPath, ft.filePath);
		}
	};

	const displayPath = () => {
		const tab = props.tab;
		return tab.type === "file" ? tab.filePath : tab.title;
	};

	const baseDir = () => {
		const tab = props.tab;
		if (tab.type !== "file") return undefined;
		const ft = tab as FileTab;
		const root = ft.fsRoot || ft.repoPath;
		if (isAbsolutePath(ft.filePath)) {
			return pathDirname(ft.filePath) || "/";
		}
		const dir = pathDirname(ft.filePath);
		return dir ? joinPath(root, dir) : root;
	};

	const fullPath = () => {
		const tab = props.tab;
		if (tab.type !== "file") return null;
		const ft = tab as FileTab;
		const root = ft.fsRoot || ft.repoPath;
		return isAbsolutePath(ft.filePath) || !root ? ft.filePath : joinPath(root, ft.filePath);
	};

	const reviewComments = createMemo(() => parseTweakComments(content()));
	const canSend = () => !!selectedAgentSession() && !sendingChanges();
	const hasReviewChanges = () =>
		editedSinceSend() || reviewComments().some((comment) => !sentCommentIds().has(comment.id));
	const reviewAgents = createMemo(() => {
		const tab = props.tab;
		if (tab.type !== "file") return [];
		return repositoryReviewAgents(
			terminalsStore.getIds().map((id) => terminalsStore.get(id)),
			tab.repoPath,
		);
	});

	createEffect(() => {
		const agents = reviewAgents();
		if (agents.some((agent) => agent.sessionId === selectedAgentSession())) return;
		setSelectedAgentSession(agents[0]?.sessionId ?? "");
	});

	const handleSendChanges = async () => {
		const sessionId = selectedAgentSession();
		const path = fullPath()?.replace(/[\r\n\0]/g, "");
		const comments = reviewComments();
		const count = comments.length;
		if (!sessionId || !path || !hasReviewChanges() || sendingChanges()) return;
		const sentContent = content();
		setSendingChanges(true);
		try {
			const outcome = await pty.enqueueCommand(sessionId, buildMarkdownReviewPrompt(path, count));
			setSentCommentIds(new Set(comments.map((comment) => comment.id)));
			if (content() === sentContent) setEditedSinceSend(false);
			const terminalId = terminalsStore.findBySessionId(sessionId);
			if (terminalId) terminalsStore.update(terminalId, { queuedCommands: outcome.queued });
			toastsStore.add(
				t("markdownTab.sentToAgent", "Sent to agent"),
				outcome.typed
					? t("markdownTab.sentToAgentNow", "The review request was delivered now.")
					: t("markdownTab.sentToAgentQueued", "The review request is queued for the agent's next idle window."),
				"info",
			);
		} catch (err) {
			const message = err instanceof Error ? err.message : String(err);
			appLogger.error("network", `Failed to queue Markdown review: ${message}`);
			toastsStore.add(t("markdownTab.sendFailed", "Couldn't send changes"), message, "error");
		} finally {
			setSendingChanges(false);
		}
	};

	// The focused element is the non-scrolling wrapper, so the browser never pages
	// the .content child on its own. Handle the bare paging keys here; leave
	// modified keys (Shift+PageUp = terminal scroll) and form controls alone.
	const handlePagingKey = (ev: KeyboardEvent) => {
		const el = scrollEl();
		if (!el || liveMode() || ev.defaultPrevented || ev.shiftKey || ev.ctrlKey || ev.altKey || ev.metaKey) return;
		const target = ev.target as HTMLElement;
		if (target.closest("input, textarea, select, [contenteditable]")) return;
		const page = Math.max(el.clientHeight - 40, 1);
		const top = {
			PageDown: el.scrollTop + page,
			PageUp: el.scrollTop - page,
			Home: 0,
			End: el.scrollHeight - el.clientHeight,
		}[ev.key];
		if (top === undefined) return;
		ev.preventDefault();
		el.scrollTop = top;
	};

	const handleCopyPath = () => {
		const path = fullPath();
		if (!path) return;
		copyPathToClipboard(path);
	};

	const handleHeaderContextMenu = (ev: MouseEvent) => {
		if (!fullPath()) return;
		ev.preventDefault();
		contextMenu.open(ev);
	};

	return (
		<div
			ref={wrapperRef}
			class={s.wrapper}
			tabIndex={-1}
			data-focus-target="md-tab"
			data-tab-id={props.tab.id}
			onKeyDown={handlePagingKey}
		>
			<div class={e.header} onContextMenu={handleHeaderContextMenu}>
				<span class={e.filename} title={displayPath()}>
					{displayPath()}
				</span>
				<Show when={props.tab.type === "file"}>
					<button class={e.btn} onClick={handleEdit} title={t("markdownTab.edit", "Edit file")}>
						<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
							<path d="M11.13 1.47a1.5 1.5 0 0 1 2.12 0l1.28 1.28a1.5 1.5 0 0 1 0 2.12L5.9 13.5a1 1 0 0 1-.5.27l-3.5.87a.5.5 0 0 1-.6-.6l.87-3.5a1 1 0 0 1 .27-.5L11.13 1.47ZM12.2 2.53l-8.46 8.47-.58 2.34 2.34-.58 8.47-8.46-1.77-1.77Z" />
						</svg>
						{t("markdownTab.editBtn", "Edit")}
					</button>
					<button
						class={e.btn}
						classList={{ [e.active]: liveMode() }}
						disabled={!liveMode() && !liveAvailable()}
						onClick={toggleLive}
						title={
							liveAvailable() || liveMode()
								? t("markdownTab.live", "Live edit: click a block to edit its source")
								: t(
										"markdownTab.liveUnavailable",
										"Live edit needs LF or CRLF line endings throughout and a file under 500 KB",
									)
						}
					>
						{t("markdownTab.liveBtn", "Live")}
						<Show when={liveDirty()}> ●</Show>
					</button>
					<Show when={(props.tab as FileTab).repoPath}>
						<button
							class={e.btn}
							onClick={() => {
								const ft = props.tab as FileTab;
								diffTabsStore.add(ft.repoPath, ft.filePath, "M");
							}}
							title={t("markdownTab.viewDiff", "View diff")}
						>
							<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
								<path
									d="M2 3h5v1H2zm0 3h5v1H2zm0 3h4v1H2zm7-6h5v1H9zm0 3h5v1H9zm0 3h4v1H9zM7.5 1v14M.5 0v16"
									fill="none"
									stroke="currentColor"
									stroke-width="1"
									opacity="0.5"
								/>
								<path d="M4 12l-2 2 2 2" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
								<path d="M12 12l2 2-2 2" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
							</svg>
							{t("markdownTab.diffBtn", "Diff")}
						</button>
					</Show>
					<Show when={hasReviewChanges()}>
						<div class={s.agentActions}>
							<label class={s.agentLabel} for={`review-agent-${props.tab.id}`}>
								{t("markdownTab.agentLabel", "Agent")}
							</label>
							<Select
								compact
								id={`review-agent-${props.tab.id}`}
								class={s.agentSelect}
								aria-label={t("markdownTab.reviewAgent", "Review agent")}
								value={selectedAgentSession()}
								disabled={reviewAgents().length === 0 || sendingChanges()}
								onChange={(event) => setSelectedAgentSession(event.currentTarget.value)}
							>
								<Show when={reviewAgents().length === 0}>
									<option value="">{t("markdownTab.noAgents", "No agents")}</option>
								</Show>
								<For each={reviewAgents()}>{(agent) => <option value={agent.sessionId}>{agent.label}</option>}</For>
							</Select>
							<button
								class={e.btn}
								disabled={!canSend()}
								data-ready={canSend() ? "true" : undefined}
								onClick={() => void handleSendChanges()}
								title={t("markdownTab.sendChanges", "Send changes to agent")}
								aria-label={t("markdownTab.sendChanges", "Send changes to agent")}
							>
								<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
									<path d="M1.5 2.25 14.75 8 1.5 13.75l1.35-5L9.5 8 2.85 7.25l-1.35-5Z" />
								</svg>
								{sendingChanges() ? t("markdownTab.sending", "Sending…") : t("markdownTab.send", "Send")}
							</button>
						</div>
					</Show>
				</Show>
			</div>

			<SearchBar
				visible={searchVisible()}
				focusToken={searchFocusToken()}
				onSearch={handleSearch}
				onNext={handleSearchNext}
				onPrev={handleSearchPrev}
				onClose={handleSearchClose}
				matchIndex={matchIndex()}
				matchCount={matchCount()}
			/>

			<Show
				when={liveMode()}
				fallback={
					<div class={s.content} ref={(el) => setScrollEl(el)}>
						<Show when={searchVisible()}>
							<DomSearchOverview scrollEl={scrollEl} fractions={overviewFractions} />
						</Show>
						<ContentRenderer
							content={content()}
							commentableBlocks={props.tab.type === "file"}
							baseDir={baseDir()}
							onLinkClick={props.tab.type === "file" ? (href) => void handleMdLink(href) : undefined}
							onCheckboxToggle={(idx, mark, col) => {
								void handleCheckboxToggle(idx, mark, col);
							}}
							contentRef={(el) => {
								contentRef = el;
								setOverlayContentEl(el);
							}}
							fontSize={props.tab.fontSize}
							emptyMessage={
								loading()
									? t("markdownTab.loading", "Loading...")
									: error()
										? `${t("markdownTab.error", "Error:")} ${error()}`
										: t("markdownTab.noContent", "No content")
							}
						/>
					</div>
				}
			>
				<LiveMarkdownEditor
					content={content()}
					onSave={writeTweakedSource}
					readDisk={() => {
						const ft = props.tab as FileTab;
						return readFileContent(ft.fsRoot || ft.repoPath, ft.filePath);
					}}
					onDirtyChange={setLiveDirty}
					baseDir={baseDir()}
					onLinkClick={props.tab.type === "file" ? (href) => void handleMdLink(href) : undefined}
					fontSize={props.tab.fontSize}
				/>
			</Show>

			{/* Mount CommentOverlay ONLY for the active file tab — otherwise every
          open markdown tab would attach its own selectionchange listener and
          they'd all fire on every cursor move across the app. */}
			<Show
				when={
					!liveMode() && props.tab.type === "file" && mdTabsStore.state.activeId === props.tab.id && overlayContentEl()
				}
				keyed
			>
				{(el) => (
					<CommentOverlay
						contentRef={el}
						onSave={handleTweakSave}
						onSaveBlock={handleTweakBlockSave}
						blockSource={(range) => content().slice(range.start, range.end)}
						onDelete={handleTweakDelete}
					/>
				)}
			</Show>

			<ContextMenu
				items={[{ label: t("markdownTab.copyPath", "Copy Path"), action: handleCopyPath }]}
				x={contextMenu.position().x}
				y={contextMenu.position().y}
				visible={contextMenu.visible()}
				onClose={contextMenu.close}
			/>
		</div>
	);
};

export default MarkdownTab;
