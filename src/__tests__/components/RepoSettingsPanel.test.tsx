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

vi.mock("../../stores/errorHandling", () => ({
  errorHandlingStore: {
    state: { config: { strategy: "retry", maxRetries: 3 } },
    setStrategy: vi.fn(), setMaxRetries: vi.fn(), resetConfig: vi.fn(),
  },
}));

vi.mock("../../stores/agentFallback", () => ({
  agentFallbackStore: {
    state: { primaryAgent: "claude", autoRecovery: true, fallbackChain: [], recoveryIntervalMs: 30000 },
    setPrimary: vi.fn(), configure: vi.fn(), forceResetToPrimary: vi.fn(),
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
  const repoContext = { kind: "repo" as const, repoPath: "/repo", displayName: "my-repo" };

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
    };
    vi.mocked(repoSettingsStore.getOrCreate).mockReturnValue(mockSettings);
  });

  it("shows display name in header", () => {
    const { container } = render(() => (
      <SettingsPanel {...defaultProps} />
    ));
    const heading = container.querySelector(".settings-title--repo h2");
    expect(heading).not.toBeNull();
    expect(heading!.textContent).toBe("my-repo");
  });

  it("shows repo path as subtitle", () => {
    const { container } = render(() => (
      <SettingsPanel {...defaultProps} />
    ));
    const pathEl = container.querySelector(".settings-path--repo");
    expect(pathEl).not.toBeNull();
    expect(pathEl!.textContent).toBe("/repo");
  });

  it("shows repo tabs and global tabs", () => {
    const { container } = render(() => (
      <SettingsPanel {...defaultProps} />
    ));
    const tabs = container.querySelectorAll(".settings-tab");
    const tabLabels = Array.from(tabs).map((t) => t.textContent);
    // Repo tabs
    expect(tabLabels).toContain("Worktree");
    expect(tabLabels).toContain("Scripts");
    // Global tabs (also present)
    expect(tabLabels).toContain("General");
    expect(tabLabels).not.toContain("Agents");
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

  it("reset button calls repoSettingsStore.reset", () => {
    const { container } = render(() => (
      <SettingsPanel {...defaultProps} />
    ));
    const resetBtn = container.querySelector(".settings-footer-reset")!;
    fireEvent.click(resetBtn);
    expect(repoSettingsStore.reset).toHaveBeenCalledWith("/repo");
  });

  it("switching to Worktree tab shows worktree content", () => {
    const { container } = render(() => (
      <SettingsPanel {...defaultProps} initialTab="repo-scripts" />
    ));

    const tabs = container.querySelectorAll(".settings-tab");
    const worktreeTab = Array.from(tabs).find((t) => t.textContent === "Worktree")!;
    fireEvent.click(worktreeTab);
    const sectionTitle = container.querySelector(".settings-section h3");
    expect(sectionTitle!.textContent).toBe("Repository");
  });

  it("switching to Scripts tab shows scripts content", () => {
    const { container } = render(() => (
      <SettingsPanel {...defaultProps} />
    ));

    const tabs = container.querySelectorAll(".settings-tab");
    const scriptsTab = Array.from(tabs).find((t) => t.textContent === "Scripts")!;
    fireEvent.click(scriptsTab);
    const sectionTitle = container.querySelector(".settings-section h3");
    expect(sectionTitle!.textContent).toBe("Automation Scripts");
  });

  it("defaults to repo Worktree tab", () => {
    const { container } = render(() => (
      <SettingsPanel {...defaultProps} />
    ));
    const sectionTitle = container.querySelector(".settings-section h3");
    expect(sectionTitle!.textContent).toBe("Repository");
  });
});
