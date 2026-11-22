import { type Component, createMemo, createSignal, onCleanup, onMount, Show } from "solid-js";
import { t } from "../../i18n";
import { type WaitingEntry, waitingEntries } from "../../stores/needsYou";
import { repositoriesStore } from "../../stores/repositories";
import { navigateToTerminal } from "../../utils/navigateToTerminal";
import s from "./Sidebar.module.css";

type Placement = "visible" | "above" | "below" | "hidden";

function rowFor(scroller: HTMLElement, entry: WaitingEntry): HTMLElement | null {
	if (!entry.repoPath || !entry.branchName) return null;
	const section = scroller.querySelector(`[data-sidebar-repo="${CSS.escape(entry.repoPath)}"]`);
	return section?.querySelector<HTMLElement>(`[data-sidebar-branch="${CSS.escape(entry.branchName)}"]`) ?? null;
}

/** Where a waiting session's sidebar row is relative to the visible part of the list. */
export function placementOf(row: HTMLElement | null, viewport: DOMRect): Placement {
	if (!row) return "hidden";
	const rect = row.getBoundingClientRect();
	if (rect.height === 0) return "hidden";
	if (rect.bottom <= viewport.top) return "above";
	if (rect.top >= viewport.bottom) return "below";
	return "visible";
}

/**
 * Sticky banner above the repository list, shown only while a waiting session's row
 * is out of sight: repository closed, filtered out, or scrolled away. Click reveals
 * and opens the oldest one.
 */
export const NeedsYouBanner: Component<{ scroller: () => HTMLElement | undefined }> = (props) => {
	const [layout, setLayout] = createSignal(0);
	let frame = 0;
	// Sidebar spinners mutate the list constantly: one recompute per frame at most.
	const bump = () => {
		if (frame) return;
		frame = requestAnimationFrame(() => {
			frame = 0;
			setLayout((n) => n + 1);
		});
	};
	onCleanup(() => cancelAnimationFrame(frame));

	onMount(() => {
		const scroller = props.scroller();
		if (!scroller) return;
		scroller.addEventListener("scroll", bump, { passive: true });
		const resize = new ResizeObserver(bump);
		resize.observe(scroller);
		// Repos expanding or collapsing change which rows exist without a scroll.
		const mutations = new MutationObserver(bump);
		mutations.observe(scroller, { childList: true, subtree: true });
		onCleanup(() => {
			scroller.removeEventListener("scroll", bump);
			resize.disconnect();
			mutations.disconnect();
		});
	});

	const hidden = createMemo(() => {
		layout();
		const scroller = props.scroller();
		const entries = waitingEntries();
		if (!scroller || entries.length === 0) return [];
		const viewport = scroller.getBoundingClientRect();
		return entries
			.map((entry) => ({ entry, placement: placementOf(rowFor(scroller, entry), viewport) }))
			.filter((item) => item.placement !== "visible");
	});

	const names = () => [...new Set(hidden().map((item) => item.entry.repoName ?? item.entry.id))].join(", ");
	const arrow = () => {
		const first = hidden()[0]?.placement;
		return first === "above" ? "↑" : "↓";
	};

	const reveal = () => {
		const target = hidden()[0]?.entry;
		if (!target) return;
		const repo = target.repoPath ? repositoriesStore.state.repositories[target.repoPath] : undefined;
		if (repo?.collapsed) repositoriesStore.toggleCollapsed(repo.path);
		if (repo && !repo.expanded) repositoriesStore.toggleExpanded(repo.path);
		navigateToTerminal(target.id);
		requestAnimationFrame(() => {
			const scroller = props.scroller();
			if (scroller) rowFor(scroller, target)?.scrollIntoView({ block: "nearest" });
		});
	};

	return (
		<Show when={hidden().length > 0}>
			<button
				type="button"
				class={s.needsYouBanner}
				data-control-layout="card"
				data-testid="needs-you-banner"
				onClick={reveal}
			>
				<span class={s.needsYouBannerDot} />
				<span>{t("needsYou.title", "{count} need you", { count: String(hidden().length) })}</span>
				<span class={s.needsYouBannerNames}>{names()}</span>
				<span class={s.needsYouBannerArrow}>{arrow()}</span>
			</button>
		</Show>
	);
};
