import { type Component, createEffect, createSignal, For, onCleanup, Show } from "solid-js";
import { settingsCatalogue } from "../../generated/settingsCatalogue";
import { t } from "../../i18n";
import { invoke } from "../../invoke";
import { shortenHomePath } from "../../platform";
import { hasCatalogueEditor, settingLabel } from "../../services/settingsCatalogue";
import { repoDefaultsStore } from "../../stores/repoDefaults";
import { type RepoSettings, repoSettingsStore } from "../../stores/repoSettings";
import { repositoriesStore } from "../../stores/repositories";
import { settingsStore } from "../../stores/settings";
import { settingsExpertStore } from "../../stores/settingsExpert";
import { toastsStore } from "../../stores/toasts";
import { uiStore } from "../../stores/ui";
import { isTauri } from "../../transport";
import { pathBasename } from "../../utils/pathUtils";
import { getRepoColor } from "../../utils/repoColor";
import { Button } from "../ui/Button";
import { AllSettingsTab } from "./AllSettingsTab";
import { SettingsHelpScope } from "./ContextualHelp";
import { DictationSettings } from "./DictationSettings";
import { ExpertModeSwitch } from "./ExpertSetting";
import { PluginSettingsTab } from "./PluginSettingsTab";
import { createPluginSettingsCatalogue, pluginFieldLabel } from "./pluginSettingsCatalogue";
import { RepositoryActions } from "./RepositoryActions";
import s from "./Settings.module.css";
import { SettingsSearchBox, SettingsSearchResults, scrollToSetting } from "./SettingsSearch";
import type { SettingsShellTab } from "./SettingsShell";
import { SettingsShell } from "./SettingsShell";
import { entryLabel, entrySection, type SettingsSearchEntry, searchSettings } from "./settingsSearchIndex";
import {
	AgentsTab,
	AiChatTab,
	AppearanceTab,
	GeneralTab,
	GitHubTab,
	KeyboardShortcutsTab,
	LocalMcpPanel,
	NotificationsTab,
	PluginsTab,
	RemoteAccessPanel,
	RemoteMachinesTab,
	RepoScriptsTab,
	RepoWorktreeTab,
	SmartPromptsTab,
	TelegramTab,
	TerminalTab,
} from "./tabs";

/** Context for initial selection when opening the panel */
export type SettingsContext = { kind: "global" } | { kind: "repo"; repoPath: string; connectionId?: string };

export interface SettingsPanelProps {
	visible: boolean;
	onClose: () => void;
	initialTab?: string;
	/** DOM id of a block to scroll to once the panel is open — see sections.ts */
	initialSection?: string;
	context?: SettingsContext;
}

/** Global pages grouped by task; each group renders as a static label row above its pages. */
const GLOBAL_TAB_GROUPS: { key: string; label: string; tabs: SettingsShellTab[] }[] = [
	{
		key: "application",
		label: t("settings.group.application", "Application"),
		tabs: [
			{ key: "general", label: t("settings.general", "General") },
			{ key: "all-settings", label: "All settings (Expert)" },
			{ key: "appearance", label: t("settings.appearance", "Appearance") },
			{ key: "notifications", label: t("settings.notifications", "Notifications") },
		],
	},
	{
		key: "workspace",
		label: t("settings.group.workspace", "Workspace"),
		tabs: [
			{ key: "terminal", label: t("settings.terminal", "Terminal") },
			{ key: "keyboard-shortcuts", label: t("settings.keyboardShortcuts", "Keyboard Shortcuts") },
			{ key: "github", label: "Git & GitHub" },
		],
	},
	{
		key: "ai",
		label: t("settings.group.ai", "AI"),
		tabs: [
			{ key: "agents", label: t("settings.agents", "Agents") },
			{ key: "ai-chat", label: t("settings.aiChat", "AI Chat") },
			{ key: "dictation", label: t("settings.voice", "Voice") },
			{ key: "smart-prompts", label: t("settings.smartPrompts", "Smart Prompts") },
		],
	},
	{
		key: "integrations",
		label: t("settings.group.integrations", "Integrations"),
		tabs: [
			{ key: "mcp", label: t("settings.mcp", "MCP") },
			{ key: "remote-access", label: t("settings.remoteAccess", "Remote Access") },
			{ key: "remote-machines", label: t("settings.remoteMachines", "Remote Machines") },
			{ key: "telegram", label: "Telegram" },
			{ key: "plugins", label: t("settings.managePlugins", "Manage plugins") },
		],
	},
];

/** Tabs whose feature is switched off right now, so their nav entry is noise. */
function hiddenTabs(): Set<string> {
	const hidden = new Set<string>();
	// Dictation is no longer desktop-only: a browser holds a hands-free
	// conversation through its own microphone and speaker over a WS audio
	// socket (#832-e730). The controls that really are local — the global
	// hotkey and this machine's input devices — are hidden inside the tab
	// rather than by hiding the whole tab.
	// AI Chat configures ego, and ego is reachable only from the AI Chat panel.
	// While that panel is behind the experimental toggle, this tab would let a
	// person set a default model for an engine they cannot open.
	if (!settingsStore.isAiChatEnabled()) hidden.add("ai-chat");
	return hidden;
}

function getGlobalTabs(): SettingsShellTab[] {
	const hidden = hiddenTabs();
	return GLOBAL_TAB_GROUPS.flatMap((group) => group.tabs.filter((tab) => !hidden.has(tab.key)));
}

function defaultTab(ctx: SettingsContext): string {
	if (ctx.kind === "repo") return `repo:${ctx.repoPath}`;
	return "general";
}

/** Nav keys of pages that were folded into another page. A deep link
 * (`tuic://settings?tab=…`) or a caller written before the move still names
 * them, and an unknown key renders no page at all. */
const RETIRED_TABS: Record<string, string> = {
	"developer-tools": "general",
	// Services & MCP split into MCP, Remote Access and Remote Machines;
	// AI Providers became AI Chat.
	services: "mcp",
	providers: "ai-chat",
};

/** The page to open for a requested tab key. */
function initialTabFor(requested: string | undefined, ctx: SettingsContext): string {
	if (requested === undefined) return defaultTab(ctx);
	return RETIRED_TABS[requested] ?? requested;
}

/** Build the full nav from global sections + configured repos */
function buildNavItems(pluginTabs: SettingsShellTab[] = []): SettingsShellTab[] {
	// All repos, including those nested in groups — grouped repos live in
	// group.repoOrder, not state.repoOrder, so iterating repoOrder alone would
	// hide them from the Settings nav. (#64)
	const repos = repositoriesStore.getAllReposOrdered();

	const hidden = hiddenTabs();
	const items: SettingsShellTab[] = GLOBAL_TAB_GROUPS.flatMap((group) => [
		{ key: `__label__:${group.key}`, label: group.label },
		...group.tabs.filter((tab) => !hidden.has(tab.key)),
		...(group.key === "integrations" && pluginTabs.length
			? [
					{
						key: "__label__:plugin-settings",
						label: "Plugin settings",
						icon: (
							<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
								<path d="M5 1h2v4h2V1h2v4h2v2h-1v2a4 4 0 0 1-3 3.87V16H7v-3.13A4 4 0 0 1 4 9V7H3V5h2V1zm1 6v2a2 2 0 0 0 4 0V7H6z" />
							</svg>
						),
					},
					...pluginTabs,
				]
			: []),
	]);

	if (repos.length > 0) {
		items.push({ key: "__label__:Repositories", label: t("settings.repositories", "REPOSITORIES") });
		for (const repo of repos) {
			const label = repo.displayName || pathBasename(repo.path) || repo.path;
			const color = getRepoColor(repo.path);
			items.push({ key: `repo:${repo.path}`, label, color });
		}
	}

	return items;
}

export const SettingsPanel: Component<SettingsPanelProps> = (props) => {
	const ctx = () => props.context ?? { kind: "global" as const };
	const [activeTab, setActiveTab] = createSignal(initialTabFor(props.initialTab, ctx()));

	const [query, setQuery] = createSignal("");
	const [catalogueSelection, setCatalogueSelection] = createSignal<string>();
	const catalogueEntries = Object.values(settingsCatalogue.fields)
		.filter(hasCatalogueEditor)
		.map((field) => ({
			tab: "all-settings",
			section: "Catalogue inspector",
			label: settingLabel(field.id, field.label),
			configKey: field.id,
		}));
	const pluginPages = createPluginSettingsCatalogue();
	const pluginTabs = () =>
		pluginPages().map((page) => ({
			key: `plugin:${page.id}`,
			label: page.label,
			source: page.id,
			icon: (
				<svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor">
					<path d="M8 2h2v5h4V2h2v5h3v4a7 7 0 0 1-6 6.93V22h-2v-4.07A7 7 0 0 1 5 11V7h3V2Zm-1 7v2a5 5 0 0 0 10 0V9H7Z" />
				</svg>
			),
		}));

	const navItems = () => buildNavItems(pluginTabs());
	const pluginEntries = () =>
		pluginPages().flatMap((page) =>
			page.fields.map((field) => ({ tab: `plugin:${page.id}`, section: page.label, label: pluginFieldLabel(field) })),
		);

	// Reset active tab when context changes or panel opens
	createEffect(() => {
		if (props.visible) {
			setActiveTab(initialTabFor(props.initialTab, ctx()));
			// A stale query would hide the tab the caller asked for behind results
			setQuery("");
			void settingsExpertStore.open();
		}
	});

	// Setting a search result asked for, consumed by the scroll effect below
	const [pendingTarget, setPendingTarget] = createSignal<{ section: string; label?: string } | null>(null);

	// Two callers need the panel scrolled to a block that sits below the fold:
	// a deep link (the MCP popup's "Manage in Settings", which names a DOM id)
	// and a search result (which names a section heading). Both have to wait for
	// the frame that inserts the tab content into the document.
	createEffect(() => {
		if (!props.visible) return;
		const target = pendingTarget();
		const section = props.initialSection;
		if (!target && !section) return;
		const frame = requestAnimationFrame(() => {
			if (target) {
				const content = document.querySelector("[data-settings-content]");
				if (content) scrollToSetting(content, target.section, target.label);
				setPendingTarget(null);
				return;
			}
			if (section) document.getElementById(section)?.scrollIntoView({ block: "start", behavior: "smooth" });
		});
		onCleanup(() => cancelAnimationFrame(frame));
	});

	/** Open the tab a search result lives in, then scroll to its section. */
	const openResult = (entry: SettingsSearchEntry) => {
		setQuery("");
		setActiveTab(entry.tab);
		if (entry.tab === "all-settings") setCatalogueSelection(entry.configKey);
		// A hidden expert control has nothing to scroll to until it is revealed
		if (entry.configKey) settingsExpertStore.reveal(entry.configKey);
		setPendingTarget({ section: entrySection(entry), label: entryLabel(entry) });
	};

	/** Repo path if a repo nav item is currently active, null otherwise */
	const activeRepoPath = (): string | null => {
		const tab = activeTab();
		return tab.startsWith("repo:") ? tab.slice(5) : null;
	};

	const activeConnectionId = (): string | undefined => {
		const path = activeRepoPath();
		return path ? repositoriesStore.getConnectionId(path) : undefined;
	};

	// Only tabs the nav actually offers (AI Chat sits behind a flag), and only
	// controls this client renders: a desktop-only control would open a tab
	// without it, so a browser is not offered it.
	const results = () =>
		searchSettings(
			query(),
			new Set([...getGlobalTabs(), ...pluginTabs()].map((tab) => tab.key)),
			isTauri() ? "desktop" : "browser",
			[...pluginEntries(), ...catalogueEntries],
		);

	const repoSettings = (path: string) => repoSettingsStore.getOrCreate(path, shortenHomePath(path));

	const updateRepoSetting =
		(repoPath: string) =>
		<K extends keyof RepoSettings>(key: K, value: RepoSettings[K]) => {
			repoSettingsStore.update(repoPath, { [key]: value });
			if (key === "displayName") {
				repositoriesStore.setDisplayName(repoPath, value as string);
			}
		};

	/** Write this repo's UI settings into a committable `.tuic.json` at its root */
	const copyToProject = async (repoPath: string) => {
		try {
			await invoke("save_repo_local_config", { repoPath });
			toastsStore.add(
				t("settings.copyToProject.done", "Saved .tuic.json"),
				t("settings.copyToProject.doneHint", "Repo settings written to the project root — commit to share"),
				"info",
			);
		} catch (err) {
			toastsStore.add(t("settings.copyToProject.failed", "Failed to write .tuic.json"), String(err), "error");
		}
	};

	const footer = () => {
		return (
			<div class={s.footer}>
				<span />
				<Button
					variant="primary"
					data-non-setting="Navigate or close Settings without editing configuration"
					class={s.footerDone}
					onClick={props.onClose}
				>
					{t("settings.done", "Done")}
				</Button>
			</div>
		);
	};

	return (
		<SettingsShell
			visible={props.visible}
			onClose={props.onClose}
			title={t("settings.title", "Settings")}
			tabs={navItems()}
			activeTab={activeTab()}
			onTabChange={setActiveTab}
			navWidth={uiStore.state.settingsNavWidth}
			onNavWidthChange={uiStore.setSettingsNavWidth}
			onNavWidthPersist={uiStore.persistUIPrefs}
			headerActions={<ExpertModeSwitch />}
			navHeader={<SettingsSearchBox value={query()} onInput={setQuery} />}
			footer={footer()}
		>
			<Show
				when={!query()}
				fallback={<SettingsSearchResults results={results()} tabs={navItems()} onSelect={openResult} />}
			>
				{/* Repo settings (shown when a repo nav item is active) */}
				<Show when={activeRepoPath()} keyed>
					{(path) => {
						const settings = repoSettings(path);
						const onUpdate = updateRepoSetting(path);
						return (
							<>
								<SettingsHelpScope page="repository">
									<RepoWorktreeTab settings={settings} defaults={repoDefaultsStore.state} onUpdate={onUpdate} />
									<RepoScriptsTab settings={settings} defaults={repoDefaultsStore.state} onUpdate={onUpdate} />
								</SettingsHelpScope>
								<RepositoryActions
									path={settings.path}
									canExport={isTauri()}
									onExport={() => void copyToProject(path)}
									onReset={() => repoSettingsStore.reset(path)}
								/>
							</>
						);
					}}
				</Show>

				{/* Live scoped plugin pages disappear with the loaded catalogue. */}
				<For each={pluginPages().filter((page) => activeTab() === `plugin:${page.id}`)}>
					{(page) => (
						<SettingsHelpScope page={page.id}>
							<PluginSettingsTab page={page} />
						</SettingsHelpScope>
					)}
				</For>

				{/* Global sections */}
				<Show when={activeTab() === "all-settings"}>
					<AllSettingsTab initialSetting={catalogueSelection()} />
				</Show>
				<Show when={activeTab() === "general"}>
					<SettingsHelpScope page="general">
						<GeneralTab onOpenAiChat={settingsStore.isAiChatEnabled() ? () => setActiveTab("ai-chat") : undefined} />
					</SettingsHelpScope>
				</Show>
				<Show when={activeTab() === "appearance"}>
					<SettingsHelpScope page="appearance">
						<AppearanceTab />
					</SettingsHelpScope>
				</Show>
				<Show when={activeTab() === "notifications"}>
					<SettingsHelpScope page="notifications">
						<NotificationsTab />
					</SettingsHelpScope>
				</Show>
				<Show when={activeTab() === "terminal"}>
					<SettingsHelpScope page="terminal">
						<TerminalTab />
					</SettingsHelpScope>
				</Show>
				<Show when={activeTab() === "keyboard-shortcuts"}>
					<SettingsHelpScope page="keyboard-shortcuts">
						<KeyboardShortcutsTab />
					</SettingsHelpScope>
				</Show>
				<Show when={activeTab() === "dictation"}>
					<SettingsHelpScope page="voice">
						<DictationSettings />
					</SettingsHelpScope>
				</Show>
				<Show when={activeTab() === "github"}>
					<SettingsHelpScope page="github">
						<GitHubTab />
					</SettingsHelpScope>
				</Show>
				<Show when={activeTab() === "mcp"}>
					<SettingsHelpScope page="mcp">
						<LocalMcpPanel />
					</SettingsHelpScope>
				</Show>
				<Show when={activeTab() === "remote-access"}>
					<SettingsHelpScope page="remote-access">
						<RemoteAccessPanel />
					</SettingsHelpScope>
				</Show>
				<Show when={activeTab() === "remote-machines"}>
					<SettingsHelpScope page="remote-machines">
						<RemoteMachinesTab />
					</SettingsHelpScope>
				</Show>
				<Show when={activeTab() === "telegram"}>
					<SettingsHelpScope page="telegram">
						<TelegramTab />
					</SettingsHelpScope>
				</Show>
				<Show when={activeTab() === "plugins"}>
					<SettingsHelpScope page="plugins">
						<PluginsTab
							onClose={props.onClose}
							settingsPluginIds={pluginPages().map((page) => page.id)}
							onOpenSettings={(id) => setActiveTab(`plugin:${id}`)}
						/>
					</SettingsHelpScope>
				</Show>
				<Show when={activeTab() === "smart-prompts"}>
					<SettingsHelpScope page="smart-prompts">
						<SmartPromptsTab />
					</SettingsHelpScope>
				</Show>
				<Show when={activeTab() === "agents"}>
					<SettingsHelpScope page="agents">
						<AgentsTab connectionId={activeConnectionId()} />
					</SettingsHelpScope>
				</Show>
				<Show when={activeTab() === "ai-chat" && settingsStore.isAiChatEnabled()}>
					<SettingsHelpScope page="ai-chat">
						<AiChatTab onClose={props.onClose} />
					</SettingsHelpScope>
				</Show>
			</Show>
		</SettingsShell>
	);
};
