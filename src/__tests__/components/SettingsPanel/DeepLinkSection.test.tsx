import { fireEvent, render } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { beforeEach, describe, expect, it, vi } from "vitest";
import "../../mocks/tauri";

// Deep-linking to `settings-upstream-mcp` opens the MCP tab, which mounts
// `LocalMcpPanel` and `UpstreamMcpPanel` — both call several `rpc()` commands
// on mount. Stub the transport instead of each resource: the fetches are
// incidental to the scroll assertions, but load_config still needs the real
// config shape so the mounted MCP page can adopt it without a reload failure,
// and get_mcp_usage needs the report object the usage panel reads `.roles` from.
vi.mock("../../../transport", async (importOriginal) => {
	const { default: defaults } = await import("../../fixtures/settings-concurrency/app-defaults.json");
	const config = {
		...defaults,
		// Current McpCapabilities::default(); the recorded snapshot predates this field.
		mcp_capabilities: { denied: ["settings", "diagnostics"], migration_unknown_tools: [] },
	};
	return {
		...(await importOriginal<typeof import("../../../transport")>()),
		rpc: vi.fn(async (command: string) => {
			if (command === "load_config") return structuredClone(config);
			if (command === "get_mcp_usage") return { enabled: true, days: 7, roles: [] };
			return [];
		}),
	};
});

vi.mock("../../../stores/settings", () => ({
	settingsStore: {
		state: { ide: "vscode", font: "JetBrains Mono", defaultFontSize: 12, customLaunchers: [] },
		isAiChatEnabled: () => false,
		isAcpConfigured: () => false,
	},
	IDE_NAMES: { vscode: "VS Code" },
	FONT_FAMILIES: { "JetBrains Mono": "JetBrains Mono" },
}));

vi.mock("../../../stores/ui", () => ({
	uiStore: {
		state: { settingsNavWidth: 180 },
		setSettingsNavWidth: vi.fn(),
		persistUIPrefs: vi.fn(),
	},
}));

vi.mock("../../../stores/repositories", () => ({
	repositoriesStore: {
		state: { repositories: {}, repoOrder: [] },
		getAllReposOrdered: () => [],
		getConnectionId: () => undefined,
		setDisplayName: vi.fn(),
	},
}));

import { SettingsPanel } from "../../../components/SettingsPanel/SettingsPanel";
import { SETTINGS_SECTION_UPSTREAM_MCP } from "../../../components/SettingsPanel/sections";
import { appLogger } from "../../../stores/appLogger";
import { rpc } from "../../../transport";

/** Run the frame the scroll effect schedules. */
const nextFrame = () => new Promise((resolve) => requestAnimationFrame(() => setTimeout(resolve, 0)));

describe("SettingsPanel — deep link to a section", () => {
	let scrolled: Element[];

	beforeEach(() => {
		vi.clearAllMocks();
		scrolled = [];
		// jsdom has no layout, so scrollIntoView does not exist.
		Element.prototype.scrollIntoView = function scrollIntoView(this: Element) {
			scrolled.push(this);
		};
	});

	// Catches: an array-shaped load_config stub makes the mounted MCP settings reload fail.
	it("does not report a config reload failure when the MCP deep link mounts", async () => {
		const warn = vi.spyOn(appLogger, "warn").mockImplementation(() => {});
		try {
			render(() => (
				<SettingsPanel visible onClose={() => {}} initialTab="mcp" initialSection={SETTINGS_SECTION_UPSTREAM_MCP} />
			));
			await nextFrame();
			expect(rpc).toHaveBeenCalledWith("load_config");
			expect(warn).not.toHaveBeenCalled();
		} finally {
			warn.mockRestore();
		}
	});

	it("scrolls the upstream MCP block into view when asked for it", async () => {
		const { container } = render(() => (
			<SettingsPanel
				visible={true}
				onClose={() => {}}
				initialTab="mcp"
				initialSection={SETTINGS_SECTION_UPSTREAM_MCP}
			/>
		));

		const block = container.querySelector(`#${SETTINGS_SECTION_UPSTREAM_MCP}`);
		expect(block, "upstream MCP block should carry the anchor id").toBeTruthy();

		await nextFrame();
		expect(scrolled).toEqual([block]);
	});

	it("leaves the tab at the top when no section is requested", async () => {
		render(() => <SettingsPanel visible={true} onClose={() => {}} initialTab="mcp" />);
		await nextFrame();
		expect(scrolled).toEqual([]);
	});

	it("does not scroll while the panel is closed", async () => {
		render(() => (
			<SettingsPanel
				visible={false}
				onClose={() => {}}
				initialTab="mcp"
				initialSection={SETTINGS_SECTION_UPSTREAM_MCP}
			/>
		));
		await nextFrame();
		expect(scrolled).toEqual([]);
	});

	it("reopens on the requested tab and section after the user moved elsewhere", async () => {
		const [visible, setVisible] = createSignal(true);
		const { container } = render(() => (
			<SettingsPanel
				visible={visible()}
				onClose={() => {}}
				initialTab="mcp"
				initialSection={SETTINGS_SECTION_UPSTREAM_MCP}
			/>
		));
		await nextFrame();
		const general = Array.from(container.querySelectorAll(".navItem")).find((n) => n.textContent === "General")!;
		fireEvent.click(general);
		expect(container.querySelector(".navItem.active")!.textContent).toBe("General");

		setVisible(false);
		scrolled = [];
		setVisible(true);
		await nextFrame();

		expect(container.querySelector(".navItem.active")!.textContent).toBe("MCP");
		expect(scrolled).toEqual([container.querySelector(`#${SETTINGS_SECTION_UPSTREAM_MCP}`)]);
	});

	// Catches: retired Services links select MCP but lose the upstream section anchor.
	it("retains the section target when opening the retired Services tab", async () => {
		const view = render(() => (
			<SettingsPanel visible onClose={() => {}} initialTab="services" initialSection={SETTINGS_SECTION_UPSTREAM_MCP} />
		));
		await nextFrame();
		expect(view.container.querySelector(".navItem.active")?.textContent).toBe("MCP");
		expect(scrolled).toEqual([view.container.querySelector(`#${SETTINGS_SECTION_UPSTREAM_MCP}`)]);
	});

	// Catches: a scheduled deep-link scroll runs after Settings closes.
	it("cancels the pending section scroll when Settings closes before the frame", async () => {
		const [visible, setVisible] = createSignal(true);
		render(() => (
			<SettingsPanel
				visible={visible()}
				onClose={() => {}}
				initialTab="mcp"
				initialSection={SETTINGS_SECTION_UPSTREAM_MCP}
			/>
		));
		setVisible(false);
		await nextFrame();
		expect(scrolled).toEqual([]);
	});

	// Catches: a removed or unsupported section scrolls to an unrelated control.
	it("leaves the selected page usable when the requested section is absent", async () => {
		const view = render(() => (
			<SettingsPanel visible onClose={() => {}} initialTab="mcp" initialSection="missing-section" />
		));
		await nextFrame();
		expect(scrolled).toEqual([]);
		expect(view.container.querySelector(".navItem.active")?.textContent).toBe("MCP");
		expect(view.getByRole("button", { name: "Done" })).toBeVisible();
	});

	it("gives MCP its own direct nav entry, findable without going through Services", () => {
		const { container } = render(() => <SettingsPanel visible={true} onClose={() => {}} />);
		const labels = Array.from(container.querySelectorAll(".navItem")).map((n) => n.textContent);
		expect(labels).toContain("MCP");
	});
});
