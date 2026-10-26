import { readdirSync, readFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { settingsCatalogue } from "../../../generated/settingsCatalogue";
import { hasCatalogueEditor } from "../../../services/settingsCatalogue";
import catalogueExclusions from "../settingsCatalogueExclusions.json";
import controlExclusions from "../settingsControlExclusions.json";
import { extractSettingControls, missingCatalogueControls, missingControlIdentities } from "./settingsControls";

const root = resolve(__dirname, "../../../..");
function files(directory: string): string[] {
	return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
		const path = join(directory, entry.name);
		return entry.isDirectory() ? files(path) : [path];
	});
}
// Shared primitives are audited at their call sites. Production pages, raw controls,
// expert/platform branches and dynamic row bodies are all parsed, even when unmounted.
const sources = [
	...files(join(root, "src/components/SettingsPanel")),
	join(root, "src/components/TunnelsPanel/TunnelEditorModal.tsx"),
].filter(
	(path) =>
		path.endsWith(".tsx") &&
		!path.includes("/__tests__/") &&
		!path.includes("/fields/") &&
		!path.endsWith("/SettingFields.tsx"),
);
const controls = () =>
	sources.flatMap((path) => extractSettingControls(relative(root, path), readFileSync(path, "utf8")));

describe("settings catalogue/control identity boundary", () => {
	// Catches: a raw input hidden in a dynamic row bypassing the settings inventory.
	it("raw_dynamic_control_missing_id_fails", () => {
		expect(missingControlIdentities(controls(), controlExclusions)).toEqual([]);
		const catalogueIds = new Set(Object.keys(settingsCatalogue.fields));
		expect(controls().filter((control) => control.settingId && !catalogueIds.has(control.settingId))).toEqual([]);
		expect(
			Object.keys(controlExclusions).filter((id) => !controls().some((control) => control.exclusion === id)),
		).toEqual([]);
	});

	// Catches: migrating a native action to shared Button/ColorSwatch silently removes it from the identity audit.
	it.each(["Button", "ColorSwatch", "Slider", "ColorInput"])("audits shared %s identities", (tag) => {
		const result = extractSettingControls("fixture.tsx", `<${tag} data-setting-id="app.theme" /><${tag} />`);
		expect(result.map((control) => control.tag)).toEqual([tag, tag]);
		expect(result[0].settingId).toBe("app.theme");
		expect(missingControlIdentities(result)).toEqual([result[1]]);
	});

	// Catches: any computed expression being accepted as a catalogue identity.
	it("rejects_unresolved_dynamic_control_ids", () => {
		const result = extractSettingControls(
			"fixture.tsx",
			`<input data-setting-id={unknown.id} /><button onClick={() => save()}>Save</button>`,
		);
		expect(missingControlIdentities(result).map((control) => control.tag)).toEqual(["input", "button"]);
	});

	// Catches: owner-dependent controls losing either of their finite catalogue identities.
	it("resolves_both_literal_owner_identities", () => {
		const result = extractSettingControls(
			"fixture.tsx",
			`<input data-setting-id={props.agentType ? "agents.agents.{key}.launch_modules.{key}" : "agents.launch_modules.{key}"} />`,
		);
		expect(result.map((control) => control.settingId)).toEqual([
			"agents.agents.{key}.launch_modules.{key}",
			"agents.launch_modules.{key}",
		]);
		expect(missingControlIdentities(result)).toEqual([]);
	});

	// Catches: an extractor skipping JSX in callbacks or stopping at an arrow's >.
	it("finds_raw_controls_inside_dynamic_rows_and_platform_fallbacks", () => {
		const source = `<Show when={isTauri()} fallback={<input data-non-setting="navigation" />}><For each={rows()}>{row => <Input onInput={e => edit(row.id, e.currentTarget.value)} />}</For></Show>`;
		const result = extractSettingControls("fixture.tsx", source);
		expect(result).toHaveLength(2);
		expect(missingControlIdentities(result).map((control) => control.tag)).toEqual(["Input"]);
	});

	// Catches: adding a descriptor without any real control, or silently treating an empty reason as approval.
	it("catalogue_without_ui_fails", () => {
		const production = controls();
		// These IDs are supported by the actual typed renderer, not a hand-maintained
		// claim list. allSettingsTab.test.tsx mounts every supported descriptor.
		if (production.some((control) => control.file.endsWith("/AllSettingsTab.tsx") && control.dynamic)) {
			for (const field of Object.values(settingsCatalogue.fields).filter(hasCatalogueEditor))
				production.push({
					file: "AllSettingsTab",
					line: 0,
					tag: "CatalogueFieldEditor",
					settingId: field.id,
					dynamic: true,
				});
		}
		expect(missingCatalogueControls(Object.keys(settingsCatalogue.fields), production, catalogueExclusions)).toEqual(
			[],
		);
	});

	// Catches: a raw enum select retaining a handwritten choice list after the owner changes it.
	it("ui_options_match_schema", () => {
		const fields = settingsCatalogue.fields as Record<string, { schema: { enum?: readonly unknown[] } }>;
		expect(
			controls().filter(
				(control) =>
					["Select", "select"].includes(control.tag) &&
					control.settingId &&
					fields[control.settingId]?.schema.enum &&
					control.enumSource !== control.settingId,
			),
		).toEqual([]);
	});

	// Catches: removed or empty ledger entries silently authorizing unrelated gaps.
	it("exclusions_name_live_native_fields_with_specific_reasons", () => {
		for (const [id, reason] of Object.entries(catalogueExclusions)) {
			expect(Object.hasOwn(settingsCatalogue.fields, id)).toBe(true);
			expect(reason.trim().length).toBeGreaterThan(0);
		}
	});

	it("empty_exclusion_is_not_coverage", () => {
		const result = extractSettingControls(
			"fixture.tsx",
			`<SettingInput settingId="app.shell" /><Input data-non-setting="search query" />`,
		);
		expect(missingCatalogueControls(["app.shell", "app.theme"], result, { "app.theme": "" })).toEqual(["app.theme"]);
	});
});

// Catches: shared switches disappearing from the identity audit, or Solid control flow creating false inputs.
it("resolves shared Switch imports without confusing Solid Switch control flow", () => {
	const source = `import { Switch } from 'solid-js';
 import { Switch as Toggle } from '../../ui/Switch';
 const Page = () => <Switch><Toggle data-setting-id="app.osc52_clipboard" /><Toggle /></Switch>;`;
	const result = extractSettingControls("fixture.tsx", source);
	expect(result.map((control) => control.tag)).toEqual(["Toggle", "Toggle"]);
	expect(missingControlIdentities(result)).toHaveLength(1);
	expect(result[0].settingId).toBe("app.osc52_clipboard");
});
