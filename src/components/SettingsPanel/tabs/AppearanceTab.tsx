import { type Component, createSignal, For, Show } from "solid-js";
import { t } from "../../../i18n";
import type { RepoGroup } from "../../../stores/repositories";
import { repositoriesStore } from "../../../stores/repositories";
import { settingsStore } from "../../../stores/settings";
import { transcriptPreferences } from "../../../stores/transcriptPreferences";
import { uiStore } from "../../../stores/ui";
import { UiLegend } from "../../HelpPanel/UiLegend";
import { ColorSwatchPicker } from "../../shared/ColorSwatchPicker";
import { DEFAULT_COLOR_PRESETS } from "../../shared/colorPresets";
import { Button } from "../../ui/Button";
import { Input } from "../../ui/Input";
import { SettingSelect, SettingSlider, SettingToggle } from "../SettingFields";
import s from "../Settings.module.css";
import { SectionCard, SettingPageHeader } from "../SettingsPrimitives";

/** Preset colors for groups and sidebar */
export { DEFAULT_COLOR_PRESETS as PRESET_COLORS } from "../../shared/colorPresets";

/** Single group row in the settings list */
const GroupSettingsItem: Component<{
	group: RepoGroup;
}> = (props) => {
	const [editing, setEditing] = createSignal(false);
	const [editName, setEditName] = createSignal(props.group.name);
	const [nameError, setNameError] = createSignal("");

	const commitRename = () => {
		const name = editName().trim();
		if (!name) {
			setNameError(t("groups.error.nameEmpty", "Name cannot be empty"));
			return;
		}
		const ok = repositoriesStore.renameGroup(props.group.id, name);
		if (!ok) {
			setNameError(t("groups.error.nameExists", "A group with this name already exists"));
			return;
		}
		setNameError("");
		setEditing(false);
	};

	const cancelEdit = () => {
		setEditName(props.group.name);
		setNameError("");
		setEditing(false);
	};

	return (
		<div class={s.groupItem}>
			<div class={s.groupRow}>
				<Show
					when={editing()}
					fallback={
						<span class={s.groupName} onDblClick={() => setEditing(true)}>
							{props.group.name}
						</span>
					}
				>
					<Input
						data-setting-id="repositories_registration.groups.{id}.name"
						class={s.groupNameInput}
						aria-label={`Group name: ${props.group.name}`}
						aria-invalid={!!nameError()}
						value={editName()}
						onInput={(e) => {
							setEditName(e.currentTarget.value);
							setNameError("");
						}}
						onKeyDown={(e) => {
							if (e.key === "Enter") commitRename();
							if (e.key === "Escape") cancelEdit();
						}}
						autofocus
					/>
				</Show>
				<Button
					variant="ghost"
					data-non-setting="Create or remove a stable repository group through its owner"
					class={s.groupDeleteBtn}
					onClick={() => repositoriesStore.deleteGroup(props.group.id)}
					title={t("groups.btn.deleteGroup", "Delete group")}
					aria-label={`${t("groups.btn.deleteGroup", "Delete group")}: ${props.group.name}`}
				>
					×
				</Button>
			</div>
			<Show when={nameError()}>
				<div class={s.groupNameError}>{nameError()}</div>
			</Show>
			<ColorSwatchPicker
				settingId="repositories_registration.groups.{id}.color"
				color={props.group.color}
				presets={DEFAULT_COLOR_PRESETS}
				onChange={(c) => repositoriesStore.setGroupColor(props.group.id, c)}
			/>
		</div>
	);
};

export const AppearanceTab: Component = () => {
	const groups = () =>
		repositoriesStore.state.groupOrder.map((id) => repositoriesStore.state.groups[id]).filter(Boolean);

	return (
		<div class={`${s.section} ${s.applicationPage}`}>
			<SettingPageHeader title="Appearance" breadcrumb="Application / Appearance" scope="Application defaults" />
			<SectionCard title={t("appearance.heading.tabs", "Tabs")}>
				<SettingSelect
					settingId="app.split_tab_mode"
					row
					label={t("appearance.label.splitTabMode", "Split Tab Mode")}
					value={settingsStore.state.splitTabMode}
					onChange={(v) => {
						if (v === "separate" || v === "unified") settingsStore.setSplitTabMode(v);
					}}
					options={[
						{ value: "separate", label: t("appearance.splitTabMode.separate", "Separate") },
						{ value: "unified", label: t("appearance.splitTabMode.unified", "Unified") },
					]}
					hint={t("appearance.hint.splitTabMode", "How worktree tabs are arranged in the tab bar")}
				/>

				<SettingSelect
					settingId="app.tab_ordering_mode"
					row
					label={t("appearance.label.tabOrderingMode", "Tab Ordering")}
					value={settingsStore.state.tabOrderingMode}
					onChange={(v) => {
						if (v === "grouped-by-type" || v === "terminals-first" || v === "free") settingsStore.setTabOrderingMode(v);
					}}
					options={[
						{ value: "grouped-by-type", label: t("appearance.tabOrderingMode.grouped", "Grouped by Type") },
						{ value: "terminals-first", label: t("appearance.tabOrderingMode.terminalsFirst", "Terminals First") },
						{ value: "free", label: t("appearance.tabOrderingMode.free", "Free") },
					]}
					hint={t(
						"appearance.hint.tabOrderingMode",
						"How tabs are ordered: grouped by type, terminals first, or freely interleaved",
					)}
				/>

				<SettingToggle
					settingId="app.tab_cycling_all_types"
					row
					checked={settingsStore.state.tabCyclingAllTypes}
					onChange={(v) => settingsStore.setTabCyclingAllTypes(v)}
					label={t("appearance.label.tabCyclingAllTypes", "Cycle All Tab Types")}
					hint={t(
						"appearance.hint.tabCyclingAllTypes",
						"Next/previous tab shortcuts cycle through diff, markdown and editor tabs too — not just terminals",
					)}
				/>

				<SettingToggle
					settingId="app.tab_tree_enabled"
					row
					checked={settingsStore.state.tabTreeEnabled}
					onChange={(v) => settingsStore.setTabTreeEnabled(v)}
					label={t("appearance.label.tabTreeEnabled", "Nested Terminal Tabs")}
					hint={t(
						"appearance.hint.tabTreeEnabled",
						"Show each branch's open sessions and agent activity in a collapsible card under its sidebar row",
					)}
				/>

				<SettingSlider
					settingId="app.max_tab_name_length"
					row
					label={t("appearance.label.maxTabNameLength", "Max Tab Name Length")}
					value={settingsStore.state.maxTabNameLength}
					onChange={(v) => settingsStore.setMaxTabNameLength(v)}
					min={10}
					max={60}
					hint={t("appearance.hint.maxTabNameLength", "Maximum characters shown in tab names before truncating")}
				/>
			</SectionCard>
			<SectionCard title={t("appearance.heading.chat", "Chat")}>
				<SettingToggle
					data-non-setting="Client-local new-text indicator preference; no backend owner"
					row
					checked={transcriptPreferences.enabled()}
					onChange={(value) => transcriptPreferences.setEnabled(value)}
					label={t("appearance.label.newTextIndicator", "New text indicator")}
					hint={t(
						"appearance.hint.newTextIndicator",
						"Show New text below when a chat receives content while you read earlier messages. Saved on this client.",
					)}
				/>

				<SettingToggle
					settingId="app.chat_mute_intermediate_text"
					row
					checked={settingsStore.state.chatMuteIntermediateText}
					onChange={(v) => settingsStore.setChatMuteIntermediateText(v)}
					label={t("appearance.label.chatMuteIntermediateText", "Mute Intermediate Text")}
					hint={t(
						"appearance.hint.chatMuteIntermediateText",
						"Show assistant text that is followed by a tool call in the same turn smaller and dimmer, so the final answer stands out",
					)}
				/>
			</SectionCard>
			<SectionCard title={t("appearance.heading.groups", "Repository Groups")}>
				<p class={s.hint}>
					{t("appearance.hint.groups", "Organize repositories into color-coded groups in the sidebar")}
				</p>

				<Show when={groups().length === 0}>
					<div class={s.groupsEmpty}>{t("groups.empty.noGroups", "No groups yet")}</div>
				</Show>

				<For each={groups()}>{(group) => <GroupSettingsItem group={group} />}</For>

				<Button
					data-non-setting="Create or remove a stable repository group through its owner"
					class={s.groupsAddBtn}
					onClick={() => repositoriesStore.createGroup(t("groups.defaultGroupName", "New Group"))}
				>
					{t("groups.btn.addGroup", "Add Group")}
				</Button>
			</SectionCard>
			<SectionCard title={t("appearance.heading.layout", "Layout")}>
				<div class={s.group}>
					<Button
						data-non-setting="Reset panel geometry through the existing UI layout owner"
						class={s.testBtn}
						onClick={() => uiStore.resetLayout()}
					>
						{t("appearance.btn.resetLayout", "Reset Panel Sizes")}
					</Button>
					<p class={s.hint}>{t("appearance.hint.resetLayout", "Reset sidebar and panel widths to default values")}</p>
				</div>
			</SectionCard>
			<SectionCard title={t("appearance.heading.uiLegend", "UI Legend")}>
				<p class={s.hint} style={{ "margin-bottom": "12px" }}>
					{t("appearance.hint.uiLegend", "Visual reference for colors, symbols, and badges used throughout the app")}
				</p>
				<UiLegend />
			</SectionCard>
		</div>
	);
};
