import { render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { VisibleIntersectionObserver } from "../helpers/visibleIntersectionObserver";

const { invoke, subscribed } = vi.hoisted(() => ({ invoke: vi.fn(), subscribed: vi.fn() }));
vi.mock("../../components/Terminal/canvasTerminalTransport", async (original) => ({
	...(await original<typeof import("../../components/Terminal/canvasTerminalTransport")>()),
	createTransport: () => ({
		onEvent: vi.fn().mockResolvedValue(undefined),
		subscribe: subscribed,
		unsubscribe: vi.fn(),
		ackFrame: vi.fn(),
		invoke,
	}),
}));
vi.mock("../../components/Terminal/gridRenderer", () => ({
	createGridRenderer: () => ({ setTheme: vi.fn(), invalidateCaches: vi.fn(), paintGrid: vi.fn(), paintRow: vi.fn() }),
}));

import CanvasTerminal from "../../components/Terminal/CanvasTerminal";
import { resetPlatformCache } from "../../platform";
import { appLogger } from "../../stores/appLogger";
import { terminalsStore } from "../../stores/terminals";

const calls = (command: string) =>
	invoke.mock.calls.filter(([name]) => name === command).map(([, args]) => args as Record<string, unknown>);

/** A full 24x80 binary frame (wire layout of decodeBinaryFrame) scrolled back by `displayOffset` rows. */
function frame(displayOffset: number): ArrayBuffer {
	const buffer = new ArrayBuffer(26 + 24 * (4 + 80 * 11));
	const view = new DataView(buffer);
	view.setUint16(0, 24, true);
	view.setUint32(7, displayOffset, true);
	view.setUint32(11, 100, true);
	view.setUint16(18, 24, true);
	view.setUint16(20, 80, true);
	for (let row = 0; row < 24; row++) {
		const offset = 26 + row * (4 + 80 * 11);
		view.setUint16(offset, row, true);
		view.setUint16(offset + 2, 80, true);
	}
	return buffer;
}

describe("CanvasTerminal plain Down while scrolled back", () => {
	let input: Element;
	let terminalId: string;
	let unmount: () => void;
	beforeEach(async () => {
		vi.spyOn(appLogger.diag, "info").mockImplementation(() => {});
		invoke.mockReset().mockResolvedValue(undefined);
		subscribed.mockReset().mockResolvedValue(undefined);
		Object.defineProperty(document, "fonts", {
			configurable: true,
			value: { load: () => Promise.resolve([]), ready: Promise.resolve() },
		});
		vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(
			new Proxy(
				{},
				{
					get: (_target, property) =>
						property === "measureText"
							? () => ({ width: 8, fontBoundingBoxAscent: 10, fontBoundingBoxDescent: 3 })
							: vi.fn(),
				},
			) as unknown as CanvasRenderingContext2D,
		);
		vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
			width: 800,
			height: 1000,
			top: 0,
			left: 0,
			right: 800,
			bottom: 1000,
			x: 0,
			y: 0,
			toJSON: () => ({}),
		});
		vi.stubGlobal(
			"ResizeObserver",
			class {
				observe() {}
				disconnect() {}
			},
		);
		vi.stubGlobal("IntersectionObserver", VisibleIntersectionObserver);
		terminalId = terminalsStore.add({
			sessionId: "down-snap",
			cwd: "/repo",
			name: "shell",
			fontSize: 13,
			awaitingInput: null,
		});
		const view = render(() => <CanvasTerminal sessionId="down-snap" terminalId={terminalId} />);
		unmount = view.unmount;
		await waitFor(() => expect(subscribed).toHaveBeenCalled());
		const keyInput = view.container.querySelector("input, textarea");
		if (!keyInput) throw new Error("Terminal keyboard input was not mounted");
		input = keyInput;
	});
	afterEach(() => {
		unmount?.();
		terminalsStore.remove(terminalId);
		vi.restoreAllMocks();
		resetPlatformCache();
		vi.unstubAllGlobals();
	});

	const pressDown = () => {
		const event = new KeyboardEvent("keydown", {
			key: "ArrowDown",
			code: "ArrowDown",
			bubbles: true,
			cancelable: true,
		});
		input.dispatchEvent(event);
		return event;
	};

	// Catches: Down forwarded to the agent while scrolled back (ego relies on Down only
	// returning the view to the live screen, never recalling input history).
	it("snaps the viewport to the bottom and writes nothing to the PTY", async () => {
		subscribed.mock.calls[0][0](frame(10));
		invoke.mockClear();
		const event = pressDown();
		await waitFor(() => expect(calls("terminal_scroll")).toEqual([{ sessionId: "down-snap", delta: -10 }]));
		expect(calls("write_pty")).toEqual([]);
		expect(event.defaultPrevented).toBe(true);
	});

	// Catches: the snap swallowing Down at the live screen, so agents and shells lose history navigation.
	it("forwards Down to the PTY when the viewport is already at the bottom", async () => {
		subscribed.mock.calls[0][0](frame(0));
		invoke.mockClear();
		pressDown();
		await waitFor(() => expect(calls("write_pty")).toEqual([{ sessionId: "down-snap", data: "\x1b[B" }]));
		expect(calls("terminal_scroll")).toEqual([]);
	});
});
