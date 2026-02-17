import { describe, it, expect, vi } from "vitest";
import { render, fireEvent } from "@solidjs/testing-library";

// Mock useRepository hook to avoid Tauri invoke calls
vi.mock("../../hooks/useRepository", () => ({
  useRepository: () => ({
    getInfo: vi.fn(),
    getDiff: vi.fn().mockResolvedValue(""),
    getDiffStats: vi.fn(),
    openInApp: vi.fn(),
    getReadme: vi.fn(),
    renameBranch: vi.fn(),
    removeWorktree: vi.fn(),
    createWorktree: vi.fn(),
    getWorktreePaths: vi.fn(),
  }),
}));

// Mock Tauri APIs
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(vi.fn()),
  emit: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: vi.fn(() => ({
    listen: vi.fn().mockResolvedValue(vi.fn()),
    setTitle: vi.fn().mockResolvedValue(undefined),
  })),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn().mockResolvedValue(null),
}));

vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: vi.fn().mockResolvedValue(undefined),
}));

import { DiffPanel } from "../../components/DiffPanel/DiffPanel";

describe("DiffPanel", () => {
  it("renders diff panel structure", () => {
    const { container } = render(() => (
      <DiffPanel visible={true} repoPath="/test/repo" onClose={() => {}} />
    ));
    const panel = container.querySelector("#diff-panel");
    expect(panel).not.toBeNull();
  });

  it("renders panel title 'Changes'", () => {
    const { container } = render(() => (
      <DiffPanel visible={true} repoPath="/test/repo" onClose={() => {}} />
    ));
    const title = container.querySelector(".panel-title");
    expect(title).not.toBeNull();
    expect(title!.textContent).toBe("Changes");
  });

  it("renders close button", () => {
    const { container } = render(() => (
      <DiffPanel visible={true} repoPath="/test/repo" onClose={() => {}} />
    ));
    const closeBtn = container.querySelector(".panel-close");
    expect(closeBtn).not.toBeNull();
  });

  it("calls onClose when close button is clicked", () => {
    const handleClose = vi.fn();
    const { container } = render(() => (
      <DiffPanel visible={true} repoPath="/test/repo" onClose={handleClose} />
    ));
    const closeBtn = container.querySelector(".panel-close")!;
    fireEvent.click(closeBtn);
    expect(handleClose).toHaveBeenCalledOnce();
  });

  it("applies hidden class when not visible", () => {
    const { container } = render(() => (
      <DiffPanel visible={false} repoPath={null} onClose={() => {}} />
    ));
    const panel = container.querySelector("#diff-panel");
    expect(panel!.classList.contains("hidden")).toBe(true);
  });

  it("has panel-content container", () => {
    const { container } = render(() => (
      <DiffPanel visible={true} repoPath="/test/repo" onClose={() => {}} />
    ));
    const content = container.querySelector(".panel-content");
    expect(content).not.toBeNull();
  });
});
