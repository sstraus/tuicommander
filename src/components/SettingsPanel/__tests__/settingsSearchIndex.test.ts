import fs from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";
import { SETTINGS_SEARCH_INDEX, type SettingsSearchEntry, searchSettings } from "../settingsSearchIndex";
import { extractRenderedTabComponents, extractRenderedTabKeys, extractTab } from "./extractSettings";

/** Nav key → every source file that renders part of that page, in render
 * order. A page composed of several components (MCP = the local server panel
 * above the upstream proxy panel) is extracted from all of them, concatenated,
 * so a label keeps the `<h3>` the DOM puts above it. A component nested inside
 * one of these (RemoteMachinesTab wraps RemoteMachinesPanel) is listed by hand;
 * the top-level ones are checked against `SettingsPanel` below. */
// Catalogue fields are indexed dynamically, not extracted as static JSX labels.
// allSettingsTab.test.tsx exercises every supported descriptor's real editor.
const DYNAMIC_TAB_SOURCES: Record<string, string[]> = { "all-settings": ["AllSettingsTab.tsx"] };
const TAB_SOURCES: Record<string, string[]> = {
	telegram: ["tabs/TelegramTab.tsx"],
	general: ["tabs/GeneralTab.tsx"],
	appearance: ["tabs/AppearanceTab.tsx"],
	notifications: ["tabs/NotificationsTab.tsx"],
	terminal: ["tabs/TerminalTab.tsx"],
	"keyboard-shortcuts": ["tabs/KeyboardShortcutsTab.tsx"],
	dictation: ["DictationSettings.tsx"],
	github: ["tabs/GitHubTab.tsx"],
	mcp: ["tabs/services/LocalMcpPanel.tsx", "tabs/services/UpstreamMcpPanel.tsx"],
	"remote-access": ["tabs/services/RemoteAccessPanel.tsx"],
	"remote-machines": ["tabs/RemoteMachinesTab.tsx", "tabs/services/RemoteMachinesPanel.tsx"],
	plugins: ["tabs/PluginsTab.tsx"],
	// The runtime-selected editor has no static scroll target. Read it first so
	// its fields cannot inherit the library heading from the tab source.
	"smart-prompts": ["../shared/SmartPromptEditor.tsx", "tabs/SmartPromptsTab.tsx"],
	agents: ["tabs/AgentsTab.tsx", "tabs/LaunchModuleSettings.tsx"],
	"ai-chat": ["tabs/EgoPerimeterSection.tsx", "tabs/AiChatTab.tsx"],
};

/** Occurrences the extraction rule cannot index, pinned so a new one is loud.
 *
 * `dynamic` = heading or label text computed at runtime (per-agent cards,
 * per-plugin rows). `orphans` = a label with no `<h3>` above it, i.e. a modal
 * form field. Neither has a stable scroll target. If a count moves, look at
 * what was added: give it a static heading if it is a real setting, otherwise
 * update the number here. */
const UNINDEXABLE: Record<string, { dynamic: number; orphans: number }> = {
	// Recent event labels come from runtime state; polling summaries are not controls.
	telegram: { dynamic: 1, orphans: 0 },
	general: { dynamic: 0, orphans: 0 },
	appearance: { dynamic: 0, orphans: 0 },
	// Notification sound row labels are runtime entries, not static search targets.
	notifications: { dynamic: 1, orphans: 0 },
	terminal: { dynamic: 0, orphans: 0 },
	// The `<label>{section.title}</label>` over each group of the shortcut list —
	// the groups and their bindings are built at runtime, so the page is indexed
	// by its heading, the global hotkey and the plugin-commands group only.
	"keyboard-shortcuts": { dynamic: 1, orphans: 0 },
	dictation: { dynamic: 0, orphans: 0 },
	github: { dynamic: 0, orphans: 0 },
	// UpstreamMcpPanel: the "Discovered tools ({count})" label, whose rendered
	// text includes a runtime count.
	mcp: { dynamic: 2, orphans: 0 },
	"remote-access": { dynamic: 0, orphans: 0 },
	// RemoteMachinesPanel: the SSH-host `<option label={`${host}…`}>` (a
	// datalist entry, not a setting), and the "Deployment" and "Keep ephemeral
	// daemon alive" labels, whose text sits in a nested `<span>`.
	"remote-machines": { dynamic: 1, orphans: 0 },
	plugins: { dynamic: 1, orphans: 0 },
	// Shared editor: runtime title and placement captions, plus eleven per-prompt fields.
	"smart-prompts": { dynamic: 3, orphans: 11 },
	// The per-agent "Native status signals", "Prevent alternate screen", and
	// "Accept workspace trust for managed spawns" toggles sit in runtime-rendered
	// cards and have no static scroll target; their static captions are orphans. Workspace trust appears only in the
	// expanded Claude and Codex cards, so a global search result could not open
	// the right card or scroll to its control.
	// The machine selector is indexed under the page header. The
	// idle-close control sits inside a collapsed, per-agent card: search cannot
	// identify which card to expand or scroll to its hidden control. Neither has
	// a stable search target.
	// Named run editors add six persistent field labels, but search cannot open their runtime forms.
	// The per-agent "Launch instructions for <agent>" editor names its agent at
	// runtime, inside a collapsed card; the global editor is indexed.
	agents: { dynamic: 2, orphans: 18 },
	// The `<optgroup label={provider.name}>` inside the default-model picker. It
	// groups the options by provider and is not a setting anybody can scroll to.
	"ai-chat": { dynamic: 1, orphans: 0 },
};

const readTab = (file: string) => fs.readFileSync(path.join(__dirname, "..", file), "utf8");

/** One page's sources, concatenated in render order. */
const readPage = (tab: string) => TAB_SOURCES[tab].map(readTab).join("\n");

/** Rebuild the index for one tab straight from its JSX, in file order.
 *
 * An entry identical to an earlier one is dropped: an add form and an edit
 * form both label a "Timeout (s):" field, and one result scrolls to both. */
function derive(tab: string): SettingsSearchEntry[] {
	const extracted = extractTab(readPage(tab));
	const entries: SettingsSearchEntry[] = [];
	const seen = new Set<string>();
	const push = (entry: SettingsSearchEntry) => {
		const id = JSON.stringify(entry);
		if (seen.has(id)) return;
		seen.add(id);
		entries.push(entry);
	};
	for (const section of extracted.sections) {
		push({
			tab,
			section: section.text,
			...(section.key ? { sectionKey: section.key } : {}),
			...(section.platform ? { platform: section.platform } : {}),
		});
	}
	for (const setting of extracted.settings) {
		if (!setting.section) continue;
		push({
			tab,
			section: setting.section,
			label: setting.text,
			...(setting.key ? { labelKey: setting.key } : {}),
			...(setting.configKey ? { expert: true, configKey: setting.configKey } : {}),
			...(setting.platform ? { platform: setting.platform } : {}),
		});
	}
	return entries;
}

/** Source file of every component `SettingsPanel` imports from `./tabs` or
 * from a sibling module, keyed by component name. */
function componentFiles(): Map<string, string> {
	const files = new Map<string, string>();
	for (const m of readTab("tabs/index.ts").matchAll(/export \{ (\w+) \} from "\.\/([^"]+)"/g)) {
		files.set(m[1], `tabs/${m[2]}.tsx`);
	}
	for (const m of readTab("SettingsPanel.tsx").matchAll(/import \{ (\w+) \} from "\.\/(\w+)"/g)) {
		files.set(m[1], `${m[2]}.tsx`);
	}
	return files;
}

describe("settings search index — drift guard", () => {
	it("indexes every tab SettingsPanel can open", () => {
		const rendered = extractRenderedTabKeys(readTab("SettingsPanel.tsx")).sort();
		expect(rendered).toEqual(Object.keys({ ...TAB_SOURCES, ...DYNAMIC_TAB_SOURCES }).sort());
	});

	it.each(Object.keys({ ...TAB_SOURCES, ...DYNAMIC_TAB_SOURCES }))(
		"extracts %s from every component the panel renders for it",
		(tab) => {
			const files = componentFiles();
			const rendered = extractRenderedTabComponents(readTab("SettingsPanel.tsx"))[tab] ?? [];
			expect(rendered.length).toBeGreaterThan(0);
			for (const component of rendered) {
				expect({ ...TAB_SOURCES, ...DYNAMIC_TAB_SOURCES }[tab], component).toContain(files.get(component));
			}
		},
	);

	it.each(Object.keys(TAB_SOURCES))("matches the settings rendered by %s", (tab) => {
		const indexed = SETTINGS_SEARCH_INDEX.filter((entry) => entry.tab === tab);
		expect(indexed).toEqual(derive(tab));
	});

	it.each(Object.keys(TAB_SOURCES))("has no new unindexable occurrence in %s", (tab) => {
		const extracted = extractTab(readPage(tab));
		expect({
			dynamic: extracted.dynamic,
			orphans: extracted.settings.filter((s) => !s.section).length,
		}).toEqual(UNINDEXABLE[tab]);
	});

	it("keeps per-agent captions out of search and indexes the page-level machine selector", () => {
		const orphans = extractTab(readPage("agents")).settings.filter((setting) => !setting.section);
		expect(orphans.map((setting) => setting.text)).toEqual([
			"Arguments",
			"Configuration name",
			"Command (binary)",
			"Model (optional)",
			"Configuration name",
			"Command (binary)",
			"Model (optional)",
			"Usage Dashboard",
			"Close idle managed child after",
			"Enabled",
			"Auto-retry on server errors",
			"Prevent alternate screen",
			"Accept workspace trust for managed spawns",
			"Native status signals",
			"Install hooks globally",
			"Track agent intent",
			"Collect progress",
			"Show suggested follow-ups",
		]);
		expect(SETTINGS_SEARCH_INDEX).toContainEqual({
			tab: "agents",
			section: "Terminal agents",
			label: "Configure agents on",
		});
	});

	it("indexes no tab the panel cannot open", () => {
		const tabs = new Set(SETTINGS_SEARCH_INDEX.map((entry) => entry.tab));
		expect([...tabs].sort()).toEqual(Object.keys(TAB_SOURCES).sort());
	});
});

const ALL_TABS = new Set(Object.keys(TAB_SOURCES));

describe("searchSettings", () => {
	it("returns nothing for an empty query", () => {
		expect(searchSettings("", ALL_TABS, "desktop")).toEqual([]);
		expect(searchSettings("   ", ALL_TABS, "desktop")).toEqual([]);
	});

	it("finds a setting in a tab that is not mounted", () => {
		const hits = searchSettings("relay server url", ALL_TABS, "desktop");
		expect(hits).toEqual([
			expect.objectContaining({ tab: "remote-access", section: "Cloud Relay", label: "Relay Server URL" }),
		]);
	});

	it.each(["Permissions", "Filesystem sandbox"])("does not lose the ego %s control from Settings search", (label) => {
		for (const client of ["desktop", "browser"] as const) {
			expect(searchSettings(label, ALL_TABS, client)).toContainEqual({
				tab: "agents",
				section: "ego permissions",
				label,
			});
		}
	});

	it("matches case-insensitively on any word order", () => {
		const hits = searchSettings("THEME terminal", ALL_TABS, "desktop");
		expect(hits).toEqual([expect.objectContaining({ tab: "terminal", section: "Theme", label: "Terminal Theme" })]);
	});

	it("matches a section heading, and the settings inside it", () => {
		const hits = searchSettings("power management", ALL_TABS, "desktop");
		// The section entry first, then every setting it contains — searching a
		// heading is how you browse a section you cannot name a field in.
		expect(hits[0]).toEqual(expect.objectContaining({ tab: "general", section: "Power Management" }));
		expect(hits[0].label).toBeUndefined();
		expect(hits.map((e) => e.label)).toEqual([
			undefined,
			"Prevent sleep when busy",
			"Auto-Standby Timeout",
			"Content Indexing",
		]);
	});

	it("spans tabs — one query, several tabs", () => {
		const tabs = new Set(searchSettings("default", ALL_TABS, "desktop").map((e) => e.tab));
		expect(tabs.size).toBeGreaterThan(1);
	});

	it("hides settings whose tab is not available", () => {
		expect(searchSettings("whisper model", ALL_TABS, "desktop")).toHaveLength(1);
		const withoutDictation = new Set([...ALL_TABS].filter((k) => k !== "dictation"));
		expect(searchSettings("whisper model", withoutDictation, "desktop")).toEqual([]);
	});

	it("offers a desktop-only control on the desktop and not in a browser", () => {
		expect(searchSettings("global hotkey", ALL_TABS, "desktop")).toEqual([
			expect.objectContaining({ tab: "keyboard-shortcuts", section: "Global Hotkey", platform: "desktop" }),
			expect.objectContaining({ tab: "keyboard-shortcuts", label: "Global Hotkey (Toggle Window)" }),
		]);
		expect(searchSettings("global hotkey", ALL_TABS, "browser")).toEqual([]);
	});

	it("offers each client the control it renders when both differ", () => {
		// Desktop picks the ego binary through a native dialog; a browser types
		// the path. Both are the "ego executable" control, rendered differently.
		const desktop = searchSettings("ego executable", ALL_TABS, "desktop");
		const browser = searchSettings("ego executable", ALL_TABS, "browser");
		expect(desktop).toEqual([expect.objectContaining({ tab: "general", platform: "desktop" })]);
		expect(browser).toEqual([expect.objectContaining({ tab: "general", platform: "browser" })]);
	});

	it("does not offer a section a browser never renders", () => {
		expect(searchSettings("tuic cli", ALL_TABS, "desktop")).not.toEqual([]);
		expect(searchSettings("tuic cli", ALL_TABS, "browser")).toEqual([]);
	});

	it("returns nothing for a query that matches no setting", () => {
		expect(searchSettings("zzzzz no such setting", ALL_TABS, "desktop")).toEqual([]);
	});
});
