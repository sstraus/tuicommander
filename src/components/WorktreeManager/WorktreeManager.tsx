import { type Component, createEffect, createMemo, createSignal, For, onCleanup, Show } from "solid-js";
import { invoke } from "../../invoke";
import { appLogger } from "../../stores/appLogger";
import { githubStore } from "../../stores/github";
import { registerModal } from "../../stores/modalStack";
import { repoSettingsStore } from "../../stores/repoSettings";
import { repositoriesStore } from "../../stores/repositories";
import { worktreeManagerStore } from "../../stores/worktreeManager";
import type { BranchPrStatus } from "../../types";
import { formatRelativeTime } from "../../utils/time";
import b from "../shared/branch.module.css";
import s from "./WorktreeManager.module.css";

/** Row data derived from repo/branch state */
interface WorktreeRow {
	/** Opaque row key for selection only. Built from the repo path and the
	 *  workspace id, and never taken apart again: the id is opaque by contract,
	 *  so a row that needs its parts reads them off the row (which carries them
	 *  as fields) instead of splitting this string. */
	id: string;
	repoPath: string;
	repoName: string;
	/** Which workspace this row IS — what every action addresses. */
	workspaceId: string;
	kind: "worktree" | "main";
	/** What it has checked out. Display, sort and PR lookup only. */
	branch: string;
	worktreePath: string;
	additions: number;
	deletions: number;
	isMain: boolean;
	prStatus: BranchPrStatus | null;
	lastCommitTs: number | null;
	lifecycleStatus?: import("../../stores/workspaceIdentity").WorkspaceLifecycleStatus;
}

/** Orphan worktree (detached HEAD, branch deleted) */
interface OrphanRow {
	repoPath: string;
	repoName: string;
	worktreePath: string;
}

/** Extract display name (last path segment) from a path */
function displayName(path: string): string {
	const segments = path.replace(/\/+$/, "").split("/");
	return segments.at(-1)! || path;
}

/** Action callbacks for worktree row operations */
export interface WorktreeActions {
	onOpenTerminal: (repoPath: string, workspaceId: string) => void;
	onDelete: (repoPath: string, workspaceId: string) => void;
	onMergeAndArchive: (repoPath: string, workspaceId: string) => void;
}

export const WorktreeManager: Component<{ actions?: WorktreeActions }> = (props) => {
	const isOpen = () => worktreeManagerStore.state.isOpen;

	// Escape-to-close is handled centrally (stores/modalStack): registering routes
	// Escape to the manager close AND stops it reaching the terminal underneath.
	createEffect(() => {
		if (!isOpen()) return;
		registerModal(() => worktreeManagerStore.close());
	});

	// Orphan worktree detection (with cancellation guard for rapid open/close)
	const [orphanRows, setOrphanRows] = createSignal<OrphanRow[]>([]);

	createEffect(() => {
		if (!isOpen()) {
			setOrphanRows([]);
			return;
		}
		const allRepos = repositoriesStore.getOrderedRepos();
		const repoFilter = worktreeManagerStore.state.repoFilter;
		// Only subscribe to revisions for repos we're displaying
		const repos = repoFilter ? allRepos.filter((r) => r.path === repoFilter) : allRepos;
		for (const repo of repos) repositoriesStore.getRevision(repo.path);

		let cancelled = false;
		const detectAll = async () => {
			const results = await Promise.allSettled(
				repos.map((repo) =>
					invoke<string[]>("detect_orphan_worktrees", { repoPath: repo.path }).then((paths) =>
						paths.map((wtPath) => ({
							repoPath: repo.path,
							repoName: repo.displayName || displayName(repo.path),
							worktreePath: wtPath,
						})),
					),
				),
			);
			if (cancelled) return;
			const allOrphans: OrphanRow[] = [];
			for (let i = 0; i < results.length; i++) {
				const r = results[i];
				if (r.status === "fulfilled") allOrphans.push(...r.value);
				else appLogger.warn("git", `detect_orphan_worktrees failed for ${repos[i].path}`, r.reason);
			}
			setOrphanRows(allOrphans);
		};
		void detectAll();
		onCleanup(() => {
			cancelled = true;
		});
	});

	async function handlePrune(orphan: OrphanRow) {
		try {
			await invoke("remove_orphan_worktree", { repoPath: orphan.repoPath, worktreePath: orphan.worktreePath });
			setOrphanRows((prev) => prev.filter((o) => o.worktreePath !== orphan.worktreePath));
		} catch (err) {
			appLogger.warn("git", `Failed to prune orphan worktree ${orphan.worktreePath}`, err);
		}
	}

	// All worktrees (unfiltered). Subscribes to revision signals scoped to visible repos.
	const allWorktrees = createMemo<WorktreeRow[]>(() => {
		if (!isOpen()) return [];
		const repos = repositoriesStore.getOrderedRepos();
		const repoFilter = worktreeManagerStore.state.repoFilter;
		// Only subscribe to revisions for repos we're displaying — avoids re-runs when unrelated repos change
		const subscribedRepos = repoFilter ? repos.filter((r) => r.path === repoFilter) : repos;
		for (const repo of subscribedRepos) repositoriesStore.getRevision(repo.path);
		const rows: WorktreeRow[] = [];

		for (const repo of repos) {
			for (const [workspaceId, workspace] of Object.entries(repo.workspaces)) {
				if (!workspace.worktreePath) continue;
				rows.push({
					id: `${repo.path}::${workspaceId}`,
					repoPath: repo.path,
					repoName: repo.displayName || displayName(repo.path),
					workspaceId,
					kind: workspace.isMain ? "main" : "worktree",
					branch: workspace.branchName,
					worktreePath: workspace.worktreePath,
					additions: workspace.additions,
					deletions: workspace.deletions,
					isMain: workspace.isMain,
					prStatus: githubStore.getPrStatus(repo.path, workspace.branchName),
					lastCommitTs: workspace.lastCommitTs ?? null,
					lifecycleStatus: workspace.lifecycleStatus,
				});
			}
		}

		rows.sort((a, b) => {
			if (a.isMain !== b.isMain) return a.isMain ? 1 : -1;
			return a.branch.localeCompare(b.branch);
		});

		return rows;
	});

	// Unique repos for filter pills
	const repoOptions = createMemo(() => {
		const seen = new Map<string, string>();
		for (const wt of allWorktrees()) {
			if (!seen.has(wt.repoPath)) {
				seen.set(wt.repoPath, wt.repoName);
			}
		}
		return Array.from(seen.entries()).map(([path, name]) => ({ path, name }));
	});

	// Filtered worktrees (repo + text)
	const worktrees = createMemo(() => {
		let rows = allWorktrees();
		const repoFilter = worktreeManagerStore.state.repoFilter;
		if (repoFilter) {
			rows = rows.filter((r) => r.repoPath === repoFilter);
		}
		const textFilter = worktreeManagerStore.state.textFilter.toLowerCase();
		if (textFilter) {
			rows = rows.filter((r) => r.branch.toLowerCase().includes(textFilter));
		}
		return rows;
	});

	// Selectable (non-main) worktree IDs from filtered set
	const selectableIds = createMemo(() =>
		worktrees()
			.filter((w) => !w.isMain)
			.map((w) => w.id),
	);

	const selectionCount = () => worktreeManagerStore.state.selectedIds.size;

	// Selection is scoped to currently visible (filtered) worktrees
	const allVisibleSelected = () =>
		selectableIds().length > 0 && selectableIds().every((id) => worktreeManagerStore.state.selectedIds.has(id));

	function handleSelectAll() {
		if (allVisibleSelected()) {
			worktreeManagerStore.clearSelection();
		} else {
			worktreeManagerStore.selectAll(selectableIds());
		}
	}

	/** The rows a batch action applies to.
	 *
	 *  Resolved by looking the selected keys up in the row list, NOT by splitting
	 *  the key back into a repo path and a name. Splitting was already fragile —
	 *  a path may contain the separator — and it cannot survive an opaque id at
	 *  all: the row is the only thing that knows which workspace it is. */
	function selectedRows(): WorktreeRow[] {
		const selected = worktreeManagerStore.state.selectedIds;
		return allWorktrees().filter((row) => selected.has(row.id));
	}

	function handleBatchDelete() {
		if (!props.actions) return;
		for (const row of selectedRows()) props.actions.onDelete(row.repoPath, row.workspaceId);
		worktreeManagerStore.clearSelection();
	}

	function handleBatchMerge() {
		if (!props.actions) return;
		for (const row of selectedRows()) props.actions.onMergeAndArchive(row.repoPath, row.workspaceId);
		worktreeManagerStore.clearSelection();
	}

	return (
		<Show when={isOpen()}>
			<div class={s.overlay} onClick={() => worktreeManagerStore.close()}>
				<div class={s.panel} onClick={(e) => e.stopPropagation()}>
					<div class={s.header}>
						<h3>Worktree Manager</h3>
						<button class={s.close} onClick={() => worktreeManagerStore.close()}>
							&times;
						</button>
					</div>

					<Show when={selectionCount() > 0}>
						<div class={s.batchBar}>
							<span>{selectionCount()} selected</span>
							<button class={s.batchMergeBtn} onClick={handleBatchMerge}>
								Merge &amp; Archive ({selectionCount()})
							</button>
							<button class={s.batchDeleteBtn} onClick={handleBatchDelete}>
								Delete ({selectionCount()})
							</button>
						</div>
					</Show>

					<div class={s.toolbar}>
						<div class={s.toolbarTop}>
							<Show when={selectableIds().length > 1}>
								<input type="checkbox" class={s.selectAll} checked={allVisibleSelected()} onChange={handleSelectAll} />
							</Show>
							<Show when={repoOptions().length > 1}>
								<div class={s.pillsRow}>
									<button
										class={`${s.filterPill} ${!worktreeManagerStore.state.repoFilter ? s.filterPillActive : ""}`}
										onClick={() => worktreeManagerStore.setRepoFilter(null)}
									>
										All
									</button>
									<For each={repoOptions()}>
										{(repo) => (
											<button
												class={`${s.filterPill} ${worktreeManagerStore.state.repoFilter === repo.path ? s.filterPillActive : ""}`}
												onClick={() => worktreeManagerStore.setRepoFilter(repo.path)}
											>
												{repo.name}
											</button>
										)}
									</For>
								</div>
							</Show>
						</div>
						<input
							class={s.searchInput}
							type="text"
							placeholder="Filter worktrees…"
							value={worktreeManagerStore.state.textFilter}
							onInput={(e) => worktreeManagerStore.setTextFilter(e.currentTarget.value)}
							autocomplete="off"
							autocorrect="off"
							spellcheck={false}
						/>
					</div>

					<div class={s.list}>
						<Show when={worktrees().length === 0 && orphanRows().length === 0}>
							<div class={s.empty}>No worktrees found. Create one from the sidebar.</div>
						</Show>

						<Show when={worktrees().length > 0 || orphanRows().length > 0}>
							<div class={s.columnHeader}>
								<span />
								<span>Repo</span>
								<span>Branch / Path</span>
								<span />
								<span>Status</span>
								<span>Committed</span>
								<span />
							</div>
						</Show>

						<For each={worktrees()}>
							{(wt) => {
								const label = () => repoSettingsStore.getEffective(wt.repoPath)?.branchLabels?.[wt.branch];
								return (
									<div class={`${s.row} ${wt.isMain ? s.mainRow : ""}`}>
										{/* Col 1: Checkbox */}
										<Show
											when={!wt.isMain && selectableIds().length > 1}
											fallback={<span class={s.checkboxPlaceholder} />}
										>
											<input
												type="checkbox"
												class={s.rowCheckbox}
												checked={worktreeManagerStore.state.selectedIds.has(wt.id)}
												onChange={() => worktreeManagerStore.toggleSelect(wt.id)}
											/>
										</Show>
										{/* Col 2: Repo */}
										<span class={s.repo}>{wt.repoName}</span>
										{/* Col 3: Branch + worktree path */}
										<div class={s.branchCell}>
											<span class={s.branch}>{label() ?? wt.branch}</span>
											<Show when={label()}>
												<span class={b.subLabel}>{wt.branch}</span>
											</Show>
											<span class={s.worktreePath}>{wt.worktreePath}</span>
										</div>
										{/* Col 4: Badges */}
										<div class={s.metaGroup}>
											<Show when={wt.isMain}>
												<span class={s.mainBadge}>main</span>
											</Show>
											<Show when={wt.lifecycleStatus?.dirtyFiles}>
												{(lost) => (
													<span
														class={s.dirtyBadge}
														title={`${lost()} staged, unstaged or untracked file${lost() === 1 ? "" : "s"} would be discarded by removing this worktree`}
													>
														{lost()} dirty
													</span>
												)}
											</Show>
											<Show
												when={
													wt.lifecycleStatus?.commitStatus === "merged" ||
													wt.lifecycleStatus?.commitStatus === "unknown"
												}
											>
												<span
													class={s.lifecycleBadge}
													title={wt.lifecycleStatus?.error ?? "Backend commit-reachability verdict"}
												>
													{wt.lifecycleStatus?.commitStatus === "unknown" ? "Unknown" : "Merged"}
												</span>
											</Show>
											<Show when={wt.prStatus}>{(pr) => <PrBadge state={pr().state} number={pr().number} />}</Show>
										</div>
										{/* Col 4: Stats */}
										<DirtyStats additions={wt.additions} deletions={wt.deletions} />
										{/* Col 5: Timestamp */}
										<span class={s.timestamp}>{formatRelativeTime(wt.lastCommitTs)}</span>
										{/* Col 6: Actions */}
										<Show when={props.actions} fallback={<span />}>
											{(actions) => (
												<div class={s.actions}>
													<button
														class={s.actionBtn}
														title="Open terminal"
														onClick={() => actions().onOpenTerminal(wt.repoPath, wt.workspaceId)}
													>
														&gt;_
													</button>
													<button
														class={s.actionBtn}
														title="Merge & archive"
														disabled={wt.isMain}
														onClick={() => actions().onMergeAndArchive(wt.repoPath, wt.workspaceId)}
													>
														&#x2714;
													</button>
													<button
														class={`${s.actionBtn} ${s.actionBtnDanger}`}
														title="Delete worktree"
														disabled={wt.isMain}
														onClick={() => actions().onDelete(wt.repoPath, wt.workspaceId)}
													>
														&#x2715;
													</button>
												</div>
											)}
										</Show>
									</div>
								);
							}}
						</For>

						<For each={orphanRows()}>
							{(orphan) => (
								<div class={`${s.row} ${s.orphanRow}`}>
									<span class={s.checkboxPlaceholder} />
									<span class={s.repo}>{orphan.repoName}</span>
									<div class={s.branchCell}>
										<span class={s.branch}>{displayName(orphan.worktreePath)}</span>
										<span class={s.worktreePath}>{orphan.worktreePath}</span>
									</div>
									<div class={s.metaGroup}>
										<span class={s.orphanBadge}>orphan</span>
									</div>
									<span />
									<span />
									<button class={s.pruneBtn} onClick={() => handlePrune(orphan)}>
										Prune
									</button>
								</div>
							)}
						</For>
					</div>

					<div class={s.footer}>
						<span>{worktrees().length} worktree(s)</span>
						<span class={s.footerRight}>Esc to close</span>
					</div>
				</div>
			</div>
		</Show>
	);
};

/** Compact PR state badge */
const PrBadge: Component<{ state: string; number: number }> = (props) => {
	const st = () => props.state.toLowerCase();
	const cls = () => {
		if (st() === "merged") return s.prMerged;
		if (st() === "closed") return s.prClosed;
		return s.prOpen;
	};
	const label = () => {
		if (st() === "merged") return "Merged";
		if (st() === "closed") return "Closed";
		return `#${props.number}`;
	};
	return <span class={`${s.prBadge} ${cls()}`}>{label()}</span>;
};

/** Compact dirty stats badge */
const DirtyStats: Component<{ additions: number; deletions: number }> = (props) => {
	const isDirty = () => props.additions > 0 || props.deletions > 0;
	return (
		<span class={s.stats}>
			<Show when={isDirty()} fallback={<span class={s.statsClean}>clean</span>}>
				<span class={s.statsAdded}>+{props.additions}</span> <span class={s.statsRemoved}>-{props.deletions}</span>
			</Show>
		</span>
	);
};
