import { beforeEach, describe, expect, it, vi } from "vitest";
import "../mocks/tauri";
import { fireEvent, render } from "@solidjs/testing-library";

vi.mock("../../stores/settings", () => ({
	settingsStore: {
		state: { ide: "vscode", font: "JetBrains Mono", defaultFontSize: 12, customLaunchers: [] },
		setIde: vi.fn(),
		setFont: vi.fn(),
		isAiChatEnabled: vi.fn().mockReturnValue(false),
		isAcpConfigured: vi.fn().mockReturnValue(false),
	},
	IDE_NAMES: { vscode: "VS Code" },
	FONT_FAMILIES: { "JetBrains Mono": "JetBrains Mono" },
}));

vi.mock("../../stores/notifications", () => ({
	notificationsStore: {
		state: {
			isAvailable: true,
			config: { enabled: true, volume: 0.5, sounds: { question: true, error: true, completion: true, warning: true } },
		},
		setEnabled: vi.fn(),
		setVolume: vi.fn(),
		setSoundEnabled: vi.fn(),
		testSound: vi.fn(),
		reset: vi.fn(),
	},
}));

vi.mock("../../stores/ui", () => ({
	uiStore: {
		state: { settingsNavWidth: 180 },
		setSettingsNavWidth: vi.fn(),
	},
}));

vi.mock("../../stores/repositories", () => {
	const repositories = {
		"/repo": { path: "/repo", displayName: "my-repo" },
	};
	const repoOrder = ["/repo"];
	return {
		repositoriesStore: {
			state: { repositories, repoOrder },
			setDisplayName: vi.fn(),
			getGroupForRepo: vi.fn(() => undefined),
			getAllReposOrdered: vi.fn(() => repoOrder.map((p) => repositories[p as keyof typeof repositories])),
		},
	};
});

vi.mock("../../stores/repoSettings", () => {
	const mockReset = vi.fn();
	const mockUpdate = vi.fn();
	const mockGet = vi.fn(() => undefined);
	const mockGetOrCreate = vi.fn().mockReturnValue({
		path: "/repo",
		displayName: "my-repo",
		baseBranch: "automatic",
		copyIgnoredFiles: false,
		copyUntrackedFiles: false,
		setupScript: "",
		runScript: "",
		color: "",
	});

	return {
		repoSettingsStore: {
			get: mockGet,
			getOrCreate: mockGetOrCreate,
			update: mockUpdate,
			reset: mockReset,
		},
	};
});

import { SettingsPanel } from "../../components/SettingsPanel/SettingsPanel";
import type { RepoSettings } from "../../stores/repoSettings";
import { repoSettingsStore } from "../../stores/repoSettings";

describe("SettingsPanel — repo context", () => {
	const repoContext = { kind: "repo" as const, repoPath: "/repo" };

	const defaultProps = {
		visible: true,
		onClose: vi.fn(),
		context: repoContext,
	};

	beforeEach(() => {
		vi.clearAllMocks();
		const mockSettings: RepoSettings = {
			path: "/repo",
			displayName: "my-repo",
			autoConsolidateWorktrees: false,
			devServerUrl: null,
			baseBranch: "automatic",
			copyIgnoredFiles: false,
			copyUntrackedFiles: false,
			setupScript: "",
			runScript: "",
			archiveScript: "",
			color: "",
			terminalMetaHotkeys: null,
			worktreeStorage: null,
			promptOnCreate: null,
			deleteBranchOnRemove: null,
			autoArchiveMerged: null,
			orphanCleanup: null,
			prMergeStrategy: null,
			afterMerge: null,
			autoFetchIntervalMinutes: null,
			autoDeleteOnPrClose: null,
			mcpUpstreams: null,
			prHideDrafts: null,
			prHideConflicting: null,
			prHideCiFailing: null,
			branchLabels: {},
		};
		vi.mocked(repoSettingsStore.getOrCreate).mockReturnValue(mockSettings);
	});

	it("shows Settings title in header (unified panel)", () => {
		const { container } = render(() => <SettingsPanel {...defaultProps} />);
		const heading = container.querySelector(".header h2");
		expect(heading!.textContent).toBe("Settings");
	});

	it("opens with repo nav item active", () => {
		const { container } = render(() => <SettingsPanel {...defaultProps} />);
		const activeItem = container.querySelector(".navItem.active");
		expect(activeItem!.classList.contains("navItemRepo")).toBe(true);
		expect(activeItem!.textContent).toBe("my-repo");
	});

	it("keeps sidebar swatches in the shared control column and preserves color updates", () => {
		// Catches: the custom color label makes the legacy grid place swatches before the row label.
		const { container } = render(() => <SettingsPanel {...defaultProps} />);
		const label = Array.from(container.querySelectorAll("label")).find((item) => item.textContent === "Sidebar Color")!;
		const row = label.closest(".settingRow");
		expect(row).not.toBeNull();
		expect(row!.querySelector(".settingRowText")?.textContent).toContain("Color-code this repo in the sidebar");
		const swatch = row!.querySelector<HTMLButtonElement>(".settingRowControl button[title='Red']")!;
		fireEvent.click(swatch);
		expect(repoSettingsStore.update).toHaveBeenLastCalledWith("/repo", { color: "#FF6B6B" });
	});

	it("shows a repo-specific Design Mode URL field", () => {
		const { container } = render(() => <SettingsPanel {...defaultProps} />);
		const field = container.querySelector<HTMLInputElement>("#repo-dev-server-url");
		expect(field).not.toBeNull();
		expect(field?.type).toBe("url");
	});

	it("shows repo settings content when repo nav item is active", () => {
		const { container } = render(() => <SettingsPanel {...defaultProps} />);
		// Section cards retain the repository headings.
		const headings = Array.from(container.querySelectorAll(".section h3")).map((h) => h.textContent);
		expect(headings).toContain("Repository");
		// RepoScriptsTab renders h3 "Automation Scripts"
		expect(headings).toContain("Automation Scripts");
	});

	it("Done button calls onClose", () => {
		const onClose = vi.fn();
		const { container } = render(() => <SettingsPanel {...defaultProps} onClose={onClose} />);
		const doneBtn = container.querySelector(".footerDone")!;
		fireEvent.click(doneBtn);
		expect(onClose).toHaveBeenCalledOnce();
	});

	it("reset button calls repoSettingsStore.reset with correct path", () => {
		const { container } = render(() => <SettingsPanel {...defaultProps} />);
		const resetBtn = container.querySelector(".footerReset")!;
		expect(resetBtn).not.toBeNull();
		fireEvent.click(resetBtn);
		expect(repoSettingsStore.reset).toHaveBeenCalledWith("/repo");
	});

	// Catches: replacing the consolidation checkbox drops its label, checked value or repository owner callback.
	it("keeps consolidation as a labelled repository switch", () => {
		const { getByRole } = render(() => <SettingsPanel {...defaultProps} />);
		const control = getByRole("switch", {
			name: "Show all worktrees of this repo in one consolidated screen",
		}) as HTMLInputElement;
		expect(control.checked).toBe(false);
		fireEvent.click(control);
		expect(repoSettingsStore.update).toHaveBeenLastCalledWith("/repo", { autoConsolidateWorktrees: true });
	});

	it("keeps explicit disabled fetch distinct from inherited defaults", () => {
		// Catches coercing the nullable sentinel or explicit zero into the same override.
		const { container } = render(() => <SettingsPanel {...defaultProps} />);
		const label = [...container.querySelectorAll("label")].find((e) => e.textContent === "Auto-Fetch Interval")!;
		const select = container.querySelector(`#${label.htmlFor}`)!;
		fireEvent.change(select, { target: { value: "0" } });
		expect(repoSettingsStore.update).toHaveBeenLastCalledWith("/repo", { autoFetchIntervalMinutes: 0 });
		fireEvent.change(select, { target: { value: "__inherit__" } });
		expect(repoSettingsStore.update).toHaveBeenLastCalledWith("/repo", { autoFetchIntervalMinutes: null });
	});
	it("clears a script override to inheritance without exporting it", () => {
		// Catches replacing the textarea without preserving its empty-to-null update boundary.
		const { container } = render(() => <SettingsPanel {...defaultProps} />);
		const input = container.querySelector("#repo-run-script")!;
		fireEvent.input(input, { target: { value: "pnpm dev" } });
		expect(repoSettingsStore.update).toHaveBeenLastCalledWith("/repo", { runScript: "pnpm dev" });
		fireEvent.input(input, { target: { value: "" } });
		expect(repoSettingsStore.update).toHaveBeenLastCalledWith("/repo", { runScript: null });
		expect(container.textContent).toContain("Scripts are never exported.");
	});

	it("keeps inherited boolean overrides distinct from explicit on and off", () => {
		// Catches flattening nullable overrides into a two-state checkbox.
		const { container } = render(() => <SettingsPanel {...defaultProps} />);
		const label = [...container.querySelectorAll("label")].find((e) => e.textContent === "Copy ignored files")!;
		const control = container.querySelector<HTMLSelectElement>(`#${label.htmlFor}`)!;
		for (const [value, expected] of [
			["true", true],
			["false", false],
			["__inherit__", null],
		] as const) {
			fireEvent.change(control, { target: { value } });
			expect(repoSettingsStore.update).toHaveBeenLastCalledWith("/repo", { copyIgnoredFiles: expected });
		}
	});

	it("clicking a global nav item switches to global content", () => {
		const { container } = render(() => <SettingsPanel {...defaultProps} />);
		const generalItem = Array.from(container.querySelectorAll(".navItem")).find((n) => n.textContent === "General")!;
		fireEvent.click(generalItem);
		const headings = Array.from(container.querySelectorAll(".section h3")).map((heading) => heading.textContent);
		expect(headings).toContain("Local tools");
		expect(headings).toContain("General");
		// Reset button gone when on global section
		expect(container.querySelector(".footerReset")).toBeNull();
	});
});
