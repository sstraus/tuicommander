import { createVirtualizer } from "@tanstack/solid-virtual";
import {
	type Component,
	createEffect,
	createMemo,
	createSignal,
	For,
	Match,
	onCleanup,
	Show,
	Switch,
	untrack,
} from "solid-js";
import { useFileBrowser } from "../../hooks/useFileBrowser";
import { useRepository } from "../../hooks/useRepository";
import { t } from "../../i18n";
import { shortenHomePath } from "../../platform";
import { appLogger } from "../../stores/appLogger";
import { mdTabsStore } from "../../stores/mdTabs";
import { repositoriesStore } from "../../stores/repositories";
import { cx, globToRegex } from "../../utils";
import { pathBasename, pathDirname } from "../../utils/pathUtils";
import { ContextMenu, type ContextMenuItem, createContextMenu } from "../ContextMenu";
import g from "../shared/git-status.module.css";
import p from "../shared/panel.module.css";
import { Dropdown } from "../ui/Dropdown";
import { PanelResizeHandle } from "../ui/PanelResizeHandle";
import { PanelWindowControls } from "../ui/PanelWindowControls";
import s from "./MarkdownPanel.module.css";

/** Markdown file entry from Rust backend */
interface MdFileEntry {
	path: string;
	git_status: string;
	is_ignored: boolean;
	modified_at: number;
}

type SortMode = "folder" | "date";
/** Filter the list by filename glob, or full-text by file contents. */
type SearchMode = "filename" | "content";

/** Filename mode icon — simple "F" in a doc shape (shared visual with FileBrowser) */
const FilenameModeIcon = () => (
	<svg viewBox="0 0 16 16" fill="currentColor" width="14" height="14">
		<path d="M4 2h5l3 3v9H4V2zm1 1v10h6V6H9V3H5zm1.5 4h3v1h-3V7zm0 2h3v1h-3V9z" />
	</svg>
);

/** Content mode icon — magnifier with lines (shared visual with FileBrowser) */
const ContentModeIcon = () => (
	<svg viewBox="0 0 16 16" fill="currentColor" width="14" height="14">
		<path d="M11.5 7a4.5 4.5 0 1 0-1.77 3.56l3.35 3.36.71-.71-3.36-3.35A4.48 4.48 0 0 0 11.5 7zM7 10.5a3.5 3.5 0 1 1 0-7 3.5 3.5 0 0 1 0 7zM5 6h4v1H5V6zm0 2h4v1H5V8z" />
	</svg>
);

/** Flat row for virtualization: either a directory header or a file entry. */
type Row = { kind: "header"; dir: string } | { kind: "file"; entry: MdFileEntry };

// Fixed row heights for the virtualizer (in px). Must match the CSS-driven
// content heights exactly — no dynamic measurement is used to avoid reflow
// loops when switching sort modes.
const ROW_HEIGHT_HEADER = 28;
const ROW_HEIGHT_FILE = 30;
const ROW_HEIGHT_FILE_WITH_PATH = 46;

export interface MarkdownPanelProps {
	visible: boolean;
	repoPath: string | null;
	/** Effective filesystem root (worktree path when on a linked worktree, otherwise same as repoPath) */
	fsRoot?: string | null;
	onClose: () => void;
	mode?: "inline" | "detached";
}

/** Git status badge CSS class (shared pattern with FileBrowser) */
const getStatusClass = (status: string): string => {
	switch (status) {
		case "modified":
			return g.modified;
		case "staged":
			return g.staged;
		case "untracked":
			return g.untracked;
		default:
			return "";
	}
};

export const MarkdownPanel: Component<MarkdownPanelProps> = (props) => {
	const mode = () => props.mode ?? "inline";
	const [files, setFiles] = createSignal<MdFileEntry[]>([]);
	const [loading, setLoading] = createSignal(false);
	const [error, setError] = createSignal<string | null>(null);
	const [searchQuery, setSearchQuery] = createSignal("");
	const [searchMode, setSearchMode] = createSignal<SearchMode>("filename");
	// Relative paths of markdown files whose contents match the content-search query.
	const [contentPaths, setContentPaths] = createSignal<Set<string>>(new Set<string>());
	const [contentSearching, setContentSearching] = createSignal(false);
	const [sortBy, setSortBy] = createSignal<SortMode>("folder");
	const [sortDropdownOpen, setSortDropdownOpen] = createSignal(false);
	const fb = useFileBrowser();
	const repo = useRepository();
	const contextMenu = createContextMenu();
	const [contextEntry, setContextEntry] = createSignal<MdFileEntry | null>(null);
	let scrollRef: HTMLDivElement | undefined;
	// Generation counter: incremented on every effect run so stale async fetches are discarded.
	let fetchGeneration = 0;

	/**
	 * Files filtered by the search query. In "filename" mode the query is a glob
	 * matched against the path; in "content" mode it's a full-text search whose
	 * matching paths (collected via search_content) are intersected with the
	 * markdown file list — so only `.md`/`.mdx` files survive automatically.
	 */
	const filteredFiles = createMemo(() => {
		const q = searchQuery().trim();
		if (!q) return files();
		if (searchMode() === "content") {
			const paths = contentPaths();
			return files().filter((f) => paths.has(f.path));
		}
		const re = globToRegex(q);
		return files().filter((f) => re.test(f.path));
	});

	// Full-text content search: mirrors FileBrowserPanel's streaming search but
	// only keeps the set of matching paths (the list-filter UX needs no snippets).
	// Reuses the shared search_content backend + content-search-batch events.
	createEffect(() => {
		if (searchMode() !== "content") return;
		const q = searchQuery().trim();
		const fsRoot = props.fsRoot || props.repoPath;

		if (!q || q.length < 3 || !fsRoot) {
			setContentPaths(new Set<string>());
			setContentSearching(false);
			return;
		}

		setContentSearching(true);
		setContentPaths(new Set<string>());

		let cancelled = false;
		let unlistenBatch: (() => void) | null = null;
		let unlistenError: (() => void) | null = null;

		const timer = setTimeout(async () => {
			if (cancelled) return;
			try {
				const batchPromise = fb.onContentSearchBatch((batch) => {
					if (cancelled) return;
					setContentPaths((prev) => {
						const next = new Set(prev);
						for (const m of batch.matches) next.add(m.path);
						return next;
					});
					if (batch.is_final) setContentSearching(false);
				});
				const errorPromise = fb.onContentSearchError((err) => {
					if (cancelled) return;
					appLogger.error("app", "Markdown content search error", err);
					setContentSearching(false);
				});

				const [batchUn, errorUn] = await Promise.all([batchPromise, errorPromise]);
				unlistenBatch = batchUn;
				unlistenError = errorUn;

				if (cancelled) {
					unlistenBatch();
					unlistenError();
					return;
				}

				// DEFERRED (2026-06-03) — search_content uses a single global cancel
				// token, so a simultaneous FileBrowser content search cancels this one
				// (and vice versa). Harmless in practice (one panel active at a time);
				// revisit only if both panels are commonly searched at once.
				await fb.searchContent(fsRoot, q);
			} catch (err) {
				if (!cancelled) {
					appLogger.error("app", "Markdown content search failed", err);
					setContentSearching(false);
				}
			}
		}, 250);

		onCleanup(() => {
			cancelled = true;
			clearTimeout(timer);
			unlistenBatch?.();
			unlistenError?.();
		});
	});

	// Load markdown files when visible, repo changes, or repo content changes.
	// Uses stale-while-revalidate: only show loading spinner on initial load,
	// keep previous file list visible during background refreshes to avoid flash.
	createEffect(() => {
		const visible = props.visible;
		const repoPath = props.repoPath;
		const fsRoot = props.fsRoot || repoPath;
		void (repoPath ? repositoriesStore.getRevision(repoPath) : 0);

		if (!visible || !fsRoot) {
			setFiles([]);
			return;
		}

		const gen = ++fetchGeneration;
		const isInitialLoad = untrack(() => files().length === 0);
		if (isInitialLoad) setLoading(true);
		setError(null);

		(async () => {
			try {
				const mdFiles = await repo.listMarkdownFiles(fsRoot);
				if (gen !== fetchGeneration) return;
				// Skip re-render when entries are identical: same count and every entry
				// matches on the fields that drive visible state (path, git badge, mtime,
				// ignored flag). New object instances from Rust would otherwise cause a
				// full virtualizer remount even when nothing changed — causing hover lag.
				const current = untrack(() => files());
				const changed =
					current.length !== mdFiles.length ||
					mdFiles.some((e, i) => {
						const c = current[i];
						return (
							e.path !== c.path ||
							e.git_status !== c.git_status ||
							e.modified_at !== c.modified_at ||
							e.is_ignored !== c.is_ignored
						);
					});
				if (changed) setFiles(mdFiles);
			} catch (err) {
				if (gen !== fetchGeneration) return;
				appLogger.error("app", "Failed to list markdown files", err);
				setError(String(err));
				setFiles([]);
			} finally {
				if (gen === fetchGeneration) setLoading(false);
			}
		})();
	});

	const handleFileClick = (filePath: string) => {
		if (!props.repoPath) return;
		const fsRoot = props.fsRoot || props.repoPath;
		mdTabsStore.add(props.repoPath, filePath, fsRoot || undefined);
	};

	/** Group files by directory for tree view, sorted by dir name — or flat list sorted by date */
	const sortedGroups = createMemo(() => {
		const allFiles = filteredFiles();

		if (sortBy() === "date") {
			// Flat list sorted by modification time (newest first), single group with empty key
			const sorted = [...allFiles].sort((a, b) => b.modified_at - a.modified_at);
			return [["", sorted]] as [string, MdFileEntry[]][];
		}

		const groups: Record<string, MdFileEntry[]> = {};
		for (const entry of allFiles) {
			const dir = pathDirname(entry.path);
			const key = dir || "/";
			if (!groups[key]) groups[key] = [];
			groups[key].push(entry);
		}
		return Object.entries(groups).sort(([a], [b]) => a.localeCompare(b));
	});

	/** Flatten grouped entries into a single row list for the virtualizer. */
	const rows = createMemo<Row[]>(() => {
		const out: Row[] = [];
		for (const [dir, entries] of sortedGroups()) {
			if (dir && dir !== "/") out.push({ kind: "header", dir });
			for (const entry of entries) out.push({ kind: "file", entry });
		}
		return out;
	});

	const virtualizer = createVirtualizer({
		get count() {
			return rows().length;
		},
		getScrollElement: () => scrollRef ?? null,
		estimateSize: (index) => {
			const row = rows()[index];
			if (!row) return ROW_HEIGHT_FILE;
			if (row.kind === "header") return ROW_HEIGHT_HEADER;
			return sortBy() === "date" && pathDirname(row.entry.path) ? ROW_HEIGHT_FILE_WITH_PATH : ROW_HEIGHT_FILE;
		},
		overscan: 8,
	});

	const handleContextMenu = (ev: MouseEvent, entry: MdFileEntry) => {
		ev.preventDefault();
		ev.stopPropagation();
		setContextEntry(entry);
		contextMenu.open(ev);
	};

	const getContextMenuItems = (): ContextMenuItem[] => {
		const entry = contextEntry();
		if (!entry || !props.repoPath) return [];
		const root = props.fsRoot || props.repoPath;
		return [
			{
				label: t("markdownPanel.copyPath", "Copy Path"),
				action: () => {
					navigator.clipboard
						.writeText(shortenHomePath(`${root}/${entry.path}`))
						.catch((err) => appLogger.error("app", "Failed to copy path", err));
				},
			},
		];
	};

	return (
		<div id="markdown-panel" class={cx(s.panel, mode() === "detached" && s.detached, !props.visible && s.hidden)}>
			<Show when={mode() === "inline"}>
				<PanelResizeHandle panelId="markdown-panel" />
			</Show>
			<div class={p.header}>
				<div class={p.headerLeft}>
					<span class={p.title}>{t("markdownPanel.title", "Markdown Files")}</span>
					<Show when={!loading() && filteredFiles().length > 0}>
						<span class={p.fileCountBadge}>{filteredFiles().length}</span>
					</Show>
					<span class={p.headerSep} />
					<div class={g.legend}>
						<span class={g.legendItem} title={t("markdownPanel.modified", "Modified (unstaged changes)")}>
							<span class={cx(g.dot, g.modified)} /> mod
						</span>
						<span class={g.legendItem} title={t("markdownPanel.staged", "Staged for commit")}>
							<span class={cx(g.dot, g.staged)} /> staged
						</span>
						<span class={g.legendItem} title={t("markdownPanel.untracked", "Untracked (new file)")}>
							<span class={cx(g.dot, g.untracked)} /> new
						</span>
					</div>
				</div>
				<PanelWindowControls panelId="markdown" mode={mode()} onInlineClose={props.onClose} />
			</div>

			{/* Search filter (filename glob / full-text content) + sort control */}
			<div class={p.search}>
				<button
					class={cx(s.modeToggle, searchMode() === "content" && s.modeToggleActive)}
					onClick={() => {
						const next = searchMode() === "filename" ? "content" : "filename";
						setSearchMode(next);
						// Reset the other mode's state so stale results don't leak across modes.
						setContentPaths(new Set<string>());
						setContentSearching(false);
					}}
					title={
						searchMode() === "filename"
							? t("markdownPanel.switchToContent", "Switch to content search")
							: t("markdownPanel.switchToFilename", "Switch to filename search")
					}
				>
					<Show when={searchMode() === "filename"} fallback={<ContentModeIcon />}>
						<FilenameModeIcon />
					</Show>
				</button>
				<input
					type="text"
					class={p.searchInput}
					placeholder={
						searchMode() === "filename"
							? t("markdownPanel.filter", "Filter... (*, ** wildcards)")
							: t("markdownPanel.searchContent", "Search in file contents…")
					}
					value={searchQuery()}
					onInput={(e) => setSearchQuery(e.currentTarget.value)}
					autocomplete="off"
					autocorrect="off"
					spellcheck={false}
				/>
				<Show when={searchQuery()}>
					<button class={p.searchClear} onClick={() => setSearchQuery("")}>
						&times;
					</button>
				</Show>
				<div class={s.sortControl}>
					<button
						class={s.sortTrigger}
						onClick={() => setSortDropdownOpen((v) => !v)}
						title={`${t("markdownPanel.sortBy", "Sort by")} ${sortBy()}`}
					>
						<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
							<path d="M1 2h14L9.5 8.5V13l-3 1.5V8.5z" />
						</svg>
					</button>
					<Dropdown
						items={[
							{ id: "folder", label: t("markdownPanel.sortFolder", "Folder") },
							{ id: "date", label: t("markdownPanel.sortDate", "Date") },
						]}
						selected={sortBy()}
						visible={sortDropdownOpen()}
						onSelect={(id) => {
							setSortBy(id as SortMode);
							setSortDropdownOpen(false);
						}}
						onClose={() => setSortDropdownOpen(false)}
					/>
				</div>
			</div>

			<div class={p.content}>
				{/* Scroll container is always mounted so the virtualizer keeps a stable
            scrollElement reference across loading/empty/populated state changes. */}
				<div ref={scrollRef} class={s.fileList}>
					<Show when={(loading() || contentSearching()) && filteredFiles().length === 0}>
						<div class={s.empty}>
							{contentSearching()
								? t("markdownPanel.searching", "Searching contents…")
								: t("markdownPanel.loading", "Loading files...")}
						</div>
					</Show>

					<Show when={error()}>
						<div class={s.error}>
							{t("markdownPanel.error", "Error:")} {error()}
						</div>
					</Show>

					<Show when={!loading() && !contentSearching() && !error() && filteredFiles().length === 0}>
						<div class={s.empty}>
							{!props.repoPath
								? t("markdownPanel.noRepo", "No repository selected")
								: searchQuery()
									? t("markdownPanel.noMatches", "No matches")
									: t("markdownPanel.noFiles", "No markdown files found")}
						</div>
					</Show>

					<Show when={!loading() && !error() && filteredFiles().length > 0}>
						<div class={s.virtualList} style={{ height: `${virtualizer.getTotalSize()}px` }}>
							<For each={virtualizer.getVirtualItems()}>
								{(vi) => {
									const row = () => rows()[vi.index];
									return (
										<div
											class={s.virtualRow}
											style={{
												top: `${vi.start}px`,
												height: `${vi.size}px`,
											}}
										>
											<Switch>
												<Match when={row()?.kind === "header" ? (row() as { kind: "header"; dir: string }) : undefined}>
													{(h) => <div class={s.dirHeader}>{h().dir}/</div>}
												</Match>
												<Match
													when={
														row()?.kind === "file" ? (row() as { kind: "file"; entry: MdFileEntry }).entry : undefined
													}
												>
													{(entry) => (
														<div
															class={cx(s.fileItem, entry().is_ignored && s.fileIgnored)}
															onClick={() => handleFileClick(entry().path)}
															onContextMenu={(ev) => handleContextMenu(ev, entry())}
															title={entry().path}
														>
															<span class={s.fileIcon}>
																<svg
																	width="14"
																	height="14"
																	viewBox="0 0 24 24"
																	fill="none"
																	stroke="currentColor"
																	stroke-width="2"
																	stroke-linecap="round"
																	stroke-linejoin="round"
																>
																	<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
																	<polyline points="14 2 14 8 20 8" />
																	<line x1="16" y1="13" x2="8" y2="13" />
																	<line x1="16" y1="17" x2="8" y2="17" />
																	<polyline points="10 9 9 9 8 9" />
																</svg>
															</span>
															<div class={s.fileName}>
																{pathBasename(entry().path) || entry().path}
																<Show when={sortBy() === "date" && pathDirname(entry().path)}>
																	<span class={s.filePath}>{pathDirname(entry().path)}/</span>
																</Show>
															</div>
															<Show when={entry().git_status}>
																<span
																	class={cx(g.dot, getStatusClass(entry().git_status))}
																	title={entry().git_status}
																/>
															</Show>
														</div>
													)}
												</Match>
											</Switch>
										</div>
									);
								}}
							</For>
						</div>
					</Show>
				</div>
			</div>

			<ContextMenu
				items={getContextMenuItems()}
				x={contextMenu.position().x}
				y={contextMenu.position().y}
				visible={contextMenu.visible()}
				onClose={contextMenu.close}
			/>
		</div>
	);
};

export default MarkdownPanel;
