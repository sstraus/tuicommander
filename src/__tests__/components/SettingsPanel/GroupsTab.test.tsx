import { fireEvent, render } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import "../../mocks/tauri";

const { mockCreateGroup, mockDeleteGroup, mockRenameGroup, mockSetGroupColor } = vi.hoisted(() => ({
	mockCreateGroup: vi.fn(() => "new-id"),
	mockDeleteGroup: vi.fn(),
	mockRenameGroup: vi.fn(() => true),
	mockSetGroupColor: vi.fn(),
}));

vi.mock("../../../stores/repositories", () => ({
	repositoriesStore: {
		state: {
			groups: {} as Record<string, unknown>,
			groupOrder: [] as string[],
		},
		createGroup: mockCreateGroup,
		deleteGroup: mockDeleteGroup,
		renameGroup: mockRenameGroup,
		setGroupColor: mockSetGroupColor,
	},
}));

vi.mock("../../../stores/settings", () => ({
	settingsStore: {
		state: {
			theme: "vscode-dark",
			font: "JetBrains Mono",
			defaultFontSize: 14,
			splitTabMode: "separate",
			maxTabNameLength: 25,
		},
		setTheme: vi.fn(),
		setFont: vi.fn(),
		setDefaultFontSize: vi.fn(),
		setSplitTabMode: vi.fn(),
		setMaxTabNameLength: vi.fn(),
	},
	FONT_FAMILIES: { "JetBrains Mono": "JetBrains Mono, monospace" },
}));

vi.mock("../../../stores/ui", () => ({
	uiStore: {
		state: {},
		resetLayout: vi.fn(),
	},
}));

vi.mock("../../../themes", () => ({
	getThemeNames: () => ({ "vscode-dark": "VS Code Dark" }),
}));

import { AppearanceTab } from "../../../components/SettingsPanel/tabs/AppearanceTab";
import { repositoriesStore } from "../../../stores/repositories";

/** Helper to set groups on the mock store */
function setGroups(groups: Record<string, unknown>, groupOrder: string[]) {
	(repositoriesStore.state as { groups: Record<string, unknown> }).groups = groups;
	(repositoriesStore.state as { groupOrder: string[] }).groupOrder = groupOrder;
}

describe("AppearanceTab — Repository Groups", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		setGroups({}, []);
	});

	it("renders empty state message when no groups", () => {
		const { container } = render(() => <AppearanceTab />);
		const empty = container.querySelector(".groupsEmpty");
		expect(empty).not.toBeNull();
		expect(empty!.textContent).toContain("No groups yet");
	});

	it("renders list of existing groups", () => {
		setGroups(
			{
				g1: { id: "g1", name: "Work", color: "#4A9EFF", collapsed: false, repoOrder: [] },
				g2: { id: "g2", name: "Personal", color: "", collapsed: false, repoOrder: [] },
			},
			["g1", "g2"],
		);
		const { container } = render(() => <AppearanceTab />);
		const items = container.querySelectorAll(".groupItem");
		expect(items.length).toBe(2);
	});

	it("Add Group button creates new group", () => {
		const { container } = render(() => <AppearanceTab />);
		const addBtn = container.querySelector(".groupsAddBtn")!;
		fireEvent.click(addBtn);
		expect(mockCreateGroup).toHaveBeenCalledWith("New Group");
	});

	it("delete button removes group", () => {
		setGroups({ g1: { id: "g1", name: "Work", color: "", collapsed: false, repoOrder: [] } }, ["g1"]);
		const { container } = render(() => <AppearanceTab />);
		const deleteBtn = container.querySelector(".groupDeleteBtn")!;
		fireEvent.click(deleteBtn);
		expect(mockDeleteGroup).toHaveBeenCalledWith("g1");
	});

	it("color picker shows presets", () => {
		setGroups({ g1: { id: "g1", name: "Work", color: "", collapsed: false, repoOrder: [] } }, ["g1"]);
		const { container } = render(() => <AppearanceTab />);
		const swatches = container.querySelectorAll('[data-form-control="swatch"], [data-form-control="color"]');
		// 8 presets + 1 custom + 1 clear
		expect(swatches.length).toBe(10);
	});

	it("clicking color swatch calls setGroupColor", () => {
		setGroups({ g1: { id: "g1", name: "Work", color: "", collapsed: false, repoOrder: [] } }, ["g1"]);
		const { container } = render(() => <AppearanceTab />);
		const firstSwatch = container.querySelector('[data-form-control="swatch"]:not([data-clear])')!;
		fireEvent.click(firstSwatch);
		expect(mockSetGroupColor).toHaveBeenCalledWith("g1", expect.any(String));
	});
	// Catches: a failed rename closes the editor or discards the rejected draft after the card migration.
	it("keeps an invalid group rename editable and Escape restores the saved name", () => {
		setGroups({ g1: { id: "g1", name: "Work", color: "", collapsed: false, repoOrder: [] } }, ["g1"]);
		const view = render(() => <AppearanceTab />);
		fireEvent.dblClick(view.getByText("Work"));
		const input = view.getByRole("textbox", { name: "Group name: Work" });
		fireEvent.input(input, { target: { value: "   " } });
		fireEvent.keyDown(input, { key: "Enter" });
		expect(mockRenameGroup).not.toHaveBeenCalled();
		expect(input).toHaveValue("   ");
		expect(input).toHaveAttribute("aria-invalid", "true");
		fireEvent.input(input, { target: { value: "Personal" } });
		mockRenameGroup.mockReturnValueOnce(false);
		fireEvent.keyDown(input, { key: "Enter" });
		expect(input).toHaveValue("Personal");
		expect(view.getByText("A group with this name already exists")).toBeInTheDocument();
		fireEvent.keyDown(input, { key: "Escape" });
		expect(view.queryByRole("textbox", { name: "Group name: Work" })).toBeNull();
		expect(view.getByText("Work")).toBeInTheDocument();
	});
});
