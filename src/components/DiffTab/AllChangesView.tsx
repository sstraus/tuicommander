import { type Component, createEffect, createMemo, createSignal, For, on, Show, untrack } from "solid-js";
import { useRepository } from "../../hooks/useRepository";
import { t } from "../../i18n";
import { invoke } from "../../invoke";
import { appLogger } from "../../stores/appLogger";
import { repositoriesStore } from "../../stores/repositories";
import { type DiffViewMode, uiStore } from "../../stores/ui";
import { cx } from "../../utils";
import { openFileAction } from "../../utils/filePreview";
import type { CommitLogEntry } from "../GitPanel/types";
import s from "../PrDiffTab/PrDiffTab.module.css";
import { DiffFileList } from "../shared/DiffFileList";
import { parseDiffFiles } from "../ui/DiffViewer";
import { SegmentedControl } from "../ui/SegmentedControl";
import { Select } from "../ui/Select";
import { Switch } from "../ui/Switch";
import v from "./AllChangesView.module.css";

export interface AllChangesViewProps {
	repoPath: string;
	/** Pass a ref callback to get the scroll container for Cmd+F search */
	contentRef?: (el: HTMLElement) => void;
}

export type AllChangesScope = "uncommitted" | "branch" | "commits";

const COMMIT_PAGE = 50;

/**
 * A base chosen in the selector lives for the app session only: it is never
 * written to `tuicommander-base`, which also drives ahead/behind and
 * "update from base". Keyed by repository and branch.
 */
const sessionBases = new Map<string, string>();

/**
 * Selected commits by hash: the log reloads on every revision bump, and a new
 * commit shifts every index, so an index would silently point at another commit.
 */
interface CommitSelection {
	anchor: string;
	end: string;
}

/**
 * All changes of a repository in one continuous scroll, in one of three scopes:
 * uncommitted work, the branch against its base, or a commit / commit range.
 */
export const AllChangesView: Component<AllChangesViewProps> = (props) => {
	const repo = useRepository();
	const [branch, setBranch] = createSignal<string | null>(null);
	const [baseOptions, setBaseOptions] = createSignal<string[]>([]);
	const [base, setBase] = createSignal<string | null>(null);
	const [scope, setScope] = createSignal<AllChangesScope | null>(null);
	const [includeUncommitted, setIncludeUncommitted] = createSignal(false);
	const [commits, setCommits] = createSignal<CommitLogEntry[]>([]);
	const [selection, setSelection] = createSignal<CommitSelection | null>(null);
	const [diff, setDiff] = createSignal("");
	const [loading, setLoading] = createSignal(true);
	const [error, setError] = createSignal<string | null>(null);

	const sessionKey = () => `${props.repoPath}\0${branch() ?? ""}`;

	// Resolve branch, base and the default scope once per repository.
	// DEFERRED (2026-11-07) — a checkout while the view stays open keeps the old branch
	// and its session base key until the view remounts; not observed, not verified.
	createEffect(
		on(
			() => props.repoPath,
			async (repoPath) => {
				if (!repoPath) return;
				setScope(null);
				setSelection(null);
				try {
					const [info, options] = await Promise.all([repo.getInfo(repoPath), repo.listBaseRefOptions(repoPath)]);
					if (repoPath !== props.repoPath) return;
					const current = info.branch || null;
					const stored = current
						? await invoke<string | null>("get_branch_base", { path: repoPath, branchName: current }).catch(() => null)
						: null;
					if (repoPath !== props.repoPath) return;
					const fallback = options.find((o) => o.is_default)?.name ?? options[0]?.name ?? null;
					setBranch(current);
					const chosen = sessionBases.get(`${repoPath}\0${current ?? ""}`) ?? stored ?? fallback;
					const names = options.map((o) => o.name);
					setBaseOptions(chosen && !names.includes(chosen) ? [chosen, ...names] : names);
					setBase(chosen);
					setScope(chosen && current && current !== chosen ? "branch" : "uncommitted");
				} catch (err) {
					appLogger.warn("git", "All Changes: cannot resolve the branch base", { error: String(err) });
					setBase(null);
					setScope("uncommitted");
				}
			},
		),
	);

	/** The diff scope string of the backend grammar, or null when nothing is selected. */
	const diffScope = createMemo((): string | null => {
		switch (scope()) {
			case "uncommitted":
				return "head";
			case "branch": {
				const b = base();
				if (!b) return null;
				return includeUncommitted() ? `base-uncommitted:${b}` : `base:${b}`;
			}
			case "commits": {
				const sel = selection();
				const list = commits();
				if (!sel) return null;
				const a = list.findIndex((c) => c.hash === sel.anchor);
				const b = list.findIndex((c) => c.hash === sel.end);
				if (a < 0 || b < 0) return null;
				const newest = list[Math.min(a, b)].hash;
				const oldest = list[Math.max(a, b)].hash;
				return newest === oldest ? newest : `range:${oldest}..${newest}`;
			}
			default:
				return null;
		}
	});

	// Commit list for the Commit / range scope.
	createEffect(() => {
		const repoPath = props.repoPath;
		if (!repoPath || scope() !== "commits") return;
		void repositoriesStore.getRevision(repoPath);
		invoke<CommitLogEntry[]>("get_commit_log", { path: repoPath, count: COMMIT_PAGE })
			.then((list) => {
				if (repoPath === props.repoPath) setCommits(list);
			})
			.catch((err) => appLogger.warn("git", "All Changes: cannot load the commit log", { error: String(err) }));
	});

	// Reactively reload when git state or the scope changes.
	let diffGen = 0;
	let shownScope: string | null = null;
	createEffect(() => {
		const repoPath = props.repoPath;
		const currentScope = diffScope();
		if (!repoPath) return;
		void repositoriesStore.getRevision(repoPath);
		// A revision-bump burst can fire several loads; only the newest may settle
		// state, so a slow earlier fetch can't overwrite a fresher diff.
		const gen = ++diffGen;
		setError(null);
		if (currentScope === null) {
			shownScope = null;
			setDiff("");
			setLoading(scope() === null);
			return;
		}
		// A new scope must not show the previous scope's diff under its label;
		// a revision bump of the same scope keeps the old diff until the new one.
		if (currentScope !== shownScope) setDiff("");
		shownScope = currentScope;
		if (!untrack(diff)) setLoading(true);
		repo
			.getDiff(repoPath, currentScope)
			.then((text) => {
				if (gen !== diffGen) return;
				setDiff(text);
				setLoading(false);
			})
			.catch((err) => {
				if (gen !== diffGen) return;
				setError(String(err));
				setLoading(false);
			});
	});

	// DEFERRED (2026-11-07) — binary, rename-only and mode-only files have no +/- lines and stay
	// hidden (filter kept from the old view); branch and range scopes show them more often.
	const files = createMemo(() => parseDiffFiles(diff()).filter((f) => f.additions > 0 || f.deletions > 0));
	const totalAdd = createMemo(() => files().reduce((sum, f) => sum + f.additions, 0));
	const totalDel = createMemo(() => files().reduce((sum, f) => sum + f.deletions, 0));

	// In scroll mode, each DiffViewer uses unified or split (not "scroll" which DiffViewer doesn't understand)
	const baseMode = (): DiffViewMode => {
		const m = uiStore.state.diffViewMode;
		return m === "scroll" ? "unified" : m;
	};

	const chooseBase = (name: string) => {
		sessionBases.set(sessionKey(), name);
		setBase(name);
	};

	const selectCommit = (hash: string, extend: boolean) => {
		const current = selection();
		setSelection(extend && current ? { anchor: current.anchor, end: hash } : { anchor: hash, end: hash });
	};

	const isSelected = (index: number) => {
		const sel = selection();
		if (!sel) return false;
		const list = commits();
		const a = list.findIndex((c) => c.hash === sel.anchor);
		const b = list.findIndex((c) => c.hash === sel.end);
		return a >= 0 && b >= 0 && index >= Math.min(a, b) && index <= Math.max(a, b);
	};

	const emptyText = () => {
		if (scope() === "branch") return t("diffScroll.noBranchChanges", "No changes against the base");
		if (scope() === "commits") return t("diffScroll.selectCommit", "Select a commit, Shift-click for a range");
		return t("diffScroll.noChanges", "No uncommitted changes");
	};

	const summaryHeader = () => (
		<div>
			<div class={s.header}>
				<span class={s.headerTitle}>{t("diffScroll.title", "All Changes")}</span>
				<span class={s.headerStats}>
					{files().length} {t("diffScroll.files", "files")} <span class={s.statAdd}>+{totalAdd()}</span>{" "}
					<span class={s.statDel}>-{totalDel()}</span>
				</span>
			</div>
			<div class={v.scopeBar}>
				<SegmentedControl<AllChangesScope>
					aria-label={t("diffScroll.scope", "Scope")}
					value={scope() ?? undefined}
					onChange={setScope}
					options={[
						{ id: "uncommitted", label: t("diffScroll.scopeUncommitted", "Uncommitted") },
						{ id: "branch", label: t("diffScroll.scopeBranch", "Branch vs base"), disabled: !base() },
						{ id: "commits", label: t("diffScroll.scopeCommits", "Commit / range") },
					]}
				/>
				<Show when={scope() === "branch"}>
					<Select
						compact
						aria-label={t("diffScroll.base", "Base")}
						value={base() ?? ""}
						onChange={(e) => chooseBase(e.currentTarget.value)}
					>
						<For each={baseOptions()}>{(name) => <option value={name}>{name}</option>}</For>
					</Select>
					<label class={v.toggle}>
						<Switch
							role="checkbox"
							checked={includeUncommitted()}
							onChange={(e) => setIncludeUncommitted(e.currentTarget.checked)}
						/>
						{t("diffScroll.includeUncommitted", "Include uncommitted")}
					</label>
				</Show>
			</div>
			<Show when={scope() === "commits"}>
				<div
					class={v.commitList}
					role="listbox"
					aria-multiselectable="true"
					aria-label={t("diffScroll.commits", "Commits")}
				>
					<For each={commits()}>
						{(commit, index) => (
							<button
								type="button"
								role="option"
								data-control-layout="row"
								aria-selected={isSelected(index())}
								class={cx(v.commitRow, isSelected(index()) && v.commitRowSelected)}
								onClick={(e) => selectCommit(commit.hash, e.shiftKey)}
							>
								<span class={v.commitHash}>{commit.hash.slice(0, 7)}</span>
								<span class={v.commitSubject}>{commit.subject}</span>
							</button>
						)}
					</For>
				</div>
			</Show>
		</div>
	);

	return (
		<Show
			when={!loading() && !error() && files().length > 0}
			fallback={
				<div class={s.container} ref={props.contentRef}>
					{summaryHeader()}
					<Show when={loading()}>
						<div class={s.emptyState}>{t("diffTab.loading", "Loading diff...")}</div>
					</Show>
					<Show when={error()}>
						<div class={s.emptyState}>
							{t("diffTab.error", "Error:")} {error()}
						</div>
					</Show>
					<Show when={!loading() && !error() && files().length === 0}>
						<div class={s.emptyState}>{emptyText()}</div>
					</Show>
				</div>
			}
		>
			<DiffFileList
				files={files()}
				mode={baseMode()}
				onOpenFile={(path) => openFileAction(path, props.repoPath)}
				scrollRef={props.contentRef}
				header={summaryHeader()}
			/>
		</Show>
	);
};
