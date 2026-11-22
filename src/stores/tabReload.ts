import { batch, createComputed, createRoot, createSignal, untrack } from "solid-js";
import { appLogger } from "./appLogger";
import type { BaseTab, TabStoreState } from "./tabManager";

const PREFIXES: Record<string, string> = { markdown: "md", editor: "edit", diff: "diff" };
const ORDER_KEY = "tui-commander-tab-order-reload";
const keyFor = (name: string) => `tui-commander-tabs-reload-${name}`;
const [ready, setReady] = createSignal(false);
const managers = new Map<string, { save: () => void; active: () => string | null; select: () => void }>();
let saveOrder = () => {};

function validTab(name: string, tab: Record<string, unknown>): boolean {
	const strings = (...keys: string[]) => keys.every((key) => typeof tab[key] === "string");
	if (["pinned", "pinAcrossRepos"].some((key) => tab[key] !== undefined && typeof tab[key] !== "boolean")) return false;
	if (["repoPath", "branchKey", "mcpUiId"].some((key) => tab[key] !== undefined && typeof tab[key] !== "string"))
		return false;
	if (name === "editor") {
		const draft = tab.reloadDraft as Record<string, unknown> | undefined;
		return (
			strings("repoPath", "fsRoot", "filePath", "fileName") &&
			typeof tab.isDirty === "boolean" &&
			(draft === undefined ||
				(draft !== null && typeof draft.text === "string" && typeof draft.savedContent === "string"))
		);
	}
	if (name === "diff") return strings("repoPath", "filePath", "fileName", "status");
	switch (tab.type) {
		case "file":
			return strings("repoPath", "filePath", "fileName");
		case "html-preview":
			return strings("repoPath", "filePath", "fileName", "title");
		case "virtual":
			return strings("title", "contentUri");
		case "plugin-panel":
			return strings("title", "pluginId", "html") && (tab.url === undefined || typeof tab.url === "string");
		case "github-ops":
			return strings("repoPath", "title");
		case "pr-diff":
			return (
				strings("repoPath", "title", "prTitle", "diff") &&
				Number.isSafeInteger(tab.prNumber) &&
				Number(tab.prNumber) > 0
			);
		case "claude-usage":
		case "codex-usage":
		case "grok-usage":
		case "command-overview":
		case "tasks":
			return strings("title");
		default:
			return false;
	}
}

/** Window-local recovery only: no shared config or cross-window tab state. */
export function readTabsForReload<T extends BaseTab>(name: string): TabStoreState<T> | undefined {
	const prefix = PREFIXES[name];
	if (!prefix) return;
	try {
		const raw = sessionStorage.getItem(keyFor(name));
		if (!raw) return;
		const value = JSON.parse(raw);
		if (value?.version !== 1 || !Array.isArray(value.tabs) || !Array.isArray(value.order))
			throw new Error("Invalid tab snapshot");
		const tabs: Record<string, T> = {};
		let counter = 0;
		for (const tab of value.tabs) {
			if (!tab || typeof tab !== "object" || typeof tab.id !== "string") continue;
			const match = new RegExp(`^${prefix}-([1-9][0-9]*)$`).exec(tab.id);
			if (!match || !Number.isSafeInteger(Number(match[1])) || tabs[tab.id] || !validTab(name, tab)) continue;
			tabs[tab.id] = tab;
			counter = Math.max(counter, Number(match[1]));
		}
		const dropped = value.tabs.length - Object.keys(tabs).length;
		if (dropped) untrack(() => appLogger.warn("store", `Dropped ${dropped} invalid ${name} reload tabs`));
		const order = [
			...new Set<string>(value.order.filter((id: unknown) => typeof id === "string" && Object.hasOwn(tabs, id))),
		];
		for (const id of Object.keys(tabs)) if (!order.includes(id)) order.push(id);
		return {
			tabs,
			_order: order,
			counter,
			activeId: typeof value.activeId === "string" && Object.hasOwn(tabs, value.activeId) ? value.activeId : null,
		};
	} catch (error) {
		untrack(() => appLogger.warn("store", `Could not read ${name} tabs for reload`, error));
		return;
	}
}

export function trackTabsForReload<T extends BaseTab>(
	name: string,
	state: TabStoreState<T>,
	selected: string | null,
	select: (id: string) => void,
): void {
	if (!PREFIXES[name]) return;
	let timer: ReturnType<typeof setTimeout> | undefined;
	const warnedDrafts = new Set<string>();
	const save = () => {
		clearTimeout(timer);
		if (!ready()) return;
		let tabs: T[] = [];
		try {
			let remainingDraftChars = 1_000_000;
			tabs = state._order
				.map((id) => state.tabs[id])
				.filter(Boolean)
				.map((tab) => {
					if (name !== "editor") return tab;
					const draft = (tab as T & { reloadDraft?: { text: string; savedContent: string } }).reloadDraft;
					if (!draft) {
						warnedDrafts.delete(tab.id);
						return tab;
					}
					const length = draft.text.length + draft.savedContent.length;
					if (length <= remainingDraftChars) {
						remainingDraftChars -= length;
						warnedDrafts.delete(tab.id);
						return tab;
					}
					if (!warnedDrafts.has(tab.id)) {
						warnedDrafts.add(tab.id);
						appLogger.warn(
							"store",
							`Editor draft ${tab.id} exceeds reload recovery size limit; save the file before reloading`,
						);
					}
					return { ...tab, reloadDraft: undefined };
				});
			if (!tabs.length) sessionStorage.removeItem(keyFor(name));
			else
				sessionStorage.setItem(
					keyFor(name),
					JSON.stringify({ version: 1, tabs, order: state._order, activeId: state.activeId }),
				);
		} catch (error) {
			// Retry editor definitions without drafts when another store uses the quota.
			if (name === "editor" && tabs.length) {
				try {
					sessionStorage.setItem(
						keyFor(name),
						JSON.stringify({
							version: 1,
							tabs: tabs.map((tab) => ({ ...tab, reloadDraft: undefined })),
							order: state._order,
							activeId: state.activeId,
						}),
					);
					untrack(() =>
						appLogger.warn(
							"store",
							"Editor reload snapshot retained tab definitions but could not retain drafts",
							error,
						),
					);
					return;
				} catch {}
			}
			// DEFERRED (2026-11-19) — oversized generated Markdown bodies can exceed
			// window storage. Fail empty with a warning, never resurrect stale closes;
			// lossless large-payload recovery needs a different persistence boundary.
			try {
				sessionStorage.removeItem(keyFor(name));
			} catch {}
			untrack(() => appLogger.warn("store", `Could not save ${name} tabs for reload`, error));
		}
	};
	managers.set(name, {
		save,
		active: () => state.activeId,
		select: () => {
			if (selected && state.tabs[selected]) select(selected);
		},
	});
	createRoot(() => {
		if (name !== "editor") {
			createComputed(save);
			return;
		}
		// Metadata (especially closes) persists immediately. Drafts and cursor
		// movement must not stringify every open buffer on each keystroke.
		createComputed(() => {
			ready();
			state.activeId;
			for (const id of state._order) {
				const tab = state.tabs[id];
				if (!tab) continue;
				for (const key of [
					"id",
					"repoPath",
					"fsRoot",
					"filePath",
					"fileName",
					"isDirty",
					"initialLine",
					"initialCol",
					"externalEditable",
					"fontSize",
					"branchKey",
					"pinned",
					"pinAcrossRepos",
					"mcpUiId",
					"openerTuicSession",
				]) {
					tab[key as keyof T];
				}
			}
			untrack(save);
		});
		createComputed(() => {
			let hasDraft = false;
			for (const id of state._order) {
				const draft = (state.tabs[id] as T & { reloadDraft?: { text: string; savedContent: string } })?.reloadDraft;
				if (draft) hasDraft = true;
				draft?.text;
				draft?.savedContent;
			}
			if (!ready()) return;
			clearTimeout(timer);
			if (hasDraft) timer = setTimeout(() => untrack(save), 400);
			else untrack(save);
		});
	});
}

export function readTabOrderForReload(): string[] {
	try {
		const value = JSON.parse(sessionStorage.getItem(ORDER_KEY) ?? "[]");
		return Array.isArray(value)
			? [...new Set(value.filter((id: unknown): id is string => typeof id === "string"))]
			: [];
	} catch {
		return [];
	}
}

export function trackTabOrderForReload(order: () => string[]): void {
	saveOrder = () => {
		if (!ready()) return;
		try {
			sessionStorage.setItem(ORDER_KEY, JSON.stringify(order()));
		} catch (error) {
			try {
				sessionStorage.removeItem(ORDER_KEY);
			} catch {}
			untrack(() => appLogger.warn("store", "Could not save tab order for reload", error));
		}
	};
	createRoot(() => createComputed(saveOrder));
}

/** Finish after terminal/branch restoration; a fresh document open wins selection. */
export function restoreNonTerminalTabsAfterReload(): void {
	if (untrack(ready)) return;
	batch(() => {
		if (![...managers.values()].some((manager) => manager.active())) {
			for (const manager of managers.values()) manager.select();
		}
		setReady(true);
	});
}

/** Flush the current snapshot before unload or a native background snapshot. */
export function saveNonTerminalTabsForReload(): void {
	if (!untrack(ready)) return;
	for (const manager of managers.values()) manager.save();
	saveOrder();
}
