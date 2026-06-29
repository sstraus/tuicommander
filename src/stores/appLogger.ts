/**
 * Centralized application-wide log store.
 *
 * Captures errors, warnings, and info messages from all layers (UI, plugins,
 * git, network, etc.) into a bounded ring buffer. The ErrorLogPanel subscribes
 * to this store for display. Logs are also forwarded to the browser console.
 *
 * The local JS ring buffer serves as a fast reactive cache for the UI.
 * Every push is also fire-and-forget mirrored to the Rust backend ring buffer
 * via `push_log`, making logs durable across webview reloads.
 *
 * The local cache is split into two independent bounded pools keyed by audience
 * ("user" vs "diagnostic"). A flood of diagnostic telemetry (freeze/perf traces)
 * can only evict diagnostic entries — it can never crowd out the user-facing
 * signal the ErrorLogPanel shows by default. `getEntries()` merges both pools
 * back into a single chronological stream.
 */

import { batch, createSignal } from "solid-js";
import { rpc } from "../transport";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type AppLogLevel = "debug" | "info" | "warn" | "error";

/** Who a log entry is for:
 *  - `"user"`: actionable by the person using TUIC (a git op failed, an MCP can't
 *    connect, a file won't open). Shown in the ErrorLogPanel by default.
 *  - `"diagnostic"`: app-internal telemetry useful only when debugging TUIC itself
 *    (freeze detector, perf traces, circuit-breaker mechanics). Hidden by default
 *    and held in a separate bounded pool, so a burst of it can't evict
 *    user-relevant entries. */
export type AppLogAudience = "user" | "diagnostic";

export type AppLogSource =
	| "app"
	| "plugin"
	| "git"
	| "network"
	| "terminal"
	| "github"
	| "ci-heal"
	| "dictation"
	| "store"
	| "config"
	| "settings"
	| "mcp"
	| "prompts"
	| "push"
	| "tailscale"
	| "sw"
	| "files"
	| "ViewportLock"
	| "ai-chat"
	| "ai-agent"
	| "conversation"
	| "panel"
	| "panel-sync"
	| "editor"
	| "outline"
	| "references";

export interface AppLogEntry {
	id: number;
	timestamp: number;
	level: AppLogLevel;
	source: AppLogSource;
	message: string;
	data?: unknown;
	/** Audience for this entry; defaults to "user" when omitted. */
	audience?: AppLogAudience;
	/** How many consecutive duplicate messages were coalesced (0 = first, 1 = seen twice, etc.) */
	repeatCount?: number;
}

/** Max characters of a raw payload to keep when logging malformed/oversized
 *  network frames — enough to diagnose, bounded so a bad frame can't flood the
 *  ring buffer. */
const LOG_PAYLOAD_PREVIEW = 500;

/** Truncate an arbitrary payload string for safe inclusion in a log entry. */
export function previewLogPayload(value: string): string {
	return value.length > LOG_PAYLOAD_PREVIEW ? `${value.slice(0, LOG_PAYLOAD_PREVIEW)}...` : value;
}

/** Shape returned by the Rust get_logs command */
interface RustLogEntry {
	id: number;
	timestamp_ms: number;
	level: string;
	source: string;
	message: string;
	data_json?: string | null;
	audience?: string;
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

/** Capacity of the user-facing pool — the signal we never want to lose. */
const USER_MAX_ENTRIES = 1000;
/** Capacity of the diagnostic pool — transient debugging telemetry, bounded
 *  independently so it can't evict user entries. */
const DIAG_MAX_ENTRIES = 500;

let nextId = 1;

// ---------------------------------------------------------------------------
// Ring buffer
//
// A small FIFO ring shared by both audience pools. Entries are appended in id
// order (ids are globally monotonic), so `toArray` always yields an
// id-ascending, chronological slice — which `getEntries` relies on to merge the
// two pools.
// ---------------------------------------------------------------------------

interface Ring {
	readonly buf: AppLogEntry[];
	head: number;
	count: number;
	readonly max: number;
}

function makeRing(max: number): Ring {
	return { buf: [], head: 0, count: 0, max };
}

/** Most recently appended entry, or undefined when empty. */
function ringLast(r: Ring): AppLogEntry | undefined {
	if (r.count === 0) return undefined;
	const idx = r.count < r.max ? r.count - 1 : (r.head + r.count - 1) % r.max;
	return r.buf[idx];
}

/** Append an entry, evicting the oldest once at capacity. */
function ringPush(r: Ring, entry: AppLogEntry): void {
	if (r.count < r.max) {
		r.buf[r.count] = entry;
		r.count++;
	} else {
		r.buf[r.head] = entry;
		r.head = (r.head + 1) % r.max;
	}
}

/** Oldest-first snapshot of the ring contents. */
function ringToArray(r: Ring): AppLogEntry[] {
	const out: AppLogEntry[] = new Array(r.count);
	for (let i = 0; i < r.count; i++) {
		out[i] = r.buf[(r.head + i) % r.max];
	}
	return out;
}

function ringClear(r: Ring): void {
	r.head = 0;
	r.count = 0;
}

/**
 * JSON.stringify replacer that unpacks Error objects. Plain JSON.stringify emits
 * `{}` for an Error because name/message/stack are non-enumerable — which is why
 * mirrored logs showed `"error":{}`. Extract them so the Rust ring buffer (and the
 * /logs HTTP endpoint) carry the actual failure.
 */
function logDataReplacer(_key: string, value: unknown): unknown {
	if (value instanceof Error) {
		return { name: value.name, message: value.message, stack: value.stack };
	}
	return value;
}

function createAppLogger() {
	// Two independent pools: a diagnostic flood can only evict diagnostic entries.
	const userRing = makeRing(USER_MAX_ENTRIES);
	const diagRing = makeRing(DIAG_MAX_ENTRIES);
	const ringFor = (audience: AppLogAudience): Ring => (audience === "diagnostic" ? diagRing : userRing);

	// Reactive signal that bumps every time a log entry is added.
	// Components subscribe to this to re-render when new entries arrive.
	const [revision, setRevision] = createSignal(0);

	// Count of errors since last time the panel was opened
	const [unseenErrorCount, setUnseenErrorCount] = createSignal(0);

	// Track whether the Rust backend is reachable.
	// Entries pushed before backend is ready are queued and drained on first success.
	let backendReady = false;
	let drainInFlight = false;
	const pendingQueue: Array<{
		level: string;
		source: string;
		message: string;
		dataJson?: string;
		audience: AppLogAudience;
	}> = [];
	const MAX_PENDING = 200;

	/** Fire-and-forget push to Rust backend. Queues if not ready yet. */
	function pushToRust(
		level: string,
		source: string,
		message: string,
		dataJson: string | undefined,
		audience: AppLogAudience,
	): void {
		if (!backendReady) {
			if (pendingQueue.length < MAX_PENDING) {
				pendingQueue.push({ level, source, message, dataJson, audience });
			}
			if (!drainInFlight) drainQueue();
			return;
		}
		rpc("push_log", { level, source, message, dataJson: dataJson ?? null, audience }).catch(() => {
			// Silently ignore — the local buffer already has the entry
		});
	}

	/** Try to send queued entries to Rust. On first success, mark backend as ready and drain rest. */
	function drainQueue(): void {
		if (pendingQueue.length === 0 || drainInFlight) return;
		drainInFlight = true;
		const entry = pendingQueue[0];
		rpc("push_log", {
			level: entry.level,
			source: entry.source,
			message: entry.message,
			dataJson: entry.dataJson ?? null,
			audience: entry.audience,
		})
			.then(() => {
				backendReady = true;
				pendingQueue.shift();
				// Drain remaining entries sequentially
				const remaining = pendingQueue.splice(0);
				for (const queued of remaining) {
					rpc("push_log", {
						level: queued.level,
						source: queued.source,
						message: queued.message,
						dataJson: queued.dataJson ?? null,
						audience: queued.audience,
					}).catch(() => {
						// Best-effort mirror — local buffer already has the entry
					});
				}
			})
			.catch(() => {
				// Backend not ready yet — entries stay queued, retry on next push
			})
			.finally(() => {
				drainInFlight = false;
			});
	}

	function push(
		level: AppLogLevel,
		source: AppLogSource,
		message: string,
		data?: unknown,
		audience: AppLogAudience = "user",
	): void {
		const ring = ringFor(audience);

		// Dedup: coalesce with the most recent entry in this pool if
		// level+source+message match. Audience already matches (same pool).
		const last = ringLast(ring);
		if (last && last.level === level && last.source === source && last.message === message) {
			last.repeatCount = (last.repeatCount ?? 0) + 1;
			last.timestamp = Date.now();
			// Bump revision so UI sees updated repeat count
			setRevision((r) => r + 1);
			return;
		}

		const entry: AppLogEntry = {
			id: nextId++,
			timestamp: Date.now(),
			level,
			source,
			message,
			data,
			audience,
		};

		ringPush(ring, entry);

		if (level === "error") {
			batch(() => {
				// Only user-facing errors drive the bell's unseen badge; diagnostic
				// telemetry must never alert the user.
				if (audience === "user") setUnseenErrorCount((c) => c + 1);
				setRevision((r) => r + 1);
			});
		} else if (level === "warn") {
			setRevision((r) => r + 1);
		}
		// info/debug: written to ring buffer but don't bump revision —
		// avoids reactive re-renders during high-throughput PTY output.

		// Forward to browser console
		const tag = `[${source}]`;
		switch (level) {
			case "error":
				console.error(tag, message, data !== undefined ? data : "");
				break;
			case "warn":
				console.warn(tag, message, data !== undefined ? data : "");
				break;
			case "info":
				console.info(tag, message, data !== undefined ? data : "");
				break;
			case "debug":
				console.debug(tag, message, data !== undefined ? data : "");
				break;
		}

		// Mirror info/warn/error to Rust backend for MCP and cross-reload durability.
		// Skip debug — too high-frequency for the ring buffer.
		if (level !== "debug") {
			const dataJson = data !== undefined ? JSON.stringify(data, logDataReplacer) : undefined;
			pushToRust(level, source, message, dataJson, audience);
		}
	}

	function getEntries(): readonly AppLogEntry[] {
		// Force reactive subscription
		revision();
		const user = ringToArray(userRing);
		const diag = ringToArray(diagRing);
		if (diag.length === 0) return user;
		if (user.length === 0) return diag;

		// Both slices are id-ascending; merge into one chronological stream.
		const merged: AppLogEntry[] = new Array(user.length + diag.length);
		let i = 0;
		let j = 0;
		let k = 0;
		while (i < user.length && j < diag.length) {
			merged[k++] = user[i].id <= diag[j].id ? user[i++] : diag[j++];
		}
		while (i < user.length) merged[k++] = user[i++];
		while (j < diag.length) merged[k++] = diag[j++];
		return merged;
	}

	function clear(): void {
		ringClear(userRing);
		ringClear(diagRing);
		setRevision((r) => r + 1);
		setUnseenErrorCount(0);

		// Best-effort: clear Rust backend ring buffer. On failure, entries will
		// re-appear on next webview reload via hydrateFromRust().
		rpc("clear_logs").catch((e) => {
			console.warn("[appLogger] Failed to clear Rust log buffer:", e);
		});
	}

	function markSeen(): void {
		setUnseenErrorCount(0);
	}

	/**
	 * Hydrate the local buffer from the Rust backend.
	 * Called on webview reload to recover logs from the previous session.
	 * Merges Rust entries that aren't already in the local buffer.
	 */
	async function hydrateFromRust(): Promise<void> {
		try {
			const rustEntries = await rpc<RustLogEntry[]>("get_logs", { limit: 0 });
			if (!rustEntries || rustEntries.length === 0) return;

			backendReady = true;

			// Build a set of existing entry IDs (across both pools) for dedup
			const existingIds = new Set<number>();
			for (const entry of ringToArray(userRing)) existingIds.add(entry.id);
			for (const entry of ringToArray(diagRing)) existingIds.add(entry.id);

			let added = 0;
			for (const re of rustEntries) {
				if (existingIds.has(re.id)) continue;

				let data: unknown;
				if (re.data_json) {
					try {
						data = JSON.parse(re.data_json);
					} catch {
						data = re.data_json;
					}
				}

				const audience: AppLogAudience = (re.audience as AppLogAudience) ?? "user";
				const entry: AppLogEntry = {
					id: re.id,
					timestamp: re.timestamp_ms,
					level: re.level as AppLogLevel,
					source: re.source as AppLogSource,
					message: re.message,
					data,
					audience,
				};

				ringPush(ringFor(audience), entry);

				if (re.level === "error" && audience === "user") {
					setUnseenErrorCount((c) => c + 1);
				}
				added++;
			}

			if (added > 0) {
				// Advance nextId past the highest Rust ID
				const maxRustId = rustEntries.reduce((max, e) => Math.max(max, e.id), 0);
				if (maxRustId >= nextId) {
					nextId = maxRustId + 1;
				}
				setRevision((r) => r + 1);
			}
		} catch {
			// Hydration failed — not critical, local buffer still works
		}
	}

	return {
		// Convenience loggers by level (user audience)
		error(source: AppLogSource, message: string, data?: unknown): void {
			push("error", source, message, data);
		},
		warn(source: AppLogSource, message: string, data?: unknown): void {
			push("warn", source, message, data);
		},
		info(source: AppLogSource, message: string, data?: unknown): void {
			push("info", source, message, data);
		},
		debug(source: AppLogSource, message: string, data?: unknown): void {
			push("debug", source, message, data);
		},

		/** Diagnostic loggers — app-internal telemetry, hidden from the default
		 *  user view of the ErrorLogPanel. Use for freeze/perf/circuit internals. */
		diag: {
			error(source: AppLogSource, message: string, data?: unknown): void {
				push("error", source, message, data, "diagnostic");
			},
			warn(source: AppLogSource, message: string, data?: unknown): void {
				push("warn", source, message, data, "diagnostic");
			},
			info(source: AppLogSource, message: string, data?: unknown): void {
				push("info", source, message, data, "diagnostic");
			},
			debug(source: AppLogSource, message: string, data?: unknown): void {
				push("debug", source, message, data, "diagnostic");
			},
		},

		/** Raw push for programmatic use */
		push,

		/** All entries in chronological order (oldest first). Reactive. */
		getEntries,

		/** Remove all entries */
		clear,

		/** Reset unseen error count (called when panel opens) */
		markSeen,

		/** Hydrate local buffer from Rust backend (call on webview reload) */
		hydrateFromRust,

		/** Number of error entries since last markSeen(). Reactive. */
		unseenErrorCount,

		/** Total entry count across both pools. Reactive. */
		entryCount(): number {
			revision();
			return userRing.count + diagRing.count;
		},
	};
}

export const appLogger = createAppLogger();
