import { appLogger } from "../../stores/appLogger";
import { isTauri, rpc } from "../../transport";
import { getRemoteBaseUrl, withRemoteToken } from "../../transportRuntime";
import { isPerfDebug } from "../../utils/perfDebug";
import { canDecodeDeflate, DEFLATE_SUBPROTOCOL, decodeTaggedFrame } from "./wsFrameCodec";

export interface TerminalTransport {
	subscribe(onFrame: (data: ArrayBuffer) => void): Promise<void>;
	resubscribe(): Promise<void>;
	unsubscribe(): void;
	invoke(cmd: string, args: Record<string, unknown>): Promise<unknown>;
	/**
	 * Report the total number of frames received, opening the backend's delivery
	 * gate once the count catches up with what it sent.
	 *
	 * Only the Tauri channel is gated this way; the WS transport recovers from a
	 * dropped frame by sequence number and has no ack command at all, so the call
	 * is a no-op there rather than a per-frame rejected invoke.
	 */
	ackFrame(received: number): void;
	onEvent(type: string, handler: (payload: unknown) => void): Promise<void>;
	/** WS lifecycle errors are local notifications, not backend PTY events. */
	onStreamError?(handler: (error: unknown) => void): void;
	onStreamReconnecting?(handler: (attempt: number, maxAttempts: number) => void): void;
	onStreamRecovered?(handler: () => void): void;
	onStreamExhausted?(handler: (maxAttempts: number) => void): void;
}

/**
 * Normalize a binary IPC payload to an ArrayBuffer, or null when it is not
 * binary at all.
 *
 * Rust hands grid frames and styled-row chunks over as raw bytes, which reach JS
 * as an ArrayBuffer on the custom-protocol IPC and as a plain `number[]` on the
 * postMessage path Tauri falls back to when that protocol is blocked. Both are
 * legitimate; anything else (an error object, null, a string) is not, and must
 * not be fed to `new Uint8Array()` — that quietly produces an empty buffer, so a
 * broken response would decode as an empty frame instead of being dropped.
 */
export function toBinaryPayload(data: unknown): ArrayBuffer | null {
	if (data instanceof ArrayBuffer) return data;
	if (ArrayBuffer.isView(data)) {
		return data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength) as ArrayBuffer;
	}
	if (Array.isArray(data)) return new Uint8Array(data).buffer;
	return null;
}

/**
 * Build the transport for a terminal.
 *
 * `connectionId` names a remote daemon; the base URL and the session token both
 * come from it, so the caller never has to hold a credential. A terminal owned
 * by a remote machine is always a WebSocket — Tauri IPC reaches this process
 * only.
 */
export function createTransport(sessionId: string, connectionId?: string): TerminalTransport {
	if (connectionId) return new WsTransport(sessionId, connectionId);
	return isTauri() ? new TauriTransport(sessionId) : new WsTransport(sessionId);
}

export class TauriTransport implements TerminalTransport {
	private sessionId: string;
	private invokeRef: ((cmd: string, args?: Record<string, unknown>) => Promise<unknown>) | null = null;
	private unlisteners: (() => void)[] = [];
	private onFrameHandler: ((data: ArrayBuffer) => void) | null = null;
	/**
	 * Epoch of the live subscription, as returned by `subscribe_terminal_grid`,
	 * or null while there is none.
	 *
	 * A terminal that remounts subscribes before the outgoing instance tears
	 * down, so the backend receives the old instance's calls against the new
	 * gate. The epoch is what makes those calls harmless: an ack for a previous
	 * subscription is dropped instead of crediting frames nobody received, and a
	 * stale unsubscribe is dropped instead of deleting the live channel.
	 */
	private epoch: number | null = null;

	constructor(sessionId: string) {
		this.sessionId = sessionId;
	}

	async subscribe(onFrame: (data: ArrayBuffer) => void): Promise<void> {
		this.onFrameHandler = onFrame;
		const { invoke, Channel } = await import("@tauri-apps/api/core");
		this.invokeRef = invoke;
		await this.registerChannel(invoke, Channel);
	}

	async resubscribe(): Promise<void> {
		if (!this.onFrameHandler) return;
		const { invoke, Channel } = await import("@tauri-apps/api/core");
		this.invokeRef = invoke;
		await this.registerChannel(invoke, Channel);
	}

	private async registerChannel(
		invoke: (cmd: string, args?: Record<string, unknown>) => Promise<unknown>,
		Channel: new () => { onmessage: (data: ArrayBuffer | number[]) => void },
	): Promise<void> {
		const onFrame = this.onFrameHandler!;
		const sessionId = this.sessionId;
		const channel = new Channel();
		channel.onmessage = (data: ArrayBuffer | number[]) => {
			const frame = toBinaryPayload(data);
			if (!frame) {
				appLogger.error("terminal", "grid channel delivered a non-binary payload", { sessionId });
				return;
			}
			try {
				onFrame(frame);
			} catch (e) {
				appLogger.error("terminal", "onFrame threw in channel callback", { sessionId, error: e });
			}
		};
		this.epoch = (await invoke("subscribe_terminal_grid", {
			sessionId: this.sessionId,
			channel,
		})) as number;
		invoke("terminal_request_frame", { sessionId: this.sessionId }).catch(() => {});
	}

	unsubscribe(): void {
		if (this.epoch !== null) {
			this.invokeRef?.("unsubscribe_terminal_grid", { sessionId: this.sessionId, epoch: this.epoch }).catch(() => {});
			this.epoch = null;
		}
		for (const unlisten of this.unlisteners) {
			// Tauri's unlisten is async under the hood and REJECTS if its internal
			// registry entry is already gone (webview/session teardown race — common
			// now that a shell exit disposes the terminal). Teardown must swallow it,
			// not surface an unhandled rejection.
			Promise.resolve(unlisten() as unknown).catch(() => {});
		}
		this.unlisteners = [];
	}

	async invoke(cmd: string, args: Record<string, unknown>): Promise<unknown> {
		if (!this.invokeRef) {
			const { invoke } = await import("@tauri-apps/api/core");
			this.invokeRef = invoke;
		}
		return this.invokeRef(cmd, args);
	}

	ackFrame(received: number): void {
		// No epoch means no live subscription: there is no gate to open, and epoch 0
		// would match none, so the backend would ignore every later ack anyway.
		if (this.epoch === null) return;
		this.invokeRef?.("ack_terminal_frame", { sessionId: this.sessionId, epoch: this.epoch, received }).catch((e) => {
			appLogger.debug("terminal", "ack_terminal_frame failed", { sessionId: this.sessionId, error: e });
		});
	}

	async onEvent(type: string, handler: (payload: unknown) => void): Promise<void> {
		const { listen } = await import("@tauri-apps/api/event");
		const eventName = `pty-${type}-${this.sessionId}`;
		const unlisten = await listen(eventName, (event: { payload: unknown }) => {
			handler(event.payload);
		});
		this.unlisteners.push(unlisten);
	}
}

const MAX_RECONNECT_ATTEMPTS = 10;
const INITIAL_RECONNECT_MS = 1000;
const INITIAL_FRAME_TIMEOUT_MS = 15_000;

export class WsTransport implements TerminalTransport {
	private sessionId: string;
	private connectionId: string | undefined;
	private ws: WebSocket | null = null;
	private onFrameHandler: ((data: ArrayBuffer) => void) | null = null;
	private eventHandlers = new Map<string, (payload: unknown) => void>();
	private closed = false;
	private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
	private reconnectAttempts = 0;
	private initialFrameTimer: ReturnType<typeof setTimeout> | null = null;
	private failureReported = false;
	private reconnectingHandler?: (attempt: number, maxAttempts: number) => void;
	private recoveredHandler?: () => void;
	private exhaustedHandler?: (maxAttempts: number) => void;

	constructor(sessionId: string, connectionId?: string) {
		this.sessionId = sessionId;
		this.connectionId = connectionId;
	}

	async subscribe(onFrame: (data: ArrayBuffer) => void): Promise<void> {
		this.onFrameHandler = onFrame;
		this.closed = false;
		this.reconnectAttempts = 0;
		await this.connect();
	}

	async resubscribe(): Promise<void> {
		this.closed = false;
		this.reconnectAttempts = 0;
		this.clearTimers();
		const previous = this.ws;
		this.ws = null;
		previous?.close();
		await this.connect();
	}

	private clearInitialFrameTimer(): void {
		if (this.initialFrameTimer !== null) clearTimeout(this.initialFrameTimer);
		this.initialFrameTimer = null;
	}

	private clearTimers(): void {
		this.clearInitialFrameTimer();
		if (this.reconnectTimer !== null) clearTimeout(this.reconnectTimer);
		this.reconnectTimer = null;
	}

	private reportStreamError(error: unknown): void {
		if (this.closed || this.failureReported) return;
		this.failureReported = true;
		this.eventHandlers.get("stream-error")?.(error);
	}

	private deliveredFrame(data: ArrayBuffer): void {
		this.onFrameHandler?.(data);
		this.replayReady();
	}

	private replayReady(): void {
		// Opening a socket is not replay. An accepted viewport or the server
		// explicitly finishing an empty replay proves the stream recovered; a healthy idle PTY owes no more output.
		const recovering = this.failureReported || this.reconnectAttempts > 0;
		this.clearInitialFrameTimer();
		this.reconnectAttempts = 0;
		this.failureReported = false;
		if (recovering) this.recoveredHandler?.();
	}

	/**
	 * Dispatch one JSON frame to the handler registered for its `type`.
	 *
	 * Shared by the plain and the negotiated path so the two cannot drift: the
	 * only difference between them is how the string was obtained.
	 */
	private handleTextFrame(text: string): void {
		try {
			const event = JSON.parse(text) as { type: string; [key: string]: unknown };
			const { type, ...payload } = event;
			if (type === "grid-replay-empty") this.replayReady();
			this.eventHandlers.get(type)?.(payload);
		} catch (err) {
			if (isPerfDebug()) {
				appLogger.debug("terminal", "WsTransport received an unparseable text frame", {
					sessionId: this.sessionId,
					frameStart: text?.slice?.(0, 100),
					error: err,
				});
			}
		}
	}

	private scheduleReconnect(): void {
		if (this.reconnectAttempts >= MAX_RECONNECT_ATTEMPTS) {
			this.exhaustedHandler?.(MAX_RECONNECT_ATTEMPTS);
			appLogger.warn("terminal", `Terminal stream disconnected after ${MAX_RECONNECT_ATTEMPTS} reconnect attempts`, {
				sessionId: this.sessionId,
			});
			return;
		}
		const delay = INITIAL_RECONNECT_MS * 2 ** Math.min(this.reconnectAttempts, 5);
		this.reconnectAttempts++;
		this.reconnectingHandler?.(this.reconnectAttempts, MAX_RECONNECT_ATTEMPTS);
		this.reconnectTimer = setTimeout(() => {
			this.reconnectTimer = null;
			this.connect().catch((err) => {
				this.reportStreamError(err);
				if (isPerfDebug()) {
					appLogger.debug("terminal", "WsTransport reconnect failed", { sessionId: this.sessionId, error: err });
				}
			});
		}, delay);
	}

	private replaceFailedSocket(ws: WebSocket): void {
		if (this.closed || this.ws !== ws) return;
		this.ws = null;
		this.clearInitialFrameTimer();
		// A half-open link may never complete the close handshake. Recovery must
		// not depend on its onclose callback; detaching also rejects stale frames.
		this.scheduleReconnect();
		ws.close();
	}

	private async connect(): Promise<void> {
		let url: string;
		const remoteBaseUrl = this.connectionId ? getRemoteBaseUrl(this.connectionId) : undefined;
		// Only a remote connection has a link worth compressing, and only a
		// runtime with the platform inflate can read the answer. Asking for an
		// encoding we cannot decode would break the terminal rather than slow
		// it down, so both have to hold.
		const asksForCompression = Boolean(this.connectionId) && canDecodeDeflate();
		const query = asksForCompression ? "format=grid&compress=deflate" : "format=grid";
		if (this.connectionId) {
			if (!remoteBaseUrl) {
				// Not connected, or connected but unauthenticated: there is no URL to
				// open. Reconnecting blindly would spin against a daemon that answers
				// 401 to the upgrade, so give up and let connect() surface the reason.
				throw new Error(`Remote connection ${this.connectionId} not connected`);
			}
			// Remote: convert http(s) baseUrl to ws(s). The upgrade request cannot
			// carry an Authorization header, so the token rides in the query string.
			const wsBase = remoteBaseUrl.replace(/^http/, "ws");
			url = withRemoteToken(
				`${wsBase}/sessions/${encodeURIComponent(this.sessionId)}/stream?${query}`,
				this.connectionId,
			);
		} else {
			// Local: use current page origin
			const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
			url = `${proto}//${window.location.host}/sessions/${encodeURIComponent(this.sessionId)}/stream?${query}`;
		}
		// Every handler below is guarded on `this.ws === ws`. A socket the transport
		// has moved on from still fires its callbacks: its onclose reads as an
		// unexpected drop and reconnects a socket nobody tracks, and its onmessage
		// feeds deltas from an older stream into the same row map — an old delta
		// landing after a newer full frame paints stale rows with no error.
		// The subprotocol is offered alongside the query parameter, and it is the
		// half that comes back: the server selects it only when it is going to tag
		// its frames. A server that predates the parameter answers with neither,
		// and `compressed` below stays false.
		const ws = asksForCompression ? new WebSocket(url, [DEFLATE_SUBPROTOCOL]) : new WebSocket(url);
		this.ws = ws;
		ws.binaryType = "arraybuffer";
		// Inflating a frame is asynchronous, and grid frames are DELTAS: one
		// overtaking another paints stale rows with no error. Every frame on a
		// negotiated socket therefore goes through this chain, text ones
		// included, so their order relative to the deltas is the order the
		// server sent them in.
		let pending: Promise<void> = Promise.resolve();
		// Set in onopen, which the WebSocket spec fires before any message event
		// on the same socket — so no frame is ever read against the wrong framing.
		let compressed = false;
		const { promise: connected, resolve: resolveConnect, reject: rejectConnect } = Promise.withResolvers<void>();
		this.initialFrameTimer = setTimeout(() => {
			if (this.closed || this.ws !== ws) return;
			const error = new Error("Timed out waiting for the initial terminal frame");
			this.reportStreamError(error);
			rejectConnect(error);
			this.replaceFailedSocket(ws);
		}, INITIAL_FRAME_TIMEOUT_MS);
		ws.onmessage = (e) => {
			if (this.ws !== ws) return;
			if (!compressed) {
				// A socket that did not ask gets the original framing, handled
				// synchronously exactly as before.
				try {
					if (e.data instanceof ArrayBuffer) this.deliveredFrame(e.data);
					else this.handleTextFrame(e.data as string);
				} catch (error) {
					this.reportStreamError(error);
					this.replaceFailedSocket(ws);
				}
				return;
			}
			pending = pending
				.then(async () => {
					if (this.ws !== ws) return;
					const frame = await decodeTaggedFrame(e.data as ArrayBuffer);
					// Re-checked after the await: the socket can be replaced while a
					// frame is inflating, and this one belongs to the old stream.
					if (this.ws !== ws) return;
					if (frame.kind === "binary") this.deliveredFrame(frame.data);
					else this.handleTextFrame(frame.data);
				})
				.catch((err) => {
					if (this.closed || this.ws !== ws) return;
					this.reportStreamError(err);
					this.replaceFailedSocket(ws);
					// An undecodable frame is a protocol disagreement, not a slow
					// link, and it will repeat. Say so once per frame at warn level
					// rather than hiding it behind the perf-debug gate.
					appLogger.warn("terminal", "WsTransport could not decode a compressed frame", {
						sessionId: this.sessionId,
						error: err,
					});
				});
		};
		ws.onclose = () => {
			if (this.closed || this.ws !== ws) return;
			this.ws = null;
			this.clearInitialFrameTimer();
			const error = new Error("Terminal stream disconnected");
			rejectConnect(error);
			this.reportStreamError(error);
			this.scheduleReconnect();
		};
		ws.onopen = () => {
			compressed = ws.protocol === DEFLATE_SUBPROTOCOL;
			if (asksForCompression && !compressed) {
				appLogger.debug("terminal", "WsTransport asked for compression and the server did not take it", {
					sessionId: this.sessionId,
				});
			}
			resolveConnect();
		};
		ws.onerror = () => {
			if (this.closed || this.ws !== ws) return;
			const error = new Error("WebSocket connection failed");
			this.reportStreamError(error);
			rejectConnect(error);
		};
		await connected;
	}

	unsubscribe(): void {
		this.closed = true;
		this.clearTimers();
		this.ws?.close();
		this.ws = null;
		this.eventHandlers.clear();
		this.reconnectingHandler = undefined;
		this.recoveredHandler = undefined;
		this.exhaustedHandler = undefined;
	}

	async invoke(cmd: string, args: Record<string, unknown>): Promise<unknown> {
		// The session lives on whichever machine this socket points at, so every
		// call about it — write, resize, scroll — goes to the same place. Undefined
		// for a browser-mode local terminal, which is what `rpc` already assumes.
		return rpc(cmd, args, this.connectionId);
	}

	ackFrame(_received: number): void {
		// No-op by design. `ack_terminal_frame` is desktop-only; this socket's
		// backend detects a dropped frame from the sequence number it stamps on
		// each one and resends the full grid, so there is nothing to acknowledge.
	}

	onStreamReconnecting(handler: (attempt: number, maxAttempts: number) => void): void {
		this.reconnectingHandler = handler;
	}

	onStreamRecovered(handler: () => void): void {
		this.recoveredHandler = handler;
	}

	onStreamExhausted(handler: (maxAttempts: number) => void): void {
		this.exhaustedHandler = handler;
	}

	onStreamError(handler: (error: unknown) => void): void {
		this.eventHandlers.set("stream-error", handler);
	}

	onEvent(type: string, handler: (payload: unknown) => void): Promise<void> {
		this.eventHandlers.set(type, handler);
		return Promise.resolve();
	}
}
