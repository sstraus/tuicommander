import { type Component, createEffect, createSignal, For, onCleanup, Show } from "solid-js";
import { createStore, reconcile } from "solid-js/store";
import { t } from "../../../../i18n";
import { settingsRefresh } from "../../../../services/settingsRefresh";
import { appLogger } from "../../../../stores/appLogger";
import { rpc } from "../../../../transport";
import { writeClipboard } from "../../../../utils/clipboard";
import { Button } from "../../../ui/Button";
import { StatusBadge } from "../../../ui/StatusBadge";
import { Switch } from "../../../ui/Switch";
import { ContextualHelp } from "../../ContextualHelp";
import { ExpertSetting } from "../../ExpertSetting";
import s from "../../Settings.module.css";
import { SectionCard, SettingPageHeader, SettingRow } from "../../SettingsPrimitives";
import { McpAuditPanel } from "./McpAuditPanel";
import layout from "./ServiceRestyle.module.css";
import { type AppConfig, type McpCapability, saveConfigField, useMcpStatusPoll } from "./servicesShared";
import { UpstreamMcpPanel } from "./UpstreamMcpPanel";

/**
 * Local MCP page: HTTP/MCP server status, the manual bridge configuration
 * snippet, and capability controls. Upstream (remote) MCP
 * servers retain their own editor, mounted once inside Server connections.
 */
export const LocalMcpPanel: Component = () => {
	const { status } = useMcpStatusPoll();
	const [capabilities, setCapabilities] = createStore<McpCapability[]>([]);
	// Poll snapshots contain fresh objects; registry names own the row identity.
	createEffect(() => setCapabilities(reconcile(status()?.native_capabilities ?? [], { key: "name" })));
	const [deniedCapabilities, setDeniedCapabilities] = createSignal<string[]>([]);
	// The toggles wait for the saved list: the `[]` placeholder would show every
	// tool as enabled until `load_config` answers.
	const [nativeToolsLoaded, setNativeToolsLoaded] = createSignal(false);
	const [migrationUnknownTools, setMigrationUnknownTools] = createSignal<string[]>([]);
	const [settingsEnabled, setSettingsEnabled] = createSignal(false);
	const [collapseTools, setCollapseTools] = createSignal<boolean>(false);
	const [callAuditEnabled, setCallAuditEnabled] = createSignal(false);
	const [bridgeInfo, setBridgeInfo] = createSignal<{ bridge_path: string; config_snippet: string } | null>(null);
	const [bridgeInfoOpen, setBridgeInfoOpen] = createSignal(false);
	const [snippetCopied, setSnippetCopied] = createSignal(false);

	onCleanup(
		settingsRefresh.register({
			section: "app",
			read: () =>
				rpc<AppConfig>("load_config").catch((error) => {
					appLogger.warn("config", "Failed to load native tool config, using defaults", error);
					return undefined;
				}),
			apply: (config) => {
				if (!config) return;
				setDeniedCapabilities(config.mcp_capabilities.denied);
				setMigrationUnknownTools(config.mcp_capabilities.migration_unknown_tools);
				setSettingsEnabled(config.settings_mcp_enabled ?? false);
				setCollapseTools(config.collapse_tools ?? false);
				setCallAuditEnabled(config.mcp_call_audit_enabled ?? false);
				setNativeToolsLoaded(true);
			},
		}),
	);

	const capabilityRow = (capability: McpCapability) => {
		const disabled = () =>
			deniedCapabilities().includes(capability.name) || (capability.name === "settings" && !settingsEnabled());
		return (
			<SettingRow label={capability.title} hint={capability.description}>
				{(id, descriptionId) => (
					<Switch
						role="checkbox"
						data-setting-id="app.mcp_capabilities.denied"
						id={id}
						aria-describedby={descriptionId()}
						aria-label={capability.title}
						checked={!disabled()}
						onChange={(event) => {
							const enabled = event.currentTarget.checked;
							const updated = enabled
								? deniedCapabilities().filter((name) => name !== capability.name)
								: [...deniedCapabilities(), capability.name];
							setDeniedCapabilities(updated);
							saveConfigField((config) => {
								config.mcp_capabilities.denied = updated;
								if (capability.name === "settings") {
									config.settings_mcp_enabled = enabled;
									setSettingsEnabled(enabled);
								}
							});
						}}
					/>
				)}
			</SettingRow>
		);
	};

	return (
		<div class={`${s.section} ${layout.page}`}>
			<SettingPageHeader title="MCP" scope="This machine" breadcrumb="Integrations / MCP" />
			<SectionCard title="Server connections">
				<SettingRow label={t("services.label.serverStatus", "Server Status")}>
					{() => (
						<>
							<StatusBadge
								label={
									status()?.running
										? t("services.status.running", "Running")
										: t("services.status.starting", "Starting...")
								}
								variant={status()?.running ? "success" : undefined}
							/>
						</>
					)}
				</SettingRow>

				<SettingRow label={t("services.label.mcpConnection", "MCP Connection")}>
					{() => <StatusBadge label="tuic-bridge" />}
				</SettingRow>

				<UpstreamMcpPanel />
				{/* ── TUIC Tools ── */}
			</SectionCard>
			<SectionCard title="TUIC Tools">
				<div class={s.group}>
					<Button
						variant="ghost"
						data-non-setting="Show the bridge integration instructions; not a preference"
						class={s.mcpDisclosure}
						onClick={() => {
							const opening = !bridgeInfoOpen();
							setBridgeInfoOpen(opening);
							if (opening && !bridgeInfo()) {
								rpc<{ bridge_path: string; config_snippet: string }>("get_mcp_bridge_info")
									.then(setBridgeInfo)
									.catch((e) => appLogger.warn("config", "Failed to fetch bridge info", e));
							}
						}}
					>
						<span class={s.mcpDisclosureArrow}>{bridgeInfoOpen() ? "▼" : "▶"}</span>
						Manual MCP configuration
					</Button>
					<Show when={bridgeInfoOpen() && bridgeInfo()}>
						<div class={s.mcpDisclosureBody}>
							<p class={s.hint} style={{ margin: "0 0 4px" }}>
								Bridge path: <code class={s.mcpCode}>{bridgeInfo()!.bridge_path}</code>
							</p>
							<p class={s.hint} style={{ margin: "0 0 6px" }}>
								Add this to your MCP client config (e.g. <code class={s.mcpCode}>~/.claude.json</code> under{" "}
								<code class={s.mcpCode}>mcpServers</code>):
							</p>
							<div class={s.mcpSnippetWrap}>
								<pre class={s.mcpSnippetPre}>{bridgeInfo()!.config_snippet}</pre>
								<Button
									data-non-setting="Copy the existing MCP bridge integration snippet"
									class={s.mcpSnippetCopy}
									onClick={() => {
										writeClipboard(bridgeInfo()!.config_snippet)
											.then(() => {
												setSnippetCopied(true);
												setTimeout(() => setSnippetCopied(false), 2000);
											})
											.catch((err) => {
												appLogger.warn("settings", "Clipboard write failed", { error: String(err) });
											});
									}}
								>
									{snippetCopied() ? "Copied" : "Copy"}
								</Button>
							</div>
						</div>
					</Show>
				</div>

				<ExpertSetting configKey="app.collapse_tools" value={collapseTools()}>
					<SettingRow label="Collapse tools — Speakeasy MCP (reduces AI context ~98%)">
						{(id, descriptionId) => (
							<Switch
								role="checkbox"
								data-setting-id="app.collapse_tools"
								id={id}
								aria-describedby={descriptionId()}
								checked={collapseTools()}
								onChange={(e) => {
									const enabled = e.currentTarget.checked;
									setCollapseTools(enabled);
									saveConfigField((c) => {
										c.collapse_tools = enabled;
									});
								}}
							/>
						)}
					</SettingRow>
				</ExpertSetting>
				<Show when={nativeToolsLoaded()}>
					<div class={s.group}>
						<label>
							Capabilities <ContextualHelp label="Capabilities" />
						</label>
					</div>
					<Show when={migrationUnknownTools().length}>
						<p role="status" class={s.hint}>
							Unknown legacy MCP denials need review: {migrationUnknownTools().join(", ")}. These names were retained,
							not discarded.
						</p>
					</Show>
					<For
						each={[
							{ name: "always", title: "Always listed" },
							{ name: "rare", title: "Rarely needed" },
							{ name: "conditional", title: "Conditional" },
						]}
					>
						{(section) => (
							<section class={layout.nativeToolGroup} aria-label={section.title}>
								<h4>{section.title}</h4>
								<For
									each={capabilities.filter(
										(capability) => capability.section === section.name && capability.parent === null,
									)}
								>
									{(parent) => (
										<div role="group" aria-label={parent.title}>
											{capabilityRow(parent)}
											<Show when={parent.availability}>
												<p class={s.hint} role="status">
													{parent.availability}
												</p>
											</Show>
											<div class={layout.nativeToolChildren}>
												<For
													each={capabilities.filter(
														(capability) => capability.domain === parent.domain && capability.name !== parent.name,
													)}
												>
													{capabilityRow}
												</For>
											</div>
										</div>
									)}
								</For>
							</section>
						)}
					</For>
				</Show>
			</SectionCard>
			<SectionCard title={t("mcpAudit.title", "Call audit")}>
				<SettingRow
					label={t("mcpAudit.toggle", "Record MCP call outcomes")}
					hint={t(
						"mcpAudit.toggleHint",
						"Keeps a body-free log of recent agent tool calls (operation, outcome, size, effect) in memory. Off by default in release builds.",
					)}
				>
					{(id, descriptionId) => (
						<Switch
							role="checkbox"
							data-setting-id="app.mcp_call_audit_enabled"
							id={id}
							aria-describedby={descriptionId()}
							checked={callAuditEnabled()}
							onChange={async (e) => {
								const enabled = e.currentTarget.checked;
								// The view fetches on mount: persist first, or it reads "disabled".
								await saveConfigField((c) => {
									c.mcp_call_audit_enabled = enabled;
								});
								setCallAuditEnabled(enabled);
							}}
						/>
					)}
				</SettingRow>
				<Show when={callAuditEnabled()}>
					<McpAuditPanel />
				</Show>
			</SectionCard>
		</div>
	);
};
