import { createSignal } from "solid-js";
import { urlResolverRegistry } from "../components/CommandPalette/urlPaste";
import { workItemProviderRegistry } from "../plugins/workItemProviderRegistry";
import { mdTabsStore } from "../stores/mdTabs";
import type { WorkItemRef } from "../types/work-items.gen";
import { handleOpenUrl } from "../utils/openUrl";

export interface TaskNavigationTarget {
	reference: WorkItemRef;
	comments: boolean;
}
const [target, setTarget] = createSignal<TaskNavigationTarget | null>(null);
let pending: AbortController | undefined;

/** Shared by PR details and native notices; URL resolvers retain provider/account ownership. */
export async function openTaskConversation(url: string): Promise<void> {
	pending?.abort();
	const controller = new AbortController();
	pending = controller;
	if (workItemProviderRegistry.list().length === 0) {
		handleOpenUrl(url);
		return;
	}
	const resolved = await urlResolverRegistry.resolve(url, controller.signal);
	if (controller.signal.aborted) return;
	const reference = resolved?.link.reference;
	if (!reference || !workItemProviderRegistry.get(reference.providerId, reference.accountId)) {
		handleOpenUrl(url);
		return;
	}
	setTarget({ reference, comments: true });
	mdTabsStore.addTasks();
}

export const taskNavigation = {
	target,
	clear: () => setTarget(null),
};

const [workspaceOpener, setWorkspaceOpener] = createSignal<((repoPath: string, workspaceId: string) => void) | null>(
	null,
);

/** App owns branch activation (including pane/terminal restoration), not the Tasks component. */
export function registerTaskWorkspaceOpener(open: (repoPath: string, workspaceId: string) => void): () => void {
	setWorkspaceOpener(() => open);
	return () => {
		if (workspaceOpener() === open) setWorkspaceOpener(null);
	};
}
export const taskWorkspaceOpener = workspaceOpener;
