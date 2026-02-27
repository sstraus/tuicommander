import { describe, it, expect, vi, beforeEach } from "vitest";
import "../../mocks/tauri";
import { render, fireEvent } from "@solidjs/testing-library";

const {
  mockSetBaseBranch,
  mockSetCopyIgnoredFiles,
  mockSetCopyUntrackedFiles,
  mockSetSetupScript,
  mockSetRunScript,
  mockSetAutoFetchIntervalMinutes,
  mockSetAutoDeleteOnPrClose,
} = vi.hoisted(() => ({
  mockSetBaseBranch: vi.fn(),
  mockSetCopyIgnoredFiles: vi.fn(),
  mockSetCopyUntrackedFiles: vi.fn(),
  mockSetSetupScript: vi.fn(),
  mockSetRunScript: vi.fn(),
  mockSetAutoFetchIntervalMinutes: vi.fn(),
  mockSetAutoDeleteOnPrClose: vi.fn(),
}));

vi.mock("../../../stores/settings", () => ({
  settingsStore: {
    state: {
      ide: "vscode",
      font: "JetBrains Mono",
      defaultFontSize: 12,
      shell: "",
      splitTabMode: "separate",
      confirmBeforeQuit: true,
      confirmBeforeClosingTab: true,
      preventSleepWhenBusy: false,
      autoUpdateEnabled: true,
      autoShowPrPopover: false,
      theme: "dark",
    },
    setIde: vi.fn(),
    setFont: vi.fn(),
    setShell: vi.fn(),
    setSplitTabMode: vi.fn(),
    setConfirmBeforeQuit: vi.fn(),
    setConfirmBeforeClosingTab: vi.fn(),
    setPreventSleepWhenBusy: vi.fn(),
    setAutoUpdateEnabled: vi.fn(),
    setAutoShowPrPopover: vi.fn(),
    setTheme: vi.fn(),
    setDefaultFontSize: vi.fn(),
  },
  IDE_NAMES: { vscode: "VS Code" },
  FONT_FAMILIES: { "JetBrains Mono": "JetBrains Mono" },
}));

vi.mock("../../../stores/updater", () => ({
  updaterStore: {
    state: { checking: false, downloading: false, available: false, version: null, error: null },
    checkForUpdate: vi.fn(),
  },
}));

vi.mock("../../../themes", () => ({
  THEME_NAMES: { dark: "Dark" },
}));

vi.mock("../../../stores/repoDefaults", () => ({
  repoDefaultsStore: {
    state: {
      baseBranch: "automatic",
      copyIgnoredFiles: false,
      copyUntrackedFiles: false,
      setupScript: "",
      runScript: "",
      worktreeStorage: "sibling",
      promptOnCreate: true,
      deleteBranchOnRemove: true,
      autoArchiveMerged: false,
      orphanCleanup: "ask",
      prMergeStrategy: "merge",
      afterMerge: "archive",
      autoFetchIntervalMinutes: 0,
      autoDeleteOnPrClose: "off",
    },
    setBaseBranch: mockSetBaseBranch,
    setCopyIgnoredFiles: mockSetCopyIgnoredFiles,
    setCopyUntrackedFiles: mockSetCopyUntrackedFiles,
    setSetupScript: mockSetSetupScript,
    setRunScript: mockSetRunScript,
    setWorktreeStorage: vi.fn(),
    setPromptOnCreate: vi.fn(),
    setDeleteBranchOnRemove: vi.fn(),
    setAutoArchiveMerged: vi.fn(),
    setOrphanCleanup: vi.fn(),
    setPrMergeStrategy: vi.fn(),
    setAfterMerge: vi.fn(),
    setAutoFetchIntervalMinutes: mockSetAutoFetchIntervalMinutes,
    setAutoDeleteOnPrClose: mockSetAutoDeleteOnPrClose,
  },
}));

import { GeneralTab } from "../../../components/SettingsPanel/tabs/GeneralTab";

describe("GeneralTab — Repository Defaults section", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders Repository Defaults heading", () => {
    const { container } = render(() => <GeneralTab />);
    const headings = Array.from(container.querySelectorAll("h3")).map(h => h.textContent);
    expect(headings).toContain("Repository Defaults");
  });

  it("shows baseBranch dropdown with current global default selected", () => {
    const { container } = render(() => <GeneralTab />);
    // The repo defaults select is the one with an "Automatic (origin/main" option
    const selects = Array.from(container.querySelectorAll("select")) as HTMLSelectElement[];
    const baseBranchSelect = selects.find(s =>
      Array.from(s.options).some(o => o.text.includes("Automatic"))
    );
    expect(baseBranchSelect).not.toBeUndefined();
    expect(baseBranchSelect!.value).toBe("automatic");
  });

  it("calls setBaseBranch when dropdown changes", () => {
    const { container } = render(() => <GeneralTab />);
    const selects = Array.from(container.querySelectorAll("select")) as HTMLSelectElement[];
    const baseBranchSelect = selects.find(s =>
      Array.from(s.options).some(o => o.text.includes("Automatic"))
    )!;
    fireEvent.change(baseBranchSelect, { target: { value: "main" } });
    expect(mockSetBaseBranch).toHaveBeenCalledWith("main");
  });

  it("shows copyIgnoredFiles and copyUntrackedFiles toggles", () => {
    const { container } = render(() => <GeneralTab />);
    // All checkboxes in the tab
    const checkboxes = container.querySelectorAll("input[type=checkbox]");
    expect(checkboxes.length).toBeGreaterThanOrEqual(4); // global booleans + existing settings
  });

  it("calls setCopyIgnoredFiles when toggle changes", () => {
    const { container } = render(() => <GeneralTab />);
    // The copyIgnoredFiles checkbox is the one that, when changed, calls mockSetCopyIgnoredFiles
    // Trigger change on ALL checkboxes and check that the right mock was called
    const checkboxes = Array.from(container.querySelectorAll("input[type=checkbox]")) as HTMLInputElement[];
    // Find the checkbox that triggers setCopyIgnoredFiles (in the repo defaults section, after the h3)
    const repoDefaultsH3 = Array.from(container.querySelectorAll("h3"))
      .find(h => h.textContent === "Repository Defaults")!;
    // Get checkboxes that are siblings of the h3 (appear after it in DOM order)
    const h3Index = checkboxes.findIndex(cb => {
      return repoDefaultsH3.compareDocumentPosition(cb) & Node.DOCUMENT_POSITION_FOLLOWING;
    });
    expect(h3Index).toBeGreaterThanOrEqual(0);
    fireEvent.change(checkboxes[h3Index], { target: { checked: true } });
    expect(mockSetCopyIgnoredFiles).toHaveBeenCalledWith(true);
  });

  it("shows setupScript and runScript textareas", () => {
    const { container } = render(() => <GeneralTab />);
    const textareas = container.querySelectorAll("textarea");
    expect(textareas.length).toBeGreaterThanOrEqual(2);
  });

  it("calls setSetupScript when first repo-defaults textarea changes", () => {
    const { container } = render(() => <GeneralTab />);
    const textareas = container.querySelectorAll("textarea");
    fireEvent.input(textareas[0], { target: { value: "npm install" } });
    expect(mockSetSetupScript).toHaveBeenCalledWith("npm install");
  });

  it("renders auto-fetch interval dropdown with Disabled selected", () => {
    const { container } = render(() => <GeneralTab />);
    const selects = Array.from(container.querySelectorAll("select")) as HTMLSelectElement[];
    const fetchSelect = selects.find(s =>
      Array.from(s.options).some(o => o.text === "Disabled")
    );
    expect(fetchSelect).toBeDefined();
    expect(fetchSelect!.value).toBe("0");
  });

  it("calls setAutoFetchIntervalMinutes when auto-fetch dropdown changes", () => {
    const { container } = render(() => <GeneralTab />);
    const selects = Array.from(container.querySelectorAll("select")) as HTMLSelectElement[];
    const fetchSelect = selects.find(s =>
      Array.from(s.options).some(o => o.text === "Disabled")
    )!;
    fireEvent.change(fetchSelect, { target: { value: "15" } });
    expect(mockSetAutoFetchIntervalMinutes).toHaveBeenCalledWith(15);
  });

  it("renders auto-delete on PR close dropdown with Off selected", () => {
    const { container } = render(() => <GeneralTab />);
    const selects = Array.from(container.querySelectorAll("select")) as HTMLSelectElement[];
    const deleteSelect = selects.find(s =>
      Array.from(s.options).some(o => o.text === "Ask before deleting")
    );
    expect(deleteSelect).toBeDefined();
    expect(deleteSelect!.value).toBe("off");
  });

  it("calls setAutoDeleteOnPrClose when auto-delete dropdown changes", () => {
    const { container } = render(() => <GeneralTab />);
    const selects = Array.from(container.querySelectorAll("select")) as HTMLSelectElement[];
    const deleteSelect = selects.find(s =>
      Array.from(s.options).some(o => o.text === "Ask before deleting")
    )!;
    fireEvent.change(deleteSelect, { target: { value: "ask" } });
    expect(mockSetAutoDeleteOnPrClose).toHaveBeenCalledWith("ask");
  });
});
