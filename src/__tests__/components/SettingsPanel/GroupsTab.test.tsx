import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@solidjs/testing-library";
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
  THEME_NAMES: { "vscode-dark": "VS Code Dark" },
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
    setGroups(
      { g1: { id: "g1", name: "Work", color: "", collapsed: false, repoOrder: [] } },
      ["g1"],
    );
    const { container } = render(() => <AppearanceTab />);
    const deleteBtn = container.querySelector(".groupDeleteBtn")!;
    fireEvent.click(deleteBtn);
    expect(mockDeleteGroup).toHaveBeenCalledWith("g1");
  });

  it("color picker shows presets", () => {
    setGroups(
      { g1: { id: "g1", name: "Work", color: "", collapsed: false, repoOrder: [] } },
      ["g1"],
    );
    const { container } = render(() => <AppearanceTab />);
    const swatches = container.querySelectorAll(".colorSwatch");
    // 8 presets + 1 clear option
    expect(swatches.length).toBeGreaterThanOrEqual(5);
  });

  it("clicking color swatch calls setGroupColor", () => {
    setGroups(
      { g1: { id: "g1", name: "Work", color: "", collapsed: false, repoOrder: [] } },
      ["g1"],
    );
    const { container } = render(() => <AppearanceTab />);
    const firstSwatch = container.querySelector(".colorSwatch")!;
    fireEvent.click(firstSwatch);
    expect(mockSetGroupColor).toHaveBeenCalledWith("g1", expect.any(String));
  });
});
