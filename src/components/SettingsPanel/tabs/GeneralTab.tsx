import { type Component, createSignal, For, onMount, Show } from "solid-js";
import { t } from "../../../i18n";
import { invoke } from "../../../invoke";
import { appLogger } from "../../../stores/appLogger";
import type { CustomLauncher, IdeType, UpdateChannel } from "../../../stores/settings";
import { IDE_NAMES, settingsStore } from "../../../stores/settings";
import { updaterStore } from "../../../stores/updater";
import { isTauri } from "../../../transport";
import { SettingInput, SettingSelect, SettingSlider, SettingToggle } from "../SettingFields";
import s from "../Settings.module.css";

interface CliStatus {
	installed: boolean;
	path: string | null;
	version_match: boolean;
	auto_updatable: boolean;
	prompt_dismissed: boolean;
}

interface MdkbStatus {
	available: boolean;
	connected: boolean;
	binaryPath: string | null;
	version: string | null;
}

export const GeneralTab: Component = () => {
	const [cliStatus, setCliStatus] = createSignal<CliStatus | null>(null);
	const [cliInstalling, setCliInstalling] = createSignal(false);
	const [mdkbStatus, setMdkbStatus] = createSignal<MdkbStatus | null>(null);
	const [mdkbInstalling, setMdkbInstalling] = createSignal(false);
	const [mdkbError, setMdkbError] = createSignal<string | null>(null);

	const refreshCliStatus = async () => {
		if (!isTauri()) return;
		try {
			const status = await invoke<CliStatus>("get_cli_status");
			setCliStatus(status);
		} catch (err) {
			appLogger.error("app", "Failed to get CLI status", err);
		}
	};

	const refreshMdkbStatus = async () => {
		if (!isTauri()) return;
		try {
			const status = await invoke<MdkbStatus>("mdkb_status");
			setMdkbStatus(status);
		} catch (err) {
			appLogger.error("app", "Failed to get mdkb status", err);
		}
	};

	onMount(() => {
		refreshCliStatus();
		refreshMdkbStatus();
	});

	const handleInstallCli = async () => {
		setCliInstalling(true);
		try {
			await invoke<string>("install_cli");
			await refreshCliStatus();
		} catch (err) {
			appLogger.error("app", "Failed to install CLI", err);
		} finally {
			setCliInstalling(false);
		}
	};

	const handleUninstallCli = async () => {
		try {
			await invoke("uninstall_cli");
			await refreshCliStatus();
		} catch (err) {
			appLogger.error("app", "Failed to uninstall CLI", err);
		}
	};

	const handleInstallMdkb = async () => {
		setMdkbInstalling(true);
		try {
			await invoke<string>("install_mdkb");
			await refreshMdkbStatus();
		} catch (err) {
			appLogger.error("app", "Failed to install mdkb", err);
		} finally {
			setMdkbInstalling(false);
		}
	};

	const handleUninstallMdkb = async () => {
		setMdkbError(null);
		try {
			await invoke("uninstall_mdkb");
			await refreshMdkbStatus();
		} catch (err) {
			const msg = typeof err === "string" ? err : String(err);
			setMdkbError(msg);
			appLogger.error("app", "Failed to uninstall mdkb", err);
		}
	};

	const ideOptions = Object.entries(IDE_NAMES).map(([value, label]) => ({ value, label }));

	// --- Custom launchers (GH #71) ---
	const launchers = (): CustomLauncher[] => settingsStore.state.customLaunchers;
	const updateLauncher = (id: string, patch: Partial<CustomLauncher>) =>
		settingsStore.setCustomLaunchers(launchers().map((l) => (l.id === id ? { ...l, ...patch } : l)));
	const addLauncher = () =>
		settingsStore.setCustomLaunchers([
			...launchers(),
			{ id: crypto.randomUUID(), name: "New tool", executable: "", args: [], enabled: true },
		]);
	const removeLauncher = (id: string) => settingsStore.setCustomLaunchers(launchers().filter((l) => l.id !== id));

	const updateChannelOptions = [
		{ value: "stable", label: t("general.channel.stable", "Stable") },
		{ value: "nightly", label: t("general.channel.nightly", "Nightly") },
	];

	return (
		<div class={s.section}>
			<h3>{t("general.heading.general", "General")}</h3>

			<SettingInput
				label={t("general.label.shell", "Shell")}
				value={settingsStore.state.shell ?? ""}
				onInput={(v) => settingsStore.setShell(v)}
				placeholder={t("general.placeholder.shell", "Default shell")}
				hint={t("general.hint.shell", "Shell used in terminals (leave blank for system default)")}
			/>

			<Show when={isTauri() && cliStatus()}>
				<h3>
					{t("general.heading.cli", "TUIC CLI")}
					<span class={s.infoBadge}>
						?
						<span class={s.infoBadgeTip}>
							{t(
								"general.hint.cliInfo",
								"The TUIC CLI lets you control TUICommander from any terminal or script. Open files and URLs as tabs, manage PTY sessions (create, list, send input, read output), and query repository status. Useful for scripting automation.",
							)}
						</span>
					</span>
				</h3>

				<div class={s.group}>
					<Show
						when={cliStatus()!.installed}
						fallback={
							<>
								<p class={s.hint}>
									{t(
										"general.hint.cliNotInstalled",
										"Install the TUIC CLI to control TUICommander from any terminal or script. Open files and URLs as tabs, manage PTY sessions, and query repository status. Useful for scripting automation.",
									)}
								</p>
								<button
									class={s.testBtn}
									onClick={handleInstallCli}
									disabled={cliInstalling()}
									style={{ "margin-top": "8px" }}
								>
									{cliInstalling()
										? t("general.btn.installing", "Installing...")
										: t("general.btn.installCli", "Install TUIC CLI")}
								</button>
							</>
						}
					>
						<p class={s.hint} style={{ color: "var(--success)" }}>
							{t("general.hint.cliInstalled", "Installed at {path}", {
								path: cliStatus()!.path ?? "/usr/local/bin/tuic",
							})}
							{!cliStatus()!.version_match && (
								<span style={{ color: "var(--warning, #e5c07b)", "margin-left": "8px" }}>
									{cliStatus()!.auto_updatable
										? t("general.hint.cliOutdated", "(update pending — restart to apply)")
										: t("general.hint.cliUpdateAvailable", "(update available)")}
								</span>
							)}
						</p>
						<div style={{ display: "flex", gap: "8px", "margin-top": "8px" }}>
							<Show when={!cliStatus()!.version_match}>
								<button class={s.testBtn} onClick={handleInstallCli} disabled={cliInstalling()}>
									{cliInstalling() ? t("general.btn.updating", "Updating...") : t("general.btn.updateCli", "Update")}
								</button>
							</Show>
							<button class={s.testBtn} onClick={handleUninstallCli}>
								{t("general.btn.uninstallCli", "Uninstall")}
							</button>
						</div>
					</Show>
				</div>
			</Show>

			<Show when={isTauri()}>
				<h3>
					{t("general.heading.codeIntelligence", "Code Intelligence")}
					<span class={s.infoBadge}>
						?
						<span class={s.infoBadgeTip}>
							{t(
								"general.hint.codeIntelligenceInfo",
								"Integrates with MDKB to provide code navigation features in the editor: Cmd+Click go-to-definition, Shift+F12 find references, and symbol outline. Also serves as a persistent memory manager for AI agents, fully integrated with TUICommander. MDKB indexes your repositories and exposes a local daemon for fast lookups.",
							)}
						</span>
					</span>
				</h3>

				<div class={s.group}>
					<Show
						when={mdkbStatus()?.available}
						fallback={
							<>
								<p class={s.hint}>
									{t(
										"general.hint.mdkbNotInstalled",
										"Install MDKB to enable outline, go-to-definition, and find references in the code editor.",
									)}
								</p>
								<button
									class={s.testBtn}
									onClick={handleInstallMdkb}
									disabled={mdkbInstalling()}
									style={{ "margin-top": "8px" }}
								>
									{mdkbInstalling()
										? t("general.btn.installing", "Installing...")
										: t("general.btn.installMdkb", "Install MDKB")}
								</button>
							</>
						}
					>
						<p class={s.hint} style={{ color: "var(--success)" }}>
							{t("general.hint.mdkbInstalled", "Installed at {path}", {
								path: mdkbStatus()!.binaryPath ?? "unknown",
							})}
							{mdkbStatus()!.version && (
								<span style={{ "margin-left": "8px", color: "var(--fg-muted)" }}>v{mdkbStatus()!.version}</span>
							)}
						</p>
						<button class={s.testBtn} onClick={handleUninstallMdkb} style={{ "margin-top": "8px" }}>
							{t("general.btn.uninstallMdkb", "Uninstall")}
						</button>
						<Show when={mdkbError()}>
							<p class={s.hint} style={{ color: "var(--error)" }}>
								{mdkbError()}
							</p>
						</Show>
					</Show>
				</div>
			</Show>

			<h3>{t("general.heading.confirmations", "Confirmations")}</h3>

			<SettingToggle
				checked={settingsStore.state.confirmBeforeQuit}
				onChange={(v) => settingsStore.setConfirmBeforeQuit(v)}
				label={t("general.toggle.confirmBeforeQuit", "Confirm before quitting")}
				hint={t("general.hint.confirmBeforeQuit", "Show a confirmation dialog when closing the app")}
			/>

			<SettingToggle
				checked={settingsStore.state.confirmBeforeClosingTab}
				onChange={(v) => settingsStore.setConfirmBeforeClosingTab(v)}
				label={t("general.toggle.confirmBeforeClosingTab", "Confirm before closing a tab")}
				hint={t("general.hint.confirmBeforeClosingTab", "Show a confirmation dialog when closing a terminal tab")}
			/>

			<h3>{t("general.heading.terminal", "Terminal")}</h3>

			<SettingToggle
				checked={settingsStore.state.copyOnSelect}
				onChange={(v) => settingsStore.setCopyOnSelect(v)}
				label={t("general.toggle.copyOnSelect", "Copy on select")}
				hint={t("general.hint.copyOnSelect", "Automatically copy selected text to clipboard")}
			/>

			<SettingToggle
				checked={settingsStore.state.showLastPrompt}
				onChange={(v) => settingsStore.setShowLastPrompt(v)}
				label="Show last prompt bar"
				hint="Display a collapsible overlay at the top of the terminal showing the last prompt sent to an agent"
			/>

			<h3>{t("general.heading.powerManagement", "Power Management")}</h3>

			<SettingToggle
				checked={settingsStore.state.preventSleepWhenBusy}
				onChange={(v) => settingsStore.setPreventSleepWhenBusy(v)}
				label={t("general.toggle.preventSleepWhenBusy", "Prevent sleep when busy")}
				hint={t("general.hint.preventSleepWhenBusy", "Keep the system awake while scripts are running")}
			/>

			<SettingSlider
				label="Auto-Standby Timeout"
				value={settingsStore.state.standbyTimeoutMinutes}
				onChange={(v) => settingsStore.setStandbyTimeoutMinutes(v)}
				min={0}
				max={60}
				step={1}
				formatValue={(v) => (v === 0 ? "Off" : `${v} min`)}
				hint="Pause idle background sessions after this duration to save resources. 0 = disabled."
			/>

			<SettingSelect
				label="Content Indexing"
				value={settingsStore.state.indexStrategy}
				onChange={(v) =>
					settingsStore.setIndexStrategy(v as "active_only" | "active_and_switch" | "all_sequential" | "disabled")
				}
				options={[
					{ value: "disabled", label: "Disabled" },
					{ value: "active_only", label: "Active repo only" },
					{ value: "active_and_switch", label: "Active + on switch" },
					{ value: "all_sequential", label: "All repos at boot" },
				]}
				hint="When to build search indexes. Set to Disabled to turn off background indexing entirely."
			/>

			<h3>{t("general.heading.updates", "Updates")}</h3>

			<SettingToggle
				checked={settingsStore.state.autoUpdateEnabled}
				onChange={(v) => settingsStore.setAutoUpdateEnabled(v)}
				label={t("general.toggle.autoUpdateEnabled", "Automatically check for updates")}
				hint={t("general.hint.autoUpdateEnabled", "Download and install updates in the background")}
			/>

			<SettingSelect
				label={t("general.label.updateChannel", "Update Channel")}
				value={settingsStore.state.updateChannel}
				onChange={(v) => settingsStore.setUpdateChannel(v as UpdateChannel)}
				options={updateChannelOptions}
				hint={
					settingsStore.state.updateChannel !== "stable"
						? t("general.hint.updateChannelWarning", "Nightly builds may be unstable")
						: t("general.hint.updateChannel", "Choose which release channel to receive updates from")
				}
				hintStyle={settingsStore.state.updateChannel !== "stable" ? { color: "var(--warning, #e5c07b)" } : undefined}
			/>

			<div class={s.group}>
				<button
					class={s.testBtn}
					onClick={() => {
						updaterStore.checkForUpdate().catch((err: unknown) => appLogger.debug("app", "Update check failed", err));
					}}
					disabled={updaterStore.state.checking || updaterStore.state.downloading}
				>
					{updaterStore.state.checking
						? t("general.btn.checking", "Checking...")
						: t("general.btn.checkNow", "Check Now")}
				</button>
				<Show when={updaterStore.state.available && updaterStore.state.version}>
					<p class={s.hint} style={{ color: "var(--success)" }}>
						{t("general.hint.updateAvailable", "Version {version} is available!", {
							version: updaterStore.state.version ?? "",
						})}
					</p>
				</Show>
				<Show
					when={
						!updaterStore.state.available &&
						!updaterStore.state.checking &&
						!updaterStore.state.error &&
						!updaterStore.state.noRelease
					}
				>
					<p class={s.hint}>{t("general.hint.latestVersion", "You are on the latest version")}</p>
				</Show>
				<Show when={updaterStore.state.noRelease}>
					<p class={s.hint} style={{ color: "var(--fg-muted)" }}>
						{t("general.hint.noRelease", "No {channel} releases published yet", {
							channel: settingsStore.state.updateChannel,
						})}
					</p>
				</Show>
				<Show when={updaterStore.state.error}>
					<p class={s.hint} style={{ color: "var(--accent-red, #f44747)" }}>
						{updaterStore.state.error}
					</p>
				</Show>
			</div>

			<SettingSelect
				label={t("general.label.defaultIde", "Default IDE")}
				value={settingsStore.state.ide}
				onChange={(v) => settingsStore.setIde(v as IdeType)}
				options={ideOptions}
				hint={t("general.hint.defaultIde", "IDE used to open repositories")}
			/>

			<Show when={isTauri()}>
				<h3>{t("general.heading.customLaunchers", "Custom Launchers")}</h3>
				<div class={s.group}>
					<p class={s.hint}>
						{t(
							"general.hint.customLaunchers",
							'Define your own tools for the "Open in" menu. Each argument may use {path}, {file}, {line}, {column} placeholders. One argument per line.',
						)}
					</p>
					<For each={launchers()}>
						{(launcher) => (
							<div
								class={s.group}
								style={{
									border: "1px solid var(--border)",
									"border-radius": "var(--radius-md)",
									padding: "8px",
									"margin-bottom": "8px",
								}}
							>
								<div style={{ display: "flex", gap: "8px", "align-items": "center", "margin-bottom": "6px" }}>
									<input
										type="checkbox"
										checked={launcher.enabled}
										title={t("general.label.launcherEnabled", "Enabled")}
										onChange={(e) => updateLauncher(launcher.id, { enabled: e.currentTarget.checked })}
									/>
									<input
										type="text"
										value={launcher.name}
										placeholder={t("general.placeholder.launcherName", "Name")}
										onInput={(e) => updateLauncher(launcher.id, { name: e.currentTarget.value })}
										style={{ flex: "1" }}
									/>
									<button class={s.testBtn} onClick={() => removeLauncher(launcher.id)}>
										{t("general.btn.remove", "Remove")}
									</button>
								</div>
								<input
									type="text"
									value={launcher.executable}
									placeholder={t("general.placeholder.launcherExec", "Executable (e.g. code, or /usr/local/bin/code)")}
									onInput={(e) => updateLauncher(launcher.id, { executable: e.currentTarget.value })}
									style={{ width: "100%", "margin-bottom": "6px" }}
								/>
								<textarea
									value={launcher.args.join("\n")}
									placeholder={"--goto\n{file}:{line}:{column}"}
									rows={3}
									onInput={(e) => updateLauncher(launcher.id, { args: e.currentTarget.value.split("\n") })}
									style={{ width: "100%", "font-family": "var(--font-mono)", "font-size": "var(--font-sm)" }}
								/>
							</div>
						)}
					</For>
					<button class={s.testBtn} onClick={addLauncher}>
						{t("general.btn.addLauncher", "Add launcher")}
					</button>
				</div>
			</Show>

			<h3>{t("general.heading.experimental", "Experimental Features")}</h3>

			<div class={s.group}>
				<p class={s.warning}>
					{t("general.hint.experimentalWarning", "These features are under active development and may be unstable.")}
				</p>
				<div class={s.toggle}>
					<input
						type="checkbox"
						checked={settingsStore.state.experimentalFeaturesEnabled}
						onChange={(e) => settingsStore.setExperimentalFeaturesEnabled(e.currentTarget.checked)}
					/>
					<span>{t("general.toggle.experimentalFeatures", "Enable experimental features")}</span>
				</div>
				<p class={s.hint}>
					{t(
						"general.hint.experimentalFeatures",
						"Opt in to features under active development. Individual options appear below when enabled.",
					)}
				</p>
			</div>

			<Show when={settingsStore.state.experimentalFeaturesEnabled}>
				<SettingToggle
					checked={settingsStore.state.aiChatEnabled}
					onChange={(v) => settingsStore.setAiChatEnabled(v)}
					label={t("general.toggle.aiChat", "AI Chat")}
					hint={t("general.hint.aiChat", "Enable the AI Chat panel, keyboard shortcut, and command palette entry.")}
				/>

				<SettingToggle
					checked={settingsStore.state.aiTriageEnabled}
					onChange={(v) => settingsStore.setAiTriageEnabled(v)}
					label={t("general.toggle.aiTriage", "AI Triage")}
					hint={t(
						"general.hint.aiTriage",
						"Enable AI-powered diff triage to classify changed files by relevance and risk.",
					)}
				/>

				<SettingToggle
					checked={settingsStore.state.aiWatchersEnabled}
					onChange={(v) => settingsStore.setAiWatchersEnabled(v)}
					label={t("general.toggle.aiWatchers", "AI Watchers")}
					hint={t(
						"general.hint.aiWatchers",
						"Enable terminal watchers that trigger AI actions on shell events (idle, busy, errors).",
					)}
				/>
			</Show>
		</div>
	);
};
