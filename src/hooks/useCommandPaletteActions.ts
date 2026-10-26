import { type Accessor, createMemo, onCleanup } from "solid-js";
import { type ActionEntry, getActionEntries } from "../actions/actionRegistry";
import { t } from "../i18n";
import { appLogger } from "../stores/appLogger";
import { commandPaletteStore } from "../stores/commandPalette";
import { contextMenuActionsStore } from "../stores/contextMenuActionsStore";
import { designModeBrowsers, designModeStore } from "../stores/designMode";
import { pluginStore } from "../stores/pluginStore";
import { promptLibraryStore, type SavedPrompt } from "../stores/promptLibrary";
import { repositoriesStore } from "../stores/repositories";
import { terminalsStore } from "../stores/terminals";
import { updaterStore } from "../stores/updater";
import { runWithFeedback } from "../utils/promptContext";
import type { useGitOperations } from "./useGitOperations";
import type { ShortcutHandlers } from "./useKeyboardShortcuts";
import type { SmartPromptResult } from "./useSmartPrompts";
import type { useSplitPanes } from "./useSplitPanes";

interface CommandPaletteActionOptions {
	shortcutHandlers: ShortcutHandlers;
	gitOps: ReturnType<typeof useGitOperations>;
	splitPanes: ReturnType<typeof useSplitPanes>;
	executeSmartPrompt: (prompt: SavedPrompt) => Promise<SmartPromptResult>;
}

/** Builds the command palette's static registry plus reactive repository, plugin, terminal, and prompt actions. */
export function useCommandPaletteActions(options: CommandPaletteActionOptions): Accessor<ActionEntry[]> {
	onCleanup(designModeStore.subscribe());
	return createMemo(() => {
		const entries = getActionEntries(options.shortcutHandlers);
		const repos = Object.values(repositoriesStore.state.repositories);

		for (const repo of repos) {
			if (repo.parked) continue;
			entries.push({
				id: `switch-repo:${repo.path}`,
				label: repo.displayName,
				category: "Repository",
				keybinding: "",
				execute: () => {
					repositoriesStore.setActive(repo.path);
					const branch = repo.activeWorkspaceId || Object.keys(repo.workspaces)[0];
					if (branch) void options.gitOps.handleBranchSelect(repo.path, branch);
				},
			});
		}

		for (const repo of repos) {
			if (!repo.parked) continue;
			entries.push({
				id: `unpark-repo:${repo.path}`,
				label: `Unpark: ${repo.displayName}`,
				category: "Repository",
				keybinding: "",
				execute: () => {
					repositoriesStore.setPark(repo.path, false);
					repositoriesStore.setActive(repo.path);
					const branch = repo.activeWorkspaceId || Object.keys(repo.workspaces)[0];
					if (branch) void options.gitOps.handleBranchSelect(repo.path, branch);
				},
			});
		}

		for (const group of Object.values(repositoriesStore.state.groups)) {
			if (group.repoOrder.length === 0) continue;
			const allParked = repositoriesStore.isGroupFullyParked(group.id);
			entries.push({
				id: allParked ? `unpark-group:${group.id}` : `park-group:${group.id}`,
				label: allParked ? `Unpark Group: ${group.name}` : `Park Group: ${group.name}`,
				category: "Repository",
				keybinding: "",
				execute: () => repositoriesStore.setParkGroup(group.id, !allParked),
			});
		}

		entries.push(
			{
				id: "add-repository",
				label: "Add Repository",
				category: "Repository",
				keybinding: "",
				execute: () => options.gitOps.handleAddRepo(),
			},
			{
				id: "check-for-updates",
				label: "Check for Updates",
				category: "Application",
				keybinding: "",
				execute: () =>
					updaterStore.checkForUpdate().catch((error) => appLogger.warn("app", "Updater check failed", error)),
			},
			{
				id: "reset-panel-sizes",
				label: "Reset Panel Sizes",
				category: "Application",
				keybinding: "",
				execute: () => options.splitPanes.resetLayout(),
			},
			{
				id: "search-terminals",
				label: "Search Terminals",
				category: "Search",
				keybinding: "",
				execute: () => commandPaletteStore.openWithQuery("~ "),
			},
			{
				id: "search-files",
				label: "Search Files",
				category: "Search",
				keybinding: "",
				execute: () => commandPaletteStore.openWithQuery("! "),
			},
			{
				id: "search-file-contents",
				label: "Search in File Contents",
				category: "Search",
				keybinding: "",
				execute: () => commandPaletteStore.openWithQuery("? "),
			},
		);

		for (const plugin of pluginStore.state.plugins) {
			if (plugin.builtIn) continue;
			const name = plugin.manifest?.name ?? plugin.id;
			entries.push({
				id: `toggle-plugin:${plugin.id}`,
				label: `${plugin.enabled ? "Disable" : "Enable"} plugin: ${name}`,
				category: "Plugins",
				keybinding: "",
				execute: () => {
					const fresh = pluginStore.getPlugin(plugin.id);
					if (!fresh) return;
					pluginStore
						.setEnabled(plugin.id, !fresh.enabled)
						.catch((error) => appLogger.error("plugin", `Failed to toggle plugin ${name}`, error));
				},
			});
		}

		const activeTermId = terminalsStore.state.activeId;
		// The active terminal only: the label names no tab, so a fallback to
		// another session would open Chrome for an agent the user cannot see.
		const designTarget = activeTermId ? terminalsStore.get(activeTermId) : undefined;
		const designSessionId = designTarget?.agentType ? designTarget.sessionId : null;
		if (designSessionId) {
			const armed = designModeStore.isArmed(designSessionId);
			if (armed)
				entries.push({
					id: "stop-design-mode",
					label: t("tabBar.stopDesignMode", "Stop Design Mode"),
					category: "Terminal",
					keybinding: "",
					execute: () => designModeStore.toggle(designSessionId),
				});
			if (!armed)
				for (const browser of designModeBrowsers)
					entries.push({
						id: `start-design-mode-${browser.id}`,
						label: `Start Design Mode in ${browser.label}`,
						category: "Terminal",
						keybinding: "",
						execute: () => designModeStore.toggle(designSessionId, browser.id),
					});
		}
		if (activeTermId) {
			for (const worktree of options.gitOps.getWorktreeTargets(activeTermId)) {
				entries.push({
					id: `move-to-worktree:${worktree.path}`,
					label: `Move to worktree: ${worktree.branchName}`,
					category: "Terminal",
					keybinding: "",
					execute: () => options.gitOps.moveTerminalToWorktree(activeTermId, worktree.path),
				});
			}
		}

		for (const prompt of promptLibraryStore.getSmartByPlacement("command-palette")) {
			entries.push({
				id: `smart:${prompt.id}`,
				label: `Smart: ${prompt.name}`,
				category: "Smart Prompts",
				keybinding: prompt.shortcut ?? "",
				execute: () => {
					void runWithFeedback(prompt, () => options.executeSmartPrompt(prompt));
				},
			});
		}

		for (const action of contextMenuActionsStore.getActions()) {
			entries.push({
				id: `plugin-action:${action.id}`,
				label: action.label,
				category: "Plugins",
				keybinding: "",
				execute: () => {
					const activeId = terminalsStore.state.activeId;
					const terminal = activeId ? terminalsStore.get(activeId) : null;
					action.action({ sessionId: terminal?.sessionId ?? null, repoPath: null });
				},
			});
		}

		return entries;
	});
}
