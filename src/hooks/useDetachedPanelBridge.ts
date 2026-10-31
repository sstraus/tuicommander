import { createEffect, onCleanup, untrack } from "solid-js";
import { invoke, listen } from "../invoke";
import { panelRegistry } from "../panelRouter";
import { stopChatVoice } from "../services/chatVoice";
import { appLogger } from "../stores/appLogger";
import { uiStore } from "../stores/ui";
import { isTauri } from "../transport";
import { createPanelSyncProvider, type PanelAction } from "../utils/panelSync";

/** Owns main-window listeners, projection sync providers, and detached-window restoration. */
export function useDetachedPanelBridge(
	onConfigureEgo?: () => void,
	onConfigureDictation?: () => void,
): { restoreDetachedPanels: () => void } {
	/**
	 * A panel comes home exactly once. `reattachPanel` emits `panel-action` and
	 * then closes the window, and closing it emits `panel-window-closed`, so one
	 * reattach click delivers both events. `detachedPanels` is the single source
	 * of truth for whether the homecoming already happened: an event for a panel
	 * that is no longer detached is the echo, and does nothing.
	 */
	const bringPanelHome = (panelId: string): void => {
		if (!uiStore.isDetached(panelId)) return;
		uiStore.clearDetached(panelId);
		panelRegistry[panelId]?.onReattach?.();
		panelRegistry[panelId]?.toggle?.();
	};

	let unlistenClosed: (() => void) | undefined;
	listen<string>("panel-window-closed", (event) => {
		// Explicit reattach has already cleared detached state. A native close
		// must revoke audio even though the panel then returns to the main window.
		if (event.payload === "ai-chat" && uiStore.isDetached("ai-chat")) void stopChatVoice();
		bringPanelHome(event.payload);
	}).then((unlisten) => {
		unlistenClosed = unlisten;
	});
	onCleanup(() => unlistenClosed?.());

	let unlistenAction: (() => void) | undefined;
	listen<PanelAction>("panel-action", (event) => {
		const { panelId, action, data } = event.payload;
		if (panelId === "ai-chat" && action === "configure-ego") {
			onConfigureEgo?.();
			return;
		}
		if (panelId === "ai-chat" && action === "configure-dictation") {
			onConfigureDictation?.();
			return;
		}
		if (action === "reattach") {
			bringPanelHome(panelId);
			return;
		}
		panelRegistry[panelId]?.handleAction?.(action, data);
	}).then((unlisten) => {
		unlistenAction = unlisten;
	});
	onCleanup(() => unlistenAction?.());

	for (const adapter of Object.values(panelRegistry)) {
		if (!adapter.serialize || !adapter.syncIntervalMs) continue;
		const projection = adapter;
		createEffect(() => {
			if (!uiStore.isDetached(projection.id)) return;
			const provider = untrack(() => {
				const next = createPanelSyncProvider(projection.id, projection.serialize!, projection.syncIntervalMs!);
				next.start();
				return next;
			});
			onCleanup(() => provider.stop());
		});
	}

	const restoreDetachedPanels = () => {
		if (!isTauri()) return;
		const panels = { ...uiStore.state.detachedPanels };
		for (const panelId of Object.keys(panels)) {
			const adapter = panelRegistry[panelId];
			if (!adapter) {
				uiStore.clearDetached(panelId);
				continue;
			}
			invoke("open_panel_window", {
				panelId,
				title: adapter.title,
				// Same params `detachPanel` sends. Restoring with `{}` reopened
				// the AI Chat window with no chat id at all, so it came back
				// blank after every app restart.
				// DEFERRED (2026-08-18) — this hands over the main window's
				// CURRENT id, not the one the window was detached with: the
				// detached-panel record keeps a label, not its params. Persist
				// the params in uiStore if a second panel needs the real one.
				params: adapter.detachParams?.() ?? {},
				width: adapter.defaultSize.width,
				height: adapter.defaultSize.height,
			}).catch((err) => {
				appLogger.warn("app", `Failed to restore detached panel: ${panelId}`, { error: String(err) });
				uiStore.clearDetached(panelId);
			});
		}
	};

	return { restoreDetachedPanels };
}
