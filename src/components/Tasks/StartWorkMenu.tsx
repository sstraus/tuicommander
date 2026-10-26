import { createSignal, For, Show } from "solid-js";
import type { WorkItemProvider } from "../../plugins/workItemProvider";
import type { WorkItem, WorkItemDetail } from "../../types/work-items.gen";
import { Button } from "../ui/Button";
import s from "./StartWorkMenu.module.css";
import { START_WORK_NEEDS_REPO, useStartWork } from "./startWork";

/** Card menu: offers the provider's start_work actions for one item, read from its detail when opened. */
export function StartWorkMenu(props: { provider: WorkItemProvider; item: WorkItem; repoPath?: string }) {
	const start = useStartWork();
	const [open, setOpen] = createSignal(false);
	const [detail, setDetail] = createSignal<WorkItemDetail>();
	const [loadError, setLoadError] = createSignal("");
	async function toggle() {
		const next = !open();
		setOpen(next);
		if (!next || detail()) return;
		setLoadError("");
		try {
			setDetail(await props.provider.detail(props.item.reference));
		} catch (cause) {
			setLoadError(cause instanceof Error ? cause.message : String(cause));
		}
	}
	const actions = () => detail()?.actions.filter((action) => action.kind === "start_work") ?? [];
	function run() {
		const value = detail();
		if (value && props.repoPath) void start.run(props.provider, value, props.repoPath);
	}
	return (
		<div
			class={s.menu}
			onKeyDown={(event) => {
				if (event.key === "Escape") setOpen(false);
			}}
		>
			<Button
				type="button"
				aria-label={`Card actions ${props.item.key}`}
				aria-haspopup="menu"
				aria-expanded={open()}
				onClick={() => void toggle()}
			>
				⋯
			</Button>
			<Show when={open()}>
				<div class={s.popup} role="menu" aria-label={`Actions for ${props.item.key}`}>
					<Show when={!detail() && !loadError()}>
						<span class={s.note}>Loading…</span>
					</Show>
					<Show when={loadError()}>
						<span class={s.note} role="alert">
							{loadError()}
						</span>
					</Show>
					<Show when={detail() && actions().length === 0}>
						<span class={s.note}>No actions</span>
					</Show>
					<For each={actions()}>
						{(action) => (
							<Button
								type="button"
								role="menuitem"
								disabled={start.pending() || !action.enabled || !props.repoPath}
								title={action.disabledReason ?? (props.repoPath ? undefined : START_WORK_NEEDS_REPO)}
								onClick={() => {
									setOpen(false);
									run();
								}}
							>
								{action.name}
							</Button>
						)}
					</For>
				</div>
			</Show>
			<Show when={start.pending()}>
				<span class={s.note}>Starting…</span>
			</Show>
			<Show when={start.startedBranch()}>
				{(branch) => (
					<span class={s.note} role="status">
						Started work on {branch()}
					</span>
				)}
			</Show>
			<Show when={start.failure()}>
				{(message) => (
					<span class={s.error} role="alert">
						{message()}
						<Button type="button" onClick={run}>
							Retry start work
						</Button>
					</span>
				)}
			</Show>
		</div>
	);
}
