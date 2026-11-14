import { emitTo } from "@tauri-apps/api/event";
import { type Component, createSignal, lazy, onCleanup, onMount, Suspense } from "solid-js";
import type { MarkdownFileLink } from "../components/MarkdownTab/MarkdownTab";
import { initPanelWindow } from "../hooks/initPanelWindow";
import { invoke, listen } from "../invoke";
import type { PanelAdapter } from "../panelRouter";
import { appLogger } from "../stores/appLogger";
import { type FileTab, mdTabsStore } from "../stores/mdTabs";
import { uiStore } from "../stores/ui";
import { openFileAction } from "../utils/filePreview";
import { markdownDocumentPanelId } from "../utils/markdownDocumentPanelId";
import { setPendingHeading } from "../utils/pendingHeadings";

const MarkdownTab = lazy(() => import("../components/MarkdownTab").then((module) => ({ default: module.MarkdownTab })));

const DetachedMarkdownDocument: Component<{ params: URLSearchParams }> = (props) => {
	const tabId = props.params.get("tabId") ?? "";
	const filePath = props.params.get("filePath") ?? "";
	const repoPath = props.params.get("repoPath") ?? "";
	const fsRoot = props.params.get("fsRoot") ?? repoPath;
	const fileName = props.params.get("fileName") ?? filePath;
	const [reloadToken, setReloadToken] = createSignal(0);
	const tab: FileTab = { id: tabId, type: "file", filePath, fileName, repoPath, fsRoot, branchKey: "" };

	onMount(() => {
		void initPanelWindow();
		mdTabsStore.setActive(tabId);
		const refresh = () => setReloadToken((value) => value + 1);
		window.addEventListener("focus", refresh);
		// The repo watcher broadcasts to every window. This window has no
		// repositories store of its own, so it reloads on its repo's events.
		const unlisten = repoPath
			? listen<{ repo_path: string }>("repo-changed", (event) => {
					if (event.payload.repo_path === repoPath) refresh();
				})
			: undefined;
		onCleanup(() => {
			window.removeEventListener("focus", refresh);
			void unlisten?.then((stop) => stop());
		});
	});

	const openInMain = (target: MarkdownFileLink) => {
		void emitTo("main", "panel-action", { panelId: markdownDocumentPanelId(tabId), action: "open-link", data: target })
			.then(() => invoke("focus_main_window"))
			.catch((error) => appLogger.warn("app", "Failed to open detached Markdown link", error));
	};

	return (
		<Suspense>
			<MarkdownTab tab={tab} reloadToken={reloadToken} onOpenFileLink={openInMain} onClose={() => window.close()} />
		</Suspense>
	);
};

export function createMarkdownDocumentPanelAdapter(tabId: string, onSelect?: (id: string) => void): PanelAdapter {
	const tab = mdTabsStore.get(tabId);
	return {
		id: markdownDocumentPanelId(tabId),
		title: tab?.type === "file" ? tab.fileName : "Markdown",
		defaultSize: { width: 900, height: 750 },
		Component: DetachedMarkdownDocument,
		detachParams: (): Record<string, string> => {
			const tab = mdTabsStore.get(tabId);
			if (tab?.type !== "file") return {};
			return {
				tabId,
				filePath: tab.filePath,
				fileName: tab.fileName,
				repoPath: tab.repoPath,
				fsRoot: tab.fsRoot ?? "",
			};
		},
		onDetach: () => mdTabsStore.setActive(null),
		toggle: () => (onSelect ? onSelect(tabId) : mdTabsStore.setActive(tabId)),
		handleAction: (action, data) => {
			if (action !== "open-link" || !data || typeof data !== "object") return;
			const target = data as Partial<MarkdownFileLink>;
			if (target.kind !== "file" || typeof target.absolute_path !== "string" || typeof target.open_path !== "string")
				return;
			if (target.is_directory) {
				uiStore.setFileBrowserExternalRoot(target.absolute_path);
				uiStore.setFileBrowserPanelVisible(true);
				return;
			}
			const source = mdTabsStore.get(tabId);
			if (source?.type !== "file") return;
			setPendingHeading(source.fsRoot || source.repoPath, { open_path: target.open_path, anchor: target.anchor });
			openFileAction(target.open_path, source.repoPath, source.fsRoot, target.line);
		},
	};
}
