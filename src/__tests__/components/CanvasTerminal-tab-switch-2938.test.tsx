import { render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeAll, beforeEach, expect, it, vi } from "vitest";
import fixture from "../../dev/fixtures/ego-intent-1948.json";
import wrappedFixture from "../../dev/fixtures/ego-intent-1949.json";

const { transport } = vi.hoisted(() => ({
	transport: {
		sink: null as ((data: ArrayBuffer) => void) | null,
		invoke: vi.fn(),
	},
}));
vi.mock("../../components/Terminal/canvasTerminalTransport", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../components/Terminal/canvasTerminalTransport")>()),
	createTransport: () => ({
		onEvent: vi.fn().mockResolvedValue(undefined),
		onStreamError: vi.fn(),
		subscribe: async (sink: (data: ArrayBuffer) => void) => {
			transport.sink = sink;
		},
		resubscribe: vi.fn().mockResolvedValue(undefined),
		unsubscribe: vi.fn(),
		ackFrame: vi.fn(),
		invoke: transport.invoke,
	}),
}));

import CanvasTerminal from "../../components/Terminal/CanvasTerminal";
import { activityStore } from "../../stores/activityStore";
import { appLogger } from "../../stores/appLogger";
import { terminalsStore } from "../../stores/terminals";
import { isPerfDebug, setPerfDebug } from "../../utils/perfDebug";
import { makeTerminal } from "../helpers/store";

// Canvas output port: retain painted rectangles until the renderer clears them.
// The assertion observes the resulting overlay, not a call to a private painter.
function recordingContext() {
	const fills: { colour: unknown; y: number; h: number }[] = [];
	const text: string[] = [];
	const state: Record<string, unknown> = {
		fillStyle: "#000000",
		fillText: (glyph: string) => text.push(glyph),
		clearRect: () => {
			fills.length = 0;
		},
		fillRect: (_x: number, y: number, _w: number, h: number) => {
			fills.push({ colour: state.fillStyle, y, h });
		},
		measureText: () => ({ width: 8, fontBoundingBoxAscent: 10, fontBoundingBoxDescent: 3 }),
	};
	const context = new Proxy(state, { get: (target, key: string) => target[key] ?? (() => {}) });
	return { context: context as unknown as CanvasRenderingContext2D, fills, text };
}

let notifyVisibility: (visible: boolean) => void;
class VisibilityObserver {
	constructor(private callback: IntersectionObserverCallback) {}
	observe(target: Element) {
		notifyVisibility = (visible) =>
			this.callback(
				[{ target, isIntersecting: visible, time: performance.now() } as IntersectionObserverEntry],
				this as unknown as IntersectionObserver,
			);
		notifyVisibility(true);
	}
	disconnect() {}
}
let boxWidth = 800;
const perfDebug = isPerfDebug();
beforeAll(() => activityStore.hydrate());
let canvases: Map<HTMLCanvasElement, ReturnType<typeof recordingContext>>;
beforeEach(() => {
	setPerfDebug(false);
	canvases = new Map();
	boxWidth = 800;
	vi.spyOn(appLogger, "info").mockImplementation(() => {});
	transport.sink = null;
	transport.invoke.mockReset().mockResolvedValue(undefined);
	Object.defineProperty(document, "fonts", {
		configurable: true,
		value: { load: async () => [], ready: Promise.resolve() },
	});
	vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
		get width() {
			return boxWidth;
		},
		height: 400,
		top: 0,
		left: 0,
		right: 800,
		bottom: 400,
		x: 0,
		y: 0,
		toJSON: () => ({}),
	});
	vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockImplementation(function (this: HTMLCanvasElement) {
		let entry = canvases.get(this);
		if (!entry) {
			entry = recordingContext();
			canvases.set(this, entry);
		}
		return entry.context;
	});
	vi.stubGlobal(
		"ResizeObserver",
		class {
			observe() {}
			disconnect() {}
		},
	);
	vi.stubGlobal("IntersectionObserver", VisibilityObserver);
});
afterEach(() => {
	setPerfDebug(perfDebug);
	vi.useRealTimers();
	vi.restoreAllMocks();
	vi.unstubAllGlobals();
});

// Catches: hiding discards the viewport rows, so show paints a blank canvas until IPC/WS returns.
it.each([800, 640])("repaints retained terminal text on show before a fresh frame (box %i)", async (width) => {
	const terminalId = terminalsStore.add(makeTerminal({ sessionId: "tab-switch-2938" }));
	const view = render(() => <CanvasTerminal sessionId="tab-switch-2938" terminalId={terminalId} />);
	const text = () =>
		[...canvases.values()]
			.flatMap((output) => output.text)
			.join("")
			.replaceAll(" ", "");
	try {
		await waitFor(() => expect(transport.sink).not.toBeNull());
		transport.sink?.(Uint8Array.from(atob(fixture.frameBase64), (c) => c.charCodeAt(0)).buffer);
		await waitFor(() => expect(text()).toContain("intent:Recording"));
		notifyVisibility(false);
		const canvas = view.container.querySelector('canvas[tabindex="0"]') as HTMLCanvasElement;
		expect([canvas.width, canvas.height]).toEqual([1, 1]);
		// A hidden output must not corrupt the last painted viewport. It will be
		// authoritative only when delivered again after the show frame request.
		transport.sink?.(Uint8Array.from(atob(wrappedFixture.frameBase64), (c) => c.charCodeAt(0)).buffer);
		for (const output of canvases.values()) output.text.length = 0;
		boxWidth = width;
		notifyVisibility(true);
		expect(text()).toContain("intent:Recording");
		expect(text()).not.toContain("Ordinaryoutputstaysoutsidetheintenttint.");
		transport.sink?.(Uint8Array.from(atob(wrappedFixture.frameBase64), (c) => c.charCodeAt(0)).buffer);
		await waitFor(() => expect(text()).toContain("Ordinaryoutputstaysoutsidetheintenttint."));
	} finally {
		view.unmount();
		terminalsStore.remove(terminalId);
	}
});
