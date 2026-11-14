import { createEffect, getOwner, onCleanup, runWithOwner, untrack } from "solid-js";
import { activityStore } from "../stores/activityStore";
import { appLogger } from "../stores/appLogger";
import { storiesUi } from "../stores/storiesUi";
import {
	fabbricaComplete,
	isFabbricaId,
	openFabbricaConsole,
	type WaitingItem,
	type WaitingSnapshot,
	waitingItemsStore,
} from "../stores/waitingItems";
import { pathBasename } from "../utils/pathUtils";

export const WAITING_ACTIVITY_SECTION_ID = "waiting";
const BELL_PREFIX = "waiting:";
const ICON =
	'<svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor"><path d="M5.5 1h5a.5.5 0 0 1 .5.5V2h1.5A1.5 1.5 0 0 1 14 3.5v10a1.5 1.5 0 0 1-1.5 1.5h-9A1.5 1.5 0 0 1 2 13.5v-10A1.5 1.5 0 0 1 3.5 2H5v-.5a.5.5 0 0 1 .5-.5zM6 2v1h4V2H6zM3.5 3a.5.5 0 0 0-.5.5v10a.5.5 0 0 0 .5.5h9a.5.5 0 0 0 .5-.5v-10a.5.5 0 0 0-.5-.5H11v.5a.5.5 0 0 1-.5.5h-5a.5.5 0 0 1-.5-.5V3H3.5zm7.85 3.35-4 4a.5.5 0 0 1-.7 0l-2-2a.5.5 0 1 1 .7-.7L7 9.29l3.65-3.64a.5.5 0 0 1 .7.7z"/></svg>';

const KIND_LABEL: Record<WaitingItem["kind"], string> = {
	manual_check: "Manual check",
	help: "Fabbrica help",
	approval: "Fabbrica approval",
	outside_request: "Fabbrica outside request",
};

/** Open the item's source: the story for a manual check, the Fabbrica console for a Fabbrica item. */
export function revealWaitingItem(item: WaitingItem): void {
	if (isFabbricaId(item.id)) {
		openFabbricaConsole();
		return;
	}
	storiesUi.reveal({
		project: item.project,
		planId: item.planId,
		storyId: item.story?.id ?? null,
	});
}

function subtitle(item: WaitingItem): string {
	if (isFabbricaId(item.id)) return item.detail;
	return `${pathBasename(item.project)} · ${KIND_LABEL[item.kind]}`;
}

function sync(snapshot: WaitingSnapshot): void {
	const current = new Map(snapshot.items.map((item) => [`${BELL_PREFIX}${item.id}`, item]));
	// Each source resolves only its own entries: an unknown Fabbrica keeps its
	// entries, and an unreadable project keeps the rest.
	const projectsComplete = snapshot.warnings.length === 0;
	const fabbricaKnown = fabbricaComplete(snapshot);
	for (const existing of [...activityStore.state.items]) {
		if (existing.pluginId !== "core" || existing.sectionId !== WAITING_ACTIVITY_SECTION_ID) continue;
		const item = current.get(existing.id);
		if (item) {
			// Persisted entries lose their action across restarts; restore it without a new notification.
			if (!existing.onClick) activityStore.updateItem(existing.id, { onClick: () => revealWaitingItem(item) });
			current.delete(existing.id);
		} else if (isFabbricaId(existing.id.slice(BELL_PREFIX.length)) ? fabbricaKnown : projectsComplete) {
			// An unreadable source hides its items without resolving them.
			activityStore.removeItem(existing.id);
		}
	}
	for (const [id, item] of current) {
		activityStore.addItem({
			id,
			pluginId: "core",
			sectionId: WAITING_ACTIVITY_SECTION_ID,
			title: item.title,
			subtitle: subtitle(item),
			icon: ICON,
			dismissible: true,
			onClick: () => revealWaitingItem(item),
		});
	}
}

/** Notify each waiting item once in the activity bell and clear it when resolved. */
export function useWaitingItems(): void {
	const section = activityStore.registerSection({
		id: WAITING_ACTIVITY_SECTION_ID,
		label: "WAITING ON YOU",
		priority: 5,
		canDismissAll: true,
	});
	const release = waitingItemsStore.retain();
	const owner = getOwner();
	let disposed = false;
	activityStore
		.whenHydrated()
		.then(() => {
			if (disposed) return;
			runWithOwner(owner, () =>
				createEffect(() => {
					const snapshot = waitingItemsStore.snapshot();
					if (snapshot) untrack(() => sync(snapshot));
				}),
			);
		})
		.catch((error: unknown) => appLogger.warn("store", "Waiting items bell sync failed", error));
	onCleanup(() => {
		disposed = true;
		release();
		section.dispose();
	});
}
