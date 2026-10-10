import type { Setter } from "solid-js";
import { AGENTS, type AgentRunConfig, type AgentType } from "../agents";
import type { ContextMenuItem } from "../components/ContextMenu";
import { invoke } from "../invoke";
import { getModifierSymbol, isWindows } from "../platform";
import { agentConfigsForRepo, ensureAgentConfigsForRepo } from "../stores/agentConfigs";
import { appLogger } from "../stores/appLogger";
import { contextMenuActionsStore } from "../stores/contextMenuActionsStore";
import { paneLayoutStore } from "../stores/paneLayout";
import { repositoriesStore } from "../stores/repositories";
import { settingsStore } from "../stores/settings";
import { terminalsStore } from "../stores/terminals";
import { prepareAgentLaunchCommand } from "../utils/agentSession";
import { writeClipboard } from "../utils/clipboard";
import { keyFor } from "../utils/hotkey";
import { getShellFamily, type ShellFamily, sendCommand } from "../utils/sendCommand";
import { escapePosixShellArg } from "../utils/shell";
import type { useAgentDetection } from "./useAgentDetection";
import type { useGitOperations } from "./useGitOperations";
import type { useSplitPanes } from "./useSplitPanes";
import type { useTerminalLifecycle } from "./useTerminalLifecycle";

interface TerminalContextMenuOptions {
	agentDetection: ReturnType<typeof useAgentDetection>;
	gitOps: ReturnType<typeof useGitOperations>;
	splitPanes: ReturnType<typeof useSplitPanes>;
	terminalLifecycle: ReturnType<typeof useTerminalLifecycle>;
	closeActiveTabOrPane: () => void;
	openPromptInspector: (sessionId: string) => void;
	setTermRenameDefault: Setter<string>;
	setTermRenamePromptVisible: Setter<boolean>;
}

function scopedLaunchCommand(
	command: string,
	env: AgentRunConfig["env"] | undefined,
	shellFamily: ShellFamily,
): string {
	const windows = isWindows();
	const entries = Object.entries(env ?? {}).filter(([key]) => {
		const name = windows || shellFamily === "windows-native" ? key.toUpperCase() : key;
		return /^[A-Za-z_][A-Za-z0-9_]*$/.test(key) && name !== "TUIC_SESSION" && name !== "TUIC_PARENT";
	});
	if (!entries.length) return command;
	if (shellFamily !== "windows-native" && (shellFamily === "posix" || !windows)) {
		return `env ${entries.map(([key, value]) => `${key}=${escapePosixShellArg(value)}`).join(" ")} ${command}`;
	}
	// EncodedCommand is accepted by both cmd and PowerShell and keeps values out of
	// the parent shell's parser and history. The child exits when the agent exits.
	const psQuote = (value: string) => `'${value.replaceAll("'", "''")}'`;
	const script = [
		...entries.map(([key, value]) => `$env:${key} = ${psQuote(value)}`),
		`Invoke-Expression ${psQuote(command)}`,
	].join("; ");
	let bytes = "";
	for (let index = 0; index < script.length; index++) {
		const codeUnit = script.charCodeAt(index);
		bytes += String.fromCharCode(codeUnit & 0xff, codeUnit >> 8);
	}
	return `powershell.exe -NoProfile -EncodedCommand ${btoa(bytes)}`;
}

function newTerminalShellFamily(): ShellFamily {
	if (!isWindows()) return "posix";
	const shell = settingsStore.state.shell?.toLowerCase() ?? "";
	return /(?:^|[\\/])(?:bash|zsh|sh|fish|dash|ksh|ash|tcsh|csh|mksh|wsl)(?:\.exe)?(?:\s|$)/.test(shell)
		? "posix"
		: "windows-native";
}

/** Builds terminal and sidebar agent context menus while keeping launch behavior in one owner. */
export function useTerminalContextMenus(options: TerminalContextMenuOptions): {
	buildSidebarAgentMenuItems: (repoPath: string, branchName: string) => ContextMenuItem[];
	getContextMenuItems: () => ContextMenuItem[];
} {
	const launchAgentInActiveTerminal = async (agentType: AgentType, command: string, env?: AgentRunConfig["env"]) => {
		const active = terminalsStore.getActive();
		if (!active?.ref || !active.sessionId) return;
		const agentSessionId = agentType === "claude" ? null : (active.tuicSession ?? null);
		const finalCommand = await prepareAgentLaunchCommand(command, agentSessionId, agentType, active.cwd);
		const shellFamily = await getShellFamily(active.sessionId);
		await sendCommand(
			(data) => invoke("write_pty", { sessionId: active.sessionId, data }),
			scopedLaunchCommand(finalCommand, env, shellFamily),
			null,
			shellFamily,
		);
		terminalsStore.update(active.id, {
			name: AGENTS[agentType].name,
			nameIsCustom: true,
			agentLaunchCommand: command,
		});
	};

	/**
	 * The run configs of the machine the active terminal runs on.
	 *
	 * A menu is built inside the click that opens it, so this reads the cached
	 * copy and warms it for the next open; a machine that came up has already
	 * been prefetched by `remoteConnections`.
	 */
	const activeTerminalConfigs = () => {
		const cwd = terminalsStore.getActive()?.cwd ?? null;
		void ensureAgentConfigsForRepo(cwd);
		return agentConfigsForRepo(cwd);
	};

	const buildAgentMenuItems = (): ContextMenuItem[] =>
		options.agentDetection
			.getAvailable()
			.filter((agent) => agent.type !== "git" && agent.type !== "api")
			.map((agent) => {
				const config = AGENTS[agent.type];
				const runConfigs = activeTerminalConfigs().getRunConfigs(agent.type);
				if (runConfigs.length > 1) {
					return {
						label: config.name,
						action: () => {},
						children: runConfigs.map((runConfig) => ({
							label: runConfig.name + (runConfig.is_default ? " (Default)" : ""),
							action: () =>
								launchAgentInActiveTerminal(
									agent.type,
									[runConfig.command, ...runConfig.args].join(" "),
									runConfig.env,
								),
						})),
					};
				}
				const command =
					runConfigs.length === 1 ? [runConfigs[0].command, ...runConfigs[0].args].join(" ") : config.binary;
				return {
					label: config.name,
					action: () => launchAgentInActiveTerminal(agent.type, command, runConfigs[0]?.env),
				};
			});

	const buildSidebarAgentMenuItems = (repoPath: string, branchName: string): ContextMenuItem[] => {
		const enabled = options.agentDetection
			.getAvailable()
			.filter((agent) => agent.type !== "git" && agent.type !== "api" && settingsStore.isAgentEnabled(agent.type));
		if (enabled.length === 0) return [];

		// The tab will run on the machine that holds this repo, so the run configs
		// offered here are that machine's, not the ones on this desktop.
		void ensureAgentConfigsForRepo(repoPath);
		const machineConfigs = agentConfigsForRepo(repoPath);

		const buildAgentEntry = (agent: (typeof enabled)[0]) => {
			const config = AGENTS[agent.type];
			const runConfigs = machineConfigs.getRunConfigs(agent.type);
			const launchAgent = async (command: string, env?: AgentRunConfig["env"]) => {
				const termId = await options.gitOps.handleAddTerminalToWorkspace(repoPath, branchName);
				if (!termId) return;
				const term = terminalsStore.get(termId);
				const agentSessionId = agent.type === "claude" ? null : (term?.tuicSession ?? null);
				const pendingInitCommand = await prepareAgentLaunchCommand(command, agentSessionId, agent.type, repoPath);
				terminalsStore.update(termId, {
					name: config.name,
					nameIsCustom: true,
					pendingInitCommand: scopedLaunchCommand(pendingInitCommand, env, newTerminalShellFamily()),
					agentType: agent.type,
					agentLaunchCommand: command,
				});
			};
			const children: ContextMenuItem[] =
				runConfigs.length > 0
					? runConfigs.map((runConfig) => ({
							label: runConfig.name + (runConfig.is_default ? " (Default)" : ""),
							action: () => launchAgent([runConfig.command, ...runConfig.args].join(" "), runConfig.env),
						}))
					: [{ label: "(Default)", action: () => launchAgent(config.binary) }];
			return { config, children };
		};

		if (enabled.length === 1) {
			const { config, children } = buildAgentEntry(enabled[0]);
			return children.length === 1
				? [{ label: `Add ${config.name}`, action: children[0].action }]
				: [{ label: `Add ${config.name}`, action: () => {}, children }];
		}

		return [
			{
				label: "Add Agent",
				action: () => {},
				children: enabled.map((agent) => {
					const { config, children } = buildAgentEntry(agent);
					return children.length === 1
						? { label: config.name, action: children[0].action }
						: { label: config.name, action: () => {}, children };
				}),
			},
		];
	};

	const splitDisabled = () => {
		if (!paneLayoutStore.isSplit()) return !terminalsStore.state.activeId;
		const activeGroupId = paneLayoutStore.state.activeGroupId;
		return !activeGroupId || !paneLayoutStore.canSplit(activeGroupId);
	};

	const activeTerminalBusy = () => {
		const activeId = terminalsStore.state.activeId;
		return !activeId || Boolean(terminalsStore.get(activeId)?.agentType);
	};

	const getContextMenuItems = (): ContextMenuItem[] => [
		{
			label: "Inspect Launch Instructions…",
			disabled: !terminalsStore.getActive()?.sessionId,
			action: () => {
				const id = terminalsStore.getActive()?.sessionId;
				if (id) options.openPromptInspector(id);
			},
		},
		...(options.agentDetection.getAvailable().length > 0
			? [{ label: "Agents", action: () => {}, disabled: activeTerminalBusy(), children: buildAgentMenuItems() }]
			: []),
		{ label: "Copy", shortcut: `${getModifierSymbol()}C`, action: options.terminalLifecycle.copyFromTerminal },
		{
			label: "Paste",
			shortcut: `${getModifierSymbol()}V`,
			action: options.terminalLifecycle.pasteToTerminal,
			separator: true,
		},
		{
			label: "Copy Block Output",
			action: async () => {
				const activeId = terminalsStore.state.activeId;
				const term = activeId ? terminalsStore.get(activeId) : undefined;
				const lastBlock = term?.commandBlocks[term.commandBlocks.length - 1];
				if (
					!term?.ref ||
					!lastBlock ||
					lastBlock.executionLine == null ||
					lastBlock.endLine == null ||
					lastBlock.endLine <= term.historyBase
				)
					return;
				// The menu invokes this without awaiting it, so a rejected buffer read
				// would surface as an unhandled rejection instead of a failed copy.
				let lines: string[];
				try {
					lines = await term.ref.getBufferLines(
						Math.max(0, lastBlock.executionLine + 1 - term.historyBase),
						lastBlock.endLine - term.historyBase,
					);
				} catch (e) {
					appLogger.warn("terminal", "Copy Block Output failed to read the buffer", { error: String(e) });
					return;
				}
				const text = lines.join("\n").trimEnd();
				if (text) void writeClipboard(text);
			},
			disabled: (() => {
				const activeId = terminalsStore.state.activeId;
				return !activeId || !terminalsStore.get(activeId)?.commandBlocks.length;
			})(),
		},
		{
			label: "Split Right",
			shortcut: keyFor("split-vertical"),
			action: () => options.splitPanes.handleSplit("vertical"),
			disabled: splitDisabled(),
		},
		{ label: "Split Left", action: () => options.splitPanes.handleSplit("vertical"), disabled: splitDisabled() },
		{
			label: "Split Down",
			shortcut: keyFor("split-horizontal"),
			action: () => options.splitPanes.handleSplit("horizontal"),
			disabled: splitDisabled(),
		},
		{
			label: "Split Up",
			action: () => options.splitPanes.handleSplit("horizontal"),
			disabled: splitDisabled(),
			separator: true,
		},
		{ label: "Clear", shortcut: keyFor("clear-terminal"), action: options.terminalLifecycle.clearTerminal },
		{
			label: "Reset Terminal",
			action: () => {
				const activeId = terminalsStore.state.activeId;
				if (activeId) terminalsStore.get(activeId)?.ref?.write("\x1bc");
			},
		},
		{
			label: "Change Title…",
			action: () => {
				const activeId = terminalsStore.state.activeId;
				if (!activeId) return;
				options.setTermRenameDefault(terminalsStore.get(activeId)?.name || "");
				options.setTermRenamePromptVisible(true);
			},
			separator: true,
		},
		...buildRegisteredActionGroup(),
		...buildSmartPromptGroup(),
		{
			label: "Close Terminal",
			shortcut: keyFor("close-terminal"),
			action: options.closeActiveTabOrPane,
			separator: true,
		},
	];

	const buildRegisteredActionGroup = (): ContextMenuItem[] => {
		const legacyActions = contextMenuActionsStore.getActions();
		const pluginActions = contextMenuActionsStore.getContextActions("terminal", {
			excludePluginId: "smart-prompts",
		});
		if (legacyActions.length === 0 && pluginActions.length === 0) return [];
		const activeId = terminalsStore.state.activeId;
		const sessionId = activeId ? (terminalsStore.get(activeId)?.sessionId ?? null) : null;
		const repoPath = repositoriesStore.state.activeRepoPath ?? null;
		const legacyContext = { sessionId, repoPath };
		const context = {
			target: "terminal" as const,
			sessionId: sessionId ?? undefined,
			repoPath: repoPath ?? undefined,
		};
		return [
			{
				label: "Actions",
				action: () => {},
				children: [
					...legacyActions.map((action) => ({
						label: action.label,
						action: () => action.action(legacyContext),
						disabled: action.disabled?.(legacyContext) ?? false,
					})),
					...pluginActions.map((action) => ({
						label: action.label,
						action: () => action.action(context),
						disabled: action.disabled?.(context) ?? false,
					})),
				],
				separator: true,
			},
		];
	};

	const buildSmartPromptGroup = (): ContextMenuItem[] => {
		const actions = contextMenuActionsStore.getContextActions("terminal", { pluginId: "smart-prompts" });
		if (actions.length === 0) return [];
		const activeId = terminalsStore.state.activeId;
		const context = {
			target: "terminal" as const,
			sessionId: activeId ? (terminalsStore.get(activeId)?.sessionId ?? undefined) : undefined,
			repoPath: repositoriesStore.state.activeRepoPath ?? undefined,
		};
		return [
			{
				label: "Prompts",
				action: () => {},
				children: actions.map((action) => ({
					label: action.label,
					action: () => action.action(context),
					disabled: action.disabled?.(context) ?? false,
				})),
				separator: true,
			},
		];
	};

	return { buildSidebarAgentMenuItems, getContextMenuItems };
}
