import { t } from "../../i18n";

/** Search index for the Settings panel — one entry per section heading and per
 * labelled setting, across every global settings tab.
 *
 * ## Design decision (2026-09-05)
 *
 * Three options were on the table for building this index:
 *
 * 1. **DOM scan of mounted tabs.** Rejected: `SettingsPanel` renders exactly one
 *    tab at a time, so a scan sees ~1/11th of the settings. Mounting every tab
 *    to scan it is worse than useless — each tab runs `onMount` side effects
 *    (`get_cli_status`, `mdkb_status`, GitHub auth probes, audio device
 *    enumeration), so a keystroke in the search box would fire a burst of
 *    backend calls.
 * 2. **Vite plugin extracting the index at build time.** Rejected for now: it
 *    makes drift structurally impossible, but it puts JSX parsing in the build
 *    graph, needs a second code path for vitest, and buys only what the drift
 *    test below already guarantees. Revisit if the index grows past a few
 *    hundred entries.
 * 3. **Committed literal index + a drift test that re-derives it from the tab
 *    sources.** Chosen. The array below is generated, not hand-typed;
 *    `__tests__/settingsSearchIndex.test.ts` re-extracts it from the `.tsx`
 *    files and fails on any difference, so the index cannot drift silently.
 *    This is the same contract `src/transport.ts`'s `COMMAND_TABLE` uses.
 *
 * ## Extraction rule
 *
 * Mechanical, so the drift test can reproduce it exactly:
 * every `<h3>` or `<SectionCard title=…>` / `<DangerZone title=…>` opens a section; every `label=` prop and every `<label>` element
 * is a setting inside the nearest preceding `<h3>`. Text comes from
 * `t("key", "Default")`, a string literal, or a leading plain-text run.
 *
 * Two categories are deliberately outside the index, and the drift test pins
 * their counts so a new one cannot slip in unnoticed:
 * - **dynamic** — text computed at runtime (per-agent cards, per-plugin rows);
 *   there is nothing stable to index or to scroll to.
 * - **orphan** — a label with no `<h3>` above it, i.e. a modal form field (the
 *   Smart Prompts editor); it is not a setting and has no scroll target.
 *
 * A label inside `<ExpertSetting configKey="…">` also carries `expert: true`
 * and that `configKey`, so search can badge it and reveal it on open.
 *
 * Repo-scoped tabs (`repo:<path>`) are not indexed: their nav key depends on
 * which repository the user means, and a global search box cannot know.
 *
 * A label inside `<Show when={isTauri()…}>` carries `platform: "desktop"`, one
 * in that Show's `fallback` carries `platform: "browser"`; search offers each
 * client only what it renders.
 *
 * ## Composed pages (mcp, remote-machines)
 *
 * A page rendered by several components is extracted from all of them, in
 * render order: `mcp` is `LocalMcpPanel` then `UpstreamMcpPanel`,
 * `remote-machines` is `RemoteMachinesTab` wrapping `RemoteMachinesPanel`. The
 * drift test holds the source list and checks it against what `SettingsPanel`
 * renders for each tab.
 */
export interface SettingsSearchEntry {
	/** `SettingsShell` nav key of the tab that renders this entry */
	tab: string;
	/** Default text of the `<h3>` this entry lives under; with `label` it is the
	 * scroll anchor `scrollToSetting` looks for in the rendered tab */
	section: string;
	sectionKey?: string;
	/** Setting label; absent when the entry is the section heading itself */
	label?: string;
	labelKey?: string;
	/** The setting sits inside an `ExpertSetting`: results mark it "Expert" */
	expert?: boolean;
	/** That `ExpertSetting`'s configKey — opening the result reveals it */
	configKey?: string;
	/** Rendered by one client only (an `isTauri()` Show or its fallback) */
	platform?: "desktop" | "browser";
}

export const SETTINGS_SEARCH_INDEX: SettingsSearchEntry[] = [
	{ tab: "telegram", section: "Telegram" },
	{ tab: "telegram", section: "Bot setup" },
	{ tab: "telegram", section: "Chat access" },
	{ tab: "telegram", section: "Agent registration" },
	{ tab: "telegram", section: "Recent messages" },
	{ tab: "telegram", section: "Bot setup", label: "Enable Telegram", labelKey: "telegram.enabled" },
	{ tab: "telegram", section: "Bot setup", label: "Bot token", labelKey: "telegram.token" },
	{ tab: "telegram", section: "Chat access", label: "Add chat by ID" },
	{ tab: "telegram", section: "Chat access", label: "Link a chat" },
	{ tab: "telegram", section: "Agent registration", label: "Registered agent" },
	{ tab: "general", section: "General" },
	{ tab: "general", section: "Local tools", platform: "desktop" },
	{ tab: "general", section: "AI chat defaults" },
	{ tab: "general", section: "General", sectionKey: "general.heading.general" },
	{ tab: "general", section: "Confirmations", sectionKey: "general.heading.confirmations" },
	{ tab: "general", section: "Power Management", sectionKey: "general.heading.powerManagement" },
	{ tab: "general", section: "Resource Limits" },
	{ tab: "general", section: "Updates", sectionKey: "general.heading.updates" },
	{ tab: "general", section: "ego", sectionKey: "general.heading.ego" },
	{ tab: "general", section: "IDE", sectionKey: "developerTools.heading.ide" },
	{ tab: "general", section: "Experimental Features", sectionKey: "general.heading.experimental" },
	{ tab: "general", section: "Local tools", label: "TUIC CLI", labelKey: "general.heading.cli", platform: "desktop" },
	{
		tab: "general",
		section: "Local tools",
		label: "Code Intelligence",
		labelKey: "general.heading.codeIntelligence",
		platform: "desktop",
	},
	{
		tab: "general",
		section: "Local tools",
		label: "Custom Launchers",
		labelKey: "general.heading.customLaunchers",
		platform: "desktop",
	},
	{ tab: "general", section: "General", label: "Language", labelKey: "general.label.language" },
	{ tab: "general", section: "General", label: "Show agent context bar" },
	{
		tab: "general",
		section: "Confirmations",
		label: "When closing the main window",
		labelKey: "general.label.closeBehavior",
		platform: "desktop",
	},
	{
		tab: "general",
		section: "Confirmations",
		label: "Tray and menu-bar icon",
		labelKey: "general.label.trayIcon",
		platform: "desktop",
	},
	{
		tab: "general",
		section: "Confirmations",
		label: "Hide Dock icon in background",
		labelKey: "general.toggle.hideDockIcon",
		platform: "desktop",
	},
	{
		tab: "general",
		section: "Confirmations",
		label: "Confirm before quitting",
		labelKey: "general.toggle.confirmBeforeQuit",
	},
	{
		tab: "general",
		section: "Confirmations",
		label: "Confirm before closing a tab",
		labelKey: "general.toggle.confirmBeforeClosingTab",
	},
	{
		tab: "general",
		section: "Power Management",
		label: "Prevent sleep when busy",
		labelKey: "general.toggle.preventSleepWhenBusy",
	},
	{
		tab: "general",
		section: "Power Management",
		label: "Auto-Standby Timeout",
		expert: true,
		configKey: "app.standby_timeout_minutes",
	},
	{
		tab: "general",
		section: "Power Management",
		label: "Content Indexing",
		expert: true,
		configKey: "app.index_strategy",
	},
	{
		tab: "general",
		section: "Resource Limits",
		label: "Content index memory budget (MB)",
		expert: true,
		configKey: "app.index_memory_budget_mb",
	},
	{
		tab: "general",
		section: "Resource Limits",
		label: "Maximum attachment size (bytes)",
		expert: true,
		configKey: "app.attachment_max_bytes",
	},
	{
		tab: "general",
		section: "Resource Limits",
		label: "Background WebView unload delay (minutes)",
		expert: true,
		configKey: "app.background.webview_unload_minutes",
		platform: "desktop",
	},
	{
		tab: "general",
		section: "Updates",
		label: "Automatically check for updates",
		labelKey: "general.toggle.autoUpdateEnabled",
	},
	{
		tab: "general",
		section: "Updates",
		label: "Wait for agents before restarting",
		labelKey: "general.toggle.deferUpdateRelaunch",
	},
	{
		tab: "general",
		section: "Updates",
		label: "Resume all agents after an update",
		labelKey: "general.toggle.eagerResumeAfterUpdate",
	},
	{
		tab: "general",
		section: "Updates",
		label: "Update Channel",
		labelKey: "general.label.updateChannel",
		expert: true,
		configKey: "app.update_channel",
	},
	{
		tab: "general",
		section: "ego",
		label: "ego executable",
		labelKey: "general.label.egoExecutable",
		platform: "browser",
	},
	{
		tab: "general",
		section: "ego",
		label: "ego executable",
		labelKey: "general.label.egoExecutable",
		platform: "desktop",
	},
	{ tab: "general", section: "ego", label: "ego profile", labelKey: "general.label.egoProfile" },
	{ tab: "general", section: "ego", label: "AI Chat workspace", labelKey: "general.label.aiChatWorkspace" },
	{ tab: "general", section: "IDE", label: "Default IDE", labelKey: "general.label.defaultIde" },
	{
		tab: "general",
		section: "Experimental Features",
		label: "Enable experimental features",
		labelKey: "general.toggle.experimentalFeatures",
	},
	{ tab: "appearance", section: "Appearance" },
	{ tab: "appearance", section: "Tabs", sectionKey: "appearance.heading.tabs" },
	{ tab: "appearance", section: "Chat", sectionKey: "appearance.heading.chat" },
	{ tab: "appearance", section: "Repository Groups", sectionKey: "appearance.heading.groups" },
	{ tab: "appearance", section: "Layout", sectionKey: "appearance.heading.layout" },
	{ tab: "appearance", section: "UI Legend", sectionKey: "appearance.heading.uiLegend" },
	{ tab: "appearance", section: "Tabs", label: "Split Tab Mode", labelKey: "appearance.label.splitTabMode" },
	{ tab: "appearance", section: "Tabs", label: "Tab Ordering", labelKey: "appearance.label.tabOrderingMode" },
	{ tab: "appearance", section: "Tabs", label: "Cycle All Tab Types", labelKey: "appearance.label.tabCyclingAllTypes" },
	{ tab: "appearance", section: "Tabs", label: "Nested Terminal Tabs", labelKey: "appearance.label.tabTreeEnabled" },
	{ tab: "appearance", section: "Tabs", label: "Max Tab Name Length", labelKey: "appearance.label.maxTabNameLength" },
	{ tab: "appearance", section: "Chat", label: "New text indicator", labelKey: "appearance.label.newTextIndicator" },
	{
		tab: "appearance",
		section: "Chat",
		label: "Mute Intermediate Text",
		labelKey: "appearance.label.chatMuteIntermediateText",
	},
	{ tab: "notifications", section: "Notifications" },
	{ tab: "notifications", section: "Notification Settings", sectionKey: "notifications.heading.notificationSettings" },
	{ tab: "notifications", section: "Visual notifications" },
	{ tab: "notifications", section: "Tasks", sectionKey: "notifications.heading.tasks" },
	{
		tab: "notifications",
		section: "Notification Settings",
		label: "Enable audio notifications",
		labelKey: "notifications.toggle.enableAudio",
	},
	{
		tab: "notifications",
		section: "Notification Settings",
		label: "Master Volume",
		labelKey: "notifications.label.masterVolume",
		expert: true,
		configKey: "notifications.volume",
	},
	{
		tab: "notifications",
		section: "Notification Settings",
		label: "Audio Output Device",
		labelKey: "notifications.label.audioDevice",
		expert: true,
		configKey: "notifications.audio_device",
		platform: "desktop",
	},
	{
		tab: "notifications",
		section: "Notification Settings",
		label: "Notification Events",
		labelKey: "notifications.label.notificationEvents",
	},
	{
		tab: "notifications",
		section: "Notification Settings",
		label: "Orchestration",
		labelKey: "notifications.label.orchestration",
	},
	{
		tab: "notifications",
		section: "Notification Settings",
		label: "Silence subagent sounds and notifications",
		labelKey: "notifications.label.silenceRemoteCompletions",
	},
	{
		tab: "notifications",
		section: "Visual notifications",
		label: "Toolbar Bell",
		labelKey: "notifications.label.toolbarBell",
	},
	{
		tab: "notifications",
		section: "Visual notifications",
		label: "Keep toasts in the bell",
		labelKey: "notifications.toggle.toastsInBell",
	},
	{
		tab: "notifications",
		section: "Visual notifications",
		label: "Show cards from the active terminal",
		labelKey: "notifications.toggle.autoToastCards",
	},
	{
		tab: "notifications",
		section: "Visual notifications",
		label: "Pull Requests",
		labelKey: "notifications.label.prNotifications",
	},
	{
		tab: "notifications",
		section: "Visual notifications",
		label: "OS notification for PR changes",
		labelKey: "notifications.toggle.prNative",
	},
	{
		tab: "notifications",
		section: "Tasks",
		label: "OS notifications for Tasks",
		labelKey: "notifications.toggle.tasks",
	},
	{ tab: "terminal", section: "Terminal" },
	{ tab: "terminal", section: "Theme", sectionKey: "appearance.heading.theme" },
	{ tab: "terminal", section: "Terminal", sectionKey: "general.heading.terminal" },
	{ tab: "terminal", section: "Preview" },
	{ tab: "terminal", section: "Behavior" },
	{ tab: "terminal", section: "Theme", label: "Terminal Theme", labelKey: "appearance.label.terminalTheme" },
	{
		tab: "terminal",
		section: "Terminal",
		label: "Shell",
		labelKey: "general.label.shell",
		expert: true,
		configKey: "app.shell",
	},
	{ tab: "terminal", section: "Terminal", label: "Terminal Font", labelKey: "appearance.label.terminalFont" },
	{ tab: "terminal", section: "Terminal", label: "Default Font Size", labelKey: "appearance.label.defaultFontSize" },
	{
		tab: "terminal",
		section: "Terminal",
		label: "Font Weight",
		labelKey: "appearance.label.fontWeight",
		expert: true,
		configKey: "app.font_weight",
	},
	{ tab: "terminal", section: "Terminal", label: "Cursor Style", labelKey: "appearance.label.cursorStyle" },
	{ tab: "terminal", section: "Behavior", label: "Copy on select", labelKey: "general.toggle.copyOnSelect" },
	{
		tab: "terminal",
		section: "Behavior",
		label: "Allow OSC 52 clipboard writes",
		labelKey: "general.toggle.osc52Clipboard",
		expert: true,
		configKey: "app.osc52_clipboard",
	},
	{
		tab: "terminal",
		section: "Behavior",
		label: "Show block timestamps",
		labelKey: "general.toggle.showBlockTimestamps",
	},
	{
		tab: "terminal",
		section: "Behavior",
		label: "Block folding",
		labelKey: "general.toggle.blockFolding",
		expert: true,
		configKey: "app.block_folding_enabled",
	},
	{
		tab: "terminal",
		section: "Behavior",
		label: "Show scrollbar marks",
		labelKey: "general.toggle.showScrollbarMarks",
		expert: true,
		configKey: "app.show_scrollbar_marks",
	},
	{
		tab: "terminal",
		section: "Behavior",
		label: "Reflow scrollback on resize",
		labelKey: "general.toggle.scrollbackReflow",
		expert: true,
		configKey: "app.scrollback_reflow",
	},
	{ tab: "keyboard-shortcuts", section: "Keyboard Shortcuts", sectionKey: "settings.keyboardShortcuts" },
	{ tab: "keyboard-shortcuts", section: "Global Hotkey", platform: "desktop" },
	{ tab: "keyboard-shortcuts", section: "Shortcut groups" },
	{
		tab: "keyboard-shortcuts",
		section: "Global Hotkey",
		label: "Global Hotkey (Toggle Window)",
		labelKey: "settings.globalHotkey",
		platform: "desktop",
	},
	{
		tab: "keyboard-shortcuts",
		section: "Shortcut groups",
		label: "Plugin Commands",
		labelKey: "helpPanel.pluginCommands",
	},
	{ tab: "dictation", section: "Voice" },
	{ tab: "dictation", section: "Dictation", sectionKey: "dictation.heading.dictation" },
	{ tab: "dictation", section: "Auto-Corrections", sectionKey: "dictation.heading.corrections" },
	{ tab: "dictation", section: "Speech recognition", sectionKey: "dictation.heading.recognition" },
	{ tab: "dictation", section: "Speech assets" },
	{ tab: "dictation", section: "Spoken replies", sectionKey: "dictation.heading.spokenReplies" },
	{ tab: "dictation", section: "Hands-free conversation", sectionKey: "dictation.heading.handsFree" },
	{ tab: "dictation", section: "Dictation", label: "Enable Dictation", labelKey: "dictation.enableLabel" },
	{ tab: "dictation", section: "Dictation", label: "Hotkey", labelKey: "dictation.hotkeyLabel", platform: "desktop" },
	{
		tab: "dictation",
		section: "Dictation",
		label: "Long-press threshold",
		labelKey: "dictation.longPressLabel",
		expert: true,
		configKey: "dictation.long_press_ms",
		platform: "desktop",
	},
	{
		tab: "dictation",
		section: "Dictation",
		label: "Auto-send",
		labelKey: "dictation.autoSendLabel",
		expert: true,
		configKey: "dictation.auto_send",
	},
	{ tab: "dictation", section: "Auto-Corrections", label: "Add correction" },
	{
		tab: "dictation",
		section: "Speech recognition",
		label: "Input device",
		labelKey: "dictation.inputDeviceLabel",
		expert: true,
		configKey: "dictation.device",
		platform: "desktop",
	},
	{ tab: "dictation", section: "Speech recognition", label: "Language", labelKey: "dictation.languageLabel" },
	{
		tab: "dictation",
		section: "Speech recognition",
		label: "Level gate",
		labelKey: "dictation.rmsLabel",
		expert: true,
		configKey: "dictation.rms_threshold",
	},
	{
		tab: "dictation",
		section: "Speech recognition",
		label: "Speech confidence gate",
		labelKey: "dictation.noSpeechLabel",
		expert: true,
		configKey: "dictation.no_speech_threshold",
	},
	{ tab: "dictation", section: "Speech assets", label: "Whisper Model", labelKey: "dictation.modelLabel" },
	{ tab: "dictation", section: "Speech assets", label: "Voice tuning", labelKey: "dictation.tuningLabel" },
	{
		tab: "dictation",
		section: "Spoken replies",
		label: "Replies are spoken in",
		labelKey: "dictation.speechLanguageLabel",
	},
	{
		tab: "dictation",
		section: "Spoken replies",
		label: "Speech engine",
		labelKey: "dictation.speechEngineLabel",
		expert: true,
		configKey: "dictation.speech_engine",
	},
	{
		tab: "dictation",
		section: "Spoken replies",
		label: "Speech command",
		labelKey: "dictation.speechCommandLabel",
		expert: true,
		configKey: "dictation.speech_command",
	},
	{ tab: "dictation", section: "Spoken replies", label: "Voice", labelKey: "dictation.voiceLabel" },
	{
		tab: "dictation",
		section: "Spoken replies",
		label: "Voice volume",
		labelKey: "dictation.voiceVolumeLabel",
		expert: true,
		configKey: "dictation.speech_volume_db",
	},
	{
		tab: "dictation",
		section: "Spoken replies",
		label: "Levelling",
		labelKey: "dictation.levellingLabel",
		expert: true,
		configKey: "dictation.speech_levelling",
	},
	{ tab: "dictation", section: "Spoken replies", label: "Voices", labelKey: "dictation.voicesLabel" },
	{ tab: "dictation", section: "Hands-free conversation", label: "Terminal" },
	{
		tab: "dictation",
		section: "Hands-free conversation",
		label: "Activation phrase",
		labelKey: "dictation.activationPhraseLabel",
	},
	{
		tab: "dictation",
		section: "Hands-free conversation",
		label: "Hold-back before sending",
		labelKey: "dictation.holdBackLabel",
		expert: true,
		configKey: "dictation.hands_free_hold_back_ms",
	},
	{
		tab: "dictation",
		section: "Hands-free conversation",
		label: "Hands-free listening",
		labelKey: "dictation.handsFreeListeningLabel",
	},
	{
		tab: "dictation",
		section: "Hands-free conversation",
		label: "Spoken replies",
		labelKey: "dictation.spokenRepliesLabel",
	},
	{ tab: "dictation", section: "Hands-free conversation", label: "Earcons", labelKey: "dictation.earconsLabel" },
	{
		tab: "dictation",
		section: "Hands-free conversation",
		label: "Notify model when hands-free changes",
		labelKey: "dictation.notifyModelLabel",
		expert: true,
		configKey: "dictation.hands_free_notify_model",
	},
	{
		tab: "dictation",
		section: "Hands-free conversation",
		label: "Start notice",
		labelKey: "dictation.startNoticeLabel",
		expert: true,
		configKey: "dictation.hands_free_start_notice",
	},
	{ tab: "github", section: "Git & GitHub" },
	{ tab: "github", section: "GitHub Authentication" },
	{ tab: "github", section: "Pull Requests" },
	{ tab: "github", section: "Issues" },
	{ tab: "github", section: "Repository Defaults" },
	{ tab: "github", section: "Worktree Defaults" },
	{ tab: "github", section: "Additional GitHub Accounts" },
	{ tab: "github", section: "CircleCI" },
	{ tab: "github", section: "Repository Bindings" },
	{ tab: "github", section: "Pull Requests", label: "Auto-show PR popover" },
	{ tab: "github", section: "Pull Requests", label: "Hide Draft PRs" },
	{ tab: "github", section: "Pull Requests", label: "Hide Conflicting PRs" },
	{ tab: "github", section: "Pull Requests", label: "Hide CI Failing PRs" },
	{
		tab: "github",
		section: "Pull Requests",
		label: "Auto-Delete on PR Close",
		expert: true,
		configKey: "repo_defaults.auto_delete_on_pr_close",
	},
	{ tab: "github", section: "Issues", label: "Show issues" },
	{ tab: "github", section: "Issues", label: "Issue Filter" },
	{ tab: "github", section: "Repository Defaults", label: "Default Base Branch" },
	{
		tab: "github",
		section: "Repository Defaults",
		label: "Copy ignored files",
		expert: true,
		configKey: "repo_defaults.copy_ignored_files",
	},
	{
		tab: "github",
		section: "Repository Defaults",
		label: "Copy untracked files",
		expert: true,
		configKey: "repo_defaults.copy_untracked_files",
	},
	{ tab: "github", section: "Repository Defaults", label: "Default Setup Script" },
	{ tab: "github", section: "Repository Defaults", label: "Default Run Script" },
	{ tab: "github", section: "Repository Defaults", label: "Default Archive Script" },
	{
		tab: "github",
		section: "Worktree Defaults",
		label: "Storage Strategy",
		expert: true,
		configKey: "repo_defaults.worktree_storage",
	},
	{ tab: "github", section: "Worktree Defaults", label: "Prompt for branch name during creation" },
	{ tab: "github", section: "Worktree Defaults", label: "Delete local branch when removing worktree" },
	{
		tab: "github",
		section: "Worktree Defaults",
		label: "Auto-archive merged worktrees",
		expert: true,
		configKey: "repo_defaults.auto_archive_merged",
	},
	{
		tab: "github",
		section: "Worktree Defaults",
		label: "Orphan Worktree Cleanup",
		expert: true,
		configKey: "repo_defaults.orphan_cleanup",
	},
	{
		tab: "github",
		section: "Worktree Defaults",
		label: "Safe orphan cleanup countdown",
		expert: true,
		configKey: "repo_defaults.orphan_cleanup_countdown_seconds",
	},
	{ tab: "github", section: "Worktree Defaults", label: "PR Merge Strategy" },
	{
		tab: "github",
		section: "Worktree Defaults",
		label: "After Merge Behavior",
		expert: true,
		configKey: "repo_defaults.after_merge",
	},
	{
		tab: "github",
		section: "Worktree Defaults",
		label: "Auto-Fetch Interval",
		expert: true,
		configKey: "repo_defaults.auto_fetch_interval_minutes",
	},
	{ tab: "github", section: "Additional GitHub Accounts", label: "Add another github.com account" },
	{ tab: "github", section: "Additional GitHub Accounts", label: "Add Enterprise account" },
	{ tab: "mcp", section: "MCP" },
	{ tab: "mcp", section: "Server connections" },
	{ tab: "mcp", section: "TUIC Tools" },
	{ tab: "mcp", section: "Call audit", sectionKey: "mcpAudit.title" },
	{ tab: "mcp", section: "Upstream MCP Servers" },
	{ tab: "mcp", section: "Server connections", label: "Server Status", labelKey: "services.label.serverStatus" },
	{ tab: "mcp", section: "Server connections", label: "MCP Connection", labelKey: "services.label.mcpConnection" },
	{
		tab: "mcp",
		section: "TUIC Tools",
		label: "Collapse tools — Speakeasy MCP (reduces AI context ~98%)",
		expert: true,
		configKey: "app.collapse_tools",
	},
	{ tab: "mcp", section: "TUIC Tools", label: "Capabilities" },
	{ tab: "mcp", section: "Call audit", label: "Record MCP call outcomes", labelKey: "mcpAudit.toggle" },
	{ tab: "mcp", section: "Upstream MCP Servers", label: "Upstreams on" },
	{ tab: "mcp", section: "Upstream MCP Servers", label: "Authentication" },
	{ tab: "mcp", section: "Upstream MCP Servers", label: "Timeout (s):" },
	{ tab: "mcp", section: "Upstream MCP Servers", label: "URL" },
	{ tab: "mcp", section: "Upstream MCP Servers", label: "Bearer token" },
	{ tab: "mcp", section: "Upstream MCP Servers", label: "OAuth client ID" },
	{ tab: "mcp", section: "Upstream MCP Servers", label: "Client Secret" },
	{ tab: "mcp", section: "Upstream MCP Servers", label: "Scopes" },
	{ tab: "mcp", section: "Upstream MCP Servers", label: "Command" },
	{ tab: "mcp", section: "Upstream MCP Servers", label: "Args" },
	{ tab: "mcp", section: "Upstream MCP Servers", label: "Working directory" },
	{ tab: "remote-access", section: "Remote Access" },
	{ tab: "remote-access", section: "Connection status" },
	{ tab: "remote-access", section: "Tailscale HTTPS" },
	{ tab: "remote-access", section: "Danger zone" },
	{ tab: "remote-access", section: "Cloud Relay", sectionKey: "services.heading.cloudRelay" },
	{
		tab: "remote-access",
		section: "Connection status",
		label: "Enable remote access",
		labelKey: "services.toggle.enableRemoteAccess",
	},
	{
		tab: "remote-access",
		section: "Connection status",
		label: "Port",
		labelKey: "services.label.port",
		expert: true,
		configKey: "app.services.server.port",
	},
	{ tab: "remote-access", section: "Connection status", label: "Username", labelKey: "services.label.username" },
	{ tab: "remote-access", section: "Connection status", label: "Password", labelKey: "services.label.password" },
	{
		tab: "remote-access",
		section: "Connection status",
		label: "Network Interface",
		labelKey: "services.label.networkInterface",
	},
	{
		tab: "remote-access",
		section: "Connection status",
		label: "Session Token Duration",
		labelKey: "services.label.tokenDuration",
		expert: true,
		configKey: "app.services.auth.session_token_duration_secs",
	},
	{
		tab: "remote-access",
		section: "Connection status",
		label: "Enable IPv6 (dual-stack)",
		labelKey: "services.toggle.enableIpv6",
		expert: true,
		configKey: "app.services.server.ipv6_enabled",
	},
	{ tab: "remote-access", section: "Tailscale HTTPS", label: "Status", labelKey: "services.label.tailscaleStatus" },
	{
		tab: "remote-access",
		section: "Cloud Relay",
		label: "Enable cloud relay",
		labelKey: "services.toggle.enableRelay",
	},
	{ tab: "remote-access", section: "Cloud Relay", label: "Relay Server URL", labelKey: "services.label.relayUrl" },
	{ tab: "remote-access", section: "Cloud Relay", label: "Bearer Token", labelKey: "services.label.relayToken" },
	{ tab: "remote-access", section: "Cloud Relay", label: "Session ID", labelKey: "services.label.relaySessionId" },
	{ tab: "remote-machines", section: "Remote Machines", sectionKey: "settings.remoteMachines" },
	{ tab: "remote-machines", section: "Connections and SSH discovery" },
	{ tab: "remote-machines", section: "Name and transport" },
	{ tab: "remote-machines", section: "Deployment and lifetime" },
	{ tab: "remote-machines", section: "Your machines" },
	{ tab: "remote-machines", section: "Name and transport", label: "Name" },
	{ tab: "remote-machines", section: "Name and transport", label: "Transport" },
	{ tab: "remote-machines", section: "Name and transport", label: "Host" },
	{ tab: "remote-machines", section: "Name and transport", label: "Port" },
	{ tab: "remote-machines", section: "Name and transport", label: "User" },
	{ tab: "remote-machines", section: "Name and transport", label: "Identity file" },
	{ tab: "remote-machines", section: "Name and transport", label: "Remote daemon port" },
	{ tab: "remote-machines", section: "Name and transport", label: "URL" },
	{ tab: "remote-machines", section: "Name and transport", label: "Pairing bundle" },
	{ tab: "remote-machines", section: "Deployment and lifetime", label: "Deployment" },
	{ tab: "remote-machines", section: "Deployment and lifetime", label: "Keep ephemeral daemon alive (minutes)" },
	{ tab: "remote-machines", section: "Deployment and lifetime", label: "Auto-update remote daemons" },
	{ tab: "remote-machines", section: "Deployment and lifetime", label: "Auth username" },
	{ tab: "remote-machines", section: "Deployment and lifetime", label: "Auth password" },
	{ tab: "plugins", section: "Manage plugins" },
	{ tab: "plugins", section: "Installed plugins" },
	{ tab: "plugins", section: "Community registry" },
	{ tab: "plugins", section: "Installed plugins", label: "Check for plugin updates" },
	{ tab: "smart-prompts", section: "Delete prompt" },
	{ tab: "smart-prompts", section: "Smart Prompts" },
	{ tab: "smart-prompts", section: "Background agent" },
	{ tab: "smart-prompts", section: "Prompt library" },
	{ tab: "smart-prompts", section: "Background agent", label: "Headless Agent" },
	{ tab: "agents", section: "Terminal agents" },
	{ tab: "agents", section: "Agent behavior" },
	{ tab: "agents", section: "ego permissions" },
	{ tab: "agents", section: "Idle agent hibernation", sectionKey: "settings.agents.hibernateHeading" },
	{ tab: "agents", section: "Launch modules" },
	{ tab: "agents", section: "Terminal agents", label: "Configure agents on" },
	{ tab: "agents", section: "Agent behavior", label: "Show agent intent as tab title" },
	{ tab: "agents", section: "Agent behavior", label: "Show suggested follow-up actions" },
	{
		tab: "agents",
		section: "Agent behavior",
		label: "Collect project progress",
		expert: true,
		configKey: "app.progress_tracking",
	},
	{ tab: "agents", section: "Agent behavior", label: "Launch instructions" },
	{ tab: "agents", section: "ego permissions", label: "Permissions" },
	{ tab: "agents", section: "ego permissions", label: "Filesystem sandbox" },
	{
		tab: "agents",
		section: "Idle agent hibernation",
		label: "Close abandoned shells after (hours)",
		labelKey: "settings.agents.abandonedHours",
		expert: true,
		configKey: "agents.abandoned_shell_hours",
	},
	{
		tab: "agents",
		section: "Idle agent hibernation",
		label: "Hibernate idle agents after",
		labelKey: "settings.agents.hibernateMinutes",
		expert: true,
		configKey: "agents.hibernate_idle_minutes",
	},
	{
		tab: "agents",
		section: "Idle agent hibernation",
		label: "Hibernate scope",
		labelKey: "settings.agents.hibernateScope",
		expert: true,
		configKey: "agents.hibernate_scope",
	},
	{ tab: "agents", section: "Launch modules", label: "Turn all launch instructions off" },
	{ tab: "agents", section: "Launch modules", label: "Markers" },
	{ tab: "agents", section: "Launch modules", label: "Working with the user" },
	{ tab: "agents", section: "Launch modules", label: "Waiting and long work" },
	{ tab: "agents", section: "Launch modules", label: "Git and worktrees" },
	{ tab: "agents", section: "Launch modules", label: "Process safety" },
	{ tab: "agents", section: "Launch modules", label: "Children (spawned children only)" },
	{ tab: "ai-chat", section: "Workspace access" },
	{ tab: "ai-chat", section: "Network and sandbox" },
	{ tab: "ai-chat", section: "Advanced and diagnostics" },
	{ tab: "ai-chat", section: "AI Chat" },
	{ tab: "ai-chat", section: "Agent and model" },
	{ tab: "ai-chat", section: "Workspace access", label: "Workspace" },
	{ tab: "ai-chat", section: "Workspace access", label: "Root directory", labelKey: "perimeter.label.rootDir" },
	{ tab: "ai-chat", section: "Workspace access", label: "Root access", labelKey: "perimeter.label.rootAccess" },
	{ tab: "ai-chat", section: "Workspace access", label: "Read allowlist", labelKey: "perimeter.label.readAllowlist" },
	{
		tab: "ai-chat",
		section: "Workspace access",
		label: "Extra writable directories",
		labelKey: "perimeter.label.writableDirs",
	},
	{ tab: "ai-chat", section: "Network and sandbox", label: "Network" },
	{ tab: "ai-chat", section: "Agent and model", label: "Default model", labelKey: "providers.label.defaultModel" },
	{ tab: "ai-chat", section: "Agent and model", label: "Provider catalogue" },
];

/** Section heading as rendered, i18n applied. */
export function entrySection(entry: SettingsSearchEntry): string {
	return entry.sectionKey ? t(entry.sectionKey, entry.section) : entry.section;
}

/** Setting label as rendered, i18n applied; undefined for a section entry. */
export function entryLabel(entry: SettingsSearchEntry): string | undefined {
	if (entry.label === undefined) return undefined;
	return entry.labelKey ? t(entry.labelKey, entry.label) : entry.label;
}

/** Every word of `query` must appear somewhere in the entry's rendered text. */
function matches(entry: SettingsSearchEntry, terms: string[]): boolean {
	const haystack = `${entrySection(entry)} ${entryLabel(entry) ?? ""}`.toLowerCase();
	return terms.every((term) => haystack.includes(term));
}

/**
 * Entries matching `query`, restricted to what the user can actually open.
 *
 * `availableTabs` is the live nav key set: AI Chat is absent while its
 * experimental flag is off, so its settings must not be offered — selecting one
 * would open a tab that does not exist. `client` drops the controls the other
 * client renders instead: the global hotkey exists only on the desktop.
 */
export function searchSettings(
	query: string,
	availableTabs: ReadonlySet<string>,
	client: "desktop" | "browser",
	pluginEntries: readonly SettingsSearchEntry[] = [],
): SettingsSearchEntry[] {
	const terms = query.toLowerCase().split(/\s+/).filter(Boolean);
	if (terms.length === 0) return [];
	return [...SETTINGS_SEARCH_INDEX, ...pluginEntries].filter(
		(entry) => availableTabs.has(entry.tab) && (!entry.platform || entry.platform === client) && matches(entry, terms),
	);
}
