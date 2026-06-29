import { type Component, createEffect, createMemo, createSignal, For, Show } from "solid-js";
import { aiTriageStore, type FileClassification, type Relevance, type TriageStats } from "../../stores/aiTriageStore";
import { diffTabsStore } from "../../stores/diffTabs";
import { editorTabsStore } from "../../stores/editorTabs";
import { repositoriesStore } from "../../stores/repositories";
import { cx } from "../../utils";
import { onClickKeyDown } from "../../utils/a11y";
import p from "../shared/panel.module.css";
import { PanelResizeHandle } from "../ui/PanelResizeHandle";
import s from "./AiTriagePanel.module.css";

function relevanceClass(r: Relevance): string {
	if (r === "high") return s.relevanceHigh;
	if (r === "medium") return s.relevanceMedium;
	return s.relevanceLow;
}

function statClass(r: Relevance): string {
	if (r === "high") return s.statHigh;
	if (r === "medium") return s.statMedium;
	return s.statLow;
}

function formatCategory(cat: string): string {
	return cat.replace(/-/g, " ");
}

function shortPath(path: string): string {
	const parts = path.split("/");
	if (parts.length <= 2) return path;
	return parts.slice(-2).join("/");
}

export interface AiTriagePanelProps {
	visible: boolean;
	repoPath: string | null;
	onClose: () => void;
}

export const AiTriagePanel: Component<AiTriagePanelProps> = (props) => {
	createEffect(() => {
		if (!props.visible || !props.repoPath) return;
		const rev = repositoriesStore.getRevision(props.repoPath);
		void rev;
		aiTriageStore.runTriage(props.repoPath);
	});

	const state = () =>
		props.repoPath
			? aiTriageStore.getState(props.repoPath)
			: {
					summary: null,
					files: [],
					loading: false,
					llmUsed: false,
					llmModel: null,
					error: null,
					stats: { llmClassified: 0, cached: 0, heuristic: 0, fallback: 0 } as TriageStats,
				};

	const statsLine = () => {
		const st = state().stats;
		const total = state().files.length;
		if (total === 0) return null;
		const parts: string[] = [];
		if (st.llmClassified > 0) parts.push(`${st.llmClassified} AI`);
		if (st.cached > 0) parts.push(`${st.cached} cached`);
		if (st.heuristic > 0) parts.push(`${st.heuristic} auto`);
		if (st.fallback > 0) parts.push(`${st.fallback} fallback`);
		const model = state().llmModel;
		const detail = parts.length > 0 ? parts.join(", ") : "";
		return `${total} files` + (detail ? ` (${detail})` : "") + (model ? ` · ${model}` : "");
	};

	const highFiles = createMemo(() => state().files.filter((f) => f.relevance === "high"));
	const mediumFiles = createMemo(() => state().files.filter((f) => f.relevance === "medium"));
	const lowFiles = createMemo(() => state().files.filter((f) => f.relevance === "low"));

	const [lowGroupOpen, setLowGroupOpen] = createSignal(false);

	function handleEdit(path: string) {
		if (props.repoPath) editorTabsStore.add(props.repoPath, path);
	}

	function handleDiff(path: string) {
		if (props.repoPath) diffTabsStore.add(props.repoPath, path, "M");
	}

	function handleRefresh() {
		if (props.repoPath) aiTriageStore.refreshTriage(props.repoPath);
	}

	const FileRow: Component<{ file: FileClassification }> = (rowProps) => {
		const file = rowProps.file;
		const hasSummary = () => file.summary && file.summary.length > 0;

		return (
			<div class={s.fileRow}>
				<div class={s.fileHeader}>
					<div class={s.fileHeaderTop}>
						<span class={cx(s.relevanceBadge, relevanceClass(file.relevance))}>{file.relevance}</span>
						<Show when={hasSummary()} fallback={<span class={s.fileSummary}>{shortPath(file.path)}</span>}>
							<span class={s.fileSummary}>{file.summary}</span>
						</Show>
						<div class={s.fileActions}>
							<button class={s.actionBtn} onClick={() => handleDiff(file.path)} title="View diff">
								Diff
							</button>
							<button class={s.actionBtn} onClick={() => handleEdit(file.path)} title="Open in editor">
								Edit
							</button>
						</div>
					</div>
					<div class={s.fileHeaderBottom}>
						<span class={s.filePath}>{file.path}</span>
						<span class={s.categoryPill}>{formatCategory(file.category)}</span>
						<span class={s.fileStats}>
							<Show when={file.additions > 0}>
								<span class={s.statsAdd}>+{file.additions}</span>
							</Show>
							<Show when={file.additions > 0 && file.deletions > 0}> </Show>
							<Show when={file.deletions > 0}>
								<span class={s.statsDel}>-{file.deletions}</span>
							</Show>
						</span>
					</div>
				</div>
			</div>
		);
	};

	return (
		<div id="ai-triage-panel" class={cx(s.panel, !props.visible && s.hidden)}>
			<PanelResizeHandle panelId="ai-triage-panel" />
			<div class={p.header}>
				<div class={p.headerLeft}>
					<span class={p.title}>AI Triage</span>
					<Show when={highFiles().length > 0}>
						<span class={cx(s.statBadge, statClass("high"))}>{highFiles().length} high</span>
					</Show>
					<Show when={mediumFiles().length > 0}>
						<span class={cx(s.statBadge, statClass("medium"))}>{mediumFiles().length} med</span>
					</Show>
					<Show when={lowFiles().length > 0}>
						<span class={cx(s.statBadge, statClass("low"))}>{lowFiles().length} low</span>
					</Show>
					<Show when={state().loading}>
						<span class={s.spinner} />
					</Show>
				</div>
				<div class={p.headerRight}>
					<button class={s.refreshBtn} onClick={handleRefresh}>
						Refresh
					</button>
					<button class={p.close} onClick={props.onClose}>
						&times;
					</button>
				</div>
			</div>

			<div class={s.content}>
				<Show when={state().error}>
					<div class={s.error}>{state().error}</div>
				</Show>

				<Show when={statsLine()}>
					<div class={s.statsLine}>{statsLine()}</div>
				</Show>

				<Show when={!state().loading && state().files.length === 0 && !state().error}>
					<div class={s.empty}>No changes detected</div>
				</Show>

				<For each={highFiles()}>{(file) => <FileRow file={file} />}</For>

				<For each={mediumFiles()}>{(file) => <FileRow file={file} />}</For>

				<Show when={lowFiles().length > 0}>
					<div class={s.lowGroup}>
						<div
							class={s.lowGroupHeader}
							role="button"
							tabIndex={0}
							onClick={() => setLowGroupOpen(!lowGroupOpen())}
							onKeyDown={onClickKeyDown(() => setLowGroupOpen(!lowGroupOpen()))}
						>
							<span class={cx(s.chevron, lowGroupOpen() && s.chevronOpen)}>&#9656;</span>
							{lowFiles().length} low-relevance files
						</div>
						<Show when={lowGroupOpen()}>
							<div class={s.lowGroupContent}>
								<For each={lowFiles()}>{(file) => <FileRow file={file} />}</For>
							</div>
						</Show>
					</div>
				</Show>
			</div>
		</div>
	);
};
