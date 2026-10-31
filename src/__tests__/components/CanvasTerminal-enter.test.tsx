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
import { isPerfDebug, setPerfDebug } from "../../utils/perfDebug";

const writes = () =>
	invoke.mock.calls.filter(([command]) => command === "write_pty").map(([, args]) => (args as { data: string }).data);

describe("CanvasTerminal Enter delivery", () => {
	let input: Element;
	let terminalId: string;
	let perfDebug: boolean;
	let unmount: () => void;
	beforeEach(async () => {
		perfDebug = isPerfDebug();
		setPerfDebug(false);
		vi.spyOn(appLogger.diag, "info").mockImplementation(() => {});
		vi.spyOn(appLogger, "info").mockImplementation(() => {});
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
		vi.stubGlobal("IntersectionObserver", VisibleIntersectionObserver);
		terminalId = terminalsStore.add({
			sessionId: "enter-delivery",
			cwd: "/repo",
			name: "shell",
			fontSize: 13,
			awaitingInput: null,
		});
		const view = render(() => <CanvasTerminal sessionId="enter-delivery" terminalId={terminalId} />);
		unmount = view.unmount;
		await waitFor(() => expect(subscribed).toHaveBeenCalled());
		const keyInput = view.container.querySelector("input, textarea");
		if (!keyInput) throw new Error("Terminal keyboard input was not mounted");
		input = keyInput;
		invoke.mockClear();
	});
	afterEach(() => {
		unmount?.();
		terminalsStore.remove(terminalId);
		for (const [source, message] of vi.mocked(appLogger.diag.info).mock.calls) {
			expect(source).toBe("terminal");
			expect(message).toMatch(/^resize_pty (request|applied) canvas /);
		}
		setPerfDebug(perfDebug);
		vi.restoreAllMocks();
		resetPlatformCache();
		vi.unstubAllGlobals();
	});

	// Catches: unsolicited CSI u corrupting bash/zsh commands without negotiation.
	it.each([
		["Ctrl+Enter", { ctrlKey: true }, "\r"],
		["Ctrl+Shift+Enter", { ctrlKey: true, shiftKey: true }, "\r"],
		["Ctrl+Alt+Enter", { ctrlKey: true, altKey: true }, "\r"],
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

	// Catches: losing ego BusySteer, applying its exception to Codex, or retaining it after exit.
	it("uses current ego identity only, then restores CR on shell return", async () => {
		for (const [agentType, expected] of [
			["ego", "\x1b[13;5u"],
			["codex", "\r"],
			[null, "\r"],
		] as const) {
			terminalsStore.update(terminalId, { agentType });
			invoke.mockClear();
			input.dispatchEvent(
				new KeyboardEvent("keydown", { key: "Enter", ctrlKey: true, bubbles: true, cancelable: true }),
			);
			await waitFor(() => expect(writes()).toEqual([expected]));
		}
	});

	// Catches: ignoring negotiated flags or keeping CSI u after the application pops the mode.
	it("follows negotiated Kitty frames without requiring ego identity", async () => {
		for (const [flags, expected] of [
			[1, "\x1b[13;5u"],
			[0, "\r"],
		] as const) {
			const frame = new ArrayBuffer(26 + 24 * (4 + 80 * 11));
			const header = new DataView(frame);
			header.setUint16(0, 24, true);
			header.setUint8(16, flags);
			header.setUint16(18, 24, true);
			header.setUint16(20, 80, true);
			for (let row = 0; row < 24; row++) {
				const offset = 26 + row * (4 + 80 * 11);
				header.setUint16(offset, row, true);
				header.setUint16(offset + 2, 80, true);
			}
			subscribed.mock.calls[0][0](frame);
			invoke.mockClear();
			input.dispatchEvent(
				new KeyboardEvent("keydown", { key: "Enter", ctrlKey: true, bubbles: true, cancelable: true }),
			);
			await waitFor(() => expect(writes()).toEqual([expected]));
		}
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
