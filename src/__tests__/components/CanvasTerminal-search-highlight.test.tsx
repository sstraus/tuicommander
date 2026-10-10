import { render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { CanvasTerminalRef } from "../../components/Terminal/CanvasTerminal";

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

const matches = [0, 1, 2, 3].flatMap((row) => [0, 2, 4, 6].map((col) => ({ row, col_start: col, col_end: col + 1 })));

// Owned grid wire format, not a fixture of an external terminal application.
function frame(firstRow = "f F f F ", colour = 0): ArrayBuffer {
	const buffer = new ArrayBuffer(26 + 4 * (4 + 8 * 11));
	const view = new DataView(buffer);
	view.setUint16(0, 4, true);
	view.setUint16(4, 7, true); // Cursor occupies a blank cell, away from matches.
	view.setUint8(6, 1);
	view.setUint16(18, 4, true);
	view.setUint16(20, 8, true);
	let offset = 26;
	for (let row = 0; row < 4; row++) {
		view.setUint16(offset, row, true);
		view.setUint16(offset + 2, 8, true);
		offset += 4;
		for (const char of row === 0 ? firstRow : "f F f F ") {
			view.setUint32(offset, char.codePointAt(0) ?? 32, true);
			view.setUint8(offset + 4, colour);
			offset += 11;
		}
	}
	return buffer;
}

// Canvas output port: retain painted rectangles until the renderer clears them.
// The assertion observes the resulting overlay, not a call to a private painter.
function recordingContext() {
	const fills: { colour: unknown; y: number; h: number }[] = [];
	const state: Record<string, unknown> = {
		fillStyle: "#000000",
		clearRect: () => {
			fills.length = 0;
		},
		fillRect: (_x: number, y: number, _w: number, h: number) => {
			fills.push({ colour: state.fillStyle, y, h });
		},
		measureText: () => ({ width: 8, fontBoundingBoxAscent: 10, fontBoundingBoxDescent: 3 }),
	};
	const context = new Proxy(state, { get: (target, key: string) => target[key] ?? (() => {}) });
	return { context: context as unknown as CanvasRenderingContext2D, fills };
}

let canvases: Map<HTMLCanvasElement, ReturnType<typeof recordingContext>>;
beforeEach(() => {
	canvases = new Map();
	transport.sink = null;
	transport.invoke
		.mockReset()
		.mockImplementation(async (command: string, args: { query?: string }) =>
			command === "terminal_search" ? (args.query === "f" ? matches : []) : undefined,
		);
	Object.defineProperty(document, "fonts", {
		configurable: true,
		value: { load: async () => [], ready: Promise.resolve() },
	});
	vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
		width: 800,
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
	vi.stubGlobal(
		"IntersectionObserver",
		class {
			observe() {}
			disconnect() {}
		},
	);
});
afterEach(() => {
	vi.useRealTimers();
	vi.restoreAllMocks();
	vi.unstubAllGlobals();
});

async function open(query = "f", terminalId = "search-flicker") {
	let ref: CanvasTerminalRef | undefined;
	const view = render(() => (
		<CanvasTerminal
			sessionId="search-flicker"
			terminalId={terminalId}
			onRef={(value) => {
				ref = value;
			}}
		/>
	));
	await waitFor(() => expect(transport.sink).not.toBeNull());
	await waitFor(() => expect(ref).toBeDefined());
	if (!ref) throw new Error("terminal ref unavailable");
	vi.useFakeTimers();
	transport.sink?.(frame());
	await vi.advanceTimersByTimeAsync(32);
	await ref.searchFind(query);
	const canvas = view.container.querySelectorAll("canvas").item(2);
	const output = canvases.get(canvas);
	if (!output) throw new Error("overlay canvas unavailable");
	const highlights = () => output.fills.filter((fill) => fill.colour === "rgba(255, 180, 50, 0.2)");
	return { view, ref, highlights };
}

function holdRefresh() {
	let release: ((value: typeof matches) => void) | undefined;
	const pending = new Promise<typeof matches>((resolve) => {
		release = resolve;
	});
	transport.invoke.mockImplementation(async (command: string) => (command === "terminal_search" ? pending : undefined));
	return () => release?.(matches);
}

describe("terminal search highlight continuity", () => {
	// Catches: full or colour-only redraws clear all matched F cells while the backend refresh is pending.
	it.each([0, 200])("retains many highlights across repeated unchanged text frames (foreground %i)", async (colour) => {
		const terminal = await open();
		const release = holdRefresh();
		try {
			expect(terminal.highlights()).toHaveLength(16);
			terminal.ref.focus();
			await vi.advanceTimersByTimeAsync(2200);
			expect(terminal.highlights()).toHaveLength(16);
			expect(transport.invoke.mock.calls.filter(([command]) => command === "terminal_search")).toHaveLength(1);
			for (let i = 0; i < 4; i++) {
				transport.sink?.(frame("f F f F ", colour));
				await vi.advanceTimersByTimeAsync(200);
				expect(terminal.highlights()).toHaveLength(16);
			}
		} finally {
			release();
			terminal.view.unmount();
		}
	});

	// Catches: preserving unchanged matches accidentally leaves highlights over replaced, nonmatching text.
	it("clears only the row whose text changed before the refresh completes", async () => {
		const terminal = await open();
		const release = holdRefresh();
		try {
			transport.sink?.(frame("x X x X "));
			await vi.advanceTimersByTimeAsync(32);
			expect(terminal.highlights()).toHaveLength(12);
			expect(terminal.highlights().every((fill) => fill.y > 0)).toBe(true);
		} finally {
			release();
			terminal.view.unmount();
		}
	});

	// Catches: a no-match search or a late response after closing resurrects visible decorations.
	it.each(["absent", "f"])("keeps a cleared search undecorated during output and a pending %s reply", async (query) => {
		const terminal = await open(query);
		const release = holdRefresh();
		try {
			if (query === "absent") expect(terminal.highlights()).toHaveLength(0);
			transport.sink?.(frame());
			await vi.advanceTimersByTimeAsync(32);
			terminal.ref.searchClear();
			release();
			await vi.advanceTimersByTimeAsync(200);
			expect(terminal.highlights()).toHaveLength(0);
			transport.sink?.(frame());
			await vi.advanceTimersByTimeAsync(200);
			expect(terminal.highlights()).toHaveLength(0);
		} finally {
			release();
			terminal.view.unmount();
		}
	});
});

// Catches: recorded cursor-row prompt evidence painting the composer as a submitted prompt.
it("paints no prompt strip for a terminal with recorded userPromptLines", async () => {
	const id = terminalsStore.add(makeTerminal());
	terminalsStore.update(id, { agentType: "claude" });
	terminalsStore.addUserPromptLine(id, 0);
	const terminal = await open("", id);
	try {
		expect(canvases.size).toBeGreaterThan(0);
		const promptStrips = () =>
			Array.from(terminal.view.container.querySelectorAll<HTMLElement>("div")).filter(
				(div) => div.style.cssText.includes("--prompt-tint") || div.style.cssText.includes("--prompt-bar"),
			);
		expect(promptStrips()).toHaveLength(0);
		terminalsStore.addUserPromptLine(id, 2);
		transport.sink?.(frame());
		await vi.advanceTimersByTimeAsync(200);
		expect(promptStrips()).toHaveLength(0);
	} finally {
		terminal.view.unmount();
		terminalsStore.remove(id);
	}
});
