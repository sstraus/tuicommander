import { type Component, createEffect, createMemo, createSignal, For, onCleanup, Show } from "solid-js";
import { t } from "../../../i18n";
import type { RepoGroup } from "../../../stores/repositories";
import { repositoriesStore } from "../../../stores/repositories";
import type { FontType } from "../../../stores/settings";
import { FONT_FAMILIES, settingsStore } from "../../../stores/settings";
import { uiStore } from "../../../stores/ui";
import { getTerminalTheme, getThemeNames } from "../../../themes";
import { UiLegend } from "../../HelpPanel/UiLegend";
import { ColorSwatchPicker } from "../../shared/ColorSwatchPicker";
import { SettingSelect, SettingSlider, SettingToggle } from "../SettingFields";
import s from "../Settings.module.css";

interface PreviewSpan {
	text: string;
	color?: string;
	bg?: string;
	bold?: boolean;
}

type PreviewLine = PreviewSpan[];

const PREVIEW_LINES: PreviewLine[] = [
	[
		{ text: " projectX ", bg: "blue", color: "white", bold: true },
		{ text: "", color: "blue", bg: "green" },
		{ text: "  main ", bg: "green", color: "black", bold: true },
		{ text: "", color: "green" },
		{ text: " cat server.ts" },
	],
	[
		{ text: "import", color: "dim" },
		{ text: " { serve } " },
		{ text: "from", color: "dim" },
		{ text: ' "http"', color: "green" },
		{ text: ";" },
	],
	[{ text: "const", color: "dim" }, { text: " port = " }, { text: "8080", color: "yellow" }, { text: ";" }],
	[{ text: "" }],
	[
		{ text: " projectX ", bg: "blue", color: "white", bold: true },
		{ text: "", color: "blue", bg: "green" },
		{ text: "  main ", bg: "green", color: "black", bold: true },
		{ text: "", color: "green" },
		{ text: " git diff" },
	],
	[{ text: "@@ -2,3 +2,5 @@", color: "cyan" }],
	[{ text: "-const port = 8080;", color: "red" }],
	[{ text: "+const port = 3000;", color: "green" }],
	[{ text: "+serve({ port });", color: "green" }],
	[{ text: "" }],
	[
		{ text: " projectX ", bg: "blue", color: "white", bold: true },
		{ text: "", color: "blue", bg: "green" },
		{ text: "  main ", bg: "green", color: "black", bold: true },
		{ text: "", color: "green" },
		{ text: " npm test" },
	],
	[{ text: "✓", color: "green" }, { text: " server " }, { text: "(4ms)", color: "dim" }],
	[{ text: "✓", color: "green" }, { text: " routes " }, { text: "(2ms)", color: "dim" }],
	[{ text: "✓", color: "green" }, { text: " logger " }, { text: "(1ms)", color: "dim" }],
	[{ text: "" }],
	[
		{ text: " projectX ", bg: "blue", color: "white", bold: true },
		{ text: "", color: "blue", bg: "green" },
		{ text: "  main ", bg: "green", color: "black", bold: true },
		{ text: "", color: "green" },
		{ text: " " },
	],
];

function getThemeColor(name: string): string {
	const theme = getTerminalTheme(settingsStore.state.theme);
	const map: Record<string, string | undefined> = {
		fg: theme.foreground,
		bg: theme.background,
		cursor: theme.cursor,
		dim: theme.brightBlack,
		red: theme.red,
		green: theme.green,
		yellow: theme.yellow,
		blue: theme.blue,
		magenta: theme.magenta,
		cyan: theme.cyan,
		white: theme.white,
		black: theme.black,
		brightRed: theme.brightRed,
		brightGreen: theme.brightGreen,
		brightYellow: theme.brightYellow,
		brightBlue: theme.brightBlue,
	};
	return map[name] ?? theme.foreground ?? "#c0c0c0";
}

const TerminalPreview: Component = () => {
	const fontFamily = createMemo(() => FONT_FAMILIES[settingsStore.state.font] ?? "monospace");
	const fontSize = () => settingsStore.state.defaultFontSize;
	const fontWeight = () => settingsStore.state.fontWeight;
	const cursorStyle = () => settingsStore.state.cursorStyle;
	let canvasRef!: HTMLCanvasElement;
	const [cursorVisible, setCursorVisible] = createSignal(true);

	const paint = (showCursor: boolean) => {
		const canvas = canvasRef;
		if (!canvas) return;
		const ctx = canvas.getContext("2d");
		if (!ctx) return;

		const dpr = window.devicePixelRatio || 1;
		const font = fontFamily();
		const size = fontSize();
		const weight = fontWeight();
		const cursor = cursorStyle();

		ctx.font = `${weight} ${size}px ${font}`;
		const mW = ctx.measureText("W");
		const cellW = Math.round(mW.width);
		const ascent = mW.fontBoundingBoxAscent ?? mW.actualBoundingBoxAscent ?? size * 0.8;
		const descent = mW.fontBoundingBoxDescent ?? mW.actualBoundingBoxDescent ?? size * 0.2;
		const charH = ascent + descent;
		const cellH = Math.floor(Math.ceil(charH * dpr) * 1.2) / dpr;
		const baseline = Math.ceil(ascent);

		const rows = PREVIEW_LINES.length;
		const maxCols = 40;
		const w = maxCols * cellW + 12;
		const h = rows * cellH + 8;

		canvas.width = Math.round(w * dpr);
		canvas.height = Math.round(h * dpr);
		canvas.style.width = `${w}px`;
		canvas.style.height = `${h}px`;
		ctx.scale(dpr, dpr);

		const bgColor = getThemeColor("bg");
		ctx.fillStyle = bgColor;
		ctx.fillRect(0, 0, w, h);

		const fgDefault = getThemeColor("fg");
		const padLeft = 6;

		for (let row = 0; row < rows; row++) {
			const line = PREVIEW_LINES[row];
			const y = row * cellH + 4;
			let col = 0;

			for (const span of line) {
				const text = span.text;
				if (!text) continue;

				if (span.bg) {
					ctx.fillStyle = getThemeColor(span.bg);
					ctx.fillRect(padLeft + col * cellW, y, text.length * cellW, cellH);
				}

				ctx.fillStyle = span.color ? getThemeColor(span.color) : fgDefault;
				const fontStr = span.bold ? `700 ${size}px ${font}` : `${weight} ${size}px ${font}`;
				ctx.font = fontStr;

				for (let i = 0; i < text.length; i++) {
					const cp = text.codePointAt(i) ?? 0;
					const gx = padLeft + (col + i) * cellW;
					if (cp === 0xe0b0) {
						ctx.beginPath();
						ctx.moveTo(gx, y);
						ctx.lineTo(gx + cellW, y + cellH / 2);
						ctx.lineTo(gx, y + cellH);
						ctx.closePath();
						ctx.fill();
					} else {
						ctx.fillText(text[i], gx, y + baseline);
					}
				}
				col += text.length;
			}
		}

		if (showCursor) {
			const lastLine = PREVIEW_LINES[rows - 1];
			let cursorCol = 0;
			for (const span of lastLine) cursorCol += span.text.length;
			const cx = padLeft + cursorCol * cellW;
			const cy = (rows - 1) * cellH + 4;
			ctx.fillStyle = getThemeColor("cursor");
			if (cursor === "bar") {
				ctx.fillRect(cx, cy, 2, cellH);
			} else if (cursor === "block") {
				ctx.fillRect(cx, cy, cellW, cellH);
			} else {
				ctx.fillRect(cx, cy + cellH - 2, cellW, 2);
			}
		}
	};

	createEffect(() => {
		fontFamily();
		fontSize();
		fontWeight();
		cursorStyle();
		settingsStore.state.theme;
		const show = cursorVisible();
		queueMicrotask(() => paint(show));
	});

	let blinkTimer: ReturnType<typeof setInterval> | undefined;
	const startBlink = () => {
		if (blinkTimer) return;
		setCursorVisible(true);
		blinkTimer = setInterval(() => setCursorVisible((v) => !v), 530);
	};
	const stopBlink = () => {
		if (blinkTimer) {
			clearInterval(blinkTimer);
			blinkTimer = undefined;
		}
	};

	createEffect(() => {
		const canvas = canvasRef;
		if (!canvas) return;
		const observer = new IntersectionObserver(
			([entry]) => {
				if (entry.isIntersecting) startBlink();
				else stopBlink();
			},
			{ threshold: 0.1 },
		);
		observer.observe(canvas);
		onCleanup(() => {
			observer.disconnect();
			stopBlink();
		});
	});

	return (
		<div class={s.terminalPreview}>
			<div class={s.terminalPreviewTitlebar}>
				<div class={`${s.terminalPreviewDot} ${s.terminalPreviewDotClose}`} />
				<div class={`${s.terminalPreviewDot} ${s.terminalPreviewDotMin}`} />
				<div class={`${s.terminalPreviewDot} ${s.terminalPreviewDotMax}`} />
				<div class={s.terminalPreviewTitleText}>zsh</div>
			</div>
			<div class={s.terminalPreviewCanvas}>
				<canvas ref={canvasRef} />
			</div>
		</div>
	);
};

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

const themeOptions = () => Object.entries(getThemeNames()).map(([value, label]) => ({ value, label }));
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
				options={themeOptions()}
				hint={t("appearance.hint.terminalTheme", "Color theme for terminal output and app chrome")}
			/>

			<h3>{t("appearance.heading.terminal", "Terminal")}</h3>

			<div class={s.terminalSplit}>
				<div class={s.terminalControls}>
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
						hint={t(
							"appearance.hint.cursorStyle",
							"Shape of the terminal cursor. Applies immediately to all terminals.",
						)}
					/>
				</div>

				<TerminalPreview />
			</div>

			<SettingToggle
				checked={settingsStore.state.offscreenRenderer}
				onChange={(v) => settingsStore.setOffscreenRenderer(v)}
				label={t("appearance.label.offscreenRenderer", "Off-Main-Thread Rendering (Experimental)")}
				hint={t(
					"appearance.hint.offscreenRenderer",
					"Render terminals in a Web Worker via OffscreenCanvas. Off by default — measured trade-off: under heavy CPU load it adds a slight first-character typing lag (the glyph trails the cursor), while the main-thread renderer keeps up anyway thanks to backend frame backpressure. Enable only if you have a specific need. Reopen terminals to apply; falls back where unsupported.",
				)}
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

			<SettingSelect
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
				checked={settingsStore.state.tabCyclingAllTypes}
				onChange={(v) => settingsStore.setTabCyclingAllTypes(v)}
				label={t("appearance.label.tabCyclingAllTypes", "Cycle All Tab Types")}
				hint={t(
					"appearance.hint.tabCyclingAllTypes",
					"Next/previous tab shortcuts cycle through diff, markdown and editor tabs too — not just terminals",
				)}
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
