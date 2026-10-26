import { getCurrentWindow } from "@tauri-apps/api/window";
import { createEffect, onCleanup } from "solid-js";
import { appLogger } from "../stores/appLogger";
import { mdTabsStore } from "../stores/mdTabs";
import { isTauri, rpc } from "../transport";

/** The main desktop document owns the snapshot; mobile and browser viewers never publish. */
export function useOpenMarkdownPublication(): void {
	if (!isTauri() || getCurrentWindow().label !== "main") return;
	type Tab = { id: string; repoPath: string; filePath: string; fsRoot?: string; openerTuicSession?: string };
	let pending: Tab[] | undefined;
	let sending = false;
	let disposed = false;
	let timer: ReturnType<typeof setTimeout> | undefined;
	async function flush() {
		if (sending || disposed || !pending) return;
		sending = true;
		const tabs = pending;
		pending = undefined;
		try {
			await rpc("sync_open_markdown_tabs", { tabs });
		} catch (error) {
			appLogger.warn("app", "Could not publish open Markdown tabs", error);
			pending ??= tabs;
		} finally {
			sending = false;
			if (pending && !disposed) timer = setTimeout(() => void flush(), 1000);
		}
	}
	createEffect(() => {
		pending = mdTabsStore.state._order.flatMap((id) => {
			const tab = mdTabsStore.state.tabs[id];
			return tab?.type === "file"
				? [
						{
							id: tab.id,
							repoPath: tab.repoPath,
							filePath: tab.filePath,
							fsRoot: tab.fsRoot,
							...(tab.openerTuicSession ? { openerTuicSession: tab.openerTuicSession } : {}),
						},
					]
				: [];
		});
		if (timer) clearTimeout(timer);
		timer = setTimeout(() => void flush(), 50);
	});
	onCleanup(() => {
		disposed = true;
		if (timer) clearTimeout(timer);
	});
}
