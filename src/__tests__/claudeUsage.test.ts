import { describe, expect, it } from "vitest";
import "./mocks/tauri";
import { describeUsageError } from "../features/agentUsage";
import { buildTickerText, formatResetCompact, getTickerPriority } from "../features/claudeUsage";

const NOW = Date.UTC(2026, 5, 11, 12, 0, 0); // fixed reference instant

function isoIn(ms: number): string {
	return new Date(NOW + ms).toISOString();
}

const HOUR = 3_600_000;
const DAY = 24 * HOUR;

describe("formatResetCompact", () => {
	it("returns days when more than a day remains", () => {
		expect(formatResetCompact(isoIn(3 * DAY + 5 * HOUR), NOW)).toBe("3d");
	});

	it("returns hours when less than a day remains", () => {
		expect(formatResetCompact(isoIn(5 * HOUR + 30 * 60_000), NOW)).toBe("5h");
	});

	it("returns minutes when less than an hour remains", () => {
		expect(formatResetCompact(isoIn(12 * 60_000), NOW)).toBe("12m");
	});

	it("returns null for a past or null reset", () => {
		expect(formatResetCompact(isoIn(-HOUR), NOW)).toBeNull();
		expect(formatResetCompact(null, NOW)).toBeNull();
		expect(formatResetCompact("not-a-date", NOW)).toBeNull();
	});
});

describe("buildTickerText", () => {
	const empty = {
		five_hour: null,
		seven_day: null,
		seven_day_oauth_apps: null,
		seven_day_opus: null,
		seven_day_sonnet: null,
		seven_day_cowork: null,
		extra_usage: null,
		plan: null,
		meta: null,
	};

	it("appends the compact countdown to the 7d bucket", () => {
		const text = buildTickerText(
			{
				...empty,
				five_hour: { utilization: 12, resets_at: null },
				seven_day: { utilization: 5, resets_at: isoIn(3 * DAY + 5 * HOUR) },
			},
			NOW,
		);
		expect(text).toBe("5h: 12% · 7d: 5% -3d");
	});

	it("omits the countdown when the 7d bucket has no reset date", () => {
		const text = buildTickerText({
			...empty,
			seven_day: { utilization: 68, resets_at: null },
		});
		expect(text).toBe("7d: 68%");
	});

	it("returns 'no data' when no buckets are present", () => {
		expect(buildTickerText(empty)).toBe("no data");
	});

	it("falls back to extra_usage when five_hour/seven_day are both null (enterprise plans)", () => {
		// Ground-truth shape captured from a real enterprise-plan /api/oauth/usage response.
		const text = buildTickerText({
			...empty,
			extra_usage: {
				is_enabled: true,
				monthly_limit: 500000,
				used_credits: 162709,
				utilization: 32.54,
				resets_at: null,
				in_use: false,
			},
		});
		expect(text).toBe("usage: 33%");
	});

	it("appends the reset countdown to the extra_usage fallback when present", () => {
		const text = buildTickerText(
			{
				...empty,
				extra_usage: {
					is_enabled: true,
					monthly_limit: 1000,
					used_credits: 900,
					utilization: 90,
					resets_at: isoIn(3 * DAY + 5 * HOUR),
					in_use: true,
				},
			},
			NOW,
		);
		expect(text).toBe("usage: 90% -3d");
	});

	it("prefers five_hour/seven_day over extra_usage when both are present", () => {
		const text = buildTickerText({
			...empty,
			five_hour: { utilization: 12, resets_at: null },
			extra_usage: {
				is_enabled: true,
				monthly_limit: 1000,
				used_credits: 500,
				utilization: 50,
				resets_at: null,
				in_use: false,
			},
		});
		expect(text).toBe("5h: 12%");
	});

	it("stays 'no data' when extra_usage is disabled", () => {
		const text = buildTickerText({
			...empty,
			extra_usage: {
				is_enabled: false,
				monthly_limit: null,
				used_credits: null,
				utilization: null,
				resets_at: null,
				in_use: false,
			},
		});
		expect(text).toBe("no data");
	});

	it("does not fall back to extra_usage when a hidden bucket (oauth_apps/cowork) has real data", () => {
		// five_hour/seven_day are null, but seven_day_oauth_apps is populated — a real bottleneck
		// in a bucket the ticker text doesn't render. Showing the low extra_usage figure here
		// would mask that bottleneck, so it must stay "no data" instead of misreporting.
		expect(
			buildTickerText({
				...empty,
				seven_day_oauth_apps: { utilization: 99, resets_at: null },
				extra_usage: {
					is_enabled: true,
					monthly_limit: 1000,
					used_credits: 100,
					utilization: 10,
					resets_at: null,
					in_use: false,
				},
			}),
		).toBe("no data");
		expect(
			buildTickerText({
				...empty,
				seven_day_cowork: { utilization: 99, resets_at: null },
				extra_usage: {
					is_enabled: true,
					monthly_limit: 1000,
					used_credits: 100,
					utilization: 10,
					resets_at: null,
					in_use: false,
				},
			}),
		).toBe("no data");
	});
});

describe("getTickerPriority", () => {
	const empty = {
		five_hour: null,
		seven_day: null,
		seven_day_oauth_apps: null,
		seven_day_opus: null,
		seven_day_sonnet: null,
		seven_day_cowork: null,
		extra_usage: null,
		plan: null,
		meta: null,
	};

	it("returns critical priority only from 96 percent, catching premature blinking", () => {
		expect(getTickerPriority({ ...empty, five_hour: { utilization: 96, resets_at: null } })).toBe(96);
		expect(getTickerPriority({ ...empty, seven_day_opus: { utilization: 99, resets_at: null } })).toBe(96);
	});

	it("returns 50 at the 70% boundary (below 96)", () => {
		expect(getTickerPriority({ ...empty, seven_day: { utilization: 70, resets_at: null } })).toBe(50);
		expect(getTickerPriority({ ...empty, seven_day: { utilization: 95.9, resets_at: null } })).toBe(50);
	});

	it("returns 10 for low utilization or no data", () => {
		expect(getTickerPriority({ ...empty, five_hour: { utilization: 5, resets_at: null } })).toBe(10);
		expect(getTickerPriority(empty)).toBe(10);
	});

	it("uses the maximum across all rate buckets", () => {
		expect(
			getTickerPriority({
				...empty,
				five_hour: { utilization: 10, resets_at: null },
				seven_day_sonnet: { utilization: 96, resets_at: null },
			}),
		).toBe(96);
	});

	it("includes extra_usage utilization in the max (enterprise plans with no rate buckets)", () => {
		expect(
			getTickerPriority({
				...empty,
				extra_usage: {
					is_enabled: true,
					monthly_limit: 1000,
					used_credits: 950,
					utilization: 96,
					resets_at: null,
					in_use: true,
				},
			}),
		).toBe(96);
		expect(
			getTickerPriority({
				...empty,
				extra_usage: {
					is_enabled: true,
					monthly_limit: 1000,
					used_credits: 750,
					utilization: 75,
					resets_at: null,
					in_use: true,
				},
			}),
		).toBe(50);
		expect(
			getTickerPriority({
				...empty,
				extra_usage: {
					is_enabled: true,
					monthly_limit: 1000,
					used_credits: 320,
					utilization: 32,
					resets_at: null,
					in_use: false,
				},
			}),
		).toBe(10);
	});

	it("blinks on an over-limit extra_usage (utilization above 100) and shows it uncapped", () => {
		const over = {
			...empty,
			extra_usage: {
				is_enabled: true,
				monthly_limit: 1000,
				used_credits: 1200,
				utilization: 120,
				resets_at: null,
				in_use: true,
			},
		};
		expect(getTickerPriority(over)).toBe(96);
		expect(buildTickerText(over)).toBe("usage: 120%");
	});

	it("handles enabled extra_usage with a null utilization (zero/absent limit) without NaN", () => {
		const noPercent = {
			...empty,
			extra_usage: {
				is_enabled: true,
				monthly_limit: 0,
				used_credits: null,
				utilization: null,
				resets_at: null,
				in_use: false,
			},
		};
		expect(getTickerPriority(noPercent)).toBe(10);
		expect(buildTickerText(noPercent)).toBe("no data");
	});

	it("ignores extra_usage utilization when disabled", () => {
		expect(
			getTickerPriority({
				...empty,
				extra_usage: {
					is_enabled: false,
					monthly_limit: null,
					used_credits: null,
					utilization: 95,
					resets_at: null,
					in_use: false,
				},
			}),
		).toBe(10);
	});

	it("escalates on seven_day_oauth_apps/seven_day_cowork even though the ticker text never renders them", () => {
		expect(getTickerPriority({ ...empty, seven_day_oauth_apps: { utilization: 99, resets_at: null } })).toBe(96);
		expect(getTickerPriority({ ...empty, seven_day_cowork: { utilization: 75, resets_at: null } })).toBe(50);
	});
});

describe("describeUsageError (claude)", () => {
	// poll() calls String(err) before passing the result here (err may be an Error
	// thrown locally, or a plain string — Tauri command rejections arrive as strings).
	it("recognizes a missing OAuth token", () => {
		expect(describeUsageError(String(new Error("No Claude OAuth token found")), "No Claude OAuth token")).toBe(
			"no token",
		);
	});

	it("recognizes 401/403 as an expired token", () => {
		expect(describeUsageError(String(new Error("API returned 401: unauthorized")), "No Claude OAuth token")).toBe(
			"token expired",
		);
		expect(describeUsageError(String(new Error("API returned 403: forbidden")), "No Claude OAuth token")).toBe(
			"token expired",
		);
	});

	it("recognizes a parse failure as an API shape change", () => {
		expect(
			describeUsageError(String(new Error("Failed to parse API response: unexpected EOF")), "No Claude OAuth token"),
		).toBe("API changed");
	});

	it("falls back to 'offline' for anything else", () => {
		expect(describeUsageError(String(new Error("API request failed: network error")), "No Claude OAuth token")).toBe(
			"offline",
		);
		expect(describeUsageError("Rate limited — waiting for backoff to expire", "No Claude OAuth token")).toBe("offline");
	});
});
