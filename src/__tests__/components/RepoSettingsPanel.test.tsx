import { describe, it, expect, vi, beforeEach } from "vitest";
import "../mocks/tauri";
import { render, fireEvent } from "@solidjs/testing-library";

vi.mock("../../stores/settings", () => ({
  settingsStore: {
    state: { ide: "vscode", font: "JetBrains Mono", defaultFontSize: 12 },
    setIde: vi.fn(),
    setFont: vi.fn(),
  },
  IDE_NAMES: { vscode: "VS Code" },
  FONT_FAMILIES: { "JetBrains Mono": "JetBrains Mono" },
}));

vi.mock("../../stores/notifications", () => ({
  notificationsStore: {
    state: { isAvailable: true, config: { enabled: true, volume: 0.5, sounds: { question: true, error: true, completion: true, warning: true } } },
    setEnabled: vi.fn(), setVolume: vi.fn(), setSoundEnabled: vi.fn(), testSound: vi.fn(), reset: vi.fn(),
  },
}));

vi.mock("../../stores/ui", () => ({
  uiStore: {
    state: { settingsNavWidth: 180 },
    setSettingsNavWidth: vi.fn(),
  },
}));

vi.mock("../../stores/repositories", () => ({
  repositoriesStore: {
    state: {
      repositories: {
        "/repo": { path: "/repo", displayName: "my-repo" },
      },
      repoOrder: ["/repo"],
    },
    setDisplayName: vi.fn(),
    getGroupForRepo: vi.fn(() => undefined),
  },
}));

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

import { repoSettingsStore } from "../../stores/repoSettings";
import type { RepoSettings } from "../../stores/repoSettings";
import { SettingsPanel } from "../../components/SettingsPanel/SettingsPanel";

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
      baseBranch: "automatic",
      copyIgnoredFiles: false,
      copyUntrackedFiles: false,
      setupScript: "",
      runScript: "",
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
    };
    vi.mocked(repoSettingsStore.getOrCreate).mockReturnValue(mockSettings);
  });

  it("shows Settings title in header (unified panel)", () => {
    const { container } = render(() => (
      <SettingsPanel {...defaultProps} />
    ));
    const heading = container.querySelector(".header h2");
    expect(heading!.textContent).toBe("Settings");
  });

  it("opens with repo nav item active", () => {
    const { container } = render(() => (
      <SettingsPanel {...defaultProps} />
    ));
    const activeItem = container.querySelector(".navItem.active");
    expect(activeItem!.classList.contains("navItemRepo")).toBe(true);
    expect(activeItem!.textContent).toBe("my-repo");
  });

  it("shows repo settings content when repo nav item is active", () => {
    const { container } = render(() => (
      <SettingsPanel {...defaultProps} />
    ));
    // RepoWorktreeTab renders h3 "Repository"
    const headings = Array.from(container.querySelectorAll(".section h3")).map(h => h.textContent);
    expect(headings).toContain("Repository");
    // RepoScriptsTab renders h3 "Automation Scripts"
    expect(headings).toContain("Automation Scripts");
  });

  it("Done button calls onClose", () => {
    const onClose = vi.fn();
    const { container } = render(() => (
      <SettingsPanel {...defaultProps} onClose={onClose} />
    ));
    const doneBtn = container.querySelector(".footerDone")!;
    fireEvent.click(doneBtn);
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("reset button calls repoSettingsStore.reset with correct path", () => {
    const { container } = render(() => (
      <SettingsPanel {...defaultProps} />
    ));
    const resetBtn = container.querySelector(".footerReset")!;
    expect(resetBtn).not.toBeNull();
    fireEvent.click(resetBtn);
    expect(repoSettingsStore.reset).toHaveBeenCalledWith("/repo");
  });

  it("clicking a global nav item switches to global content", () => {
    const { container } = render(() => (
      <SettingsPanel {...defaultProps} />
    ));
    const generalItem = Array.from(container.querySelectorAll(".navItem"))
      .find((n) => n.textContent === "General")!;
    fireEvent.click(generalItem);
    const h3 = container.querySelector(".section h3");
    expect(h3!.textContent).toBe("General");
    // Reset button gone when on global section
    expect(container.querySelector(".footerReset")).toBeNull();
  });
});
