import { fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import fixture from "../../dev/fixtures/ego-intent-1948.json";
import { VisibleIntersectionObserver } from "../helpers/visibleIntersectionObserver";

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
import { terminalsStore } from "../../stores/terminals";
import { makeTerminal } from "../helpers/store";

// The fixture frame is the size another viewer gave the PTY: 24 rows x 80 cols.
const FOREIGN = { rows: 24, cols: 80 };

beforeEach(() => {
	transport.sink = null;
	transport.invoke.mockReset().mockResolvedValue(undefined);
	Object.defineProperty(document, "fonts", {
		configurable: true,
		value: { load: async () => [], ready: Promise.resolve() },
	});
	vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
		width: 400,
		height: 300,
		top: 0,
		left: 0,
		right: 400,
		bottom: 300,
		x: 0,
		y: 0,
		toJSON: () => ({}),
	});
	const ctx = new Proxy({} as Record<string, unknown>, {
		get: (t, k: string) =>
			k === "measureText"
				? () => ({ width: 8, fontBoundingBoxAscent: 10, fontBoundingBoxDescent: 3 })
				: (t[k] ?? (() => {})),
	});
	vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(ctx as unknown as CanvasRenderingContext2D);
	vi.stubGlobal(
		"ResizeObserver",
		class {
			observe() {}
			disconnect() {}
		},
	);
	vi.stubGlobal("IntersectionObserver", VisibleIntersectionObserver);
});
afterEach(() => {
	vi.restoreAllMocks();
	vi.unstubAllGlobals();
});

const resizes = () => transport.invoke.mock.calls.filter((c) => c[0] === "resize_pty").map((c) => c[1]);

// Catches: last-writer-wins with no reclaim — viewer B (14x37-class box) gets focus or
// input after viewer A set the PTY to another size, and the child keeps A's size.
it("re-sends its own size on focus and on input once another viewer changed the PTY size", async () => {
	const terminalId = terminalsStore.add(makeTerminal({ sessionId: "reclaim-2131" }));
	const view = render(() => <CanvasTerminal sessionId="reclaim-2131" terminalId={terminalId} />);
	try {
		await waitFor(() => expect(resizes()).toHaveLength(1));
		const mine = resizes()[0] as { rows: number; cols: number };
		expect(mine).not.toMatchObject(FOREIGN);
		await waitFor(() => expect(transport.sink).not.toBeNull());
		transport.sink?.(Uint8Array.from(atob(fixture.frameBase64), (c) => c.charCodeAt(0)).buffer);
		await new Promise((r) => setTimeout(r, 50));
		expect(resizes()).toHaveLength(1);

		const input = view.container.querySelector("textarea, input") as HTMLElement;
		fireEvent.focus(input);
		await waitFor(() => expect(resizes()).toHaveLength(2));
		expect(resizes()[1]).toMatchObject({ sessionId: "reclaim-2131", ...mine });

		// Typing before the new frame arrives must not send one request per key.
		fireEvent.keyDown(input, { key: "a" });
		fireEvent.keyDown(input, { key: "b" });
		await new Promise((r) => setTimeout(r, 50));
		expect(resizes()).toHaveLength(2);
	} finally {
		view.unmount();
		terminalsStore.remove(terminalId);
	}
});

// Catches: the once-per-foreign-size memory outlives the reclaim — viewer B reclaims on focus, the
// frame confirms B's size, viewer A takes the same size again, and B's next focus is swallowed
// because "24x80" was already asked for.
it("reclaims again when another viewer takes the same size back after a confirmed reclaim", async () => {
	const terminalId = terminalsStore.add(makeTerminal({ sessionId: "reclaim-again-2131" }));
	const view = render(() => <CanvasTerminal sessionId="reclaim-again-2131" terminalId={terminalId} />);
	try {
		await waitFor(() => expect(resizes()).toHaveLength(1));
		const mine = resizes()[0] as { rows: number; cols: number };
		await waitFor(() => expect(transport.sink).not.toBeNull());
		const foreign = () => Uint8Array.from(atob(fixture.frameBase64), (c) => c.charCodeAt(0)).buffer;
		const ownSize = () => {
			const buf = foreign();
			const v = new DataView(buf);
			v.setUint16(18, mine.rows, true);
			v.setUint16(20, mine.cols, true);
			return buf;
		};
		transport.sink?.(foreign());
		await new Promise((r) => setTimeout(r, 50));
		const input = view.container.querySelector("textarea, input") as HTMLElement;
		fireEvent.focus(input);
		await waitFor(() => expect(resizes()).toHaveLength(2));

		transport.sink?.(ownSize());
		await new Promise((r) => setTimeout(r, 50));
		transport.sink?.(foreign());
		await new Promise((r) => setTimeout(r, 50));
		fireEvent.blur(input);
		fireEvent.focus(input);
		await waitFor(() => expect(resizes()).toHaveLength(3));
	} finally {
		view.unmount();
		terminalsStore.remove(terminalId);
	}
});
