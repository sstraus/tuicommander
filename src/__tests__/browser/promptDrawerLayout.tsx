/** Synthetic prompt data; production editor, stores, shared rows and CSS. No real backend writes. */
import { render } from "solid-js/web";
import commander from "../../../src-tauri/src/themes/commander.json";
import paper from "../../../src-tauri/src/themes/vscode-light.json";
import { PromptDrawer } from "../../components/PromptDrawer/PromptDrawer";

import { promptLibraryStore, type SavedPrompt } from "../../stores/promptLibrary";
import "../../global.css";
import "../../styles.css";

const params = new URLSearchParams(location.search);
const light = params.get("theme") === "light";
const names: Record<string, string> = {
	background: "bg-primary",
	surface: "bg-secondary",
	surfaceElevated: "bg-tertiary",
	highlight: "bg-highlight",
	foreground: "fg-primary",
	foregroundSecondary: "fg-secondary",
	mutedForeground: "fg-muted",
	accentForeground: "text-on-accent",
	errorForeground: "text-on-error",
	successForeground: "text-on-success",
};
document.documentElement.dataset.theme = light ? "vscode-light" : "commander";
document.documentElement.style.colorScheme = light ? "light" : "dark";
for (const [key, value] of Object.entries((light ? paper : commander).appChrome)) {
	document.documentElement.style.setProperty(
		`--${names[key] ?? key.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`)}`,
		value,
	);
}
const mode = (params.get("mode") ?? "inject") as SavedPrompt["executionMode"];
promptLibraryStore.createPrompt({
	name: "Merge in main",
	content: "Merge the latest main into this branch. Resolve conflicts thoughtfully, stage and commit the merge.",
	description: "Review changes",
	category: "custom",
	isFavorite: false,
	tags: ["smart"],
	enabled: true,
	placement: ["git-changes"],
	executionMode: mode,
	injectTarget: params.has("compose") ? "compose" : "terminal",
	autoExecute: true,
	shortcut: "Cmd+Alt+G",
});
promptLibraryStore.openDrawer();
render(() => <PromptDrawer />, document.getElementById("app")!);
requestAnimationFrame(() => {
	document.querySelector<HTMLButtonElement>('button[title="Edit"]')?.click();
});
