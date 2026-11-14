import { emitTo } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { type Component, createEffect, createSignal, lazy, onCleanup, onMount, Show, Suspense } from "solid-js";
import { initPanelWindow } from "../hooks/initPanelWindow";
import { guardTauriUnlisten } from "../invoke";
import { closePanel, type PanelAdapter } from "../panelRouter";
import { appLogger } from "../stores/appLogger";
import { repositoriesStore } from "../stores/repositories";
import { STORIES_REVEAL_EVENT, type StoriesTarget, storiesUi } from "../stores/storiesUi";

const StoriesDialog = lazy(() =>
	import("../components/StoriesDialog/StoriesDialog").then((module) => ({ default: module.StoriesDialog })),
);

const DetachedStories: Component<{ params: URLSearchParams }> = (props) => {
	const [project, setProject] = createSignal(props.params.get("project"));
	storiesUi.restoreControls(Object.fromEntries(props.params));
	createEffect(() => {
		// Keep the main WebView's session controls current even when this window closes natively.
		void emitTo("main", "panel-action", { panelId: "stories", action: "controls", data: storiesUi.controls() }).catch(
			(error) => appLogger.warn("panel", "Failed to return detached controls", { error: String(error) }),
		);
	});
	let unlisten: (() => void) | undefined;
	onCleanup(() => unlisten?.());
	onMount(() => {
		void initPanelWindow();
		// A bell click in the main window lands here: select the target, switching project if needed.
		Promise.resolve()
			.then(() =>
				getCurrentWebviewWindow().listen<StoriesTarget>(STORIES_REVEAL_EVENT, (event) => {
					storiesUi.reveal(event.payload);
					setProject(event.payload.project);
				}),
			)
			.then((fn) => {
				unlisten = guardTauriUnlisten(fn);
			})
			.catch((error) => appLogger.warn("panel", "Failed to listen for Stories reveal", { error: String(error) }));
	});
	return (
		<Show when={project()} keyed>
			{(path) => (
				<Suspense>
					<StoriesDialog project={path} mode="detached" onClose={() => void closePanel("stories")} />
				</Suspense>
			)}
		</Show>
	);
};

export const storiesPanelAdapter: PanelAdapter = {
	id: "stories",
	title: "Plans and Stories",
	defaultSize: { width: 1120, height: 800 },
	detachParams: (): Record<string, string> => {
		// DEFERRED (2026-10-23) — shared detached-panel prefs retain labels, not params;
		// restart falls back to the active project, like other repo-scoped panels.
		const project = storiesUi.project() ?? repositoriesStore.state.activeRepoPath;
		return { ...(project ? { project } : {}), ...storiesUi.controls() };
	},
	handleAction: (action, data) => {
		if (action === "controls") storiesUi.restoreControls(data);
	},
	onDetach: () => storiesUi.close(),
	toggle: () => storiesUi.open(storiesUi.project() ?? repositoriesStore.state.activeRepoPath),
	Component: DetachedStories,
};
