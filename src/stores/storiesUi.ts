import { batch, createSignal } from "solid-js";
import { togglePanel } from "../panelRouter";
import type { StoryType, StoryVisibility } from "../types/stories.gen";
import { uiStore } from "./ui";

const [visible, setVisible] = createSignal(false);
const [project, setProject] = createSignal<string | null>(null);

const [visibility, setVisibility] = createSignal<StoryVisibility>("all");
const [storyType, setStoryType] = createSignal<StoryType | "all" | "untyped">("all");
const [view, setView] = createSignal<"list" | "kanban">("list");

export const storiesUi = {
	controls: () => ({ visibility: visibility(), storyType: storyType(), view: view() }),
	restoreControls(data: unknown): void {
		if (!data || typeof data !== "object") return;
		const controls = data as Record<string, unknown>;
		batch(() => {
			if (
				controls.visibility === "all" ||
				controls.visibility === "open" ||
				controls.visibility === "closed" ||
				controls.visibility === "qa"
			) {
				setVisibility(controls.visibility);
			}
			if (
				controls.storyType === "all" ||
				controls.storyType === "untyped" ||
				controls.storyType === "feature" ||
				controls.storyType === "fix" ||
				controls.storyType === "refactor" ||
				controls.storyType === "chore" ||
				controls.storyType === "task"
			) {
				setStoryType(controls.storyType);
			}
			if (controls.view === "list" || controls.view === "kanban") setView(controls.view);
		});
	},
	visibility,
	setVisibility,
	storyType,
	setStoryType,
	view,
	setView,
	visible,
	project,
	open(path: string | null | undefined): void {
		if (!path) return;
		if (uiStore.isDetached("stories")) {
			togglePanel("stories");
			return;
		}
		setProject(path);
		setVisible(true);
	},
	close(): void {
		setVisible(false);
	},
};
