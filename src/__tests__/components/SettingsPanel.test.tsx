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

vi.mock("../../stores/ui", () => ({
  uiStore: {
    state: {
      settingsNavWidth: 180,
    },
    setSettingsNavWidth: vi.fn(),
  },
}));

vi.mock("../../stores/repositories", () => ({
  repositoriesStore: {
    state: {
      repositories: {
        "/repo/alpha": { path: "/repo/alpha", displayName: "Alpha" },
        "/repo/beta": { path: "/repo/beta", displayName: "Beta" },
      },
      repoOrder: ["/repo/alpha", "/repo/beta"],
    },
    setDisplayName: vi.fn(),
    getGroupForRepo: vi.fn(() => undefined),
  },
}));

vi.mock("../../stores/repoSettings", () => ({
  repoSettingsStore: {
    get: vi.fn(() => undefined),
    getOrCreate: vi.fn().mockReturnValue({
      path: "/repo/alpha",
      displayName: "Alpha",
      baseBranch: "automatic",
      copyIgnoredFiles: false,
      copyUntrackedFiles: false,
      setupScript: "",
      runScript: "",
      color: "",
    }),
    update: vi.fn(),
    reset: vi.fn(),
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
    const overlay = container.querySelector(".overlay");
    expect(overlay).toBeNull();
  });

  it("renders when visible=true", () => {
    const { container } = render(() => (
      <SettingsPanel visible={true} onClose={() => {}} />
    ));
    const overlay = container.querySelector(".overlay");
    expect(overlay).not.toBeNull();
  });

  it("shows Settings header", () => {
    const { container } = render(() => (
      <SettingsPanel visible={true} onClose={() => {}} />
    ));
    const heading = container.querySelector(".header h2");
    expect(heading).not.toBeNull();
    expect(heading!.textContent).toBe("Settings");
  });

  it("shows nav items (General, Notifications)", () => {
    const { container } = render(() => (
      <SettingsPanel visible={true} onClose={() => {}} />
    ));
    const navItems = container.querySelectorAll(".navItem");
    const labels = Array.from(navItems).map((n) => n.textContent);
    expect(labels).toContain("General");
    expect(labels).toContain("Appearance");
    expect(labels).toContain("Notifications");
    expect(labels).not.toContain("Agents");
    expect(labels).not.toContain("Groups");
  });

  it("close button calls onClose", () => {
    const onClose = vi.fn();
    const { container } = render(() => (
      <SettingsPanel visible={true} onClose={onClose} />
    ));
    const closeBtn = container.querySelector(".close")!;
    fireEvent.click(closeBtn);
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("overlay click calls onClose", () => {
    const onClose = vi.fn();
    const { container } = render(() => (
      <SettingsPanel visible={true} onClose={onClose} />
    ));
    const overlay = container.querySelector(".overlay")!;
    fireEvent.click(overlay);
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("switching nav items shows correct content", () => {
    const { container } = render(() => (
      <SettingsPanel visible={true} onClose={() => {}} />
    ));

    // Default is General (visual settings moved to Appearance tab)
    const headings = container.querySelectorAll(".section h3");
    expect(headings.length).toBeGreaterThanOrEqual(5);
    expect(headings[0]!.textContent).toBe("General");
    expect(headings[1]!.textContent).toBe("Confirmations");
    expect(headings[2]!.textContent).toBe("Power Management");
    expect(headings[3]!.textContent).toBe("Updates");
    expect(headings[4]!.textContent).toBe("Git Integration");

    // Click Notifications nav item
    const navItems = container.querySelectorAll(".navItem");
    const notificationsItem = Array.from(navItems).find((n) => n.textContent === "Notifications")!;
    fireEvent.click(notificationsItem);
    const sectionTitle = container.querySelector(".section h3");
    expect(sectionTitle!.textContent).toBe("Notification Settings");
  });

  it("shows repos as nav items in the sidebar", () => {
    const { container } = render(() => (
      <SettingsPanel visible={true} onClose={() => {}} />
    ));
    const repoItems = container.querySelectorAll(".navItemRepo");
    const labels = Array.from(repoItems).map((n) => n.textContent);
    expect(labels).toContain("Alpha");
    expect(labels).toContain("Beta");
  });

  it("shows REPOSITORIES section label above repo items", () => {
    const { container } = render(() => (
      <SettingsPanel visible={true} onClose={() => {}} />
    ));
    const label = container.querySelector(".navLabel");
    expect(label).not.toBeNull();
    expect(label!.textContent).toBe("REPOSITORIES");
  });

  it("opens on General when no context given", () => {
    const { container } = render(() => (
      <SettingsPanel visible={true} onClose={() => {}} />
    ));
    const activeItem = container.querySelector(".navItem.active");
    expect(activeItem!.textContent).toBe("General");
  });

  it("opens directly on repo nav item when context is repo", () => {
    const { container } = render(() => (
      <SettingsPanel
        visible={true}
        onClose={() => {}}
        context={{ kind: "repo", repoPath: "/repo/alpha" }}
      />
    ));
    const activeItem = container.querySelector(".navItem.active");
    expect(activeItem!.classList.contains("navItemRepo")).toBe(true);
    expect(activeItem!.textContent).toBe("Alpha");
  });

  it("shows repo settings content when repo nav item is active", () => {
    const { container } = render(() => (
      <SettingsPanel
        visible={true}
        onClose={() => {}}
        context={{ kind: "repo", repoPath: "/repo/alpha" }}
      />
    ));
    // RepoWorktreeTab has a h3 "Repository"
    const h3 = container.querySelector(".section h3");
    expect(h3!.textContent).toBe("Repository");
  });

  it("shows Reset to Defaults button only when repo nav item is active", () => {
    const { container } = render(() => (
      <SettingsPanel visible={true} onClose={() => {}} />
    ));
    // Global context → no Reset button
    expect(container.querySelector(".footerReset")).toBeNull();
  });

  it("shows Reset to Defaults when repo nav item is active", () => {
    const { container } = render(() => (
      <SettingsPanel
        visible={true}
        onClose={() => {}}
        context={{ kind: "repo", repoPath: "/repo/alpha" }}
      />
    ));
    expect(container.querySelector(".footerReset")).not.toBeNull();
  });

  it("renders split layout with nav sidebar", () => {
    const { container } = render(() => (
      <SettingsPanel visible={true} onClose={() => {}} />
    ));
    expect(container.querySelector(".body")).not.toBeNull();
    expect(container.querySelector(".nav")).not.toBeNull();
    expect(container.querySelector(".content")).not.toBeNull();
  });
});
