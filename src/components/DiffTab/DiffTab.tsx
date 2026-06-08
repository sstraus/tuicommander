import { type Component, createEffect, createMemo, createSignal, For, onCleanup, Show } from "solid-js";
import { usePty } from "../../hooks/usePty";
import { useRepository } from "../../hooks/useRepository";
import { t } from "../../i18n";
import { invoke } from "../../invoke";
import { getModifierSymbol } from "../../platform";
import { appLogger } from "../../stores/appLogger";
import { diffTabsStore } from "../../stores/diffTabs";
import { editorTabsStore } from "../../stores/editorTabs";
import { repositoriesStore } from "../../stores/repositories";
import { terminalsStore } from "../../stores/terminals";
import { type DiffViewMode, uiStore } from "../../stores/ui";
import { cx } from "../../utils";
import { ConfirmDialog } from "../ConfirmDialog";
import type { SearchOptions } from "../shared/DomSearchEngine";
import { DomSearchEngine } from "../shared/DomSearchEngine";
import { SearchBar } from "../shared/SearchBar";
import { DiffViewer } from "../ui";
import { BranchDiffScrollView } from "./BranchDiffScrollView";
import s from "./DiffTab.module.css";
import { buildPartialPatch, extractHunks, extractSelectedLines } from "./diffPatch";
import { diffLineCount, isDiffTooLarge } from "./diffSize";

export interface DiffTabProps {
	tabId: string;
	repoPath: string;
	filePath: string;
	scope?: string;
	untracked?: boolean;
	onClose?: () => void;
}

/** Check if this diff tab supports restore actions (working tree or staged, not commit/untracked) */
function canRestore(scope?: string, untracked?: boolean): boolean {
	if (untracked) return false;
	// scope is undefined (working tree) or "staged" — both support restore
	// scope is a commit hash — read-only
	if (!scope || scope === "staged") return true;
	return false;
}

export const DiffTab: Component<DiffTabProps> = (props) => {
	const pty = usePty();
	const [diff, setDiff] = createSignal("");
	const [loading, setLoading] = createSignal(false);
	const [error, setError] = createSignal<string | null>(null);
	const repo = useRepository();

	// Search state
	const [searchVisible, setSearchVisible] = createSignal(false);
	const [matchIndex, setMatchIndex] = createSignal(-1);
	const [matchCount, setMatchCount] = createSignal(0);

	// Hunk restore state
	const [confirmVisible, setConfirmVisible] = createSignal(false);
	const [pendingHunkPatch, setPendingHunkPatch] = createSignal<string | null>(null);
	const [hoverHunkIdx, setHoverHunkIdx] = createSignal<number | null>(null);

	// Line-level selection state
	const [selectedLines, setSelectedLines] = createSignal<Set<number>>(new Set<number>());
	const [selectedHunkIdx, setSelectedHunkIdx] = createSignal<number | null>(null);

	/** Parsed hunks, memoized — re-parsing the whole diff per revert/select op was wasteful */
	const hunks = createMemo(() => extractHunks(diff()));

	// Pathological single-file diffs (full-file rewrites) would render thousands
	// of DOM rows — @git-diff-view has no virtualization. Guard above this line
	// count and let the user opt in. (perf pass 2026-06-07)
	const diffLines = createMemo(() => diffLineCount(diff()));
	const tooLarge = createMemo(() => isDiffTooLarge(diff()));
	const [forceRenderLarge, setForceRenderLarge] = createSignal(false);

	// Clear selection when diff changes (and drop the DOM row-mapping cache —
	// the DiffViewer rebuilds its rows on diff/mode change)
	createEffect(() => {
		diff();
		uiStore.state.diffViewMode;
		setSelectedLines(new Set<number>());
		setSelectedHunkIdx(null);
		setForceRenderLarge(false);
		rowCache = null;
	});

	let contentRef: HTMLElement | undefined;

	/**
	 * Cached map from each rendered <tr> to its hunk/line position. Built once per
	 * diff render and reused across every drag mousemove + selection restyle, so
	 * those handlers don't re-run querySelectorAll("tr") + a full O(rows) walk on
	 * every pointer event. Invalidated when the diff/mode changes (rows rebuilt).
	 */
	type RowInfo = { hunkIdx: number; lineIdx: number; isChange: boolean };
	let rowCache: { rows: HTMLTableRowElement[]; info: Map<HTMLTableRowElement, RowInfo> } | null = null;

	function getRowCache() {
		if (rowCache) return rowCache;
		if (!contentRef) return null;
		const rows = Array.from(contentRef.querySelectorAll("tr")) as HTMLTableRowElement[];
		const info = new Map<HTMLTableRowElement, RowInfo>();
		let hunkIdx = -1;
		let lineCount = 0;
		for (const r of rows) {
			if (r.querySelector("[class*='diff-line-hunk']")) {
				hunkIdx++;
				lineCount = 0;
				info.set(r, { hunkIdx, lineIdx: -1, isChange: false });
			} else {
				info.set(r, { hunkIdx, lineIdx: lineCount, isChange: isChangeLine(r) });
				lineCount++;
			}
		}
		rowCache = { rows, info };
		return rowCache;
	}
	let engine: DomSearchEngine | undefined;
	let lastSearchTerm = "";
	let lastSearchOpts: SearchOptions = { caseSensitive: false, regex: false, wholeWord: false };
	let searchDebounceTimer: ReturnType<typeof setTimeout> | undefined;

	// Expose openSearch for Cmd+F
	createEffect(() => {
		diffTabsStore.setHandle(props.tabId, { openSearch: () => setSearchVisible(true) });
		onCleanup(() => diffTabsStore.clearHandle(props.tabId));
	});

	// Load file diff when props change or the repo revision bumps (git index/HEAD changed)
	let diffGen = 0;
	createEffect(() => {
		const repoPath = props.repoPath;
		const filePath = props.filePath;
		const scope = props.scope;
		void (repoPath ? repositoriesStore.getRevision(repoPath) : 0);

		if (!repoPath || !filePath) {
			setDiff("");
			return;
		}

		if (!diff()) setLoading(true);
		setError(null);

		// A revision-bump burst can fire several loads; only the newest may settle
		// state, so stale awaits don't overwrite the current diff.
		const gen = ++diffGen;
		(async () => {
			try {
				const diffContent = await repo.getFileDiff(repoPath, filePath, scope, props.untracked);
				if (gen !== diffGen) return;
				setDiff(diffContent);
			} catch (err) {
				if (gen !== diffGen) return;
				setError(String(err));
				setDiff("");
			} finally {
				if (gen === diffGen) setLoading(false);
			}
		})();
	});

	// Re-apply search when diff content changes or view mode changes
	createEffect(() => {
		diff();
		uiStore.state.diffViewMode;
		if (!searchVisible() || !lastSearchTerm) return;
		// Cancel a still-pending RAF on re-run and on unmount (Solid runs this
		// onCleanup before each re-execution and on disposal).
		const raf = requestAnimationFrame(() => rerunSearch());
		onCleanup(() => cancelAnimationFrame(raf));
	});

	function rerunSearch() {
		if (!contentRef) return;
		engine = new DomSearchEngine(contentRef);
		const count = engine.search(lastSearchTerm, lastSearchOpts);
		setMatchCount(count);
		setMatchIndex(count > 0 ? 0 : -1);
	}

	const handleSearch = (term: string, opts: SearchOptions) => {
		lastSearchTerm = term;
		lastSearchOpts = opts;

		clearTimeout(searchDebounceTimer);
		if (!term) {
			engine?.clear();
			setMatchCount(0);
			setMatchIndex(-1);
			return;
		}

		searchDebounceTimer = setTimeout(() => {
			rerunSearch();
		}, 150);
	};

	const handleSearchNext = () => {
		if (!engine || matchCount() === 0) return;
		setMatchIndex(engine.next());
	};

	const handleSearchPrev = () => {
		if (!engine || matchCount() === 0) return;
		setMatchIndex(engine.prev());
	};

	const handleSearchClose = () => {
		engine?.clear();
		setSearchVisible(false);
		setMatchCount(0);
		setMatchIndex(-1);
	};

	/** One-sided diffs (new/deleted files) only support unified view */
	const isOneSided = (): boolean => {
		const d = diff();
		return !!d && (d.includes("new file mode") || d.includes("deleted file mode"));
	};

	/** Force unified mode for one-sided diffs where split wastes half the screen */
	const mode = (): DiffViewMode => {
		if (isOneSided()) return "unified";
		return uiStore.state.diffViewMode;
	};

	// --- Hunk restore ---

	/** Find the hunk index from a DOM element within the diff content */
	function findHunkIndex(el: HTMLElement): number | null {
		// Walk up to find a .diff-line-hunk-action or .diff-line-hunk-content
		const hunkRow = el.closest("tr, [class*='diff-line-hunk']")?.closest("tr");
		if (!hunkRow || !contentRef) return null;
		// Count how many hunk rows precede this one in the DOM
		const allHunkRows = contentRef.querySelectorAll("[class*='diff-line-hunk-content']");
		for (let i = 0; i < allHunkRows.length; i++) {
			if (allHunkRows[i].closest("tr") === hunkRow) return i;
		}
		return null;
	}

	function handleRevertClick(hunkIdx: number) {
		const h = hunks();
		if (hunkIdx < 0 || hunkIdx >= h.length) return;
		setPendingHunkPatch(h[hunkIdx]);
		setConfirmVisible(true);
	}

	async function confirmRevert() {
		const patch = pendingHunkPatch();
		if (!patch) return;
		setConfirmVisible(false);
		setPendingHunkPatch(null);

		try {
			await invoke("git_apply_reverse_patch", {
				path: props.repoPath,
				patch,
				scope: props.scope || undefined,
			});
		} catch (err) {
			appLogger.error("git", "Failed to revert hunk", err);
		}
		clearSelection();
		// The repo revision bump from git changes will trigger diff reload automatically
	}

	function cancelRevert() {
		setConfirmVisible(false);
		setPendingHunkPatch(null);
	}

	const isStaged = () => props.scope === "staged";

	// --- Line-level selection ---

	/** Detect whether a <tr> is an addition, deletion, or neither.
	 *  The @git-diff-view library marks additions with data-line-new-num only,
	 *  deletions with data-line-old-num only, and context lines with both. */
	function isChangeLine(row: Element): boolean {
		const hasNew = row.querySelector("[data-line-new-num]");
		const hasOld = row.querySelector("[data-line-old-num]");
		return !!hasNew !== !!hasOld;
	}

	/** Find which hunk and line index a DOM element belongs to (via the row cache). */
	function findLineInfo(el: HTMLElement): { hunkIdx: number; lineIdx: number; isChange: boolean } | null {
		const row = el.closest("tr") as HTMLTableRowElement | null;
		if (!row) return null;
		const cache = getRowCache();
		const ri = cache?.info.get(row);
		if (!ri?.isChange || ri.hunkIdx < 0) return null;
		return { hunkIdx: ri.hunkIdx, lineIdx: ri.lineIdx, isChange: true };
	}

	// --- Drag-to-select ---
	let isDragging = false;
	let dragAnchorLine = -1;
	let dragAnchorHunk = -1;

	function selectRange(hunkIdx: number, from: number, to: number) {
		const start = Math.min(from, to);
		const end = Math.max(from, to);
		const next = new Set<number>();
		const h = hunks();
		if (hunkIdx < h.length) {
			const hunkLines = h[hunkIdx].split("\n");
			const bodyStart = hunkLines.findIndex((l) => l.startsWith("@@"));
			if (bodyStart >= 0) {
				const body = hunkLines.slice(bodyStart + 1);
				for (let i = start; i <= end; i++) {
					if (i < body.length && (body[i].startsWith("+") || body[i].startsWith("-"))) {
						next.add(i);
					}
				}
			}
		}
		setSelectedHunkIdx(hunkIdx);
		setSelectedLines(next);
	}

	function handleLineMouseDown(e: MouseEvent) {
		if ((e.target as HTMLElement).closest(`.${s.revertBtn}`)) return;
		if (e.button !== 0) return;

		const info = findLineInfo(e.target as HTMLElement);
		if (!info) return;

		e.preventDefault();
		isDragging = true;
		dragAnchorLine = info.lineIdx;
		dragAnchorHunk = info.hunkIdx;

		if (selectedHunkIdx() !== null && selectedHunkIdx() !== info.hunkIdx) {
			setSelectedLines(new Set<number>());
		}
		setSelectedHunkIdx(info.hunkIdx);

		const next = new Set<number>();
		next.add(info.lineIdx);
		setSelectedLines(next);
		applyLineSelectionStyles();
	}

	function handleLineMouseMove(e: MouseEvent) {
		if (!isDragging) return;
		const info = findLineInfo(e.target as HTMLElement);
		if (!info || info.hunkIdx !== dragAnchorHunk) return;
		selectRange(dragAnchorHunk, dragAnchorLine, info.lineIdx);
		applyLineSelectionStyles();
	}

	function handleLineMouseUp() {
		isDragging = false;
	}

	const globalMouseUp = () => {
		isDragging = false;
	};
	document.addEventListener("mouseup", globalMouseUp);
	onCleanup(() => document.removeEventListener("mouseup", globalMouseUp));

	/** Apply/remove CSS class on selected rows (via the row cache) */
	function applyLineSelectionStyles() {
		const cache = getRowCache();
		if (!cache) return;
		const sel = selectedLines();
		const hIdx = selectedHunkIdx();

		for (const [row, ri] of cache.info) {
			if (ri.lineIdx < 0) continue; // hunk header row
			if (ri.hunkIdx === hIdx && sel.has(ri.lineIdx)) {
				row.classList.add(s.lineSelected);
			} else {
				row.classList.remove(s.lineSelected);
			}
		}
	}

	// Re-apply styles when selection changes
	createEffect(() => {
		selectedLines();
		selectedHunkIdx();
		// Cancel a still-pending RAF on re-run and on unmount.
		const raf = requestAnimationFrame(() => applyLineSelectionStyles());
		onCleanup(() => cancelAnimationFrame(raf));
	});

	function handleRestoreSelected() {
		const hIdx = selectedHunkIdx();
		if (hIdx === null) return;
		const sel = selectedLines();
		if (sel.size === 0) return;

		const patch = buildPartialPatch(diff(), hIdx, sel);
		if (!patch) return;

		setPendingHunkPatch(patch);
		setConfirmVisible(true);
	}

	function clearSelection() {
		setSelectedLines(new Set<number>());
		setSelectedHunkIdx(null);
		if (contentRef) {
			contentRef.querySelectorAll(`.${s.lineSelected}`).forEach((el) => el.classList.remove(s.lineSelected));
		}
	}

	// --- Comment on selected lines ---
	const [commentVisible, setCommentVisible] = createSignal(false);
	const [commentText, setCommentText] = createSignal("");
	const [commentError, setCommentError] = createSignal<string | null>(null);
	let commentTextareaRef: HTMLTextAreaElement | undefined;

	function handleCommentOpen() {
		setCommentVisible(true);
		setCommentError(null);
		requestAnimationFrame(() => commentTextareaRef?.focus());
	}

	async function handleCommentSend() {
		const text = commentText().trim();
		if (!text) return;

		const term = terminalsStore.findTerminalWithSession();
		if (!term) {
			setCommentError("No terminal with active session — open a terminal first");
			return;
		}

		const hIdx = selectedHunkIdx();
		if (hIdx === null) return;

		const { lines, startLine, endLine } = extractSelectedLines(diff(), hIdx, selectedLines());
		if (lines.length === 0) return;

		try {
			const lineRange = startLine === endLine ? `L${startLine}` : `L${startLine}-L${endLine}`;
			const sanitize = (s: string) => s.replace(/[\r\0]/g, "");
			const codeSnippet = lines.map((l) => `${l.type}${sanitize(l.content)}`).join("\n");

			const message = `[${props.filePath.replace(/[\r\n\0]/g, "")}:${lineRange}]\n${codeSnippet}\n— ${text}`;

			await pty.sendCommand(term.sessionId, message, term.agentType);

			setCommentText("");
			setCommentError(null);
			setCommentVisible(false);
			clearSelection();
		} catch (err) {
			appLogger.error("git", "Failed to send comment to terminal", err);
			setCommentError("Failed to send — see logs for details");
		}
	}

	function handleCommentKeyDown(e: KeyboardEvent) {
		if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
			e.preventDefault();
			void handleCommentSend();
		}
		if (e.key === "Escape") {
			e.preventDefault();
			setCommentVisible(false);
		}
	}

	const selectedCount = () => selectedLines().size;

	const confirmTitle = () => {
		if (selectedCount() > 0) {
			const count = selectedCount();
			return isStaged()
				? `Unstage ${count} selected line${count > 1 ? "s" : ""}?`
				: `Discard ${count} selected line${count > 1 ? "s" : ""}?`;
		}
		return isStaged()
			? t("diffTab.unstageHunk", "Unstage this change?")
			: t("diffTab.discardHunk", "Discard this change?");
	};
	const confirmMessage = () =>
		isStaged()
			? t(
					"diffTab.unstageHunkMsg",
					"This will remove this hunk from the staging area. The changes will remain in your working directory.",
				)
			: t("diffTab.discardHunkMsg", "This will permanently discard this change. This cannot be undone.");

	return (
		<div class={s.content}>
			{/* Toolbar with view mode toggle */}
			<div class={s.toolbar}>
				<button
					class={cx(s.modeBtn, mode() === "split" && s.modeBtnActive)}
					onClick={() => uiStore.setDiffViewMode("split")}
					title={t("diffTab.splitView", "Side-by-side")}
					disabled={isOneSided()}
				>
					<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
						<path d="M1 2h6v12H1V2zm8 0h6v12H9V2zM2 3v10h4V3H2zm8 0v10h4V3h-4z" />
					</svg>
				</button>
				<button
					class={cx(s.modeBtn, mode() === "unified" && s.modeBtnActive)}
					onClick={() => uiStore.setDiffViewMode("unified")}
					title={t("diffTab.unifiedView", "Inline")}
				>
					<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
						<path d="M1 2h14v12H1V2zm1 1v10h12V3H2z" />
					</svg>
				</button>
				<button
					class={cx(s.modeBtn, mode() === "scroll" && s.modeBtnActive)}
					onClick={() => uiStore.setDiffViewMode("scroll")}
					title={t("diffScroll.scrollView", "All files")}
					disabled={isOneSided()}
				>
					<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
						<path d="M2 2h12v1H2zm0 3h12v1H2zm0 3h10v1H2zm0 3h8v1H2z" />
					</svg>
				</button>
				<div style={{ "margin-left": "auto" }}>
					<button
						class={s.modeBtn}
						onClick={() => editorTabsStore.add(props.repoPath, props.filePath)}
						title={t("diffTab.editFile", "Edit file")}
					>
						<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
							<path d="M12.1 1.3a1.5 1.5 0 0 1 2.1 0l.5.5a1.5 1.5 0 0 1 0 2.1L5.8 12.8l-3.5.9.9-3.5L12.1 1.3zM11 3.4 4.1 10.3l-.5 1.9 1.9-.5L12.4 4.8 11 3.4z" />
						</svg>
					</button>
				</div>
			</div>
			<SearchBar
				visible={searchVisible()}
				onSearch={handleSearch}
				onNext={handleSearchNext}
				onPrev={handleSearchPrev}
				onClose={handleSearchClose}
				matchIndex={matchIndex()}
				matchCount={matchCount()}
			/>
			{/* Scroll mode: show all-files view instead of single-file diff */}
			<Show when={mode() === "scroll"}>
				<BranchDiffScrollView
					repoPath={props.repoPath}
					contentRef={(el) => {
						contentRef = el;
					}}
				/>
			</Show>
			<Show when={mode() !== "scroll"}>
				<div
					class={s.diffWrapper}
					onMouseOver={(e) => {
						if (!canRestore(props.scope, props.untracked)) return;
						const idx = findHunkIndex(e.target as HTMLElement);
						setHoverHunkIdx(idx);
					}}
					onMouseOut={() => setHoverHunkIdx(null)}
					onClick={(e) => {
						const btn = (e.target as HTMLElement).closest(`.${s.revertBtn}`);
						if (btn) {
							const idx = parseInt(btn.getAttribute("data-hunk-idx") || "-1", 10);
							if (idx >= 0) handleRevertClick(idx);
						}
					}}
					onMouseDown={handleLineMouseDown}
					onMouseMove={handleLineMouseMove}
					onMouseUp={handleLineMouseUp}
				>
					<Show
						when={!tooLarge() || forceRenderLarge()}
						fallback={
							<div class={s.largeDiffNotice}>
								<div>
									{t("diffTab.largeDiffTitle", "This diff is large")} ({diffLines().toLocaleString()}{" "}
									{t("diffTab.lines", "lines")})
								</div>
								<button class={s.largeDiffBtn} onClick={() => setForceRenderLarge(true)}>
									{t("diffTab.renderAnyway", "Render anyway")}
								</button>
							</div>
						}
					>
						<DiffViewer
							diff={diff()}
							mode={mode()}
							contentRef={(el: HTMLElement) => {
								contentRef = el;
							}}
							emptyMessage={
								loading()
									? t("diffTab.loading", "Loading diff...")
									: error()
										? `${t("diffTab.error", "Error:")} ${error()}`
										: t("diffTab.noChanges", "No changes")
							}
						/>
						{/* Floating revert buttons — rendered after diff, positioned via CSS */}
						<Show when={canRestore(props.scope, props.untracked) && diff().trim()}>
							<HunkRevertOverlay
								diff={diff()}
								contentRef={contentRef}
								hoverIdx={hoverHunkIdx()}
								isStaged={isStaged()}
							/>
						</Show>
					</Show>
					<Show when={selectedCount() > 0}>
						<div class={s.selectionFloater}>
							<Show when={commentVisible()}>
								<div class={s.commentBox}>
									<textarea
										ref={(el) => {
											commentTextareaRef = el;
										}}
										class={s.commentTextarea}
										placeholder="Write a comment about the selected lines..."
										value={commentText()}
										onInput={(e) => setCommentText(e.currentTarget.value)}
										onKeyDown={handleCommentKeyDown}
										rows={3}
									/>
									<Show when={commentError()}>
										<div class={s.commentErrorMsg}>{commentError()}</div>
									</Show>
									<div class={s.commentActions}>
										<span class={s.commentHint}>{getModifierSymbol()}+Enter to send</span>
										<button class={s.commentCancelBtn} onClick={() => setCommentVisible(false)}>
											Cancel
										</button>
										<button class={s.commentSendBtn} disabled={!commentText().trim()} onClick={handleCommentSend}>
											Send
										</button>
									</div>
								</div>
							</Show>
							<div class={s.selectionBar}>
								<Show when={canRestore(props.scope, props.untracked)}>
									<button class={s.restoreSelectedBtn} onClick={handleRestoreSelected}>
										{isStaged() ? "Unstage" : "Discard"} {selectedCount()} line{selectedCount() > 1 ? "s" : ""}
									</button>
								</Show>
								<button class={s.commentBtn} onClick={handleCommentOpen} title="Comment on selected lines">
									<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
										<path d="M1 3a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2v7a2 2 0 0 1-2 2H5.236L2.22 14.54A.5.5 0 0 1 1 14.14V3zm2-1a1 1 0 0 0-1 1v9.86l2.236-1.86A.5.5 0 0 1 4.56 11H13a1 1 0 0 0 1-1V3a1 1 0 0 0-1-1H3z" />
									</svg>
									Comment
								</button>
								<button
									class={s.clearSelectionBtn}
									onClick={() => {
										clearSelection();
										setCommentVisible(false);
									}}
									title="Clear selection"
								>
									&times;
								</button>
							</div>
						</div>
					</Show>
				</div>
				<ConfirmDialog
					visible={confirmVisible()}
					title={confirmTitle()}
					message={confirmMessage()}
					confirmLabel={isStaged() ? t("diffTab.unstage", "Unstage") : t("diffTab.discard", "Discard")}
					kind={isStaged() ? "info" : "warning"}
					onConfirm={confirmRevert}
					onClose={cancelRevert}
				/>
			</Show>
		</div>
	);
};

/** Overlay that positions revert buttons on hunk header rows */
const HunkRevertOverlay: Component<{
	diff: string;
	contentRef: HTMLElement | undefined;
	hoverIdx: number | null;
	isStaged: boolean;
}> = (props) => {
	// Button positions depend on the rendered rows, not on hover. Recompute only
	// when the diff changes or the content reflows (ResizeObserver) — NOT on every
	// hoverIdx change, which previously re-ran getBoundingClientRect for every hunk.
	const [reflowTick, setReflowTick] = createSignal(0);
	createEffect(() => {
		const el = props.contentRef;
		if (!el) return;
		const ro = new ResizeObserver(() => setReflowTick((n) => n + 1));
		ro.observe(el);
		onCleanup(() => ro.disconnect());
	});

	const buttons = createMemo<Array<{ top: number; idx: number }>>(() => {
		props.diff; // track: rows rebuilt on diff change
		reflowTick(); // track: content finished rendering / reflowed
		if (!props.contentRef) return [];
		const hunkEls = props.contentRef.querySelectorAll("[class*='diff-line-hunk-content']");
		const result: Array<{ top: number; idx: number }> = [];
		const containerRect = props.contentRef.getBoundingClientRect();
		hunkEls.forEach((el, i) => {
			const row = el.closest("tr");
			if (!row) return;
			const rect = row.getBoundingClientRect();
			result.push({ top: rect.top - containerRect.top + props.contentRef!.scrollTop, idx: i });
		});
		return result;
	});

	return (
		<For each={buttons()}>
			{(b) => (
				<button
					class={cx(s.revertBtn, props.hoverIdx === b.idx && s.revertBtnVisible)}
					style={{ top: `${b.top}px` }}
					data-hunk-idx={b.idx}
					title={
						props.isStaged
							? t("diffTab.unstageHunk", "Unstage this change")
							: t("diffTab.discardHunk", "Discard this change")
					}
					aria-label={props.isStaged ? "Unstage hunk" : "Discard hunk"}
				>
					<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
						<path d="M2 8a6 6 0 1 1 12 0A6 6 0 0 1 2 8zm6-4a4 4 0 1 0 0 8 4 4 0 0 0 0-8zM6.5 7.5l3-2v4l-3-2z" />
					</svg>
				</button>
			)}
		</For>
	);
};

export default DiffTab;
