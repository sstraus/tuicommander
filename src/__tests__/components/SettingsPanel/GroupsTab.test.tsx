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

import { GroupsTab } from "../../../components/SettingsPanel/tabs/GroupsTab";
import { repositoriesStore } from "../../../stores/repositories";

/** Helper to set groups on the mock store */
function setGroups(groups: Record<string, unknown>, groupOrder: string[]) {
  (repositoriesStore.state as { groups: Record<string, unknown> }).groups = groups;
  (repositoriesStore.state as { groupOrder: string[] }).groupOrder = groupOrder;
}

describe("GroupsTab", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setGroups({}, []);
  });

  it("renders empty state message when no groups", () => {
    const { container } = render(() => <GroupsTab />);
    const empty = container.querySelector(".groups-empty");
    expect(empty).not.toBeNull();
    expect(empty!.textContent).toContain("No groups");
  });

  it("renders list of existing groups", () => {
    setGroups(
      {
        g1: { id: "g1", name: "Work", color: "#4A9EFF", collapsed: false, repoOrder: [] },
        g2: { id: "g2", name: "Personal", color: "", collapsed: false, repoOrder: [] },
      },
      ["g1", "g2"],
    );
    const { container } = render(() => <GroupsTab />);
    const items = container.querySelectorAll(".group-settings-item");
    expect(items.length).toBe(2);
  });

  it("Add Group button creates new group", () => {
    const { container } = render(() => <GroupsTab />);
    const addBtn = container.querySelector(".groups-add-btn")!;
    fireEvent.click(addBtn);
    expect(mockCreateGroup).toHaveBeenCalledWith("New Group");
  });

  it("delete button removes group", () => {
    setGroups(
      { g1: { id: "g1", name: "Work", color: "", collapsed: false, repoOrder: [] } },
      ["g1"],
    );
    const { container } = render(() => <GroupsTab />);
    const deleteBtn = container.querySelector(".group-delete-btn")!;
    fireEvent.click(deleteBtn);
    expect(mockDeleteGroup).toHaveBeenCalledWith("g1");
  });

  it("color picker shows 5 presets", () => {
    setGroups(
      { g1: { id: "g1", name: "Work", color: "", collapsed: false, repoOrder: [] } },
      ["g1"],
    );
    const { container } = render(() => <GroupsTab />);
    const swatches = container.querySelectorAll(".color-swatch");
    // 5 presets + 1 clear option
    expect(swatches.length).toBeGreaterThanOrEqual(5);
  });

  it("clicking color swatch calls setGroupColor", () => {
    setGroups(
      { g1: { id: "g1", name: "Work", color: "", collapsed: false, repoOrder: [] } },
      ["g1"],
    );
    const { container } = render(() => <GroupsTab />);
    const firstSwatch = container.querySelector(".color-swatch")!;
    fireEvent.click(firstSwatch);
    expect(mockSetGroupColor).toHaveBeenCalledWith("g1", expect.any(String));
  });
});
