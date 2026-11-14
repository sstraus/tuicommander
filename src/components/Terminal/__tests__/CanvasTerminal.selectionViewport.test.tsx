import { render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { appLogger } from "../../../stores/appLogger";
import { terminalsStore } from "../../../stores/terminals";
import { isPerfDebug, setPerfDebug } from "../../../utils/perfDebug";
import CanvasTerminal, { type CanvasTerminalRef } from "../CanvasTerminal";

const harness = vi.hoisted(() => ({
	frame: undefined as ((data: ArrayBuffer) => void) | undefined,
	paint: vi.fn(),
}));
vi.mock("../canvasTerminalTransport", async (original) => ({
	...(await original<typeof import("../canvasTerminalTransport")>()),
	createTransport: () => ({
		subscribe: async (callback: (data: ArrayBuffer) => void) => {
			harness.frame = callback;
		},
		unsubscribe() {},
		onEvent: async () => {},
		ackFrame() {},
		invoke: async (command: string) =>
			command === "terminal_get_styled_rows" ? new ArrayBuffer(0) : command === "terminal_search" ? [] : null,
	}),
}));
// Observe the component's renderer boundary, not a replacement scroll/selection implementation.
vi.mock("../gridRenderer", () => ({
	createGridRenderer: () => ({ paintGrid: harness.paint, setTheme() {}, invalidateCaches() {} }),
}));

function frame(displayOffset: number): ArrayBuffer {
	// Native full-row wire format: history=100, chosen offset, two rows of four cells.
	const buffer = new ArrayBuffer(26 + 2 * (4 + 4 * 11));
	const view = new DataView(buffer);
	view.setUint16(0, 2, true);
	view.setUint32(7, displayOffset, true);
	view.setUint32(11, 100, true);
	view.setUint16(18, 2, true);
	view.setUint16(20, 4, true);
	let at = 26;
	for (let row = 0; row < 2; row++) {
		view.setUint16(at, row, true);
		view.setUint16(at + 2, 4, true);
		at += 4;
		for (const character of "text") {
			view.setUint32(at, character.codePointAt(0)!, true);
			at += 11;
		}
	}
	return buffer;
}

let dispose: (() => void) | undefined;
let terminalId: string | undefined;
let perfBefore: boolean;
let rafs: Map<number, FrameRequestCallback>;
function flushFrame() {
	const pending = [...rafs.values()];
	rafs.clear();
	for (const callback of pending) callback(0);
}
beforeEach(() => {
	perfBefore = isPerfDebug();
	setPerfDebug(false);
	vi.spyOn(appLogger, "info").mockImplementation(() => {});
	harness.paint.mockClear();
	harness.frame = undefined;
	rafs = new Map();
	let sequence = 0;
	vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
		rafs.set(++sequence, callback);
		return sequence;
	});
	vi.stubGlobal("cancelAnimationFrame", (id: number) => rafs.delete(id));
	vi.stubGlobal(
		"IntersectionObserver",
		class {
			constructor(private callback: IntersectionObserverCallback) {}
			observe(target: Element) {
				this.callback(
					[{ target, isIntersecting: true, time: 1 } as IntersectionObserverEntry],
					this as unknown as IntersectionObserver,
				);
			}
			disconnect() {}
		},
	);
	vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue(new DOMRect(0, 0, 80, 40));
	const context = new Proxy(
		{
			canvas: document.createElement("canvas"),
			measureText: () => ({ width: 10, actualBoundingBoxAscent: 12, actualBoundingBoxDescent: 4 }),
		},
		{
			get(target, key) {
				return key in target ? target[key as keyof typeof target] : () => {};
			},
		},
	);
	vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(context as unknown as CanvasRenderingContext2D);
	Object.defineProperty(document, "fonts", {
		configurable: true,
		value: { load: async () => [], ready: Promise.resolve() },
	});
});
afterEach(() => {
	dispose?.();
	dispose = undefined;
	if (terminalId) {
		terminalsStore.remove(terminalId);
		expect(appLogger.info).toHaveBeenCalledWith("terminal", `TermStore.remove(${terminalId})`, expect.any(Object));
	}
	terminalId = undefined;
	vi.restoreAllMocks();
	vi.unstubAllGlobals();
	setPerfDebug(perfBefore);
});

// Catches selection's direct backend-frame repaint replacing a locally scrolled viewport.
it.each([
	{ backendOffset: 0, wheel: true, pending: false },
	{ backendOffset: 50, wheel: true, pending: false },
	{ backendOffset: 50, wheel: false, pending: false },
	// Catches a normal repaint queued before wheel stealing ownership at execution.
	{ backendOffset: 50, wheel: true, pending: true },
])(
	"keeps selection repaint at the displayed viewport (backend=$backendOffset, local scroll=$wheel, queued=$pending)",
	async ({ backendOffset, wheel, pending }) => {
		const id = terminalsStore.add({
			sessionId: "selection-test",
			cwd: "/test",
			repoPath: "/test",
			name: "Selection test",
			fontSize: 14,
			awaitingInput: null,
		});
		terminalId = id;
		const mounted = render(() => <CanvasTerminal sessionId="selection-test" terminalId={id} />);
		dispose = mounted.unmount;
		await waitFor(() => expect(harness.frame).toBeTypeOf("function"));
		harness.frame!(frame(backendOffset));
		flushFrame();
		const canvas = mounted.container.querySelector<HTMLCanvasElement>("canvas[tabindex]")!;
		if (pending) harness.frame!(frame(backendOffset));
		const paintsBeforeWheel = harness.paint.mock.calls.length;
		if (wheel) canvas.dispatchEvent(new WheelEvent("wheel", { deltaY: -200, bubbles: true, cancelable: true }));
		flushFrame();
		flushFrame();
		const scrolledTop = harness.paint.mock.calls.at(-1)![2].viewportTop;
		if (wheel) expect(scrolledTop).toBeLessThan(100 - backendOffset);
		else expect(scrolledTop).toBe(50);
		for (const paint of harness.paint.mock.calls.slice(paintsBeforeWheel))
			expect(paint[2].viewportTop).toBe(scrolledTop);
		const paintsBeforeDrag = harness.paint.mock.calls.length;
		canvas.dispatchEvent(
			new MouseEvent("mousedown", { button: 0, buttons: 1, clientX: 10, clientY: 10, bubbles: true }),
		);
		document.dispatchEvent(new MouseEvent("mousemove", { buttons: 1, clientX: 30, clientY: 20, bubbles: true }));
		flushFrame();
		flushFrame();
		const dragPaints = harness.paint.mock.calls.slice(paintsBeforeDrag);
		expect(dragPaints.length).toBeGreaterThan(0);
		for (const paint of dragPaints) expect(paint[2].viewportTop).toBe(scrolledTop);
	},
);

// Catches search and timestamp repaints painting the backend frame over a locally scrolled viewport.
it.each([
	{
		trigger: "agent output refreshes an open search",
		act: async (_ref: CanvasTerminalRef) => harness.frame!(frame(50)),
	},
	{ trigger: "searchClear", act: async (ref: CanvasTerminalRef) => ref.searchClear() },
	{ trigger: "searchFind with an empty query", act: async (ref: CanvasTerminalRef) => void (await ref.searchFind("")) },
	{
		trigger: "timestamp toggle",
		act: async (_ref: CanvasTerminalRef, input: HTMLInputElement) => {
			input.dispatchEvent(new KeyboardEvent("keydown", { key: "Meta", ctrlKey: true, metaKey: true, bubbles: true }));
			input.dispatchEvent(new KeyboardEvent("keyup", { key: "Meta", bubbles: true }));
		},
	},
])("keeps the displayed viewport when $trigger repaints during a smooth scroll", async ({ act }) => {
	const id = terminalsStore.add({
		sessionId: "search-test",
		cwd: "/test",
		repoPath: "/test",
		name: "Search test",
		fontSize: 14,
		awaitingInput: null,
	});
	terminalId = id;
	let ref: CanvasTerminalRef | undefined;
	const mounted = render(() => (
		<CanvasTerminal sessionId="search-test" terminalId={id} onRef={(value) => (ref = value)} />
	));
	dispose = mounted.unmount;
	await waitFor(() => expect(harness.frame).toBeTypeOf("function"));
	harness.frame!(frame(50));
	flushFrame();
	await ref!.searchFind("text");
	const canvas = mounted.container.querySelector<HTMLCanvasElement>("canvas[tabindex]")!;
	canvas.dispatchEvent(new WheelEvent("wheel", { deltaY: -200, bubbles: true, cancelable: true }));
	flushFrame();
	flushFrame();
	const scrolledTop = harness.paint.mock.calls.at(-1)![2].viewportTop;
	expect(scrolledTop).toBeLessThan(50);
	const paintsBefore = harness.paint.mock.calls.length;
	await act(ref!, mounted.container.querySelector<HTMLInputElement>("input")!);
	await new Promise((resolve) => setTimeout(resolve, 0));
	flushFrame();
	flushFrame();
	const paints = harness.paint.mock.calls.slice(paintsBefore);
	expect(paints.length).toBeGreaterThan(0);
	for (const paint of paints) expect(paint[2].viewportTop).toBe(scrolledTop);
});
