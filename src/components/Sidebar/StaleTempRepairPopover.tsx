import { type Component, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { appLogger } from "../../stores/appLogger";
import { repositoriesStore } from "../../stores/repositories";
import { Button } from "../ui/Button";
import s from "./Sidebar.module.css";

export interface StaleTempRepairPopoverProps {
	onClose: () => void;
}

/**
 * Discoverable repair entry point for stale-temp ghost rows (#763-d219):
 * previews the exact candidates the backend classifier quarantined from the
 * sidebar, and repairs only on explicit confirmation. Never deletes silently
 * — a candidate stays visible here (and out of the sidebar) until repaired.
 */
export const StaleTempRepairPopover: Component<StaleTempRepairPopoverProps> = (props) => {
	let popoverRef: HTMLDivElement | undefined;
	const [status, setStatus] = createSignal<"idle" | "repairing" | "error">("idle");
	const [errorMessage, setErrorMessage] = createSignal("");

	const candidates = () => repositoriesStore.getStaleTempCandidates();

	onMount(() => {
		let attached = false;
		const handleClick = (e: MouseEvent) => {
			if (popoverRef && !popoverRef.contains(e.target as Node)) props.onClose();
		};
		const handleKey = (e: KeyboardEvent) => {
			if (e.key === "Escape") props.onClose();
		};
		const rafId = requestAnimationFrame(() => {
			attached = true;
			document.addEventListener("mousedown", handleClick);
			document.addEventListener("keydown", handleKey);
		});
		onCleanup(() => {
			cancelAnimationFrame(rafId);
			if (attached) {
				document.removeEventListener("mousedown", handleClick);
				document.removeEventListener("keydown", handleKey);
			}
		});
	});

	const handleRepairAll = async () => {
		const paths = candidates().map((c) => c.path);
		if (paths.length === 0) return;
		setStatus("repairing");
		setErrorMessage("");
		try {
			await repositoriesStore.repairStaleTemp(paths);
			setStatus("idle");
			if (repositoriesStore.getStaleTempCandidates().length === 0) props.onClose();
		} catch (err) {
			appLogger.error("store", "Stale-temp repository repair failed", err);
			setStatus("error");
			setErrorMessage(err instanceof Error ? err.message : String(err));
			// Preview may be stale (another client already repaired one, or a
			// row changed) — refresh so the list on screen matches disk again.
			void repositoriesStore.refreshStaleTempCandidates();
		}
	};

	return (
		<div ref={popoverRef} class={s.parkedPopover}>
			<div class={s.parkedPopoverHeader}>Stale Repositories</div>
			<Show when={candidates().length === 0}>
				<div class={s.parkedPopoverEmpty}>No stale repositories found</div>
			</Show>
			<For each={candidates()}>
				{(candidate) => (
					<div class={s.parkedPopoverItem}>
						<span class={s.parkedPopoverName} data-tooltip={candidate.path} data-tooltip-pos="bottom">
							{candidate.displayName}
						</span>
					</div>
				)}
			</For>
			<Show when={candidates().length > 0}>
				<div class={s.staleTempActions}>
					<Button
						size="compact"
						variant="danger"
						type="button"
						class={s.staleTempRepairButton}
						disabled={status() === "repairing"}
						onClick={() => void handleRepairAll()}
					>
						{status() === "repairing"
							? "Repairing…"
							: `Repair ${candidates().length} stale ${candidates().length === 1 ? "repository" : "repositories"}`}
					</Button>
					<Show when={status() === "error"}>
						<div class={s.staleTempError}>Repair failed: {errorMessage()}</div>
					</Show>
				</div>
			</Show>
		</div>
	);
};
