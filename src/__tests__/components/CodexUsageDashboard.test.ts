import { render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
	barHeightPercent,
	CodexUsageDashboard,
	formatDuration,
	formatPercent,
	formatTokens,
	labelWindows,
} from "../../components/CodexUsageDashboard/CodexUsageDashboard";
import type { CodexUsageApiResponse } from "../../features/codexUsage";
import { appLogger } from "../../stores/appLogger";
import { providerQuotaStore } from "../../stores/providerQuota";
import { mockInvoke } from "../mocks/tauri";

const win = (usedPercent: number, seconds: number | null) => ({
	used_percent: usedPercent,
	limit_window_seconds: seconds,
	reset_after_seconds: null,
	reset_at: null,
});

describe("formatTokens", () => {
	it("compacts the billions Codex reports", () => {
		expect(formatTokens(25_241_643_691)).toBe("25.2B");
		expect(formatTokens(2_988_540_050)).toBe("3.0B");
		expect(formatTokens(511_198_907)).toBe("511.2M");
		expect(formatTokens(3720)).toBe("3.7K");
		expect(formatTokens(42)).toBe("42");
	});

	it("shows a placeholder rather than NaN or zero for absent data", () => {
		expect(formatTokens(null)).toBe("--");
		expect(formatTokens(Number.NaN)).toBe("--");
	});
});

describe("formatDuration", () => {
	it("renders a multi-hour turn in hours and minutes", () => {
		expect(formatDuration(61_603)).toBe("17h 6m");
		expect(formatDuration(90)).toBe("1m");
	});

	it("treats absent or non-positive durations as no data", () => {
		expect(formatDuration(null)).toBe("--");
		expect(formatDuration(0)).toBe("--");
	});
});

describe("formatPercent", () => {
	it("keeps one decimal so a sub-1% value is not rounded to zero", () => {
		expect(formatPercent(0.09912030727295)).toBe("0.1%");
		expect(formatPercent(31.16903545784292)).toBe("31.2%");
		expect(formatPercent(null)).toBe("--");
	});
});

describe("labelWindows", () => {
	const api: CodexUsageApiResponse = {
		plan_type: "pro",
		rate_limit: {
			allowed: false,
			limit_reached: true,
			primary_window: win(100, 604_800),
			secondary_window: null,
		},
		additional_rate_limits: [
			{
				limit_name: "GPT-5.3-Codex-Spark",
				metered_feature: "codex_bengalfox",
				rate_limit: {
					allowed: true,
					limit_reached: false,
					primary_window: win(0, 18_000),
					secondary_window: win(4, 604_800),
				},
			},
		],
		credits: null,
		model_usage: {},
	};

	it("puts the account windows first, unprefixed", () => {
		expect(labelWindows(api)[0].label).toBe("7d");
	});

	it("prefixes per-model windows so they cannot read as account headroom", () => {
		const labels = labelWindows(api).map((w) => w.label);
		expect(labels).toEqual(["7d", "GPT-5.3-Codex-Spark 5h", "GPT-5.3-Codex-Spark 7d"]);
	});

	it("gives every window a distinct key for the render list", () => {
		const keys = labelWindows(api).map((w) => w.key);
		expect(new Set(keys).size).toBe(keys.length);
	});

	it("falls back to the metered feature when a limit has no display name", () => {
		const unnamed: CodexUsageApiResponse = {
			...api,
			additional_rate_limits: [
				{
					limit_name: null,
					metered_feature: "codex_bengalfox",
					rate_limit: { allowed: true, limit_reached: false, primary_window: win(3, 18_000), secondary_window: null },
				},
			],
		};
		expect(labelWindows(unnamed)[1].label).toBe("codex_bengalfox 5h");
	});

	it("returns nothing when the plan exposes no windows", () => {
		expect(
			labelWindows({ plan_type: null, rate_limit: null, additional_rate_limits: [], credits: null, model_usage: {} }),
		).toEqual([]);
	});
});

describe("barHeightPercent", () => {
	it("scales each day against the busiest day", () => {
		expect(barHeightPercent(3_000_000_000, 3_000_000_000)).toBe(100);
		expect(barHeightPercent(1_500_000_000, 3_000_000_000)).toBe(50);
	});

	it("keeps a quiet day visible instead of collapsing it to nothing", () => {
		expect(barHeightPercent(1, 3_000_000_000)).toBe(2);
	});

	it("does not divide by a zero peak", () => {
		expect(barHeightPercent(0, 0)).toBe(0);
	});
});

describe("CodexUsageDashboard refresh", () => {
	const REFRESH_MS = 5 * 60 * 1000;

	const emptyApi: CodexUsageApiResponse = {
		plan_type: "pro",
		rate_limit: null,
		additional_rate_limits: [],
		credits: null,
		model_usage: {},
	};

	const emptyStats = {
		stats: {
			lifetime_tokens: null,
			peak_daily_tokens: null,
			current_streak_days: null,
			longest_streak_days: null,
			total_threads: null,
			longest_running_turn_sec: null,
			fast_mode_usage_percentage: null,
			total_skills_used: null,
			unique_skills_used: null,
			most_used_reasoning_effort: null,
			most_used_reasoning_effort_percentage: null,
			daily_usage_buckets: [],
		},
	};

	/** Calls the dashboard made for one endpoint — other stores share the mock. */
	const callsFor = (command: string) => mockInvoke.mock.calls.filter((c) => c[0] === command).length;

	const respondOk = () =>
		mockInvoke.mockImplementation((command: string) =>
			Promise.resolve(
				command === "get_provider_quotas"
					? [
							{
								key: { owner: "codex-cli", provider: "openai-codex", credential: "/codex" },
								session_ids: ["codex"],
								state: "fresh",
								snapshot: {
									windows: [],
									detail: { provider: "codex", observed_at_ms: 1, response: emptyApi },
									plan: "pro",
									credits: null,
									account: null,
									observed_at_ms: 1,
									source: "active",
								},
							},
						]
					: emptyStats,
			),
		);

	beforeEach(() => {
		vi.useFakeTimers();
		mockInvoke.mockReset();
		respondOk();
	});

	afterEach(() => {
		providerQuotaStore.stop();
		vi.useRealTimers();
	});

	it("refreshes local stats but never polls provider quotas on the interval", async () => {
		render(() => CodexUsageDashboard({}));
		await vi.advanceTimersByTimeAsync(0);
		expect(callsFor("get_codex_usage_api")).toBe(0);
		expect(callsFor("get_provider_quotas")).toBe(1);

		await vi.advanceTimersByTimeAsync(REFRESH_MS);

		expect(callsFor("get_codex_usage_api")).toBe(0);
		expect(callsFor("get_provider_quotas")).toBe(1);
		expect(callsFor("get_codex_usage_stats")).toBe(2);
	});

	it("stops refreshing once the tab is closed", async () => {
		const { unmount } = render(() => CodexUsageDashboard({}));
		await vi.advanceTimersByTimeAsync(0);
		unmount();

		await vi.advanceTimersByTimeAsync(3 * REFRESH_MS);

		expect(callsFor("get_codex_usage_api")).toBe(0);
		expect(callsFor("get_provider_quotas")).toBe(1);
	});

	it("clears a stale error once a later refresh succeeds", async () => {
		const warning = vi.spyOn(appLogger, "warn").mockImplementation(() => {});
		mockInvoke.mockImplementation((command: string) =>
			command === "get_codex_usage_stats" ? Promise.reject(new Error("codex offline")) : Promise.resolve([]),
		);

		const { container } = render(() => CodexUsageDashboard({}));
		await vi.advanceTimersByTimeAsync(0);
		expect(container.textContent).toContain("codex offline");
		expect(warning).toHaveBeenCalledWith("network", "Codex stats fetch failed", "Error: codex offline");

		respondOk();
		await vi.advanceTimersByTimeAsync(REFRESH_MS);

		expect(container.textContent).not.toContain("codex offline");
		warning.mockRestore();
	});
});
