import type { LogLine } from "../mobile/utils/logLine";

// ---------------------------------------------------------------------------
// MCP upstream config types (mirrors Rust structs in mcp_upstream_config.rs)
// ---------------------------------------------------------------------------

export type UpstreamTransport =
	| { type: "http"; url: string }
	| { type: "stdio"; command: string; args?: string[]; env?: Record<string, string>; cwd?: string };

export type FilterMode = "allow" | "deny";

export interface ToolFilter {
	mode: FilterMode;
	patterns: string[];
}

export type UpstreamAuth =
	| { type: "bearer"; token: string }
	| {
			type: "oauth2";
			client_id: string;
			client_secret?: string;
			scopes?: string[];
			authorization_endpoint?: string;
			token_endpoint?: string;
	  };

export interface UpstreamHeader {
	name: string;
	credential_ref: string;
}

export interface UpstreamMcpServer {
	id: string;
	name: string;
	transport: UpstreamTransport;
	enabled: boolean;
	timeout_secs: number;
	tool_filter?: ToolFilter;
	auth?: UpstreamAuth;
	headers?: UpstreamHeader[];
}

export interface UpstreamMcpConfig {
	servers: UpstreamMcpServer[];
}

export interface UpstreamMcpSaveRequest {
	base: UpstreamMcpConfig;
	config: UpstreamMcpConfig;
}

/** HTTP method + path mapping for a Tauri command */
export interface HttpMapping {
	method: "GET" | "POST" | "PUT" | "DELETE";
	path: string;
	body?: unknown;
	/** This route may return raw text as well as JSON (plugin data). */
	allowText?: boolean;
	/** Transform the HTTP response before returning (e.g. for can_spawn_session) */
	transform?: (data: unknown) => unknown;
	/**
	 * Treat an HTTP 404 as a successful `null` result instead of throwing.
	 * Bridges Tauri commands whose contract is `Option<T>` (None → null) onto
	 * REST routes that signal "not found" with 404 (e.g. read_plugin_data).
	 */
	notFoundAsNull?: boolean;
}

/** Unsubscribe function returned by subscribe() */
export type Unsubscribe = () => void;

/**
 * A PTY subscription: still callable to dispose it, plus the two controls a
 * backgrounded client needs.
 *
 * It is a callable object rather than a record so every existing caller — which
 * only ever invokes the handle — keeps working unchanged.
 */
export interface PtySubscription extends Unsubscribe {
	/**
	 * Stop draining the stream and drop the socket. NOT a session exit: `onExit`
	 * stays silent, and the consumed-line cursor survives so `resume` picks up
	 * exactly where delivery stopped.
	 */
	pause(): void;
	/** Re-open from the live cursor. A no-op unless currently paused. */
	resume(): void;
}

/** Parsed event from WebSocket JSON framing */
export interface WsParsedEvent {
	type: string;
	[key: string]: unknown;
}

/**
 * Subscribe to PTY session events.
 *
 * In Tauri: uses listen() for pty-activity-{sessionId}, pty-exit-{sessionId}.
 * In browser: uses WebSocket to /sessions/{sessionId}/stream with JSON framing:
 *   - {"type":"output","data":"..."} for raw PTY output
 *   - {"type":"activity"} for the throttled "output happened" pulse
 *   - {"type":"parsed","event":{...}} for structured events (questions, rate limits)
 *   - {"type":"exit"} / {"type":"closed"} for session lifecycle
 *
 * NOTE ON `onData`: desktop delivers NO output through this subscription. The
 * canvas renders from grid frames and plugin watcher lines are assembled in
 * Rust, so no desktop consumer needs the bytes and none crosses the IPC
 * boundary. Browser/PWA still receives them — `src/mobile/OutputView.tsx` reads
 * the stream directly. Use `onActivity` for "is this session producing output",
 * which is the question both transports answer identically.
 *
 * @param sessionId - PTY session ID
 * @param onData - Called with each chunk of PTY output (browser/PWA only)
 * @param onExit - Called when the session exits
 * @param onParsed - Optional: called with structured parsed events (browser mode)
 * @returns Promise resolving to an unsubscribe function
 */
export interface SubscribePtyOptions {
	/** Request ANSI-stripped plain text from the server (for non-terminal views like mobile) */
	stripAnsi?: boolean;
	/**
	 * Use VT100-extracted log lines (`format=log`).
	 * When `onLogLines` is set, structured LogLine objects are delivered there.
	 * Otherwise `onData` is called with `\n`-joined plain text (backward compat).
	 * Overrides `stripAnsi` when set.
	 */
	format?: "log";
	/**
	 * Receive structured LogLine objects from `format=log` frames.
	 * Each LogLine has `spans: [{text, fg?, bg?, bold?, italic?, underline?}]`.
	 */
	onLogLines?: (lines: LogLine[]) => void;
	/** Receive current screen rows (LogLine objects with styled spans) pushed alongside log frames. */
	onScreenRows?: (rows: unknown[]) => void;
	/** Receive the current PTY input line text (extracted from prompt row). */
	onInputLine?: (text: string | null) => void;
	/** Starting offset for log-mode catch-up (skip lines already fetched via HTTP). */
	logOffset?: number;
	/** Receive real-time SessionState snapshots pushed by the server on parsed events. */
	onStateChange?: (state: Record<string, unknown>) => void;
	/**
	 * "This session produced output." Throttled to ~1/s by the Rust producer and
	 * payload-free: it answers whether bytes are flowing, not what they were.
	 * Delivered on both transports from one backend signal.
	 */
	onActivity?: () => void;
	onTitle?: (title: string) => void;
	onParsed?: (event: WsParsedEvent) => void;
	/** Called when WebSocket drops and reconnect is attempted (browser mode only). */
	onReconnecting?: (attempt: number, maxAttempts: number) => void;
	/** Called when WebSocket reconnect succeeds (browser mode only). */
	onReconnected?: () => void;
}

/** Why `onResync` fired: events were missed, and this says how. */
export type ResyncReason = "reconnect" | "lagged";

export interface SubscribeEventsOptions {
	/** Base URL of a remote instance. Omit to talk to this one. */
	baseUrl?: string;
	/**
	 * Called when a gap opened in the stream and the consumer should re-read
	 * whatever state it derives from these events. NOT called for a normal
	 * event, and NOT called on the first connection — a consumer that has just
	 * subscribed does its own initial read.
	 *
	 * Only the SSE transport can fire it. Tauri `listen()` is in-process: there
	 * is no connection to drop and no bounded channel to fall behind, so a
	 * desktop resync path would be a second path for a transition that already
	 * has one, which is the defect the AGENTS.md fix-quality rule names.
	 */
	onResync?: (reason: ResyncReason) => void;
}
