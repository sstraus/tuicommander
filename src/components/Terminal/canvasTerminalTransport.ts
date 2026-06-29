import { appLogger } from "../../stores/appLogger";
import { isTauri, rpc } from "../../transport";
import { isPerfDebug } from "../../utils/perfDebug";

export interface TerminalTransport {
	subscribe(onFrame: (data: ArrayBuffer) => void): Promise<void>;
	resubscribe(): Promise<void>;
	unsubscribe(): void;
	invoke(cmd: string, args: Record<string, unknown>): Promise<unknown>;
	onEvent(type: string, handler: (payload: unknown) => void): Promise<void>;
}

export function createTransport(sessionId: string, baseUrl?: string): TerminalTransport {
	if (baseUrl) return new WsTransport(sessionId, baseUrl);
	return isTauri() ? new TauriTransport(sessionId) : new WsTransport(sessionId);
}

export class TauriTransport implements TerminalTransport {
	private sessionId: string;
	private invokeRef: ((cmd: string, args?: Record<string, unknown>) => Promise<unknown>) | null = null;
	private unlisteners: (() => void)[] = [];
	private onFrameHandler: ((data: ArrayBuffer) => void) | null = null;

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
			try {
				onFrame(data instanceof ArrayBuffer ? data : new Uint8Array(data).buffer);
			} catch (e) {
				appLogger.error("terminal", "onFrame threw in channel callback", { sessionId, error: e });
			}
		};
		await invoke("subscribe_terminal_grid", {
			sessionId: this.sessionId,
			channel,
		});
		invoke("terminal_request_frame", { sessionId: this.sessionId }).catch(() => {});
	}

	unsubscribe(): void {
		this.invokeRef?.("unsubscribe_terminal_grid", { sessionId: this.sessionId }).catch(() => {});
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

export class WsTransport implements TerminalTransport {
	private sessionId: string;
	private baseUrl: string | undefined;
	private ws: WebSocket | null = null;
	private onFrameHandler: ((data: ArrayBuffer) => void) | null = null;
	private eventHandlers = new Map<string, (payload: unknown) => void>();
	private closed = false;
	private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
	private reconnectAttempts = 0;

	constructor(sessionId: string, baseUrl?: string) {
		this.sessionId = sessionId;
		this.baseUrl = baseUrl;
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
		this.ws?.close();
		this.ws = null;
		await this.connect();
	}

	private async connect(): Promise<void> {
		let url: string;
		if (this.baseUrl) {
			// Remote: convert http(s) baseUrl to ws(s)
			const wsBase = this.baseUrl.replace(/^http/, "ws");
			url = `${wsBase}/sessions/${encodeURIComponent(this.sessionId)}/stream?format=grid`;
		} else {
			// Local: use current page origin
			const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
			url = `${proto}//${window.location.host}/sessions/${encodeURIComponent(this.sessionId)}/stream?format=grid`;
		}
		this.ws = new WebSocket(url);
		this.ws.binaryType = "arraybuffer";
		this.ws.onmessage = (e) => {
			if (e.data instanceof ArrayBuffer) {
				this.onFrameHandler?.(e.data);
			} else {
				try {
					const event = JSON.parse(e.data as string) as { type: string; [key: string]: unknown };
					const { type, ...payload } = event;
					this.eventHandlers.get(type)?.(payload);
				} catch (err) {
					if (isPerfDebug()) {
						appLogger.debug("terminal", "WsTransport received an unparseable text frame", {
							sessionId: this.sessionId,
							frameStart: (e.data as string)?.slice?.(0, 100),
							error: err,
						});
					}
				}
			}
		};
		this.ws.onclose = () => {
			if (this.closed) return;
			if (this.reconnectAttempts >= MAX_RECONNECT_ATTEMPTS) {
				appLogger.warn("terminal", `Terminal stream disconnected after ${MAX_RECONNECT_ATTEMPTS} reconnect attempts`, {
					sessionId: this.sessionId,
				});
				return;
			}
			const delay = INITIAL_RECONNECT_MS * 2 ** Math.min(this.reconnectAttempts, 5);
			this.reconnectAttempts++;
			this.reconnectTimer = setTimeout(() => {
				this.connect()
					.then(() => {
						this.reconnectAttempts = 0;
					})
					.catch((err) => {
						if (isPerfDebug()) {
							appLogger.debug("terminal", "WsTransport reconnect failed", { sessionId: this.sessionId, error: err });
						}
					});
			}, delay);
		};
		const ws = this.ws;
		await new Promise<void>((resolve, reject) => {
			ws.onopen = () => resolve();
			ws.onerror = () => reject(new Error("WebSocket connection failed"));
		});
	}

	unsubscribe(): void {
		this.closed = true;
		if (this.reconnectTimer) {
			clearTimeout(this.reconnectTimer);
			this.reconnectTimer = null;
		}
		this.ws?.close();
		this.ws = null;
		this.eventHandlers.clear();
	}

	async invoke(cmd: string, args: Record<string, unknown>): Promise<unknown> {
		return rpc(cmd, args);
	}

	onEvent(type: string, handler: (payload: unknown) => void): Promise<void> {
		this.eventHandlers.set(type, handler);
		return Promise.resolve();
	}
}
