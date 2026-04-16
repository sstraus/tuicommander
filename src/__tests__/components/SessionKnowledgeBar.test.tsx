import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent, waitFor } from "@solidjs/testing-library";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue({
    session_id: "s1",
    commands_count: 3,
    recent_outcomes: [
      { timestamp: 1, command: "cargo build", exit_code: 0, duration_ms: 500, kind: "success", error_type: null },
      { timestamp: 2, command: "git status", exit_code: 0, duration_ms: 100, kind: "inferred", error_type: null },
    ],
    recent_errors: [
      { timestamp: 3, command: "make fail", exit_code: 1, duration_ms: 200, kind: "error", error_type: "exit_code" },
    ],
    tui_mode: "vim",
    tui_apps_seen: ["vim", "htop"],
  }),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(vi.fn()),
}));

import { SessionKnowledgeBar } from "../../components/AIChatPanel/SessionKnowledgeBar";

describe("SessionKnowledgeBar", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders summary row with command count", async () => {
    const { container } = render(() => <SessionKnowledgeBar sessionId="s1" />);
    await waitFor(() => {
      expect(container.textContent).toContain("3 cmds");
    });
  });

  it("shows error count when errors exist", async () => {
    const { container } = render(() => <SessionKnowledgeBar sessionId="s1" />);
    await waitFor(() => {
      expect(container.textContent).toContain("1 recent err");
    });
  });

  it("shows tui mode label", async () => {
    const { container } = render(() => <SessionKnowledgeBar sessionId="s1" />);
    await waitFor(() => {
      expect(container.textContent).toContain("tui: vim");
    });
  });

  it("expands to show details on click", async () => {
    const { container } = render(() => <SessionKnowledgeBar sessionId="s1" />);
    await waitFor(() => {
      expect(container.textContent).toContain("3 cmds");
    });
    const btn = container.querySelector("button")!;
    fireEvent.click(btn);
    await waitFor(() => {
      expect(container.textContent).toContain("cargo build");
      expect(container.textContent).toContain("git status");
      expect(container.textContent).toContain("make fail");
      expect(container.textContent).toContain("vim, htop");
    });
  });

  it("truncates long commands", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    const longCmd = "a".repeat(100);
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValue({
      session_id: "s1",
      commands_count: 1,
      recent_outcomes: [
        { timestamp: 1, command: longCmd, exit_code: 0, duration_ms: 100, kind: "success", error_type: null },
      ],
      recent_errors: [],
      tui_mode: null,
      tui_apps_seen: [],
    });
    const { container } = render(() => <SessionKnowledgeBar sessionId="s1" />);
    await waitFor(() => {
      expect(container.textContent).toContain("1 cmds");
    });
    const btn = container.querySelector("button")!;
    fireEvent.click(btn);
    await waitFor(() => {
      expect(container.textContent).toContain("…");
      expect(container.textContent).not.toContain(longCmd);
    });
  });

  it("hides when sessionId is null", () => {
    const { container } = render(() => <SessionKnowledgeBar sessionId={null} />);
    const bar = container.firstElementChild as HTMLElement;
    expect(bar.className).toContain("hidden");
  });
});
