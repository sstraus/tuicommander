import { render, waitFor } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";

const { createSession, resize } = vi.hoisted(() => ({
	createSession: vi.fn().mockResolvedValue("fresh-session"),
	resize: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("../../hooks/usePty", () => ({
	usePty: () => ({ createSession, resize, close: vi.fn(), getKittyFlags: vi.fn().mockResolvedValue(0) }),
}));
vi.mock("../../stores/agentConfigs", () => ({
	ensureAgentConfigsForRepo: vi.fn().mockResolvedValue({ getEnvFlags: () => ({}) }),
	agentConfigsForRepo: () => ({ getEnvFlags: () => ({}) }),
}));
vi.mock("../../components/Terminal/glyphCache", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../components/Terminal/glyphCache")>()),
	getSharedMetrics: () => ({ cellWidth: 8, cellHeight: 16 }),
}));
vi.mock("../../components/Terminal/CanvasTerminal", () => ({
	default: () => <div data-terminal-container />,
}));

import { gridDimsForBox } from "../../components/Terminal/canvasTerminalUtils";
import { Terminal } from "../../components/Terminal/Terminal";
import { terminalsStore } from "../../stores/terminals";

type Box = { width: number; height: number };

function rect(box: Box): DOMRect {
	return { ...box, x: 0, y: 0, top: 0, left: 0, right: box.width, bottom: box.height, toJSON: () => box };
}

describe("terminal grid box attacks (2128-153a)", () => {
	const restore: Array<() => void> = [];
	afterEach(() => {
		for (const undo of restore.splice(0)) undo();
		resize.mockClear();
		createSession.mockClear();
	});

	/** Mount a Terminal whose pane is `pane`, styled with `padding`; frames are queued, not run. */
	function mount(opts: { sessionId: string | null; pane: Box; padding: string; canvas?: Box }) {
		const frames: Array<() => void> = [];
		let sized = opts.pane.width;
		const style = document.createElement("style");
		style.textContent = `.content { padding: ${opts.padding}; }`;
		document.head.append(style);
		const original = HTMLElement.prototype.getBoundingClientRect;
		HTMLElement.prototype.getBoundingClientRect = function (this: HTMLElement) {
			if (this.hasAttribute("data-terminal-container") && opts.canvas) return rect(opts.canvas);
			if (this.classList.contains("content")) return rect(opts.pane);
			return original.call(this);
		};
		const width = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "offsetWidth");
		const height = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "offsetHeight");
		Object.defineProperty(HTMLElement.prototype, "offsetWidth", { configurable: true, get: () => sized });
		Object.defineProperty(HTMLElement.prototype, "offsetHeight", { configurable: true, get: () => opts.pane.height });
		const oldRaf = globalThis.requestAnimationFrame;
		globalThis.requestAnimationFrame = (cb) => {
			frames.push(() => cb(0));
			return frames.length;
		};
		restore.push(() => {
			style.remove();
			HTMLElement.prototype.getBoundingClientRect = original;
			if (width) Object.defineProperty(HTMLElement.prototype, "offsetWidth", width);
			if (height) Object.defineProperty(HTMLElement.prototype, "offsetHeight", height);
			globalThis.requestAnimationFrame = oldRaf;
		});
		const id = terminalsStore.add({
			sessionId: opts.sessionId,
			cwd: "/tmp/r",
			name: "shell",
			fontSize: 13,
			awaitingInput: null,
		});
		const view = render(() => <Terminal id={id} cwd="/tmp/r" alwaysVisible />);
		restore.push(view.unmount);
		return {
			runFrames: () => {
				for (let i = 0; i < 50 && frames.length; i++) frames.shift()?.();
			},
			layOut: () => {
				sized = opts.pane.width;
			},
			collapse: () => {
				sized = 0;
			},
		};
	}

	// Catches: the fallback (canvas not mounted yet, fresh spawn) subtracts only one side
	// of the padding, so an asymmetric pane spawns a PTY a row or column off the canvas.
	it("fresh spawn subtracts all four padding sides from the pane", async () => {
		const h = mount({ sessionId: null, pane: { width: 330, height: 314 }, padding: "3px 5px 11px 7px" });
		h.runFrames();
		await waitFor(() => expect(createSession).toHaveBeenCalled());
		const want = gridDimsForBox(330 - 12, 314 - 14, 8, 16);
		expect(createSession).toHaveBeenCalledWith(expect.objectContaining({ rows: want.rows, cols: want.cols }));
	});

	// Catches: the reconnect measures the pane content box instead of the canvas's own
	// container. Anything else that takes room inside the pane (a banner, a scrollbar
	// strip, a CSS change) makes the PTY larger than the canvas draws and reopens 2128-153a.
	it("reconnect sizes the PTY from the canvas box even when it is smaller than the pane content box", async () => {
		const h = mount({
			sessionId: "live-session",
			pane: { width: 330, height: 314 },
			padding: "8px 0 8px 8px",
			canvas: { width: 322, height: 250 },
		});
		h.runFrames();
		await waitFor(() => expect(resize).toHaveBeenCalled());
		const want = gridDimsForBox(322, 250, 8, 16);
		expect(resize).toHaveBeenCalledWith("live-session", want.rows, want.cols);
	});

	// Catches: a pane smaller than its own padding yields a negative box and the
	// backend rejects the resize; the reconnect path then abandons a live session
	// and spawns a fresh one.
	it("a pane smaller than its padding still asks for a valid size", async () => {
		const h = mount({ sessionId: null, pane: { width: 6, height: 6 }, padding: "8px 0 8px 8px" });
		h.runFrames();
		await waitFor(() => expect(createSession).toHaveBeenCalled());
		const arg = createSession.mock.calls[0][0] as { rows: number; cols: number };
		expect(Number.isInteger(arg.rows) && arg.rows >= 2).toBe(true);
		expect(Number.isInteger(arg.cols) && arg.cols >= 2).toBe(true);
	});

	// Catches: a background tab that is not laid out yet is measured through its
	// (zero-size) canvas box and its live PTY is shrunk to the 2x2 clamp.
	it("a collapsed tab does not resize its live PTY before it is laid out", async () => {
		const h = mount({
			sessionId: "live-session",
			pane: { width: 330, height: 314 },
			padding: "8px 0 8px 8px",
			canvas: { width: 322, height: 298 },
		});
		h.collapse();
		h.runFrames();
		expect(resize).not.toHaveBeenCalled();
		expect(createSession).not.toHaveBeenCalled();
		h.layOut();
		h.runFrames();
		await waitFor(() => expect(resize).toHaveBeenCalled());
		const want = gridDimsForBox(322, 298, 8, 16);
		expect(resize).toHaveBeenCalledWith("live-session", want.rows, want.cols);
	});
});
