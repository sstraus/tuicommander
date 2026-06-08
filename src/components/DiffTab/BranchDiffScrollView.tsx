import { type Component, createEffect, createMemo, createSignal, Show } from "solid-js";
import { useRepository } from "../../hooks/useRepository";
import { t } from "../../i18n";
import { repositoriesStore } from "../../stores/repositories";
import { type DiffViewMode, uiStore } from "../../stores/ui";
import { openFileAction } from "../../utils/filePreview";
import s from "../PrDiffTab/PrDiffTab.module.css";
import { DiffFileList } from "../shared/DiffFileList";
import { parseDiffFiles } from "../ui/DiffViewer";

export interface BranchDiffScrollViewProps {
	repoPath: string;
	/** Pass a ref callback to get the scroll container for Cmd+F search */
	contentRef?: (el: HTMLElement) => void;
}

/**
 * All-files diff scroll view for the current working tree.
 * Shows every changed file as a collapsible section in a continuous scroll.
 * Reactively reloads on git operations via repositoriesStore.getRevision.
 */
export const BranchDiffScrollView: Component<BranchDiffScrollViewProps> = (props) => {
	const repo = useRepository();
	const [diff, setDiff] = createSignal("");
	const [loading, setLoading] = createSignal(true);
	const [error, setError] = createSignal<string | null>(null);

	// Reactively reload when git state changes
	createEffect(() => {
		const repoPath = props.repoPath;
		if (!repoPath) return;
		// Track revision for reactivity
		void repositoriesStore.getRevision(repoPath);

		if (!diff()) setLoading(true);
		setError(null);
		// Fetch both unstaged and staged diffs, concatenate for a full picture
		Promise.all([repo.getDiff(repoPath), repo.getDiff(repoPath, "staged")])
			.then(([unstaged, staged]) => {
				// Concatenate: staged first, then unstaged (avoids duplicate files
				// since git diff and git diff --cached don't overlap)
				setDiff([staged, unstaged].filter(Boolean).join("\n"));
				setLoading(false);
			})
			.catch((err) => {
				setError(String(err));
				setLoading(false);
			});
	});

	const files = createMemo(() => parseDiffFiles(diff()).filter((f) => f.additions > 0 || f.deletions > 0));
	const totalAdd = createMemo(() => files().reduce((sum, f) => sum + f.additions, 0));
	const totalDel = createMemo(() => files().reduce((sum, f) => sum + f.deletions, 0));

	// In scroll mode, each DiffViewer uses unified or split (not "scroll" which DiffViewer doesn't understand)
	const baseMode = (): DiffViewMode => {
		const m = uiStore.state.diffViewMode;
		return m === "scroll" ? "unified" : m;
	};

	const summaryHeader = () => (
		<div class={s.header}>
			<span class={s.headerTitle}>{t("diffScroll.title", "All Changes")}</span>
			<span class={s.headerStats}>
				{files().length} {t("diffScroll.files", "files")} <span class={s.statAdd}>+{totalAdd()}</span>{" "}
				<span class={s.statDel}>-{totalDel()}</span>
			</span>
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
						<div class={s.emptyState}>{t("diffScroll.noChanges", "No uncommitted changes")}</div>
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
