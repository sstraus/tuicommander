/** Rust owns account/session selection; this presenter owns display words and client-local time. */
export interface QuotaWindow {
	provider: string;
	plan: string | null;
	window_seconds: number | null;
	window_name: string;
	used_percent: number;
	resets_at: string | null;
}

export function formatUsageTime(timestamp: string | number, timeZone?: string): string | null {
	const date = new Date(timestamp);
	if (!Number.isFinite(date.getTime())) return null;
	return new Intl.DateTimeFormat(undefined, {
		timeZone,
		month: "short",
		day: "numeric",
		hour: "2-digit",
		minute: "2-digit",
		hourCycle: "h23",
	}).format(date);
}

export function formatQuotaPlan(quota: QuotaWindow): string {
	const plan = quota.plan ? ` ${quota.plan}` : "";
	return `${quota.provider}${plan} plan`;
}

export function formatQuotaWindow(quota: QuotaWindow, timeZone?: string): string {
	return `${formatQuotaPlan(quota)}, ${formatQuotaWindowDetails(quota, timeZone)}`;
}

/** Window-only wording for a quota list with one shared plan heading. */
export function formatQuotaWindowDetails(quota: QuotaWindow, timeZone?: string): string {
	const seconds = quota.window_seconds;
	let window = quota.window_name;
	if (seconds && seconds > 0) {
		const [unit, divisor] =
			seconds % 86400 === 0
				? (["day", 86400] as const)
				: seconds % 3600 === 0
					? (["hour", 3600] as const)
					: (["minute", 60] as const);
		window = `${seconds / divisor}-${unit}`;
	}
	const reset = quota.resets_at ? formatUsageTime(quota.resets_at, timeZone) : null;
	return `${window} window: ${quota.used_percent}% used${reset ? `, resets ${reset} local` : ""}`;
}

/** Compact token counts: 25241643691 -> "25.2B". Raw digits are unreadable. */
export function formatTokens(n: number | null): string {
	if (n === null || !Number.isFinite(n)) return "--";
	const abs = Math.abs(n);
	if (abs >= 1e9) return `${(n / 1e9).toFixed(1)}B`;
	if (abs >= 1e6) return `${(n / 1e6).toFixed(1)}M`;
	if (abs >= 1e3) return `${(n / 1e3).toFixed(1)}K`;
	return String(n);
}
