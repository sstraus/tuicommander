// @vitest-environment jsdom
import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { SubscribePtyOptions } from "../../transport";

const stream = vi.hoisted(() => ({ subscribe: vi.fn() }));
vi.mock("../../transport", () => ({ subscribePty: stream.subscribe }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { warn: vi.fn(), error: vi.fn(), debug: vi.fn() } }));

import { ConnectionBanner } from "../components/ConnectionBanner";
import { OutputView } from "../components/OutputView";

let options: SubscribePtyOptions;
let exit: () => void;
const stop = vi.fn();
function Harness(props: { sessionExists?: boolean } = {}) {
	const [error, setError] = createSignal<string | null>(null);
	let retry: (() => void) | undefined;
	return (
		<>
			<ConnectionBanner offline={error() !== null} authError={false} error={error()} onRetry={() => retry?.()} />
			<OutputView
				sessionExists={props.sessionExists}
				sessionId="test-session"
				onConnectionError={setError}
				onRegisterRetry={(value) => {
					retry = value;
				}}
			/>
		</>
	);
}
beforeEach(() => {
	vi.useFakeTimers();
	stream.subscribe.mockReset().mockImplementation(async (_id, _data, _exit, opts) => {
		options = opts;
		exit = _exit;
		return Object.assign(stop, { pause: vi.fn(), resume: vi.fn() });
	});
	vi.stubGlobal(
		"fetch",
		vi.fn(async () => ({ ok: true, json: async () => ({ lines: [], screen: [], total_lines: 0 }) })),
	);
	vi.stubGlobal(
		"ResizeObserver",
		class {
			observe() {}
			disconnect() {}
		},
	);
	vi.stubGlobal("requestAnimationFrame", () => 0);
});
afterEach(() => {
	cleanup();
	vi.useRealTimers();
	vi.unstubAllGlobals();
});

describe("terminal stream connection landing", () => {
	// Catches: a rejected handshake after session removal permanently claims the healthy server is unreachable.
	it("does not report a server outage when the session ends during its initial handshake", async () => {
		let rejectHandshake!: (reason: Error) => void;
		stream.subscribe.mockImplementationOnce(
			() =>
				new Promise((_resolve, reject) => {
					rejectHandshake = reject;
				}),
		);
		const [exists, setExists] = createSignal(true);
		const view = render(() => <Harness sessionExists={exists()} />);
		await vi.advanceTimersByTimeAsync(0);
		setExists(false);
		rejectHandshake(new Error("WebSocket closed before opening (code 1006): session not found"));
		await vi.advanceTimersByTimeAsync(30_000);
		expect(view.queryByRole("region", { name: "Connection test" })).toBeNull();
	});

	// Catches: healthy session API hides a failed WebSocket, leaving an apparently frozen terminal forever.
	it("shows an outage after five seconds of reconnects and removes it when the stream returns", async () => {
		const view = render(() => <Harness />);
		await vi.advanceTimersByTimeAsync(0);
		options.onReconnecting?.(1, 10);
		await vi.advanceTimersByTimeAsync(3_000);
		options.onReconnecting?.(2, 10);
		await vi.advanceTimersByTimeAsync(2_000);
		expect(view.getByRole("heading", { name: "Server unreachable" })).toBeTruthy();
		options.onReconnected?.();
		expect(view.queryByRole("region", { name: "Connection test" })).toBeNull();
	});
	// Catches: transport retry exhaustion is treated as session death and removes a still-needed outage landing.
	it("keeps the outage visible after socket retry exhaustion and reconnects", async () => {
		const view = render(() => <Harness />);
		await vi.advanceTimersByTimeAsync(0);
		options.onReconnecting?.(10, 10);
		await vi.advanceTimersByTimeAsync(5_000);
		exit();
		expect(view.getByRole("region", { name: "Connection test" })).toBeTruthy();
		await vi.advanceTimersByTimeAsync(3_000);
		expect(view.queryByRole("region", { name: "Connection test" })).toBeNull();
	});
	// Catches: the first rejected handshake has no subscription to resume and manual Retry does nothing.
	it("retries an initial subscription failure from the connection landing", async () => {
		stream.subscribe.mockRejectedValueOnce(new Error("connection refused"));
		const view = render(() => <Harness />);
		await vi.advanceTimersByTimeAsync(0);
		expect(view.getByRole("heading", { name: "Server unreachable" })).toBeTruthy();
		fireEvent.click(view.getByRole("button", { name: "Retry now" }));
		await vi.advanceTimersByTimeAsync(0);
		expect(view.queryByRole("region", { name: "Connection test" })).toBeNull();
	});
	// Catches: an initial failed handshake stays failed after the server returns unless the user presses Retry.
	it("automatically recovers an initial subscription failure", async () => {
		stream.subscribe.mockRejectedValueOnce(new Error("connection refused"));
		const view = render(() => <Harness />);
		await vi.advanceTimersByTimeAsync(0);
		expect(view.getByRole("region", { name: "Connection test" })).toBeTruthy();
		await vi.advanceTimersByTimeAsync(3_000);
		expect(view.queryByRole("region", { name: "Connection test" })).toBeNull();
	});
});
