import { describe, it, expect, vi } from "vitest";
import { render, fireEvent } from "@solidjs/testing-library";

// Mock useRepository hook to avoid Tauri invoke calls
vi.mock("../../hooks/useRepository", () => ({
  useRepository: () => ({
    getInfo: vi.fn(),
    getDiff: vi.fn(),
    getDiffStats: vi.fn(),
    openInApp: vi.fn(),
    renameBranch: vi.fn(),
    removeWorktree: vi.fn(),
    createWorktree: vi.fn(),
    getWorktreePaths: vi.fn(),
    listMarkdownFiles: vi.fn().mockResolvedValue([]),
    readFile: vi.fn().mockResolvedValue(""),
    getChangedFiles: vi.fn().mockResolvedValue([]),
    getFileDiff: vi.fn().mockResolvedValue(""),
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

import { MarkdownPanel } from "../../components/MarkdownPanel/MarkdownPanel";

describe("MarkdownPanel", () => {
  it("renders markdown panel structure", () => {
    const { container } = render(() => (
      <MarkdownPanel visible={true} repoPath="/test/repo" onClose={() => {}} />
    ));
    const panel = container.querySelector(".panel");
    expect(panel).not.toBeNull();
  });

  it("renders panel title 'Markdown Files'", () => {
    const { container } = render(() => (
      <MarkdownPanel visible={true} repoPath="/test/repo" onClose={() => {}} />
    ));
    const title = container.querySelector(".title");
    expect(title).not.toBeNull();
    expect(title!.textContent).toBe("Markdown Files");
  });

  it("renders close button", () => {
    const { container } = render(() => (
      <MarkdownPanel visible={true} repoPath="/test/repo" onClose={() => {}} />
    ));
    const closeBtn = container.querySelector(".close");
    expect(closeBtn).not.toBeNull();
  });

  it("calls onClose when close button is clicked", () => {
    const handleClose = vi.fn();
    const { container } = render(() => (
      <MarkdownPanel visible={true} repoPath="/test/repo" onClose={handleClose} />
    ));
    const closeBtn = container.querySelector(".close")!;
    fireEvent.click(closeBtn);
    expect(handleClose).toHaveBeenCalledOnce();
  });

  it("applies hidden class when not visible", () => {
    const { container } = render(() => (
      <MarkdownPanel visible={false} repoPath={null} onClose={() => {}} />
    ));
    const panel = container.querySelector(".panel");
    expect(panel!.classList.contains("hidden")).toBe(true);
  });

  it("has panel-content container", () => {
    const { container } = render(() => (
      <MarkdownPanel visible={true} repoPath="/test/repo" onClose={() => {}} />
    ));
    const content = container.querySelector(".content");
    expect(content).not.toBeNull();
  });
});
