import { createSignal } from "solid-js";
import { invoke } from "../invoke";
import { appLogger } from "../stores/appLogger";
import { editorTabsStore } from "../stores/editorTabs";

const [path, setPath] = createSignal<string | null>(null);
let pending: Promise<string> | null = null;

/** Resolve the scratchpad path once per window; later calls share the answer. */
function resolveScratchpadPath(): Promise<string> {
	pending ??= invoke<string>("get_scratchpad_path").then((resolved) => {
		setPath(resolved);
		return resolved;
	});
	pending.catch(() => {
		pending = null;
	});
	return pending;
}

/** True when `filePath` is the global scratchpad. Starts the one path lookup
 *  on first use, so a tab restored after reload is recognised too. */
export function isScratchpadPath(filePath: string): boolean {
	if (path() === null) void resolveScratchpadPath().catch(() => {});
	return path() === filePath;
}

/**
 * Open the global scratchpad: one Markdown file in the config directory,
 * shown in the regular editor tab, pinned in every repository.
 * The empty repo path keeps the tab out of any repo scope; the editor reads
 * and writes the absolute path through the external-file commands.
 */
export async function openScratchpad(): Promise<void> {
	try {
		const resolved = await resolveScratchpadPath();
		const id = editorTabsStore.add("", resolved, undefined, { externalEditable: true });
		editorTabsStore.setPinned(id, true, true);
	} catch (err) {
		appLogger.error("app", "Failed to open scratchpad", err);
	}
}
