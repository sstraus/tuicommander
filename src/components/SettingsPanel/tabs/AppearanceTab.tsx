import { type Component, createSignal, For, Show } from "solid-js";
import { t } from "../../../i18n";
import type { RepoGroup } from "../../../stores/repositories";
import { repositoriesStore } from "../../../stores/repositories";
import type { FontType } from "../../../stores/settings";
import { FONT_FAMILIES, settingsStore } from "../../../stores/settings";
import { uiStore } from "../../../stores/ui";
import { THEME_NAMES } from "../../../themes";
import { UiLegend } from "../../HelpPanel/UiLegend";
import { ColorSwatchPicker } from "../../shared/ColorSwatchPicker";
import { SettingSelect, SettingSlider } from "../SettingFields";
import s from "../Settings.module.css";

/** Preset colors for groups and sidebar */
export const PRESET_COLORS = [
	{ hex: "#4A9EFF", name: "Blue" },
	{ hex: "#FF6B6B", name: "Red" },
	{ hex: "#50C878", name: "Green" },
	{ hex: "#FFB347", name: "Orange" },
	{ hex: "#B19CD9", name: "Purple" },
	{ hex: "#FF85A2", name: "Pink" },
	{ hex: "#5BC0BE", name: "Teal" },
	{ hex: "#FFD93D", name: "Yellow" },
];

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
					<input
						class={s.groupNameInput}
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
				<button
					class={s.groupDeleteBtn}
					onClick={() => repositoriesStore.deleteGroup(props.group.id)}
					title={t("groups.btn.deleteGroup", "Delete group")}
				>
					×
				</button>
			</div>
			<Show when={nameError()}>
				<div class={s.groupNameError}>{nameError()}</div>
			</Show>
			<ColorSwatchPicker
				color={props.group.color}
				onChange={(c) => repositoriesStore.setGroupColor(props.group.id, c)}
			/>
		</div>
	);
};

const themeOptions = Object.entries(THEME_NAMES).map(([value, label]) => ({ value, label }));
const fontOptions = Object.keys(FONT_FAMILIES).map((value) => ({ value, label: value }));

export const AppearanceTab: Component = () => {
	const groups = () =>
		repositoriesStore.state.groupOrder.map((id) => repositoriesStore.state.groups[id]).filter(Boolean);

	return (
		<div class={s.section}>
			<h3>{t("appearance.heading.theme", "Theme")}</h3>

			<SettingSelect
				label={t("appearance.label.terminalTheme", "Terminal Theme")}
				value={settingsStore.state.theme}
				onChange={(v) => settingsStore.setTheme(v)}
				options={themeOptions}
				hint={t("appearance.hint.terminalTheme", "Color theme for terminal output and app chrome")}
			/>

			<h3>{t("appearance.heading.terminal", "Terminal")}</h3>

			<SettingSelect
				label={t("appearance.label.terminalFont", "Terminal Font")}
				value={settingsStore.state.font}
				onChange={(v) => settingsStore.setFont(v as FontType)}
				options={fontOptions}
				hint={t("appearance.hint.terminalFont", "Monospace font for terminals")}
			/>

			<SettingSlider
				label={t("appearance.label.defaultFontSize", "Default Font Size")}
				value={settingsStore.state.defaultFontSize}
				onChange={(v) => settingsStore.setDefaultFontSize(v)}
				min={8}
				max={32}
				suffix="px"
				hint={t("appearance.hint.defaultFontSize", "Default font size for new terminals")}
			/>

			<SettingSlider
				label={t("appearance.label.fontWeight", "Font Weight")}
				value={settingsStore.state.fontWeight}
				onChange={(v) => settingsStore.setFontWeight(v)}
				min={100}
				max={900}
				step={100}
				hint={t("appearance.hint.fontWeight", "Terminal font weight (200 = ExtraLight, 400 = Regular, 700 = Bold)")}
			/>

			<SettingSelect
				label={t("appearance.label.cursorStyle", "Cursor Style")}
				value={settingsStore.state.cursorStyle}
				onChange={(v) => settingsStore.setCursorStyle(v as "bar" | "block" | "underline")}
				options={[
					{ value: "bar", label: t("appearance.cursorStyle.bar", "Bar") },
					{ value: "block", label: t("appearance.cursorStyle.block", "Block") },
					{ value: "underline", label: t("appearance.cursorStyle.underline", "Underline") },
				]}
				hint={t("appearance.hint.cursorStyle", "Shape of the terminal cursor. Applies immediately to all terminals.")}
			/>

			<h3>{t("appearance.heading.tabs", "Tabs")}</h3>

			<SettingSelect
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

			<SettingSlider
				label={t("appearance.label.maxTabNameLength", "Max Tab Name Length")}
				value={settingsStore.state.maxTabNameLength}
				onChange={(v) => settingsStore.setMaxTabNameLength(v)}
				min={10}
				max={60}
				hint={t("appearance.hint.maxTabNameLength", "Maximum characters shown in tab names before truncating")}
			/>

			<h3>{t("appearance.heading.groups", "Repository Groups")}</h3>
			<p class={s.hint}>
				{t("appearance.hint.groups", "Organize repositories into color-coded groups in the sidebar")}
			</p>

			<Show when={groups().length === 0}>
				<div class={s.groupsEmpty}>{t("groups.empty.noGroups", "No groups yet")}</div>
			</Show>

			<For each={groups()}>{(group) => <GroupSettingsItem group={group} />}</For>

			<button
				class={s.groupsAddBtn}
				onClick={() => repositoriesStore.createGroup(t("groups.defaultGroupName", "New Group"))}
			>
				{t("groups.btn.addGroup", "Add Group")}
			</button>

			<h3>{t("appearance.heading.layout", "Layout")}</h3>

			<div class={s.group}>
				<button class={s.testBtn} onClick={() => uiStore.resetLayout()}>
					{t("appearance.btn.resetLayout", "Reset Panel Sizes")}
				</button>
				<p class={s.hint}>{t("appearance.hint.resetLayout", "Reset sidebar and panel widths to default values")}</p>
			</div>

			<h3>{t("appearance.heading.uiLegend", "UI Legend")}</h3>
			<p class={s.hint} style={{ "margin-bottom": "12px" }}>
				{t("appearance.hint.uiLegend", "Visual reference for colors, symbols, and badges used throughout the app")}
			</p>
			<UiLegend />
		</div>
	);
};
