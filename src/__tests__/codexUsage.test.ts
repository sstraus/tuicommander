import { describe, expect, it } from "vitest";
import { describeUsageError, toUsageAgent } from "../features/agentUsage";
import {
	buildCodexTickerText,
	type CodexUsageApiResponse,
	formatResetAfter,
	getCodexTickerPriority,
	windowLabel,
} from "../features/codexUsage";
import { HttpRpcError } from "../transport";

const empty: CodexUsageApiResponse = {
	plan_type: null,
	rate_limit: null,
	additional_rate_limits: [],
	credits: null,
	model_usage: {},
};

/** The payload the live endpoint returned while the weekly limit was exhausted. */
const exhausted: CodexUsageApiResponse = {
	plan_type: "pro",
	rate_limit: {
		allowed: false,
		limit_reached: true,
		primary_window: {
			used_percent: 100,
			limit_window_seconds: 604_800,
			reset_after_seconds: 143_951,
			reset_at: 1_788_747_989,
		},
		secondary_window: null,
	},
	additional_rate_limits: [
		{
			limit_name: "GPT-5.3-Codex-Spark",
			metered_feature: "codex_bengalfox",
			rate_limit: {
				allowed: true,
				limit_reached: false,
				primary_window: {
					used_percent: 0,
					limit_window_seconds: 18_000,
					reset_after_seconds: 18_000,
					reset_at: 1_788_622_039,
				},
				secondary_window: null,
			},
		},
	],
	credits: { has_credits: false, unlimited: false, balance: "0" },
	model_usage: {},
};

describe("windowLabel", () => {
	it("names windows by duration because Codex does not label them", () => {
		expect(windowLabel(18_000)).toBe("5h");
		expect(windowLabel(604_800)).toBe("7d");
	});

	it("degrades to a marker rather than inventing a window", () => {
		expect(windowLabel(null)).toBe("?");
		expect(windowLabel(0)).toBe("?");
	});
});

describe("formatResetAfter", () => {
	it("uses seconds-remaining so a wrong local clock cannot shift the countdown", () => {
		expect(formatResetAfter(30 * 60)).toBe("30m");
		expect(formatResetAfter(5 * 3600)).toBe("5h");
		expect(formatResetAfter(143_951)).toBe("1d");
	});

	it("returns null for an elapsed or absent reset", () => {
		expect(formatResetAfter(0)).toBeNull();
		expect(formatResetAfter(-10)).toBeNull();
		expect(formatResetAfter(null)).toBeNull();
	});
});

describe("buildCodexTickerText", () => {
	it("shows the account window with its countdown", () => {
		expect(buildCodexTickerText(exhausted)).toBe("7d: 100% -1d");
	});

	it("joins both account windows when the plan has two", () => {
		const text = buildCodexTickerText({
			...exhausted,
			rate_limit: {
				allowed: true,
				limit_reached: false,
				primary_window: {
					used_percent: 12,
					limit_window_seconds: 18_000,
					reset_after_seconds: 3600,
					reset_at: null,
				},
				secondary_window: {
					used_percent: 40,
					limit_window_seconds: 604_800,
					reset_after_seconds: null,
					reset_at: null,
				},
			},
		});
		expect(text).toBe("5h: 12% -1h · 7d: 40%");
	});

	it("says so rather than rendering an empty ticker", () => {
		expect(buildCodexTickerText(empty)).toBe("no data");
	});

	it("does not leak a per-model limit into the account reading", () => {
		// The Spark limit is a cold 5h window; rendering it beside the account
		// number would suggest headroom while the user is hard blocked.
		expect(buildCodexTickerText(exhausted)).not.toContain("5h");
	});
});

describe("getCodexTickerPriority", () => {
	it("escalates on the hottest window, account or per-model", () => {
		expect(getCodexTickerPriority(exhausted)).toBe(96);
	});

	it("escalates on a per-model limit even when the account is cold", () => {
		const hotModel: CodexUsageApiResponse = {
			...exhausted,
			rate_limit: {
				allowed: true,
				limit_reached: false,
				primary_window: {
					used_percent: 5,
					limit_window_seconds: 604_800,
					reset_after_seconds: null,
					reset_at: null,
				},
				secondary_window: null,
			},
			additional_rate_limits: [
				{
					limit_name: "GPT-5.3-Codex-Spark",
					metered_feature: "codex_bengalfox",
					rate_limit: {
						allowed: false,
						limit_reached: true,
						primary_window: {
							used_percent: 96,
							limit_window_seconds: 18_000,
							reset_after_seconds: 60,
							reset_at: null,
						},
						secondary_window: null,
					},
				},
			],
		};
		expect(getCodexTickerPriority(hotModel)).toBe(96);
	});

	it("does not escalate below 96 percent, catching premature blinking", () => {
		for (const used_percent of [90, 95, 95.9, 96]) {
			const api = {
				...empty,
				rate_limit: {
					...exhausted.rate_limit!,
					primary_window: { ...exhausted.rate_limit!.primary_window!, used_percent },
				},
			};
			expect(getCodexTickerPriority(api)).toBe(used_percent < 96 ? 50 : 96);
		}
	});

	it("stays quiet when nothing is close to a limit", () => {
		expect(getCodexTickerPriority(empty)).toBe(10);
	});
});

describe("toUsageAgent", () => {
	it("accepts only the agents that expose a usage API", () => {
		expect(toUsageAgent("claude")).toBe("claude");
		expect(toUsageAgent("codex")).toBe("codex");
		expect(toUsageAgent("grok")).toBe("grok");
	});

	it("rejects agents without one, so the ticker keeps the last known agent", () => {
		expect(toUsageAgent("opencode")).toBeNull();
		expect(toUsageAgent("gemini")).toBeNull();
		expect(toUsageAgent(null)).toBeNull();
		expect(toUsageAgent(undefined)).toBeNull();
	});
});

describe("describeUsageError", () => {
	// Catches: Codex registry cooldown errors falling through to offline.
	it("shows the Codex retry time across native and HTTP errors", () => {
		for (const error of [
			"Codex usage rate-limited, retry 20:56",
			new Error("Codex usage rate-limited, retry 20:56"),
			new HttpRpcError("get_codex_usage_api", 500, JSON.stringify({ error: "Codex usage rate-limited, retry 20:56" })),
		]) {
			expect(describeUsageError(error, "not logged in")).toBe("rate-limited, retry 20:56");
		}
	});

	it("separates a missing token from an expired one", () => {
		expect(describeUsageError("No Codex credentials at /x: nope", "No Codex OAuth token")).toBe("no token");
		expect(describeUsageError("Codex usage returned 401: {}", "No Codex OAuth token")).toBe("token expired");
	});

	it("names a moved endpoint, the failure mode an internal API invites", () => {
		expect(describeUsageError("Codex usage returned 404: {}", "No Codex OAuth token")).toBe("API moved");
	});

	it("surfaces a removed provider method as an API change", () => {
		expect(describeUsageError("Codex usage failed: Method not found", "not logged in")).toBe("API changed");
	});

	it("falls back to offline for anything unrecognised", () => {
		expect(describeUsageError("dns failure", "No Codex OAuth token")).toBe("offline");
	});
});
