/** Header a refresh sends; the gateway strips it and maps it to its Force freshness. */
export const FRESHNESS_HEADER = "x-tuic-freshness";

const forced = new WeakSet<AbortSignal>();

/**
 * Marks the signal of a refresh. Provider calls only carry a signal down to
 * `host.httpFetch`, so the mark is how a Refresh reaches the fetch of a plugin.
 */
export function forceFreshness(signal: AbortSignal): AbortSignal {
	forced.add(signal);
	return signal;
}

/** The request headers for `httpFetch`: the plugin's own plus the freshness request, or null for none. */
export function withFreshness(options?: {
	headers?: Record<string, string>;
	freshness?: "force";
	signal?: AbortSignal;
}): Record<string, string> | null {
	const force = options?.freshness === "force" || (options?.signal ? forced.has(options.signal) : false);
	if (!force) return options?.headers ?? null;
	return { ...options?.headers, [FRESHNESS_HEADER]: "force" };
}
