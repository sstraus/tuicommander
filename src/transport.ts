/**
 * Transport abstraction layer — auto-detects Tauri IPC vs HTTP/WebSocket.
 *
 * In Tauri mode: uses invoke() for RPC, listen() for events.
 * In browser mode: uses fetch() for RPC, WebSocket for PTY streaming.
 */

export { DEDICATED_WS_COMMANDS, INTENTIONALLY_UNMAPPED, mapCommandToHttp } from "./transport/commandTable";
export { isTauri } from "./transport/environment";
export { buildHttpUrl, HttpRpcError, owningConnectionFor, rpc } from "./transport/http";
export { subscribeEvents, subscribePty } from "./transport/ipc";
export type {
	FilterMode,
	HttpMapping,
	PtySubscription,
	ResyncReason,
	SubscribeEventsOptions,
	SubscribePtyOptions,
	ToolFilter,
	Unsubscribe,
	UpstreamAuth,
	UpstreamHeader,
	UpstreamMcpConfig,
	UpstreamMcpSaveRequest,
	UpstreamMcpServer,
	UpstreamTransport,
	WsParsedEvent,
} from "./transport/types";
