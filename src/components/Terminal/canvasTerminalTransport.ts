import { isTauri, rpc } from "../../transport";

export interface TerminalTransport {
  subscribe(onFrame: (data: ArrayBuffer) => void): Promise<void>;
  unsubscribe(): void;
  invoke(cmd: string, args: Record<string, unknown>): Promise<unknown>;
  onEvent(type: string, handler: (payload: unknown) => void): Promise<void>;
}

export function createTransport(sessionId: string): TerminalTransport {
  return isTauri() ? new TauriTransport(sessionId) : new WsTransport(sessionId);
}

export class TauriTransport implements TerminalTransport {
  private sessionId: string;
  private invokeRef: ((cmd: string, args?: Record<string, unknown>) => Promise<unknown>) | null = null;
  private unlisteners: (() => void)[] = [];

  constructor(sessionId: string) {
    this.sessionId = sessionId;
  }

  async subscribe(onFrame: (data: ArrayBuffer) => void): Promise<void> {
    const { invoke, Channel } = await import("@tauri-apps/api/core");
    this.invokeRef = invoke;
    const channel = new Channel<ArrayBuffer | number[]>();
    channel.onmessage = (data) => {
      if (data instanceof ArrayBuffer) {
        onFrame(data);
      } else {
        onFrame(new Uint8Array(data).buffer);
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
    for (const unlisten of this.unlisteners) unlisten();
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
  private ws: WebSocket | null = null;
  private onFrameHandler: ((data: ArrayBuffer) => void) | null = null;
  private eventHandlers = new Map<string, (payload: unknown) => void>();
  private closed = false;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private reconnectAttempts = 0;

  constructor(sessionId: string) {
    this.sessionId = sessionId;
  }

  async subscribe(onFrame: (data: ArrayBuffer) => void): Promise<void> {
    this.onFrameHandler = onFrame;
    this.closed = false;
    this.reconnectAttempts = 0;
    await this.connect();
  }

  private async connect(): Promise<void> {
    const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
    const url = `${proto}//${window.location.host}/sessions/${encodeURIComponent(this.sessionId)}/stream?format=grid`;
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
          console.debug("WsTransport: unparseable text frame", (e.data as string)?.slice?.(0, 100), err);
        }
      }
    };
    this.ws.onclose = () => {
      if (this.closed) return;
      if (this.reconnectAttempts >= MAX_RECONNECT_ATTEMPTS) {
        console.warn(`WsTransport: giving up after ${MAX_RECONNECT_ATTEMPTS} reconnect attempts`, this.sessionId);
        return;
      }
      const delay = INITIAL_RECONNECT_MS * Math.pow(2, Math.min(this.reconnectAttempts, 5));
      this.reconnectAttempts++;
      this.reconnectTimer = setTimeout(() => {
        this.connect().then(() => {
          this.reconnectAttempts = 0;
        }).catch((err) => {
          console.debug("WsTransport: reconnect failed", this.sessionId, err);
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
