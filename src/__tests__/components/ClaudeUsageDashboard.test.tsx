import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@solidjs/testing-library";

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
  ask: vi.fn().mockResolvedValue(false),
  message: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("@tauri-apps/plugin-global-shortcut", () => ({
  register: vi.fn().mockResolvedValue(undefined),
  unregister: vi.fn().mockResolvedValue(undefined),
  isRegistered: vi.fn().mockResolvedValue(false),
}));

vi.mock("tauri-plugin-user-input-api", () => ({
  startListening: vi.fn().mockResolvedValue(undefined),
  stopListening: vi.fn().mockResolvedValue(undefined),
  setEventTypes: vi.fn().mockResolvedValue(undefined),
  isListening: vi.fn().mockResolvedValue(false),
  EventTypeEnum: { KeyPress: "KeyPress", KeyRelease: "KeyRelease" },
}));

import { invoke } from "@tauri-apps/api/core";
import { ClaudeUsageDashboard } from "../../components/ClaudeUsageDashboard";

const mockUsageApiResponse = {
  five_hour: null,
  seven_day: null,
  seven_day_opus: null,
  seven_day_sonnet: null,
  seven_day_cowork: null,
  extra_usage: null,
};

const mockSessionStats = {
  total_sessions: 5,
  total_assistant_messages: 100,
  total_user_messages: 50,
  total_input_tokens: 10000,
  total_output_tokens: 5000,
  total_cache_creation_tokens: 1000,
  total_cache_read_tokens: 500,
  model_usage: {},
  daily_activity: {},
  per_project: {},
  per_project_daily: {},
  active_hours: 10,
};

const mockProjectList = [
  { slug: "project-a", session_count: 3, display_path: "/home/user/project-a" },
  { slug: "project-b", session_count: 2, display_path: "/home/user/project-b" },
];

const mockTimeline: unknown[] = [];

describe("ClaudeUsageDashboard", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_claude_usage_api") return mockUsageApiResponse;
      if (cmd === "get_claude_session_stats") return mockSessionStats;
      if (cmd === "get_claude_project_list") return mockProjectList;
      if (cmd === "get_claude_usage_timeline") return mockTimeline;
      return undefined;
    });
  });

  describe("scope change — single fetch cycle", () => {
    it("calls get_claude_session_stats exactly once on initial mount", async () => {
      render(() => <ClaudeUsageDashboard />);
      // Wait for microtasks/promises to settle
      await vi.waitFor(() => {
        const calls = vi.mocked(invoke).mock.calls.filter(
          ([cmd]) => cmd === "get_claude_session_stats",
        );
        expect(calls).toHaveLength(1);
      });
    });

    it("calls get_claude_usage_timeline exactly once on initial mount", async () => {
      render(() => <ClaudeUsageDashboard />);
      await vi.waitFor(() => {
        const calls = vi.mocked(invoke).mock.calls.filter(
          ([cmd]) => cmd === "get_claude_usage_timeline",
        );
        expect(calls).toHaveLength(1);
      });
    });

    it("calls get_claude_session_stats exactly once more when scope changes", async () => {
      const { getByRole } = render(() => <ClaudeUsageDashboard />);

      // Wait for initial load to settle
      await vi.waitFor(() => {
        const calls = vi.mocked(invoke).mock.calls.filter(
          ([cmd]) => cmd === "get_claude_session_stats",
        );
        expect(calls).toHaveLength(1);
      });

      const statsCallsBefore = vi.mocked(invoke).mock.calls.filter(
        ([cmd]) => cmd === "get_claude_session_stats",
      ).length;
      const timelineCallsBefore = vi.mocked(invoke).mock.calls.filter(
        ([cmd]) => cmd === "get_claude_usage_timeline",
      ).length;

      // Change scope via the select dropdown
      const select = getByRole("combobox");
      fireEvent.change(select, { target: { value: "project-a" } });

      // After scope change, each fetch should fire exactly once more (not twice)
      await vi.waitFor(() => {
        const statsCallsAfter = vi.mocked(invoke).mock.calls.filter(
          ([cmd]) => cmd === "get_claude_session_stats",
        ).length;
        expect(statsCallsAfter).toBe(statsCallsBefore + 1);
      });

      const timelineCallsAfter = vi.mocked(invoke).mock.calls.filter(
        ([cmd]) => cmd === "get_claude_usage_timeline",
      ).length;
      expect(timelineCallsAfter).toBe(timelineCallsBefore + 1);
    });

    it("does not call get_claude_usage_api again when scope changes", async () => {
      const { getByRole } = render(() => <ClaudeUsageDashboard />);

      await vi.waitFor(() => {
        const calls = vi.mocked(invoke).mock.calls.filter(
          ([cmd]) => cmd === "get_claude_usage_api",
        );
        expect(calls).toHaveLength(1);
      });

      const apiCallsBefore = vi.mocked(invoke).mock.calls.filter(
        ([cmd]) => cmd === "get_claude_usage_api",
      ).length;

      const select = getByRole("combobox");
      fireEvent.change(select, { target: { value: "project-a" } });

      // Allow time for any erroneous extra calls to appear
      await new Promise((resolve) => setTimeout(resolve, 50));

      const apiCallsAfter = vi.mocked(invoke).mock.calls.filter(
        ([cmd]) => cmd === "get_claude_usage_api",
      ).length;
      expect(apiCallsAfter).toBe(apiCallsBefore);
    });
  });
});
