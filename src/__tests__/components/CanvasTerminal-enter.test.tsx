import { render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

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
import { isPerfDebug, setPerfDebug } from "../../utils/perfDebug";

const writes = () =>
	invoke.mock.calls.filter(([command]) => command === "write_pty").map(([, args]) => (args as { data: string }).data);

describe("CanvasTerminal Enter delivery", () => {
	let input: Element;
	let perfDebug: boolean;
	let unmount: () => void;
	beforeEach(async () => {
		perfDebug = isPerfDebug();
		setPerfDebug(false);
		vi.spyOn(appLogger.diag, "info").mockImplementation(() => {});
		invoke.mockReset().mockResolvedValue(undefined);
		subscribed.mockReset().mockResolvedValue(undefined);
		vi.spyOn(navigator, "maxTouchPoints", "get").mockReturnValue(5);
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
		vi.stubGlobal(
			"IntersectionObserver",
			class {
				observe() {}
				disconnect() {}
			},
		);
		const view = render(() => <CanvasTerminal sessionId="enter-delivery" terminalId="enter-delivery" />);
		unmount = view.unmount;
		await waitFor(() => expect(subscribed).toHaveBeenCalled());
		const keyInput = view.container.querySelector("input, textarea");
		if (!keyInput) throw new Error("Terminal keyboard input was not mounted");
		input = keyInput;
		invoke.mockClear();
	});
	afterEach(() => {
		unmount?.();
		for (const [source, message] of vi.mocked(appLogger.diag.info).mock.calls) {
			expect(source).toBe("terminal");
			expect(message).toMatch(/^resize_pty (request|applied) canvas /);
		}
		setPerfDebug(perfDebug);
		vi.restoreAllMocks();
		resetPlatformCache();
		vi.unstubAllGlobals();
	});

	// Catches: Control silently collapsing to CR before ego can observe BusySteer.
	it.each([
		["Ctrl+Enter", { ctrlKey: true }, "\x1b[13;5u"],
		["Ctrl+Shift+Enter", { ctrlKey: true, shiftKey: true }, "\x1b[13;6u"],
		["Ctrl+Alt+Enter", { ctrlKey: true, altKey: true }, "\x1b[13;7u"],
		["plain Enter", {}, "\r"],
		["Shift+Enter", { shiftKey: true }, "\x1b\r"],
		["Alt+Enter", { altKey: true }, "\r"],
	])("writes %s once at the PTY boundary without Kitty negotiation", async (_label, modifiers, expected) => {
		const event = new KeyboardEvent("keydown", {
			key: "Enter",
			code: "Enter",
			bubbles: true,
			cancelable: true,
			...modifiers,
		});
		input.dispatchEvent(event);
		await waitFor(() => expect(writes()).toEqual([expected]));
		expect(event.defaultPrevented).toBe(true);
	});

	// Catches: stealing Cmd+Enter from document-level user shortcuts.
	it("leaves Cmd+Enter to app shortcuts instead of writing CR", () => {
		const event = new KeyboardEvent("keydown", {
			key: "Enter",
			code: "Enter",
			metaKey: true,
			bubbles: true,
			cancelable: true,
		});
		input.dispatchEvent(event);
		expect(writes()).toEqual([]);
		expect(event.defaultPrevented).toBe(false);
	});
});
