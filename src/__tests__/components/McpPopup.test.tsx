import { describe, it, expect, vi, beforeEach } from "vitest";
import { render } from "@solidjs/testing-library";

const mockToggleServer = vi.fn();
const mockToggleServerForProject = vi.fn().mockResolvedValue(undefined);
const mockEffectiveEnabledForRepo = vi.fn().mockReturnValue(true);
const mockClose = vi.fn();
const mockOnOpenSettings = vi.fn();

let mockState = {
  isOpen: true,
  servers: [
    { id: "id-alpha", name: "alpha", transport: { type: "http", url: "http://localhost:8080/mcp" }, enabled: true, timeout_secs: 30 },
    { id: "id-beta", name: "beta", transport: { type: "stdio", command: "npx", args: ["-y", "server"] }, enabled: false, timeout_secs: 30 },
  ],
  status: [
    { name: "alpha", status: "ready", transport: { type: "http" }, tool_count: 5 },
    { name: "beta", status: "disabled", transport: { type: "stdio" }, tool_count: 0 },
  ],
  saving: false,
  projectAllowlist: null as string[] | null,
};

vi.mock("../../stores/mcpPopup", () => ({
  mcpPopupStore: {
    get state() { return mockState; },
    toggleServer: mockToggleServer,
    toggleServerForProject: mockToggleServerForProject,
    effectiveEnabledForRepo: mockEffectiveEnabledForRepo,
    close: mockClose,
    listenForStatusChanges: vi.fn().mockResolvedValue(() => {}),
    refreshStatus: vi.fn().mockResolvedValue(undefined),
  },
}));

describe("McpPopup", () => {
  let McpPopup: typeof import("../../components/McpPopup/McpPopup").McpPopup;

  beforeEach(async () => {
    vi.resetModules();
    mockToggleServer.mockReset();
    mockToggleServerForProject.mockReset().mockResolvedValue(undefined);
    mockEffectiveEnabledForRepo.mockReset().mockReturnValue(true);
    mockClose.mockReset();
    mockOnOpenSettings.mockReset();

    mockState = {
      isOpen: true,
      servers: [
        { id: "id-alpha", name: "alpha", transport: { type: "http", url: "http://localhost:8080/mcp" }, enabled: true, timeout_secs: 30 },
        { id: "id-beta", name: "beta", transport: { type: "stdio", command: "npx", args: ["-y", "server"] }, enabled: false, timeout_secs: 30 },
      ],
      status: [
        { name: "alpha", status: "ready", transport: { type: "http" }, tool_count: 5 },
        { name: "beta", status: "disabled", transport: { type: "stdio" }, tool_count: 0 },
      ],
      saving: false,
      projectAllowlist: null,
    };

    vi.doMock("../../stores/mcpPopup", () => ({
      mcpPopupStore: {
        get state() { return mockState; },
        toggleServer: mockToggleServer,
        toggleServerForProject: mockToggleServerForProject,
        effectiveEnabledForRepo: mockEffectiveEnabledForRepo,
        close: mockClose,
        listenForStatusChanges: vi.fn().mockResolvedValue(() => {}),
        refreshStatus: vi.fn().mockResolvedValue(undefined),
      },
    }));

    McpPopup = (await import("../../components/McpPopup/McpPopup")).McpPopup;
  });

  it("renders server rows when open", () => {
    const { container } = render(() => <McpPopup onOpenSettings={mockOnOpenSettings} />);
    const names = container.querySelectorAll("[class*='name']");
    const nameTexts = Array.from(names).map((n) => n.textContent);
    expect(nameTexts).toContain("alpha");
    expect(nameTexts).toContain("beta");
  });

  describe("per-project toggle", () => {
    it("renders project toggle when projectAllowlist is set (active repo)", () => {
      mockState.projectAllowlist = ["alpha"];
      mockEffectiveEnabledForRepo.mockImplementation((name: string) => name === "alpha");

      const { container } = render(() => <McpPopup onOpenSettings={mockOnOpenSettings} />);

      // Should have project toggle elements
      const projectToggles = container.querySelectorAll("[data-testid='project-toggle']");
      expect(projectToggles.length).toBeGreaterThan(0);
    });

    it("hides project toggle when projectAllowlist is null (no active repo)", () => {
      mockState.projectAllowlist = null;

      const { container } = render(() => <McpPopup onOpenSettings={mockOnOpenSettings} />);

      const projectToggles = container.querySelectorAll("[data-testid='project-toggle']");
      expect(projectToggles.length).toBe(0);
    });

    it("calls toggleServerForProject when project toggle is clicked", () => {
      mockState.projectAllowlist = ["alpha"];
      mockEffectiveEnabledForRepo.mockImplementation((name: string) => name === "alpha");

      const { container } = render(() => <McpPopup onOpenSettings={mockOnOpenSettings} />);

      const projectToggles = container.querySelectorAll("[data-testid='project-toggle']");
      // Click the first project toggle (alpha row)
      const alphaToggle = Array.from(projectToggles).find((el) => {
        const row = el.closest("[class*='item']");
        return row?.querySelector("[class*='name']")?.textContent === "alpha";
      });

      expect(alphaToggle).toBeDefined();
      (alphaToggle as HTMLElement).click();

      expect(mockToggleServerForProject).toHaveBeenCalledWith("alpha");
    });

    it("shows filtered indicator when server is globally enabled but project-disabled", () => {
      mockState.projectAllowlist = ["beta"]; // alpha excluded
      mockEffectiveEnabledForRepo.mockImplementation((name: string) => name !== "alpha");

      const { container } = render(() => <McpPopup onOpenSettings={mockOnOpenSettings} />);

      // The alpha row should have a filtered indicator
      const filteredBadges = container.querySelectorAll("[data-testid='project-filtered']");
      expect(filteredBadges.length).toBeGreaterThan(0);
    });

    it("does not show filtered indicator when all servers match global state", () => {
      // No project allowlist — everything follows global
      mockState.projectAllowlist = null;
      mockEffectiveEnabledForRepo.mockReturnValue(true);

      const { container } = render(() => <McpPopup onOpenSettings={mockOnOpenSettings} />);

      const filteredBadges = container.querySelectorAll("[data-testid='project-filtered']");
      expect(filteredBadges.length).toBe(0);
    });
  });
});
