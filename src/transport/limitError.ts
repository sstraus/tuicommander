/**
 * The typed refusal a core command or a plugin client returns when the gateway
 * limits or sheds a call. Core commands carry it after the `rate-limit:` prefix of
 * their error string, which IPC rejects with and HTTP wraps in `{ "error": … }`;
 * plugin clients throw it as an object. Both read through this one function.
 */
export interface LimitError {
	kind: "rate_limited" | "shed";
	bucket: string | null;
	/** When the gateway lets the call through again, epoch milliseconds. */
	retry_at: number;
}

const PREFIX = "rate-limit:";

function fromObject(value: unknown): LimitError | null {
	if (!value || typeof value !== "object") return null;
	const { kind, bucket, retry_at } = value as Record<string, unknown>;
	if (kind !== "rate_limited" && kind !== "shed") return null;
	if (typeof retry_at !== "number" || !Number.isFinite(retry_at)) return null;
	return { kind, bucket: typeof bucket === "string" ? bucket : null, retry_at };
}

function fromText(text: string): LimitError | null {
	if (!text.startsWith(PREFIX)) return null;
	try {
		return fromObject(JSON.parse(text.slice(PREFIX.length)));
	} catch {
		return null;
	}
}

export function parseLimitError(error: unknown): LimitError | null {
	if (typeof error === "string") return fromText(error);
	const typed = fromObject(error);
	if (typed) return typed;
	if (error instanceof Error) {
		const detail = (error as { detail?: unknown }).detail;
		return fromText(typeof detail === "string" ? detail : error.message);
	}
	return null;
}
