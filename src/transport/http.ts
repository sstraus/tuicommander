import { getRemoteBaseUrl, resolveOwningConnection, transportLogger, withRemoteToken } from "../transportRuntime";
import { COMMAND_TABLE, mapCommandToHttp } from "./commandTable";
import { isTauri } from "./environment";

export class HttpRpcError extends Error {
	public readonly detail: string;

	constructor(
		public readonly command: string,
		public readonly status: number,
		public readonly body: string,
	) {
		super(`RPC ${command} failed: ${status} ${body}`);
		this.name = "HttpRpcError";
		let detail = body;
		try {
			const parsed: unknown = JSON.parse(body);
			if (parsed && typeof parsed === "object" && "error" in parsed && typeof parsed.error === "string") {
				detail = parsed.error;
			}
		} catch {
			// Plain-text HTTP errors already carry their useful message in body.
		}
		this.detail = detail;
	}
}

/** Build a full URL for HTTP transport using current window origin or a remote baseUrl */
export function buildHttpUrl(path: string, baseUrl?: string): string {
	if (baseUrl) return `${baseUrl}${path}`;
	if (typeof window !== "undefined" && window.location?.origin) {
		return `${window.location.origin}${path}`;
	}
	return path;
}

/**
 * In-flight deduplication for idempotent (GET) RPC calls.
 * Concurrent identical calls share the same Promise — cleared on settle.
 */
const _inflight = new Map<string, Promise<unknown>>();

/** True if the command maps to an HTTP GET (idempotent, safe to deduplicate). */
function isIdempotentRpc(command: string, args: Record<string, unknown>): boolean {
	try {
		return mapCommandToHttp(command, args).method === "GET";
	} catch {
		return false;
	}
}

/**
 * Per-session single-flight queue: one request in flight, and everything that
 * piles up behind it leaves as ONE follow-up request.
 *
 * Two call sites need this, for opposite reasons, which is why `merge` is a
 * parameter rather than baked in:
 *
 * - `write_pty` — parallel HTTP POSTs can arrive out of order and reorder the
 *   letters the user typed, so writes must be chained. Chaining alone capped
 *   typing at one character per round trip, so keystrokes that arrive during a
 *   request accumulate. Nothing may be dropped: `merge` appends.
 * - `resize_pty` — a drag fires one per frame, the backend reflows on the
 *   blocking pool, and two in flight race for the per-session lock. Applied
 *   newest-first they leave the PTY at the OLD size with nothing to correct it.
 *   Only the newest size means anything: `merge` replaces, so an intermediate
 *   size is never in flight and can never land last.
 */
interface CoalescingQueue<P> {
	/** Resolves when the queue, including anything still pending, has drained. */
	tail: Promise<unknown>;
	/** What arrived during the in-flight request, already merged. */
	pending: P | null;
	/** Set once a flush is chained for `pending`, so it is not chained twice. */
	flushChained: boolean;
}

/**
 * @param merge Folds a newly arrived payload into whatever is already waiting.
 *   Receives `null` when nothing is waiting yet.
 */
function coalescedRpc<P>(
	queues: Map<string, CoalescingQueue<P>>,
	queueKey: string,
	payload: P,
	merge: (pending: P | null, next: P) => P,
	send: (payload: P) => Promise<unknown>,
): Promise<unknown> {
	const existing = queues.get(queueKey);
	if (existing) {
		existing.pending = merge(existing.pending, payload);
		if (!existing.flushChained) {
			existing.flushChained = true;
			// Chain on failure too: dropping the batch would reorder the line
			// just as surely as a parallel POST would.
			const flush = () => {
				const batch = existing.pending as P;
				existing.pending = null;
				existing.flushChained = false;
				return send(batch).finally(() => reapDrainedQueue(queues, queueKey, existing));
			};
			existing.tail = existing.tail.then(flush, flush);
		}
		return existing.tail;
	}
	const queue: CoalescingQueue<P> = { tail: Promise.resolve(), pending: null, flushChained: false };
	queues.set(queueKey, queue);
	queue.tail = send(payload).finally(() => reapDrainedQueue(queues, queueKey, queue));
	return queue.tail;
}

/** Forget a queue once nothing is in flight and nothing is waiting behind it. */
function reapDrainedQueue<P>(
	queues: Map<string, CoalescingQueue<P>>,
	queueKey: string,
	queue: CoalescingQueue<P>,
): void {
	if (queues.get(queueKey) === queue && !queue.flushChained) {
		queues.delete(queueKey);
	}
}

const _writeQueues = new Map<string, CoalescingQueue<string[]>>();
const _resizeQueues = new Map<string, CoalescingQueue<{ rows: number; cols: number }>>();

/** Two connections to the same session id are two different backends. */
function queueKeyFor(sessionId: string, connectionId?: string): string {
	return connectionId ? `${connectionId}:${sessionId}` : sessionId;
}

/** Commands already reported as unroutable, so the warning below fires once each. */
const _hostOnlyOnRemoteWarned = new Set<string>();

/**
 * The remote connection this call belongs to, or undefined for a local one.
 *
 * Only a command with an HTTP mapping can be routed. The host-only ones
 * (`INTENTIONALLY_UNMAPPED`) and the WS-streamed ones have no request/response
 * route to send, so routing one would turn a working local call into a throw.
 * They keep running on this machine — and say so once, instead of leaving a
 * remote repo looking as if it were local.
 *
 * Exported for `invoke()`, which short-circuits to Tauri IPC on the desktop and
 * so never reaches `rpc()`. Both entry points ask this one function; nothing
 * else decides where a call goes.
 */
export function owningConnectionFor(command: string, args: Record<string, unknown>): string | undefined {
	const owner = resolveOwningConnection(args);
	if (!owner) return undefined;
	if (COMMAND_TABLE[command]) return owner;
	if (!_hostOnlyOnRemoteWarned.has(command)) {
		_hostOnlyOnRemoteWarned.add(command);
		transportLogger().warn("network", `"${command}" has no remote route and ran on the local machine`, {
			command,
			connectionId: owner,
		});
	}
	return undefined;
}

/**
 * RPC call — uses Tauri invoke() or HTTP fetch() based on environment.
 * Concurrent identical idempotent calls are coalesced into a single in-flight request.
 * write_pty calls are serialized per-session in browser mode to prevent reordering.
 * Usage: `const result = await rpc<string>("create_pty", { config });`
 *
 * `explicitConnectionId` is for the callers that hold a connection but no repo
 * yet — probing a machine, or reading a path before it is registered. Everything
 * else leaves it out and is routed from its own arguments.
 */
export function rpc<T>(command: string, args: Record<string, unknown> = {}, explicitConnectionId?: string): Promise<T> {
	const connectionId = explicitConnectionId ?? owningConnectionFor(command, args);
	// Serialize write_pty per session in browser mode to prevent letter reordering
	if (command === "write_pty" && (!isTauri() || connectionId)) {
		const sessionId = (args.sessionId ?? args.id) as string;
		if (sessionId && typeof args.data === "string") {
			return coalescedRpc(
				_writeQueues,
				// Keyed with the connection: two connections to the same session id
				// are two different backends, and their bytes must not merge.
				queueKeyFor(sessionId, connectionId),
				[args.data],
				(pending, next) => (pending ?? []).concat(next),
				// One round trip either way, but the inputs stay SEPARATE: the
				// backend runs its per-input bookkeeping once per part, and joining
				// them would change what it reads (a lone "/" opens slash mode, an
				// exact option key clears a choice prompt). A solitary keystroke has
				// no batch to describe, so it keeps the single-input route.
				(parts) =>
					parts.length === 1
						? rpcImpl<T>(command, { ...args, data: parts[0] }, connectionId)
						: rpcImpl<T>("write_pty_parts", { sessionId, parts }, connectionId),
			) as Promise<T>;
		}
	}
	// Unlike writes, this is NOT browser-only: `resize_pty` is an async Tauri
	// command, so the desktop hands each call to the blocking pool too and has
	// the same newest-first hazard.
	if (command === "resize_pty") {
		const sessionId = (args.sessionId ?? args.id) as string;
		if (sessionId && typeof args.rows === "number" && typeof args.cols === "number") {
			return coalescedRpc(
				_resizeQueues,
				queueKeyFor(sessionId, connectionId),
				{ rows: args.rows, cols: args.cols },
				(_pending, next) => next,
				(dims) => rpcImpl<T>(command, { ...args, ...dims }, connectionId),
			) as Promise<T>;
		}
	}
	// Desktop talks to the backend over Tauri invoke(), never HTTP, so the
	// GET/POST distinction isIdempotentRpc()/mapCommandToHttp() computes is
	// meaningless here — but every desktop RPC still paid for a COMMAND_TABLE
	// lookup (and a thrown+caught Error for every command not in that table,
	// which is most of them) to work that out. Skip straight to invoke().
	if (!connectionId && isTauri()) {
		return rpcImpl<T>(command, args, connectionId);
	}
	if (isIdempotentRpc(command, args)) {
		const key = connectionId
			? `${connectionId}:${command}:${JSON.stringify(args)}`
			: `${command}:${JSON.stringify(args)}`;
		const existing = _inflight.get(key) as Promise<T> | undefined;
		if (existing) return existing;
		const promise = rpcImpl<T>(command, args, connectionId).finally(() => _inflight.delete(key));
		_inflight.set(key, promise as Promise<unknown>);
		return promise;
	}
	return rpcImpl<T>(command, args, connectionId);
}

/** Cached after the first resolution — `import()` of an already-loaded module
 *  is cheap, but calling it fresh on every desktop RPC (once per keystroke) still
 *  pays a module-registry lookup + Promise wrap that a plain reference avoids. */
let cachedTauriInvoke: typeof import("@tauri-apps/api/core").invoke | undefined;

async function rpcImpl<T>(command: string, args: Record<string, unknown>, connectionId?: string): Promise<T> {
	// When connectionId is provided, always use HTTP fetch (remote daemon is accessed via HTTP)
	if (!connectionId && isTauri()) {
		if (!cachedTauriInvoke) {
			({ invoke: cachedTauriInvoke } = await import("@tauri-apps/api/core"));
		}
		// Only pass args if non-empty (matches Tauri invoke signature)
		if (Object.keys(args).length > 0) {
			return cachedTauriInvoke<T>(command, args);
		}
		return cachedTauriInvoke<T>(command);
	}

	const mapping = mapCommandToHttp(command, args);
	const baseUrl = connectionId ? getRemoteBaseUrl(connectionId) : undefined;
	if (connectionId && !baseUrl) {
		throw new Error(`Remote connection ${connectionId} not connected`);
	}
	const requestUrl = buildHttpUrl(mapping.path, baseUrl);
	const url = withRemoteToken(requestUrl, connectionId);

	// No client-side deadline: Tauri invoke() has none, and a fixed cap here cut
	// backend calls that own a longer deadline (ego initialize: 60 s) with
	// "signal is aborted" instead of the backend's own message.
	const init: RequestInit = {
		method: mapping.method,
		headers: { "Content-Type": "application/json" },
	};
	if (mapping.body !== undefined) {
		init.body = JSON.stringify(mapping.body);
	}

	const resp = await fetch(url, init);
	if (!resp.ok) {
		if (resp.status === 404 && mapping.notFoundAsNull) {
			return null as T;
		}
		const text = await resp.text().catch(() => resp.statusText);
		throw new HttpRpcError(command, resp.status, text);
	}

	const contentType = resp.headers.get("content-type") || "";
	let data: unknown;
	if (contentType.includes("application/octet-stream")) {
		// Packed binary (styled row chunks). Text/JSON decoding would corrupt it,
		// and an empty body is a legitimate "nothing to send", so this returns the
		// buffer directly and skips the empty-body guard below.
		const buffer = await resp.arrayBuffer();
		return (mapping.transform ? mapping.transform(buffer) : buffer) as T;
	}
	const text = await resp.text();
	if (text.length === 0) {
		throw new Error(`RPC ${command}: empty response body`);
	}
	if (contentType.includes("application/json")) {
		try {
			data = JSON.parse(text);
		} catch (error) {
			const detail = error instanceof Error ? `: ${error.message}` : "";
			throw new Error(`RPC ${command}: invalid JSON response${detail}`);
		}
	} else if (mapping.allowText && (!contentType || contentType.toLowerCase().split(";")[0].trim() === "text/plain")) {
		data = text;
	} else {
		// Some JSON routes omit content-type. Never pass a failed decode to the
		// consumer as a value: an HTML fallback is not a home directory.
		try {
			if (contentType) throw new Error("Unexpected content type");
			data = JSON.parse(text);
		} catch {
			// Use the URL before the connection token is attached.
			throw new Error(
				`RPC ${command}: expected JSON from ${requestUrl}, received ${contentType || "missing content-type"}`,
			);
		}
	}

	if (mapping.transform) {
		return mapping.transform(data) as T;
	}
	if (data === undefined) {
		throw new Error(`RPC ${command}: empty response body`);
	}
	return data as T;
}
