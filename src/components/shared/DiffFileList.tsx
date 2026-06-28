import { createVirtualizer } from "@tanstack/solid-virtual";
import { type Component, createSignal, For, type JSX, Show } from "solid-js";
import type { DiffViewMode } from "../../stores/ui";
import { cx } from "../../utils";
import { onClickKeyDown } from "../../utils/a11y";
import { type DiffFileSection, DiffViewer } from "../ui/DiffViewer";
import s from "./diffFileList.module.css";

/** Reconstruct the raw diff string for a single file section. */
export function sectionToRawDiff(section: DiffFileSection): string {
	return section.lines.map((l) => l.content).join("\n");
}

/** A single collapsible file diff. The chevron and header toggle collapse; the
 *  file path opens the file when `onOpen` is provided (working-tree view). */
const FileSection: Component<{ file: DiffFileSection; mode: DiffViewMode; onOpen?: () => void }> = (props) => {
	const [collapsed, setCollapsed] = createSignal(false);

	return (
		<div class={s.fileSection}>
			<div class={s.fileHeader} role="button" tabIndex={0} onClick={() => setCollapsed(!collapsed())} onKeyDown={onClickKeyDown(() => setCollapsed(!collapsed()))}>
				<svg
					class={cx(s.chevron, collapsed() && s.chevronCollapsed)}
					width="12"
					height="12"
					viewBox="0 0 16 16"
					fill="currentColor"
				>
					<path d="M4 6l4 4 4-4" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
				</svg>
				<span
					class={s.filePath}
					onClick={(e) => {
						if (!props.onOpen) return;
						e.stopPropagation();
						props.onOpen();
					}}
					style={props.onOpen ? { cursor: "pointer" } : undefined}
				>
					{props.file.path}
				</span>
				<span class={s.fileStats}>
					<Show when={props.file.additions > 0}>
						<span class={s.statAdd}>+{props.file.additions}</span>
					</Show>
					<Show when={props.file.deletions > 0}>
						<span class={s.statDel}>-{props.file.deletions}</span>
					</Show>
				</span>
			</div>
			<Show when={!collapsed()}>
				<div class={s.fileDiff}>
					<DiffViewer diff={sectionToRawDiff(props.file)} mode={props.mode} />
				</div>
			</Show>
		</div>
	);
};

export interface DiffFileListProps {
	files: DiffFileSection[];
	mode: DiffViewMode;
	/** When provided, clicking a file path opens it (working-tree view). */
	onOpenFile?: (path: string) => void;
	/** Exposes the scroll container element (for Cmd+F search). */
	scrollRef?: (el: HTMLElement) => void;
	/** Optional content rendered above the list (sticky summary header). */
	header?: JSX.Element;
}

/**
 * Virtualized list of per-file diffs. Only sections inside the scroll viewport
 * (plus overscan) are mounted — a 100-file diff parses + renders ~5 DiffViewers
 * instead of 100. Items use `top` (not `transform`) positioning so the per-file
 * `position: sticky` headers keep working.
 *
 * DEFERRED (2026-06-07) — Cmd+F via DomSearchEngine only matches *mounted*
 * sections in this virtualized list (off-screen files aren't in the DOM). A
 * complete fix needs data-level search over parseDiffFiles output + scroll-to;
 * that's a separate change. Surfaced here rather than degrading silently.
 */
export const DiffFileList: Component<DiffFileListProps> = (props) => {
	let scrollEl: HTMLDivElement | undefined;

	const virtualizer = createVirtualizer({
		get count() {
			return props.files.length;
		},
		getScrollElement: () => scrollEl ?? null,
		estimateSize: () => 320,
		overscan: 3,
		getItemKey: (i) => props.files[i]?.path ?? i,
	});

	return (
		<div
			class={s.container}
			ref={(el) => {
				scrollEl = el;
				props.scrollRef?.(el);
			}}
		>
			{props.header}
			<div style={{ height: `${virtualizer.getTotalSize()}px`, position: "relative", width: "100%" }}>
				<For each={virtualizer.getVirtualItems()}>
					{(vi) => (
						<div
							data-index={vi.index}
							ref={(el) => virtualizer.measureElement(el)}
							style={{ position: "absolute", top: `${vi.start}px`, left: "0", width: "100%" }}
						>
							<FileSection
								file={props.files[vi.index]}
								mode={props.mode}
								onOpen={props.onOpenFile ? () => props.onOpenFile?.(props.files[vi.index].path) : undefined}
							/>
						</div>
					)}
				</For>
			</div>
		</div>
	);
};
