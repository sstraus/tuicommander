import type { LogLine } from "../mobile/utils/logLine";
import {
	getRemoteBaseUrl,
	getSessionConnection,
	previewLogPayload,
	transportLogger,
	withRemoteToken,
} from "../transportRuntime";
import { isTauri } from "./environment";
import { buildHttpUrl } from "./http";
import type { PtySubscription, SubscribeEventsOptions, SubscribePtyOptions, Unsubscribe, WsParsedEvent } from "./types";

export async function subscribePty(
	sessionId: string,
	onData: (data: string) => void,
	onExit: () => void,
	onParsedOrOptions?: ((event: WsParsedEvent) => void) | SubscribePtyOptions,
): Promise<PtySubscription> {
	// Normalize overloaded 4th param: function (legacy) or options object
	const opts: SubscribePtyOptions =
		typeof onParsedOrOptions === "function" ? { onParsed: onParsedOrOptions } : (onParsedOrOptions ?? {});
	const onParsed = opts.onParsed;
	// Shared by both transports so a caller can pause without knowing which one
	// it is on. Desktop has no socket to drop, so pausing there means suppressing
	// delivery — the same observable contract, at the only cost desktop has.
	let paused = false;
	const connectionId = getSessionConnection(sessionId);
	if (isTauri() && !connectionId) {
		const { listen } = await import("@tauri-apps/api/event");
		// No pty-output listener: nothing emits that event. It was removed from
		// Rust in cda39f31 when line assembly moved to the reader thread, and the
		// listener outlived it by a commit — silently freezing lastDataAt and the
		// unread flag on desktop (story 625-56b0).
		const unlistenActivity = await listen(`pty-activity-${sessionId}`, () => {
			if (paused) return;
			opts.onActivity?.();
		});
		// No `paused` guard: an exit is lifecycle, not data. Desktop has no
		// reconnect to eventually notice a dead session, so suppressing it here
		// would lose it for good.
		const unlistenTitle = opts.onTitle
			? await listen<string>(`pty-title-${sessionId}`, (event) => {
					if (!paused) opts.onTitle?.(event.payload);
				})
			: undefined;
		const unlistenExit = await listen(`pty-exit-${sessionId}`, () => {
			onExit();
		});
		// Idempotent dispose. Tauri's unlisten is async and REJECTS if its internal
		// registry entry is already gone (double-unregister / session-exit race:
		// listeners[eventId].handlerId on undefined). A sync try/catch can't catch an
		// async rejection — swallow the promise rejection explicitly instead.
		let disposed = false;
		const dispose = () => {
			if (disposed) return;
			disposed = true;
			Promise.resolve(unlistenActivity() as unknown).catch(() => {});
			Promise.resolve(unlistenExit() as unknown).catch(() => {});
			if (unlistenTitle) Promise.resolve(unlistenTitle() as unknown).catch(() => {});
		};
		return Object.assign(dispose, {
			pause: () => {
				paused = true;
			},
			resume: () => {
				paused = false;
			},
		});
	}

	// Browser mode: WebSocket with JSON framing and auto-reconnect
	const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
	const queryFormat = opts.format === "log" ? "log" : opts.stripAnsi ? "text" : null;

	// Track server-side write offset for delta catch-up on reconnect
	let lastTotalWritten: number | null = null;
	let disposed = false;
	let activeWs: WebSocket | null = null;
	let reconnectTimer: ReturnType<typeof setTimeout> | null = null;

	/**
	 * `socket` is the connection the frame arrived on. `close()` is asynchronous,
	 * so a socket dropped by `pause()` can still deliver after the `resume()` that
	 * replaced it — and by then a `paused` flag reads false again. Identity
	 * against `activeWs` is the guard that survives that; the flag cannot be.
	 */
	const handleMessage = (event: MessageEvent, socket: WebSocket) => {
		if (disposed) return;
		const raw = event.data as string;
		// JSON frame detection: starts with { and contains "type"
		if (raw.startsWith("{")) {
			try {
				const frame = JSON.parse(raw) as WsParsedEvent;
				// Lifecycle is never suppressed. A session that dies while the page
				// is hidden is still dead, and swallowing this frame would leave the
				// view showing it as live until the reconnect backoff gave up ten
				// attempts later.
				if (frame.type === "exit" || frame.type === "closed") {
					disposed = true; // Session truly ended — don't reconnect
					onExit();
					return;
				}
				// Everything below is data, and only the live socket's data counts.
				// A paused subscription has no live socket, so nothing reaches the
				// consumer — and because the cursor only advances on delivery,
				// nothing is skipped either.
				if (socket !== activeWs) return;
				// Track total_written for reconnect delta
				if (typeof (frame as Record<string, unknown>).total_written === "number") {
					lastTotalWritten = (frame as Record<string, unknown>).total_written as number;
				}
				switch (frame.type) {
					case "output":
						onData(frame.data as string);
						break;
					case "title":
						if (typeof frame.title === "string") opts.onTitle?.(frame.title);
						break;
					case "activity":
						opts.onActivity?.();
						break;
					case "log": {
						// Track the monotonic line cursor so reconnect resumes from the last
						// line we consumed instead of replaying from the mount offset (which
						// duplicated the whole scrollback on every WS reconnect).
						const logCursor = (frame as Record<string, unknown>).total_lines;
						if (typeof logCursor === "number") {
							lastTotalWritten = logCursor;
						}
						const lines = frame.lines as LogLine[] | undefined;
						if (lines && lines.length > 0) {
							if (opts.onLogLines) {
								opts.onLogLines(lines);
							} else {
								// Backward compat: join span texts as plain string
								const texts = lines.map((l) => {
									if (typeof l === "string") return l;
									if (l && typeof l === "object" && "spans" in l) {
										return ((l as { spans: { text: string }[] }).spans || []).map((s) => s.text).join("");
									}
									return String(l);
								});
								onData(texts.join("\n"));
							}
						}
						const screen = frame.screen as unknown[] | undefined;
						if (screen && opts.onScreenRows) {
							opts.onScreenRows(screen);
						}
						if (opts.onInputLine && frame.screen !== undefined) {
							const il = (frame as Record<string, unknown>).input_line;
							opts.onInputLine(typeof il === "string" ? il : null);
						}
						break;
					}
					case "state":
						if (opts.onStateChange && frame.state) {
							opts.onStateChange(frame.state as Record<string, unknown>);
						}
						break;
					case "parsed":
						onParsed?.(frame);
						break;
				}
				return;
			} catch {
				// Not valid JSON — treat as raw output (backward compat)
			}
		}
		if (socket !== activeWs) return;
		onData(raw);
	};

	/** Build the WS URL with current params (including offset for reconnect). */
	const buildWsUrl = (reconnectOffset?: number | null): string => {
		const params = new URLSearchParams();
		if (queryFormat) params.set("format", queryFormat);
		if (reconnectOffset != null) {
			params.set("offset", String(reconnectOffset));
		} else if (opts.logOffset != null) {
			params.set("offset", String(opts.logOffset));
		}
		const query = params.size > 0 ? `?${params}` : "";
		const baseUrl = connectionId ? getRemoteBaseUrl(connectionId) : undefined;
		if (connectionId && !baseUrl) throw new Error(`Remote connection ${connectionId} not connected`);
		const wsBase = baseUrl ? baseUrl.replace(/^http/, "ws") : `${protocol}//${window.location.host}`;
		return withRemoteToken(`${wsBase}/sessions/${encodeURIComponent(sessionId)}/stream${query}`, connectionId);
	};

	/** Connect (or reconnect) the WebSocket. */
	const connect = (reconnectOffset?: number | null): Promise<void> =>
		new Promise<void>((resolve, reject) => {
			const wsUrl = buildWsUrl(reconnectOffset);
			const ws = new WebSocket(wsUrl);
			activeWs = ws;

			ws.onopen = () => {
				// A pause, or a newer attempt, replaced this one while it was still
				// opening. Leaving it open would stream a second copy of the session
				// into the same consumer, so it closes itself and reports failure to
				// whoever is awaiting it.
				if (ws !== activeWs) {
					ws.close();
					reject(new Error(`WebSocket attempt superseded: ${sessionId}`));
					return;
				}
				transportLogger().debug("network", `WebSocket connected: ${sessionId}`);
				// Re-wire onclose for live session
				ws.onclose = (evt: CloseEvent) => {
					if (disposed) return;
					// This socket is no longer the live one: a pause dropped it, or a
					// resume already replaced it. Reading that as a session exit is
					// the trap this feature exists to avoid — the view would print
					// "session exited" and clear the screen on every tab switch — and
					// reconnecting on it would race the live socket.
					if (ws !== activeWs) return;
					if (evt.code === 1000 || evt.code === 1001) {
						// Normal close or going away — don't reconnect. Terminal, like
						// the exit frame and like retry exhaustion: the same news by a
						// third route. Without this the consumer is told the session
						// exited while the subscription still thinks a later resume
						// may reopen it.
						disposed = true;
						onExit();
						return;
					}
					transportLogger().debug("network", `WebSocket closed abnormally (code ${evt.code}), will reconnect`);
					scheduleReconnect();
				};
				resolve();
			};

			ws.onerror = () => {
				// onerror is always followed by onclose, so reject is handled there
			};

			ws.onclose = (evt: CloseEvent) => {
				reject(new Error(`WebSocket closed before opening (code ${evt.code}): ${evt.reason || "no reason"}`));
			};

			ws.onmessage = (event: MessageEvent) => handleMessage(event, ws);
		});

	// Reconnect with exponential backoff
	const MAX_RETRIES = 10;
	const BASE_DELAY_MS = 1000;
	const MAX_DELAY_MS = 30_000;
	let retryCount = 0;

	/**
	 * Both success paths report through here so a consumer's exception can never
	 * be mistaken for a failed connection. The callback shares a promise chain
	 * with `connect()`, and a throw landing in that rejection handler would
	 * announce a reconnect nothing broke and open a second socket beside the
	 * healthy one.
	 */
	const announceReconnected = () => {
		try {
			opts.onReconnected?.();
		} catch {
			transportLogger().warn("network", `onReconnected callback threw: ${sessionId}`);
		}
	};

	const scheduleReconnect = () => {
		if (disposed || paused) return;
		if (retryCount >= MAX_RETRIES) {
			transportLogger().warn("network", `WebSocket reconnect failed after ${MAX_RETRIES} attempts: ${sessionId}`);
			// Terminal, like the exit frame. Without this the subscription stays
			// live-looking, and a later resume would restart the whole doomed
			// budget and report the exit again when it ran out.
			disposed = true;
			onExit();
			return;
		}
		const delay = Math.min(BASE_DELAY_MS * 2 ** retryCount, MAX_DELAY_MS);
		retryCount++;
		transportLogger().debug("network", `WebSocket reconnecting in ${delay}ms (attempt ${retryCount}/${MAX_RETRIES})`);
		opts.onReconnecting?.(retryCount, MAX_RETRIES);
		reconnectTimer = setTimeout(async () => {
			if (disposed || paused) return;
			const pending = connect(lastTotalWritten);
			// connect() installs its socket synchronously, so this names THIS
			// attempt. A pause or a newer attempt during the handshake replaces it,
			// and a superseded attempt must not schedule anything of its own.
			const mine = activeWs;
			try {
				await pending;
				retryCount = 0; // Reset on success
				announceReconnected();
			} catch {
				// connect() failed (e.g. session gone → 404 triggers immediate close)
				if (mine !== activeWs) return;
				scheduleReconnect();
			}
		}, delay);
	};

	// Initial connection
	await connect();

	const dispose = () => {
		disposed = true;
		if (reconnectTimer) clearTimeout(reconnectTimer);
		activeWs?.close();
	};

	return Object.assign(dispose, {
		pause: () => {
			if (disposed || paused) return;
			paused = true;
			// A backoff timer already in flight would otherwise reopen the socket
			// behind the pause: it was scheduled before the flag was set.
			if (reconnectTimer) {
				clearTimeout(reconnectTimer);
				reconnectTimer = null;
			}
			activeWs?.close();
			activeWs = null;
		},
		resume: () => {
			if (disposed || !paused) return;
			paused = false;
			// Reconnect at once rather than through the backoff, so coming back to
			// the tab is not made to wait out a delay earned before it was hidden.
			// The retry budget is NOT refilled here: only a successful connect
			// clears it, or a dead session would get a fresh ten attempts on every
			// hide/show and never reach the exit the user needs to see.
			const pending = connect(lastTotalWritten);
			const mine = activeWs;
			// A reconnect already announced to the consumer is finished by this
			// connect, not abandoned by it. Without this, a pause landing between
			// `onReconnecting` and its backoff leaves the consumer's banner up for
			// good, even though the socket is healthy again.
			const wasReconnecting = retryCount > 0;
			pending
				.then(() => {
					retryCount = 0;
					if (wasReconnecting) announceReconnected();
				})
				.catch(() => {
					if (mine === activeWs) scheduleReconnect();
				});
		},
	});
}
/**
 * Subscribe to application-level events (head-changed, repo-changed, etc.)
 *
 * In Tauri: delegates to individual listen() calls.
 * In browser: creates a single EventSource to /events SSE endpoint.
 *
 * @param handlers - Map of event type → callback
 * @param options - Remote base URL and the missed-events callback
 * @returns Promise resolving to an unsubscribe function
 */
export async function subscribeEvents(
	handlers: Record<string, (payload: unknown) => void>,
	options: SubscribeEventsOptions = {},
): Promise<Unsubscribe> {
	const { baseUrl, onResync } = options;
	if (!baseUrl && isTauri()) {
		const { listen } = await import("@tauri-apps/api/event");
		const unsubscribers: Array<() => void> = [];
		for (const [eventType, handler] of Object.entries(handlers)) {
			const unlisten = await listen(eventType, (event) => handler(event.payload));
			unsubscribers.push(unlisten);
		}
		return () => unsubscribers.forEach((fn) => fn());
	}

	// Browser/remote mode: SSE via EventSource
	const types = Object.keys(handlers).join(",");
	const url = buildHttpUrl(`/events?types=${encodeURIComponent(types)}`, baseUrl);
	const es = new EventSource(url);

	for (const [eventType, handler] of Object.entries(handlers)) {
		es.addEventListener(eventType, ((event: MessageEvent) => {
			try {
				const payload = JSON.parse(event.data);
				handler(payload);
			} catch {
				transportLogger().warn("network", `Failed to parse SSE event "${eventType}"`, {
					eventData: previewLogPayload(event.data),
				});
			}
		}) as EventListener);
	}

	es.addEventListener("lagged", ((event: MessageEvent) => {
		transportLogger().warn("network", "SSE lagged", { eventData: previewLogPayload(event.data) });
		// The backend's broadcast channel dropped events for this subscriber. They
		// are gone; only the consumer knows how to re-derive what they carried.
		onResync?.("lagged");
	}) as EventListener);

	// `onopen` fires on every successful connection, so the FIRST one is the
	// initial connect and every later one is a reconnect. Only the later ones are
	// a gap: EventSource reconnects itself, silently, and whatever the backend
	// published while it was down was never queued for us.
	let everOpened = false;
	es.onopen = () => {
		if (everOpened) onResync?.("reconnect");
		everOpened = true;
	};

	es.onerror = () => {
		transportLogger().debug("network", "SSE connection error — will auto-reconnect");
	};

	return () => es.close();
}
