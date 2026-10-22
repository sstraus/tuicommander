import { fireEvent, render } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { beforeEach, describe, expect, it, vi } from "vitest";

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

import { invoke } from "@tauri-apps/api/core";
import { ClaudeUsageDashboard } from "../../components/ClaudeUsageDashboard";
import { MdTabContent } from "../../components/shared/MdTabContent";
import type { ClaudeUsageTab } from "../../stores/mdTabs";

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

	it("requests API usage for the session that opened the dashboard", async () => {
		render(() => <ClaudeUsageDashboard sessionId="private-session" />);
		await vi.waitFor(() => {
			expect(invoke).toHaveBeenCalledWith("get_claude_usage_api", { sessionId: "private-session" });
		});
	});

	it("passes the Claude tab's session through the shared panel renderer", async () => {
		const tab = { type: "claude-usage", sessionId: "private-session" } as ClaudeUsageTab;
		render(() => <MdTabContent tab={tab} onClose={() => {}} />);
		await vi.waitFor(() => {
			expect(invoke).toHaveBeenCalledWith("get_claude_usage_api", { sessionId: "private-session" });
		});
	});

	it("clears the previous account quota when the next profile has no credentials", async () => {
		vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
			if (cmd === "get_claude_usage_api") {
				if ((args as { sessionId?: string } | undefined)?.sessionId === "missing")
					throw new Error("No Claude OAuth token found");
				return { ...mockUsageApiResponse, five_hour: { utilization: 21, resets_at: null } };
			}
			if (cmd === "get_claude_session_stats") return mockSessionStats;
			if (cmd === "get_claude_project_list") return mockProjectList;
			if (cmd === "get_claude_usage_timeline") return mockTimeline;
			return undefined;
		});
		const [sessionId, setSessionId] = createSignal("first");
		const view = render(() => <ClaudeUsageDashboard sessionId={sessionId} />);
		await vi.waitFor(() => expect(view.getByText("21%")).toBeTruthy());
		setSessionId("missing");
		await vi.waitFor(() => expect(view.getByText("Rate limit data unavailable")).toBeTruthy());
		expect(view.queryByText("21%")).toBeNull();
	});

	it("ignores a late response from the previous profile", async () => {
		let resolveFirst: ((value: unknown) => void) | undefined;
		vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
			if (cmd === "get_claude_usage_api") {
				const sessionId = (args as { sessionId?: string } | undefined)?.sessionId;
				if (sessionId === "first")
					return new Promise((resolve) => {
						resolveFirst = resolve;
					});
				return { ...mockUsageApiResponse, five_hour: { utilization: 42, resets_at: null } };
			}
			if (cmd === "get_claude_session_stats") return mockSessionStats;
			if (cmd === "get_claude_project_list") return mockProjectList;
			if (cmd === "get_claude_usage_timeline") return mockTimeline;
			return undefined;
		});
		const [sessionId, setSessionId] = createSignal("first");
		const view = render(() => <ClaudeUsageDashboard sessionId={sessionId} />);
		await vi.waitFor(() => expect(resolveFirst).toBeDefined());
		setSessionId("second");
		await vi.waitFor(() => expect(view.getByText("42%")).toBeTruthy());
		resolveFirst?.({ ...mockUsageApiResponse, five_hour: { utilization: 21, resets_at: null } });
		await Promise.resolve();
		expect(view.queryByText("21%")).toBeNull();
		expect(view.getByText("42%")).toBeTruthy();
	});

	describe("scope change — single fetch cycle", () => {
		it("calls get_claude_session_stats exactly once on initial mount", async () => {
			render(() => <ClaudeUsageDashboard />);
			// Wait for microtasks/promises to settle
			await vi.waitFor(() => {
				const calls = vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "get_claude_session_stats");
				expect(calls).toHaveLength(1);
			});
		});

		it("calls get_claude_usage_timeline exactly once on initial mount", async () => {
			render(() => <ClaudeUsageDashboard />);
			await vi.waitFor(() => {
				const calls = vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "get_claude_usage_timeline");
				expect(calls).toHaveLength(1);
			});
		});

		it("calls get_claude_session_stats exactly once more when scope changes", async () => {
			const { getByRole } = render(() => <ClaudeUsageDashboard />);

			// Wait for initial load to settle
			await vi.waitFor(() => {
				const calls = vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "get_claude_session_stats");
				expect(calls).toHaveLength(1);
			});

			const statsCallsBefore = vi
				.mocked(invoke)
				.mock.calls.filter(([cmd]) => cmd === "get_claude_session_stats").length;
			const timelineCallsBefore = vi
				.mocked(invoke)
				.mock.calls.filter(([cmd]) => cmd === "get_claude_usage_timeline").length;

			// Change scope via the select dropdown
			const select = getByRole("combobox");
			fireEvent.change(select, { target: { value: "project-a" } });

			// After scope change, each fetch should fire exactly once more (not twice)
			await vi.waitFor(() => {
				const statsCallsAfter = vi
					.mocked(invoke)
					.mock.calls.filter(([cmd]) => cmd === "get_claude_session_stats").length;
				expect(statsCallsAfter).toBe(statsCallsBefore + 1);
			});

			const timelineCallsAfter = vi
				.mocked(invoke)
				.mock.calls.filter(([cmd]) => cmd === "get_claude_usage_timeline").length;
			expect(timelineCallsAfter).toBe(timelineCallsBefore + 1);
		});

		it("does not call get_claude_usage_api again when scope changes", async () => {
			const { getByRole } = render(() => <ClaudeUsageDashboard />);

			await vi.waitFor(() => {
				const calls = vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "get_claude_usage_api");
				expect(calls).toHaveLength(1);
			});

			const apiCallsBefore = vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "get_claude_usage_api").length;

			const select = getByRole("combobox");
			fireEvent.change(select, { target: { value: "project-a" } });

			// Allow time for any erroneous extra calls to appear
			await new Promise((resolve) => setTimeout(resolve, 50));

			const apiCallsAfter = vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "get_claude_usage_api").length;
			expect(apiCallsAfter).toBe(apiCallsBefore);
		});
	});

	describe("enterprise plan (spend-based, no five_hour/seven_day buckets)", () => {
		// Ground-truth shape captured from a real enterprise-plan /api/oauth/usage response:
		// the named rate buckets are all null, and extra_usage carries the only usage signal.
		const enterpriseUsageResponse = {
			five_hour: null,
			seven_day: null,
			seven_day_opus: null,
			seven_day_sonnet: null,
			seven_day_cowork: null,
			extra_usage: {
				is_enabled: true,
				monthly_limit: 500000,
				used_credits: 162709,
				utilization: 32.54,
				resets_at: null,
				in_use: false,
			},
			plan: {
				subscription_type: "enterprise",
				rate_limit_tier: "default_claude_zero",
				scopes: ["user:inference"],
			},
		};

		beforeEach(() => {
			vi.mocked(invoke).mockImplementation(async (cmd: string) => {
				if (cmd === "get_claude_usage_api") return enterpriseUsageResponse;
				if (cmd === "get_claude_session_stats") return mockSessionStats;
				if (cmd === "get_claude_project_list") return mockProjectList;
				if (cmd === "get_claude_usage_timeline") return mockTimeline;
				return undefined;
			});
		});

		it("renders the plan name and rate-limit tier from PlanInfo", async () => {
			const { container } = render(() => <ClaudeUsageDashboard />);
			await vi.waitFor(() => {
				expect(container.textContent).toContain("enterprise");
				expect(container.textContent).toContain("default_claude_zero");
			});
		});

		it("renders extra_usage credits/percentage even though five_hour/seven_day are absent", async () => {
			const { container } = render(() => <ClaudeUsageDashboard />);
			await vi.waitFor(() => {
				expect(container.textContent).toContain("162,709");
				expect(container.textContent).toContain("500,000");
				expect(container.textContent).toContain("32.5% used");
			});
		});
	});
});
