import { type Component, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { t } from "../../../i18n";
import { type EgoPerimeterClient, egoPerimeterCli } from "../../../services/egoCli";
import { settingOptions } from "../../../services/settingsCatalogue";
import { reportSettingsConflicts, settingsRefresh } from "../../../services/settingsRefresh";
import { appLogger } from "../../../stores/appLogger";
import { asEgoCliError, type EgoPerimeterView, type EgoRootAccess, type EgoRootsEdit } from "../../../types/ego";
import { mergeSettingsSnapshot } from "../../../utils/mergeSettingsSnapshot";
import { Button } from "../../ui/Button";
import { Input } from "../../ui/Input";
import { Select } from "../../ui/Select";
import { StatusBadge } from "../../ui/StatusBadge";
import { Textarea } from "../../ui/Textarea";
import s from "../Settings.module.css";
import { ListRow, SectionCard, SettingRow, StatusSummary } from "../SettingsPrimitives";
import layout from "./AiChatTab.module.css";

export const EgoPerimeterSection: Component<{ client?: EgoPerimeterClient }> = (props) => {
	const client = () => props.client ?? egoPerimeterCli;
	const [view, setView] = createSignal<EgoPerimeterView | null>(null);
	const [draft, setDraft] = createSignal<EgoRootsEdit>({
		rootDir: "",
		rootAccess: "read-write",
		readAllowlist: "",
		writableDirs: "",
	});
	const [editingRoots, setEditingRoots] = createSignal(false);
	const [verifyOpen, setVerifyOpen] = createSignal(false);
	const [busy, setBusy] = createSignal(false);
	const [error, setError] = createSignal<string | null>(null);

	const attempt = async (call: () => Promise<EgoPerimeterView>, adoptRoots: boolean): Promise<void> => {
		setBusy(true);
		try {
			const result = await call();
			if (adoptRoots) setDraft(result.roots);
			setView(result);
			setError(null);
		} catch (failure) {
			const egoError = asEgoCliError(failure);
			if (!egoError) appLogger.error("config", "ego perimeter request failed", failure);
			setError(
				egoError ? `${egoError.message}\n${egoError.command}\n${egoError.stderr || egoError.stdout}` : String(failure),
			);
		} finally {
			setBusy(false);
		}
	};
	onCleanup(
		settingsRefresh.register({
			section: "ego",
			read: () => client().perimeter(),
			apply: (incoming) => {
				const current = view();
				const merged = mergeSettingsSnapshot(
					current?.roots ?? incoming.roots,
					current ? draft() : incoming.roots,
					incoming.roots,
				);
				setDraft(merged.value);
				setView(incoming);
				reportSettingsConflicts(merged.conflicts);
			},
		}),
	);
	onMount(() => void attempt(() => client().perimeter(), true));
	const rootsDirty = () => {
		const saved = view()?.roots;
		return !!saved && JSON.stringify(saved) !== JSON.stringify(draft());
	};
	const edit = <K extends keyof EgoRootsEdit>(key: K, value: EgoRootsEdit[K]) =>
		setDraft((current) => ({ ...current, [key]: value }));

	return (
		<div class={layout.perimeter}>
			<Show when={error()}>
				{(message) => (
					<>
						<p class={s.warning} role="alert">
							{t(
								"perimeter.failure",
								"The write or readback failed. Reload from ego to check what is stored. Check the ego executable and AI Chat workspace in General if the command could not start.",
							)}
						</p>
						<pre class={s.mcpSnippetPre}>{message()}</pre>
					</>
				)}
			</Show>
			<Show when={view()}>
				{(loaded) => (
					<>
						<SectionCard
							title="Workspace access"
							actions={
								<Button
									data-non-setting="Open or close perimeter editing and recovery instructions"
									class={s.testBtn}
									onClick={() => setEditingRoots(!editingRoots())}
								>
									{editingRoots() ? "Hide folder editor" : "Add folder"}
								</Button>
							}
						>
							<p class={s.hint}>
								{t("perimeter.target", "Profile:")} {loaded().profile ?? t("perimeter.userDefaults", "user defaults")}
							</p>
							<SettingRow label="Workspace">
								{(id, descriptionId) => (
									<Input
										data-non-setting="Read-only effective workspace reported by ego"
										id={id}
										aria-describedby={descriptionId()}
										value={loaded().effective.workspace}
										readOnly
										title={loaded().effective.workspace}
									/>
								)}
							</SettingRow>
							<For each={loaded().effective.roots}>
								{(root) => (
									<ListRow
										name={root.path}
										description={root.source}
										badges={<StatusBadge label={root.access === "read" ? "Read only" : "Read and write"} />}
										actions={
											<Button
												data-non-setting="Open or close perimeter editing and recovery instructions"
												class={s.testBtn}
												onClick={() => setEditingRoots(true)}
											>
												Edit
											</Button>
										}
									/>
								)}
							</For>
							<Show when={editingRoots()}>
								<SettingRow
									label={t("perimeter.label.rootDir", "Root directory")}
									hint={t(
										"perimeter.emptyRoot",
										"Leave all paths empty for workspace-only roots. An absent roots setting defaults to ~/Gits when that directory exists.",
									)}
								>
									{(id, descriptionId) => (
										<>
											<Input
												data-setting-id="ego.roots"
												type="text"
												id={id}
												aria-describedby={descriptionId()}
												value={draft().rootDir}
												disabled={busy()}
												onInput={(event) => edit("rootDir", event.currentTarget.value)}
												placeholder="~/Gits"
											/>
										</>
									)}
								</SettingRow>
								<SettingRow label={t("perimeter.label.rootAccess", "Root access")}>
									{(id, descriptionId) => (
										<>
											<Select
												data-setting-id="ego.roots"
												id={id}
												aria-describedby={descriptionId()}
												value={draft().rootAccess}
												disabled={busy()}
												onChange={(event) => {
													const value = event.currentTarget.value;
													if (value === "read" || value === "read-write")
														edit("rootAccess", value satisfies EgoRootAccess);
												}}
											>
												<For
													each={settingOptions(
														"ego.roots",
														{
															"read-write": t("perimeter.readWrite", "Read and write"),
															read: t("perimeter.read", "Read only"),
														},
														["items", "properties", "access"],
													)}
												>
													{(option) => <option value={option.value}>{option.label}</option>}
												</For>
											</Select>
										</>
									)}
								</SettingRow>
								<SettingRow
									label={t("perimeter.label.readAllowlist", "Read allowlist")}
									hint="One absolute path or ~/ path per line. These entries grant read access; they do not revoke a writable parent root."
								>
									{(id, descriptionId) => (
										<>
											<Textarea
												data-setting-id="ego.roots"
												id={id}
												aria-describedby={descriptionId()}
												rows={2}
												value={draft().readAllowlist}
												disabled={busy()}
												onInput={(event) => edit("readAllowlist", event.currentTarget.value)}
											/>
										</>
									)}
								</SettingRow>
								<SettingRow
									label={t("perimeter.label.writableDirs", "Extra writable directories")}
									hint={t(
										"perimeter.paths",
										"One absolute path or ~/ path per line. Read-only entries do not revoke access granted by a containing writable root. ego validates roots and system ceilings.",
									)}
								>
									{(id, descriptionId) => (
										<>
											<Textarea
												data-setting-id="ego.roots"
												id={id}
												aria-describedby={descriptionId()}
												rows={2}
												value={draft().writableDirs}
												disabled={busy()}
												onInput={(event) => edit("writableDirs", event.currentTarget.value)}
											/>
										</>
									)}
								</SettingRow>
								<div class={s.actions}>
									<Button
										data-setting-id="ego.roots"
										disabled={busy()}
										data-ready={rootsDirty() ? "true" : undefined}
										onClick={() => void attempt(() => client().setRoots(draft()), true)}
									>
										{t("perimeter.saveRoots", "Save roots in ego")}
									</Button>
								</div>
							</Show>
						</SectionCard>
						<SectionCard title="Network and sandbox">
							<SettingRow label="Network" hint="Network access for new ego runs.">
								{(id, descriptionId) => (
									<Select
										data-setting-id="ego.network"
										id={id}
										aria-describedby={descriptionId()}
										value={loaded().networkEnabled ? "on" : "off"}
										disabled={busy()}
										onChange={(event) => {
											const enabled = event.currentTarget.value === "on";
											event.currentTarget.value = loaded().networkEnabled ? "on" : "off";
											void attempt(() => client().setNetwork(enabled), false);
										}}
									>
										{/* This boolean-only owner action selects an explicit value; inheritance is available in All settings. */}
										<For
											each={settingOptions("ego.network", { on: "Allowed", off: "Blocked" }).filter(
												(option) => option.value !== "",
											)}
										>
											{(option) => <option value={option.value}>{option.label}</option>}
										</For>
									</Select>
								)}
							</SettingRow>
							<StatusSummary
								label={
									loaded().execEnforcement === "enforcedByOs"
										? t("perimeter.osEnforced", "Enforced by OS")
										: loaded().execEnforcement === "promptOnly"
											? t("perimeter.promptOnly", "Prompt only")
											: "Not verified here"
								}
								tone={
									loaded().execEnforcement === "enforcedByOs"
										? "success"
										: loaded().execEnforcement === "promptOnly"
											? "warning"
											: undefined
								}
								source={loaded().effective.capabilities_reason}
							>
								<Button
									data-non-setting="Open or close perimeter editing and recovery instructions"
									class={s.testBtn}
									onClick={() => setVerifyOpen(!verifyOpen())}
								>
									How to verify
								</Button>
							</StatusSummary>
							<Show when={verifyOpen()}>
								<p class={s.hint}>Run ego doctor in a terminal and inspect its command-isolation report.</p>
							</Show>
							<Show when={loaded().execEnforcement === "promptOnly"}>
								<p class={s.hint}>
									{t("perimeter.incomplete", "Command isolation is not fully enforced by the operating system.")}
								</p>
							</Show>
						</SectionCard>
						<SectionCard
							title="Advanced and diagnostics"
							actions={
								<Button
									data-non-setting="Explicitly refresh the configured Ego owner without saving"
									class={s.testBtn}
									disabled={busy()}
									onClick={() => void attempt(() => client().perimeter(), true)}
								>
									{t("perimeter.reload", "Reload from ego")}
								</Button>
							}
						>
							<details>
								<summary>{t("perimeter.preview", "Last successful effective perimeter from ego")}</summary>
								<pre class={s.mcpSnippetPre}>{loaded().preview}</pre>
							</details>
						</SectionCard>
					</>
				)}
			</Show>
			<Show when={!view()}>
				<Button
					data-non-setting="Explicitly refresh the configured Ego owner without saving"
					class={s.testBtn}
					disabled={busy()}
					onClick={() => void attempt(() => client().perimeter(), true)}
				>
					{t("perimeter.reload", "Reload from ego")}
				</Button>
			</Show>
		</div>
	);
};
