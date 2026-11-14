import { emitTo } from "@tauri-apps/api/event";
import { batch, createSignal } from "solid-js";
import { togglePanel } from "../panelRouter";
import type { StoryType, StoryVisibility } from "../types/stories.gen";
import { appLogger } from "./appLogger";
import { uiStore } from "./ui";

/** Window event that hands a target to the detached Plans and Stories window. */
export const STORIES_REVEAL_EVENT = "stories-reveal";

const [visible, setVisible] = createSignal(false);
const [project, setProject] = createSignal<string | null>(null);

const VISIBILITY_KEY = "tuic.stories.visibility";
const VISIBILITIES: readonly string[] = ["all", "open", "closed", "qa"];

/** Hide Done by default; the last choice survives reopen and restart. */
function loadVisibility(): StoryVisibility {
	try {
		const saved = localStorage.getItem(VISIBILITY_KEY);
		if (saved && VISIBILITIES.includes(saved)) return saved as StoryVisibility;
	} catch {
		// Storage unavailable: fall back to the default.
	}
	return "open";
}

const [visibility, setVisibilitySignal] = createSignal<StoryVisibility>(loadVisibility());
const setVisibility = (next: StoryVisibility): void => {
	setVisibilitySignal(next);
	try {
		localStorage.setItem(VISIBILITY_KEY, next);
	} catch {
		// Not persisted; the in-memory choice still applies.
	}
};
const [storyType, setStoryType] = createSignal<StoryType | "all" | "untyped">("all");
const [view, setView] = createSignal<"list" | "kanban">("list");

/** Where Plans and Stories should land: a plan and optionally one of its stories. */
export interface StoriesTarget {
	project: string;
	planId: string | null;
	storyId: string | null;
}
const [target, setTarget] = createSignal<StoriesTarget | null>(null);

function open(path: string | null | undefined): void {
	if (!path) return;
	if (uiStore.isDetached("stories")) {
		togglePanel("stories");
		return;
	}
	setProject(path);
	setVisible(true);
}

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
	open,
	target,
	/** Open the project and select the target's story. */
	reveal(next: StoriesTarget): void {
		if (uiStore.isDetached("stories")) {
			// The detached window owns its own store: hand it the target, never leave it pending here.
			emitTo("panel-stories", STORIES_REVEAL_EVENT, next).catch((error: unknown) =>
				appLogger.warn("panel", "Failed to reveal in detached Plans and Stories", { error: String(error) }),
			);
			togglePanel("stories");
			return;
		}
		setTarget(next);
		open(next.project);
	},
	/** Consume the pending target when it belongs to this project. */
	takeTarget(path: string): StoriesTarget | null {
		const pending = target();
		if (pending?.project !== path) return null;
		setTarget(null);
		return pending;
	},
	close(): void {
		setVisible(false);
	},
};
