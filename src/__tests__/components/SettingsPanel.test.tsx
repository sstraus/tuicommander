import { describe, it, expect, vi, beforeEach } from "vitest";
import "../mocks/tauri";
import { render, fireEvent } from "@solidjs/testing-library";

vi.mock("../../stores/settings", () => ({
  settingsStore: {
    state: {
      ide: "vscode",
      font: "JetBrains Mono",
      defaultFontSize: 12,
      confirmBeforeQuit: true,
      confirmBeforeClosingTab: true,
    },
    setIde: vi.fn(),
    setFont: vi.fn(),
    setConfirmBeforeQuit: vi.fn(),
    setConfirmBeforeClosingTab: vi.fn(),
  },
  IDE_NAMES: { vscode: "VS Code", cursor: "Cursor" },
  FONT_FAMILIES: { "JetBrains Mono": "JetBrains Mono", "Fira Code": "Fira Code" },
}));

vi.mock("../../stores/notifications", () => ({
  notificationsStore: {
    state: {
      isAvailable: true,
      config: {
        enabled: true,
        volume: 0.5,
        sounds: {
          question: true,
          error: true,
          completion: true,
          warning: true,
        },
      },
    },
    setEnabled: vi.fn(),
    setVolume: vi.fn(),
    setSoundEnabled: vi.fn(),
    testSound: vi.fn(),
    reset: vi.fn(),
  },
}));

vi.mock("../../stores/errorHandling", () => ({
  errorHandlingStore: {
    state: {
      config: {
        strategy: "retry",
        maxRetries: 3,
      },
    },
    setStrategy: vi.fn(),
    setMaxRetries: vi.fn(),
    resetConfig: vi.fn(),
  },
}));

vi.mock("../../stores/agentFallback", () => ({
  agentFallbackStore: {
    state: {
      primaryAgent: "claude",
      autoRecovery: true,
      fallbackChain: [],
      recoveryIntervalMs: 30000,
    },
    setPrimary: vi.fn(),
    configure: vi.fn(),
    forceResetToPrimary: vi.fn(),
  },
}));

import { SettingsPanel } from "../../components/SettingsPanel/SettingsPanel";

describe("SettingsPanel", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("does not render when visible=false", () => {
    const { container } = render(() => (
      <SettingsPanel visible={false} onClose={() => {}} />
    ));
    const overlay = container.querySelector(".settings-overlay");
    expect(overlay).toBeNull();
  });

  it("renders when visible=true", () => {
    const { container } = render(() => (
      <SettingsPanel visible={true} onClose={() => {}} />
    ));
    const overlay = container.querySelector(".settings-overlay");
    expect(overlay).not.toBeNull();
  });

  it("shows Settings header", () => {
    const { container } = render(() => (
      <SettingsPanel visible={true} onClose={() => {}} />
    ));
    const heading = container.querySelector(".settings-header h2");
    expect(heading).not.toBeNull();
    expect(heading!.textContent).toBe("Settings");
  });

  it("shows tab buttons (General, Notifications)", () => {
    const { container } = render(() => (
      <SettingsPanel visible={true} onClose={() => {}} />
    ));
    const tabs = container.querySelectorAll(".settings-tab");
    const tabLabels = Array.from(tabs).map((t) => t.textContent);
    expect(tabLabels).toContain("General");
    expect(tabLabels).toContain("Notifications");
    expect(tabLabels).not.toContain("Agents");
    expect(tabLabels).not.toContain("Appearance");
  });

  it("close button calls onClose", () => {
    const onClose = vi.fn();
    const { container } = render(() => (
      <SettingsPanel visible={true} onClose={onClose} />
    ));
    const closeBtn = container.querySelector(".settings-close")!;
    fireEvent.click(closeBtn);
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("overlay click calls onClose", () => {
    const onClose = vi.fn();
    const { container } = render(() => (
      <SettingsPanel visible={true} onClose={onClose} />
    ));
    const overlay = container.querySelector(".settings-overlay")!;
    fireEvent.click(overlay);
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("switching tabs shows correct content", () => {
    const { container } = render(() => (
      <SettingsPanel visible={true} onClose={() => {}} />
    ));

    // Default tab is General (includes confirmations, power, updates, git, appearance)
    const headings = container.querySelectorAll(".settings-section h3");
    expect(headings.length).toBeGreaterThanOrEqual(6);
    expect(headings[0]!.textContent).toBe("General");
    expect(headings[1]!.textContent).toBe("Confirmations");
    expect(headings[2]!.textContent).toBe("Power Management");
    expect(headings[3]!.textContent).toBe("Updates");
    expect(headings[4]!.textContent).toBe("Git Integration");
    expect(headings[5]!.textContent).toBe("Appearance");

    // Click Notifications tab
    const tabs = container.querySelectorAll(".settings-tab");
    const notificationsTab = Array.from(tabs).find((t) => t.textContent === "Notifications")!;
    fireEvent.click(notificationsTab);
    const sectionTitle = container.querySelector(".settings-section h3");
    expect(sectionTitle!.textContent).toBe("Notification Settings");
  });
});
