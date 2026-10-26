import { SplitSide } from "@git-diff-view/solid";
import { type Component, createEffect, createMemo, createSignal, For, Index, on, onMount, Show } from "solid-js";
import { t } from "../../i18n";
import { invoke } from "../../invoke";
import { type DiffViewMode, uiStore } from "../../stores/ui";
import { classifyDiffFile, parseLinguistGenerated } from "../../utils/diffFileScope";
import { DiffFileList } from "../shared/DiffFileList";
import { Button } from "../ui/Button";
import { type DiffFileSection, type DiffViewerProps, parseDiffFiles } from "../ui/DiffViewer";
import { DraftCommentView, NewDraftForm } from "./DraftCommentView";
import s from "./PrDiffTab.module.css";
import { ReviewSubmitBar } from "./ReviewSubmitBar";
import { ReviewThreadView } from "./ReviewThreadView";
import { groupByFile, type LineAnnotation } from "./reviewThreads";
import { createReviewSession } from "./useReviewSession";

export interface PrDiffTabProps {
	prNumber: number;
	prTitle: string;
	diff: string;
	/** Head the diff was fetched at; drafts are pinned to it. Changes when the diff is reopened. */
	headSha?: string;
	/** Reports a head pinned by the first thread read when `headSha` was not supplied. */
	onPin?: (headSha: string) => void;
	/** Local checkout, used to read `.gitattributes` for `linguist-generated` patterns. */
	repoPath?: string;
}

export const PrDiffTab: Component<PrDiffTabProps> = (props) => {
	const files = createMemo(() => parseDiffFiles(props.diff));
	const totalAdd = createMemo(() => files().reduce((sum, f) => sum + f.additions, 0));
	const totalDel = createMemo(() => files().reduce((sum, f) => sum + f.deletions, 0));

	const [generatedPatterns, setGeneratedPatterns] = createSignal<string[]>([]);
	// Best effort: the local checkout's .gitattributes stands in for the PR base's.
	onMount(() => {
		if (!props.repoPath) return;
		invoke<string>("read_file", { path: props.repoPath, file: ".gitattributes" })
			.then((text) => setGeneratedPatterns(parseLinguistGenerated(text)))
			.catch(() => {});
	});
	const collapsedByDefault = (file: DiffFileSection) => classifyDiffFile(file.path, generatedPatterns()) !== null;
	const collapsedCount = createMemo(() => files().filter(collapsedByDefault).length);

	// Line threads and the draft review need the checkout path to resolve the GitHub account.
	const session = props.repoPath
		? createReviewSession(props.repoPath, props.prNumber, props.headSha, (sha) => props.onPin?.(sha))
		: undefined;
	onMount(() => void session?.refresh());
	// Reopening the diff swaps its head on the live tab; the session must follow it.
	createEffect(
		on(
			() => props.headSha,
			(head) => {
				if (session && head !== session.headSha()) void session.reset(head);
			},
			{ defer: true },
		),
	);
	const byFile = createMemo(() => groupByFile(session?.threads() ?? [], session?.drafts() ?? []));

	const renderLine: NonNullable<DiffViewerProps["renderExtendLine"]> = ({ data }) => {
		const line = data as LineAnnotation;
		return (
			<>
				<Index each={line.threads}>
					{(thread) => session && <ReviewThreadView thread={thread()} session={session} />}
				</Index>
				<For each={line.drafts}>
					{(draft) => <DraftCommentView draft={draft} onRemove={() => session?.removeDraft(draft.key)} />}
				</For>
			</>
		);
	};
	// One stable render function per file: DiffView re-renders when the function identity changes.
	const widgetRenderers = new Map<string, NonNullable<DiffViewerProps["renderWidgetLine"]>>();
	const widgetFor = (path: string) => {
		let render = widgetRenderers.get(path);
		if (!render) {
			render = ({ side, lineNumber, onClose }) => (
				<NewDraftForm
					disabled={session?.headMoved() ?? true}
					onClose={onClose}
					onAdd={(body) =>
						session?.addDraft({ path, line: lineNumber, side: side === SplitSide.old ? "LEFT" : "RIGHT", body })
					}
				/>
			);
			widgetRenderers.set(path, render);
		}
		return render;
	};
	const viewerProps = (file: DiffFileSection): Omit<DiffViewerProps, "diff" | "mode"> =>
		session
			? {
					// Never undefined: the viewer keeps stale rows when annotations disappear into undefined.
					extendData: byFile().get(file.path)?.lines ?? {},
					renderExtendLine: renderLine,
					diffViewAddWidget: !session.headMoved(),
					renderWidgetLine: widgetFor(file.path),
				}
			: {};
	const fileExtras = (file: DiffFileSection) => (
		<Index each={byFile().get(file.path)?.outdated}>
			{(thread) => session && <ReviewThreadView thread={thread()} session={session} />}
		</Index>
	);

	const mode = (): DiffViewMode => uiStore.state.diffViewMode;

	const header = () => (
		<div class={s.header}>
			<span class={s.headerTitle}>
				#{props.prNumber} {props.prTitle}
			</span>
			<span class={s.headerStats}>
				{files().length} {t("prDiff.files", "files")} <span class={s.statAdd}>+{totalAdd()}</span>{" "}
				<span class={s.statDel}>-{totalDel()}</span>
			</span>
			<Show when={collapsedCount() > 0}>
				<span
					class={s.headerStats}
					title={t("prDiff.collapsedHint", "Lockfiles, generated and test files; click a file to expand")}
				>
					{collapsedCount()} {t("prDiff.collapsed", "collapsed")}
				</span>
			</Show>
			<div class={s.modeToggle}>
				<Button
					variant="ghost"
					size="compact"
					aria-pressed={mode() === "split"}
					onClick={() => uiStore.setDiffViewMode("split")}
					title={t("diffTab.splitView", "Side-by-side")}
				>
					<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
						<path d="M1 2h6v12H1V2zm8 0h6v12H9V2zM2 3v10h4V3H2zm8 0v10h4V3h-4z" />
					</svg>
				</Button>
				<Button
					variant="ghost"
					size="compact"
					aria-pressed={mode() === "unified"}
					onClick={() => uiStore.setDiffViewMode("unified")}
					title={t("diffTab.unifiedView", "Inline")}
				>
					<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
						<path d="M1 2h14v12H1V2zm1 1v10h12V3H2z" />
					</svg>
				</Button>
			</div>
		</div>
	);

	return (
		<Show
			when={files().length > 0}
			fallback={
				<div class={s.container}>
					{header()}
					<div class={s.emptyState}>{t("prDiff.empty", "No changes")}</div>
				</div>
			}
		>
			<DiffFileList
				files={files()}
				mode={mode()}
				header={
					<>
						{header()}
						<Show when={session}>{(active) => <ReviewSubmitBar session={active()} />}</Show>
					</>
				}
				collapsedByDefault={collapsedByDefault}
				viewerProps={viewerProps}
				fileExtras={fileExtras}
			/>
		</Show>
	);
};
