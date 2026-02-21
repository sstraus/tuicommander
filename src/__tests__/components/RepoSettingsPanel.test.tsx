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
  },
}));

vi.mock("../../stores/repoSettings", () => {
  const mockReset = vi.fn();
  const mockUpdate = vi.fn();
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
    };
    vi.mocked(repoSettingsStore.getOrCreate).mockReturnValue(mockSettings);
  });

  it("shows Settings title in header (unified panel)", () => {
    const { container } = render(() => (
      <SettingsPanel {...defaultProps} />
    ));
    const heading = container.querySelector(".settings-header h2");
    expect(heading!.textContent).toBe("Settings");
  });

  it("opens with repo nav item active", () => {
    const { container } = render(() => (
      <SettingsPanel {...defaultProps} />
    ));
    const activeItem = container.querySelector(".settings-nav-item.active");
    expect(activeItem!.classList.contains("settings-nav-item--repo")).toBe(true);
    expect(activeItem!.textContent).toBe("my-repo");
  });

  it("shows repo settings content when repo nav item is active", () => {
    const { container } = render(() => (
      <SettingsPanel {...defaultProps} />
    ));
    // RepoWorktreeTab renders h3 "Repository"
    const headings = Array.from(container.querySelectorAll(".settings-section h3")).map(h => h.textContent);
    expect(headings).toContain("Repository");
    // RepoScriptsTab renders h3 "Automation Scripts"
    expect(headings).toContain("Automation Scripts");
  });

  it("Done button calls onClose", () => {
    const onClose = vi.fn();
    const { container } = render(() => (
      <SettingsPanel {...defaultProps} onClose={onClose} />
    ));
    const doneBtn = container.querySelector(".settings-footer-done")!;
    fireEvent.click(doneBtn);
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("reset button calls repoSettingsStore.reset with correct path", () => {
    const { container } = render(() => (
      <SettingsPanel {...defaultProps} />
    ));
    const resetBtn = container.querySelector(".settings-footer-reset")!;
    expect(resetBtn).not.toBeNull();
    fireEvent.click(resetBtn);
    expect(repoSettingsStore.reset).toHaveBeenCalledWith("/repo");
  });

  it("clicking a global nav item switches to global content", () => {
    const { container } = render(() => (
      <SettingsPanel {...defaultProps} />
    ));
    const generalItem = Array.from(container.querySelectorAll(".settings-nav-item"))
      .find((n) => n.textContent === "General")!;
    fireEvent.click(generalItem);
    const h3 = container.querySelector(".settings-section h3");
    expect(h3!.textContent).toBe("General");
    // Reset button gone when on global section
    expect(container.querySelector(".settings-footer-reset")).toBeNull();
  });
});
