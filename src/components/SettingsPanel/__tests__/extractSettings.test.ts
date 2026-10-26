import { describe, expect, it } from "vitest";
import { extractTab } from "./extractSettings";

describe("extractTab — expert controls", () => {
	it("tags a label inside an ExpertSetting with its configKey", () => {
		const src = `
			<h3>{t("x.heading", "Section")}</h3>
			<SettingToggle label={t("x.basic", "Basic toggle")} />
			<ExpertSetting configKey="app.osc52_clipboard" value={settingsStore.state.osc52Clipboard}>
				<SettingToggle label={t("x.expert", "Expert toggle")} />
			</ExpertSetting>
			<label>After</label>
		`;
		expect(extractTab(src).settings).toEqual([
			{ key: "x.basic", text: "Basic toggle", section: "Section" },
			{ key: "x.expert", text: "Expert toggle", section: "Section", configKey: "app.osc52_clipboard" },
			{ text: "After", section: "Section" },
		]);
	});

	it("rejects an ExpertSetting whose configKey is not a string literal", () => {
		const src = `<h3>S</h3><ExpertSetting configKey={key} value={1}><label>L</label></ExpertSetting>`;
		expect(() => extractTab(src)).toThrow(/configKey/);
	});
});

describe("extractTab — client gating", () => {
	it("tags what an isTauri() Show renders, and what its fallback renders instead", () => {
		const src = `
			<h3>Section</h3>
			<Show
				when={isTauri()}
				fallback={<SettingInput label={t("x.path", "Path")} />}
			>
				<label>Picker</label>
				<Show when={ready()}>
					<label>Nested</label>
				</Show>
			</Show>
			<label>Everywhere</label>
		`;
		expect(extractTab(src).settings).toEqual([
			{ key: "x.path", text: "Path", section: "Section", platform: "browser" },
			{ text: "Picker", section: "Section", platform: "desktop" },
			{ text: "Nested", section: "Section", platform: "desktop" },
			{ text: "Everywhere", section: "Section" },
		]);
	});

	it("tags a section heading gated on the desktop, even under a compound condition", () => {
		const src = `<Show when={isTauri() && cliStatus()}><h3>Desktop only</h3></Show><h3>Always</h3>`;
		expect(extractTab(src).sections).toEqual([{ text: "Desktop only", platform: "desktop" }, { text: "Always" }]);
	});

	it("leaves a Show gated on anything but the client untagged", () => {
		const src = `<h3>S</h3><Show when={showAdd()}><label>Form field</label></Show>`;
		expect(extractTab(src).settings).toEqual([{ text: "Form field", section: "S" }]);
	});
});

describe("extractTab — text read from source", () => {
	// Catches: shared Switch migration makes a static toggle caption disappear from the search drift oracle.
	it.each(["input", "Switch"])("reads the caption after a self-closing %s", (control) => {
		const result = extractTab(
			`<h3>Prompts</h3><label><${control} checked={enabled()} /><span>Auto-execute</span></label>`,
		);
		expect(result.settings).toEqual([{ text: "Auto-execute", section: "Prompts" }]);
		expect(result.dynamic).toBe(0);
	});
	it("ignores accessible names on form controls while indexing setting labels", () => {
		const src = `<input aria-label="Model" /><h3>Agents</h3><SettingToggle label="Show agent intent as tab title" />`;
		expect(extractTab(src).settings).toEqual([{ text: "Show agent intent as tab title", section: "Agents" }]);
	});
	it("counts a label whose text continues into a runtime expression as dynamic", () => {
		// Rendered as "Discovered tools (3)"; the index could only hold the
		// prefix, which `scrollToSetting` would never find.
		const src = `<h3>S</h3><label>Discovered tools ({tools().length})</label><label>Static</label>`;
		const extracted = extractTab(src);
		expect(extracted.settings).toEqual([{ text: "Static", section: "S" }]);
		expect(extracted.dynamic).toBe(1);
	});
});

// Catches: migrating a heading to shared card props silently drops its search section.
describe("shared Settings vocabulary extraction", () => {
	it("indexes page and card anchors without treating status badges as controls", () => {
		const result = extractTab(
			`<SettingPageHeader title="General" /><StatusSummary label="Connected" source="Runtime" /><StatusBadge label="Updated" /><SectionCard title={t("x.card", "Behavior")}><SettingRow label="Language" /></SectionCard><DangerZone title="Disconnect"><SettingRow label="Remove access" /></DangerZone>`,
		);
		expect(result.sections).toEqual([{ text: "General" }, { key: "x.card", text: "Behavior" }, { text: "Disconnect" }]);
		expect(result.settings).toEqual([
			{ text: "Language", section: "Behavior" },
			{ text: "Remove access", section: "Disconnect" },
		]);
	});
});
