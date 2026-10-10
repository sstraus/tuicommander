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

const writes = () =>
	invoke.mock.calls.filter(([command]) => command === "write_pty").map(([, args]) => (args as { data: string }).data);

describe("CanvasTerminal paste precedence", () => {
	let input: Element;
	let unmount: () => void;
	beforeEach(async () => {
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
		const view = render(() => <CanvasTerminal sessionId="paste-precedence" terminalId="paste-precedence" />);
		unmount = view.unmount;
		await waitFor(() => expect(subscribed).toHaveBeenCalled());
		const keyInput = view.container.querySelector("input, textarea");
		if (!keyInput) throw new Error("Terminal keyboard input was not mounted");
		input = keyInput;
		invoke.mockClear();
	});
	afterEach(() => {
		unmount?.();
		vi.restoreAllMocks();
		resetPlatformCache();
		vi.unstubAllGlobals();
	});

	function paste(text: string | null, imageType = "image/png") {
		const items = [
			...(text === null ? [] : [{ kind: "string", type: "text/plain", getAsFile: () => null }]),
			{ kind: "file", type: imageType, getAsFile: () => new File(["image"], "clip.png", { type: imageType }) },
		];
		const event = new Event("paste", { bubbles: true, cancelable: true });
		Object.defineProperty(event, "clipboardData", {
			value: { items, getData: (type: string) => (type === "text" || type === "text/plain" ? (text ?? "") : "") },
		});
		input.dispatchEvent(event);
		expect(event.defaultPrevented).toBe(true);
	}

	it("pastes substantive text instead of attaching an incidental clipboard image", () => {
		paste("Please explain this line");
		expect(writes()).toEqual(["Please explain this line"]);
	});

	it.each(["image/png", "image/tiff"])("keeps screenshot-only %s pastes as Ctrl+V for the terminal agent", (type) => {
		paste(null, type);
		expect(writes()).toEqual(["\x16"]);
	});

	it("keeps Finder image filename pastes as Ctrl+V rather than typing the path", () => {
		paste("/Users/Boss/Pictures/clip.png");
		expect(writes()).toEqual(["\x16"]);
	});
});
