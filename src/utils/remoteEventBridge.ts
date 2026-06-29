/**
 * SSE event bridge for remote daemon connections.
 *
 * Subscribes to repo-changed, session-status, and session-closed events
 * from a remote TUIC daemon and routes them into local stores.
 */

import { appLogger, previewLogPayload } from "../stores/appLogger";
import { repositoriesStore } from "../stores/repositories";

/**
 * Start an SSE bridge to a remote daemon's /events endpoint.
 *
 * @param connectionId - The remote connection ID (for logging)
 * @param baseUrl - The base URL of the remote daemon (e.g. http://127.0.0.1:12345)
 * @returns A cleanup function that closes the EventSource and stops reconnection
 */
export function startRemoteEventBridge(connectionId: string, baseUrl: string): () => void {
	let es: EventSource | null = null;
	let closed = false;
	let reconnectDelay = 1000;
	let reconnectTimer: ReturnType<typeof setTimeout> | null = null;

	function connect(): void {
		if (closed) return;

		const url = `${baseUrl}/events?types=${encodeURIComponent("repo-changed,session-status,session-closed")}`;
		es = new EventSource(url);

		es.onopen = () => {
			reconnectDelay = 1000;
			appLogger.debug("network", `SSE bridge connected for ${connectionId}`);
		};

		// The server sends named SSE events (event: repo-changed\ndata: {...}\n\n),
		// so we use addEventListener per event type — matching transport.ts subscribeEvents.
		es.addEventListener("repo-changed", ((event: MessageEvent) => {
			try {
				const payload = JSON.parse(event.data) as { path?: string };
				if (typeof payload.path === "string") {
					repositoriesStore.bumpRevision(payload.path);
				}
			} catch {
				appLogger.warn("network", "Failed to parse repo-changed SSE event", {
					connectionId,
					eventData: previewLogPayload(event.data),
				});
			}
		}) as EventListener);

		// session-status and session-closed are handled by terminal polling;
		// we listen here only so EventSource keeps the types registered.
		es.addEventListener("session-status", (() => {
			// Handled by terminal polling — no-op
		}) as EventListener);

		es.addEventListener("session-closed", (() => {
			// Handled by terminal polling — no-op
		}) as EventListener);

		es.onerror = () => {
			es?.close();
			if (closed) return;
			appLogger.debug("network", `SSE bridge error for ${connectionId}, reconnecting in ${reconnectDelay}ms`);
			reconnectTimer = setTimeout(() => {
				reconnectDelay = Math.min(reconnectDelay * 2, 30_000);
				connect();
			}, reconnectDelay);
		};
	}

	connect();

	return () => {
		closed = true;
		es?.close();
		if (reconnectTimer) clearTimeout(reconnectTimer);
	};
}
