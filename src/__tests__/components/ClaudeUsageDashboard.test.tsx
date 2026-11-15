import { fireEvent, render } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { providerQuotaStore } from "../../stores/providerQuota";

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

const quotaView = (response: unknown, session = "private-session") => {
	const fiveHour = (response as { five_hour?: { utilization: number } })?.five_hour;
	return {
		key: { owner: "claude-code", provider: "anthropic", credential: `/${session}` },
		session_ids: [session],
		state: "fresh",
		snapshot: {
			windows: fiveHour
				? [
						{
							kind: "five_hour",
							label: "5h",
							used_percent: fiveHour.utilization,
							window_seconds: 18000,
							resets_at: null,
							model: null,
						},
					]
				: [],
			detail: { provider: "claude", observed_at_ms: 1, response },
			plan: null,
			credits: null,
			account: null,
			observed_at_ms: 1,
			source: "active",
		},
	};
};
const mockBackend = (handler: (cmd: string, args?: unknown) => Promise<unknown>) => {
	vi.mocked(invoke).mockImplementation(async (cmd, args) =>
		cmd === "get_provider_quotas" ? [quotaView(await handler("quota_detail_fixture"))] : handler(cmd, args),
	);
};
afterEach(() => providerQuotaStore.stop());
describe("ClaudeUsageDashboard", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mockBackend(async (cmd: string) => {
			if (cmd === "quota_detail_fixture") return mockUsageApiResponse;
			if (cmd === "get_claude_session_stats") return mockSessionStats;
			if (cmd === "get_claude_project_list") return mockProjectList;
			if (cmd === "get_claude_usage_timeline") return mockTimeline;
			return undefined;
		});
	});

	it("renders the session that opened the dashboard without a provider read", async () => {
		mockBackend(async (cmd) =>
			cmd === "quota_detail_fixture"
				? { ...mockUsageApiResponse, five_hour: { utilization: 21 } }
				: cmd === "get_claude_session_stats"
					? mockSessionStats
					: [],
		);
		const result = render(() => <ClaudeUsageDashboard sessionId="private-session" />);
		await vi.waitFor(() => expect(result.getByText("21%")).toBeTruthy());
		expect(invoke).not.toHaveBeenCalledWith("get_claude_usage_api", expect.anything());
	});
	it("passes the Claude tab session through the shared panel renderer", async () => {
		mockBackend(async (cmd) =>
			cmd === "quota_detail_fixture"
				? { ...mockUsageApiResponse, five_hour: { utilization: 21 } }
				: cmd === "get_claude_session_stats"
					? mockSessionStats
					: [],
		);
		const tab = { type: "claude-usage", sessionId: "private-session" } as ClaudeUsageTab;
		const result = render(() => <MdTabContent tab={tab} onClose={() => {}} />);
		await vi.waitFor(() => expect(result.getByText("21%")).toBeTruthy());
	});
	it("clears the previous account quota when the next session is unbound", async () => {
		mockBackend(async (cmd) =>
			cmd === "quota_detail_fixture"
				? { ...mockUsageApiResponse, five_hour: { utilization: 21 } }
				: cmd === "get_claude_session_stats"
					? mockSessionStats
					: [],
		);
		const [sessionId, setSessionId] = createSignal("private-session");
		const result = render(() => <ClaudeUsageDashboard sessionId={sessionId} />);
		await vi.waitFor(() => expect(result.getByText("21%")).toBeTruthy());
		setSessionId("missing");
		await vi.waitFor(() => expect(result.getByText("Rate limit data unavailable")).toBeTruthy());
		expect(result.queryByText("21%")).toBeNull();
	});
	it("a late initial inventory uses the currently selected session", async () => {
		let resolve!: (value: unknown) => void;
		vi.mocked(invoke).mockImplementation(async (cmd) =>
			cmd === "get_provider_quotas"
				? new Promise((done) => {
						resolve = done;
					})
				: cmd === "get_claude_session_stats"
					? mockSessionStats
					: [],
		);
		const [sessionId, setSessionId] = createSignal("first");
		const result = render(() => <ClaudeUsageDashboard sessionId={sessionId} />);
		await vi.waitFor(() => expect(resolve).toBeDefined());
		setSessionId("second");
		resolve([
			quotaView({ five_hour: { utilization: 21 } }, "first"),
			quotaView({ five_hour: { utilization: 42 } }, "second"),
		]);
		await vi.waitFor(() => expect(result.getByText("42%")).toBeTruthy());
		expect(result.queryByText("21%")).toBeNull();
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

			// Wait for both sequential reads before measuring the next scope refresh.
			await vi.waitFor(() => {
				for (const command of ["get_claude_session_stats", "get_claude_usage_timeline"]) {
					expect(vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === command)).toHaveLength(1);
				}
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

			await vi.waitFor(() => {
				const timelineCallsAfter = vi
					.mocked(invoke)
					.mock.calls.filter(([cmd]) => cmd === "get_claude_usage_timeline").length;
				expect(timelineCallsAfter).toBe(timelineCallsBefore + 1);
			});
		});

		it("does not reload the quota registry when scope changes", async () => {
			const { getByRole } = render(() => <ClaudeUsageDashboard />);

			await vi.waitFor(() => {
				const calls = vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "get_provider_quotas");
				expect(calls).toHaveLength(1);
			});

			const apiCallsBefore = vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "get_provider_quotas").length;

			const select = getByRole("combobox");
			fireEvent.change(select, { target: { value: "project-a" } });

			// Allow time for any erroneous extra calls to appear
			await new Promise((resolve) => setTimeout(resolve, 50));

			const apiCallsAfter = vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "get_provider_quotas").length;
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
			mockBackend(async (cmd: string) => {
				if (cmd === "quota_detail_fixture") return enterpriseUsageResponse;
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

		it.each([true, false])("keeps real zero utilization visible with credit detail=%s", async (withCredits) => {
			mockBackend(async (cmd: string) => {
				if (cmd === "quota_detail_fixture") {
					return {
						...enterpriseUsageResponse,
						extra_usage: {
							...enterpriseUsageResponse.extra_usage,
							monthly_limit: withCredits ? 500000 : null,
							used_credits: withCredits ? 0 : null,
							utilization: 0,
						},
					};
				}
				if (cmd === "get_claude_session_stats") return mockSessionStats;
				if (cmd === "get_claude_project_list") return mockProjectList;
				if (cmd === "get_claude_usage_timeline") return mockTimeline;
				return undefined;
			});
			const { container } = render(() => <ClaudeUsageDashboard />);
			await vi.waitFor(() => {
				expect(container.textContent).toContain(withCredits ? "0.0% used" : "0%");
			});
			expect(container.textContent).not.toMatch(/unavailable/i);
		});

		it("shows an unavailable value, not 0%, when extra_usage.utilization is null", async () => {
			mockBackend(async (cmd: string) => {
				if (cmd === "quota_detail_fixture") {
					return {
						...enterpriseUsageResponse,
						extra_usage: { ...enterpriseUsageResponse.extra_usage, utilization: null },
					};
				}
				if (cmd === "get_claude_session_stats") return mockSessionStats;
				if (cmd === "get_claude_project_list") return mockProjectList;
				if (cmd === "get_claude_usage_timeline") return mockTimeline;
				return undefined;
			});
			const { container } = render(() => <ClaudeUsageDashboard />);
			await vi.waitFor(() => {
				expect(container.textContent).toContain("162,709");
			});
			expect(container.textContent).toContain("Usage percentage unavailable");
			expect(container.textContent).not.toContain("0.0% used");
		});

		it("shows an unavailable value, not 0%, when utilization and credit detail are both missing", async () => {
			mockBackend(async (cmd: string) => {
				if (cmd === "quota_detail_fixture") {
					return {
						...enterpriseUsageResponse,
						extra_usage: {
							is_enabled: true,
							monthly_limit: null,
							used_credits: null,
							utilization: null,
							resets_at: null,
							in_use: false,
						},
					};
				}
				if (cmd === "get_claude_session_stats") return mockSessionStats;
				if (cmd === "get_claude_project_list") return mockProjectList;
				if (cmd === "get_claude_usage_timeline") return mockTimeline;
				return undefined;
			});
			const { container } = render(() => <ClaudeUsageDashboard />);
			await vi.waitFor(() => {
				expect(container.textContent).toContain("Extra Usage");
			});
			expect(container.textContent).toContain("Unavailable");
			expect(container.textContent).not.toMatch(/(^|[^0-9.])0%/);
		});
	});
});
