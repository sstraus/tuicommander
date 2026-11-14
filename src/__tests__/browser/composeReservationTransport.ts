import type { TerminalTransport } from "../../components/Terminal/canvasTerminalTransport";

export { toBinaryPayload } from "../../components/Terminal/canvasTerminalTransport";

// Only the external transport is replaced. CanvasTerminal's measurement, metrics,
// ResizeObserver, debounce and resize_pty dispatch remain the production path.
export const calls: Array<{ cmd: string; args: Record<string, unknown> }> = [];
export let ready = false;
export function createTransport(): TerminalTransport {
	return {
		async subscribe() {
			ready = true;
		},
		async resubscribe() {},
		unsubscribe() {},
		async invoke(cmd, args) {
			calls.push({ cmd, args });
			return null;
		},
		ackFrame() {},
		setVisible() {},
		async onEvent() {},
	};
}
