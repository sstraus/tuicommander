import { AGENT_USAGE_BLINK_THRESHOLD } from "./usageThresholds";

/**
 * Claude Usage — types and ticker formatting.
 *
 * The lifecycle (polling, ticker writes, switching between agents) lives in
 * `agentUsage.ts`, which drives both Claude and Codex through one ticker slot.
 */

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface RateBucket {
	utilization: number;
	resets_at: string | null;
}

/** Spend-based overage bucket. The primary source of usage data for plans
 * (e.g. enterprise) whose accounts don't populate `five_hour`/`seven_day`. */
interface ExtraUsage {
	is_enabled: boolean;
	monthly_limit: number | null;
	used_credits: number | null;
	utilization: number | null;
	resets_at: string | null;
	in_use: boolean;
}

export interface UsageApiResponse {
	five_hour: RateBucket | null;
	seven_day: RateBucket | null;
	seven_day_oauth_apps: RateBucket | null;
	seven_day_opus: RateBucket | null;
	seven_day_sonnet: RateBucket | null;
	seven_day_cowork: RateBucket | null;
	extra_usage: ExtraUsage | null;
	plan: unknown;
	meta: unknown;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Compact countdown to a reset, e.g. "3d", "5h", "12m".
 * Returns null when there is no date or the reset is in the past.
 * Days use the same floor as the dashboard's formatResetTime so the two agree. */
export function formatResetCompact(isoStr: string | null, now: number = Date.now()): string | null {
	if (!isoStr) return null;
	const diffMs = new Date(isoStr).getTime() - now;
	if (Number.isNaN(diffMs) || diffMs <= 0) return null;
	const diffMin = Math.floor(diffMs / 60_000);
	if (diffMin < 60) return `${diffMin}m`;
	const diffHrs = Math.floor(diffMin / 60);
	if (diffHrs < 24) return `${diffHrs}h`;
	return `${Math.floor(diffHrs / 24)}d`;
}

/** True when any windowed rate-limit bucket (shown in the ticker text or not) has data.
 * Used to decide whether it's safe to fall back to `extra_usage` — if a bucket the ticker
 * doesn't render text for (e.g. `seven_day_oauth_apps`) is populated, showing an unrelated
 * `extra_usage` percentage instead could mask a real bottleneck in that hidden bucket. */
function hasAnyRateBucket(api: UsageApiResponse): boolean {
	return (
		api.five_hour !== null ||
		api.seven_day !== null ||
		api.seven_day_oauth_apps !== null ||
		api.seven_day_opus !== null ||
		api.seven_day_sonnet !== null ||
		api.seven_day_cowork !== null
	);
}

/** Build status bar ticker text from API data.
 * The API returns utilization as a direct percentage (e.g. 3.0 = 3%, 68.0 = 68%).
 * Some plans (e.g. enterprise/spend-based) never populate `five_hour`/`seven_day` —
 * for those, `extra_usage` is the only usage signal the API returns, so it's used
 * as a fallback rather than falling through to "no data". */
export function buildTickerText(api: UsageApiResponse, now: number = Date.now()): string {
	const parts: string[] = [];
	if (api.five_hour) {
		parts.push(`5h: ${Math.round(api.five_hour.utilization)}%`);
	}
	if (api.seven_day) {
		const reset = formatResetCompact(api.seven_day.resets_at, now);
		const suffix = reset ? ` -${reset}` : "";
		parts.push(`7d: ${Math.round(api.seven_day.utilization)}%${suffix}`);
	}
	if (
		parts.length === 0 &&
		!hasAnyRateBucket(api) &&
		api.extra_usage?.is_enabled &&
		api.extra_usage.utilization != null
	) {
		const reset = formatResetCompact(api.extra_usage.resets_at, now);
		const suffix = reset ? ` -${reset}` : "";
		parts.push(`usage: ${Math.round(api.extra_usage.utilization)}%${suffix}`);
	}
	return parts.length > 0 ? parts.join(" · ") : "no data";
}

/** Determine ticker priority from usage levels.
 * Utilization values are direct percentages (0-100). Includes every windowed rate bucket
 * (not just the ones rendered in the ticker text) plus `extra_usage`, so a bottleneck in a
 * bucket the ticker doesn't display (e.g. `seven_day_oauth_apps`) still escalates priority. */
export function getTickerPriority(api: UsageApiResponse): number {
	const utils = [
		api.five_hour,
		api.seven_day,
		api.seven_day_oauth_apps,
		api.seven_day_opus,
		api.seven_day_sonnet,
		api.seven_day_cowork,
	]
		.filter((b): b is RateBucket => b !== null)
		.map((b) => b.utilization);
	if (api.extra_usage?.is_enabled && api.extra_usage.utilization != null) {
		utils.push(api.extra_usage.utilization);
	}
	const maxUtil = Math.max(0, ...utils);
	if (maxUtil >= AGENT_USAGE_BLINK_THRESHOLD) return AGENT_USAGE_BLINK_THRESHOLD;
	if (maxUtil >= 70) return 50;
	return 10;
}
