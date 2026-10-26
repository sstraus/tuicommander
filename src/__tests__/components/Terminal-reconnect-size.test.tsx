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
// The real CanvasTerminal sizes the PTY from its own container (data-terminal-container).
vi.mock("../../components/Terminal/CanvasTerminal", () => ({
	default: () => <div data-terminal-container />,
}));

import { gridDimsForBox } from "../../components/Terminal/canvasTerminalUtils";
import { Terminal } from "../../components/Terminal/Terminal";
import { terminalsStore } from "../../stores/terminals";

// Pane geometry recorded live on agent-test (story 2128-153a): the pane border box
// is 330x314 and its padding (8px 0 8px 8px) leaves the canvas container 322x298.
const PANE = { width: 330, height: 314 };
const CANVAS = { width: 322, height: 298 };

function rect(box: { width: number; height: number }): DOMRect {
	return { ...box, x: 0, y: 0, top: 0, left: 0, right: box.width, bottom: box.height, toJSON: () => box };
}

describe("terminal PTY size on mount", () => {
	const restore: Array<() => void> = [];
	afterEach(() => {
		for (const undo of restore.splice(0)) undo();
		resize.mockClear();
		createSession.mockClear();
	});

	function mount(sessionId: string | null) {
		const style = document.createElement("style");
		style.textContent = ".content { padding: 8px 0 8px 8px; }";
		document.head.append(style);
		const original = HTMLElement.prototype.getBoundingClientRect;
		HTMLElement.prototype.getBoundingClientRect = function (this: HTMLElement) {
			if (this.hasAttribute("data-terminal-container")) return rect(CANVAS);
			if (this.classList.contains("content")) return rect(PANE);
			return original.call(this);
		};
		const width = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "offsetWidth");
		const height = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "offsetHeight");
		Object.defineProperty(HTMLElement.prototype, "offsetWidth", { configurable: true, get: () => PANE.width });
		Object.defineProperty(HTMLElement.prototype, "offsetHeight", { configurable: true, get: () => PANE.height });
		const oldRaf = globalThis.requestAnimationFrame;
		globalThis.requestAnimationFrame = (cb) => {
			cb(0);
			return 1;
		};
		restore.push(() => {
			style.remove();
			HTMLElement.prototype.getBoundingClientRect = original;
			if (width) Object.defineProperty(HTMLElement.prototype, "offsetWidth", width);
			if (height) Object.defineProperty(HTMLElement.prototype, "offsetHeight", height);
			globalThis.requestAnimationFrame = oldRaf;
		});
		const id = terminalsStore.add({ sessionId, cwd: "/tmp/r", name: "shell", fontSize: 13, awaitingInput: null });
		const view = render(() => <Terminal id={id} cwd="/tmp/r" alwaysVisible />);
		restore.push(view.unmount);
	}

	// Catches: the reconnect resize measures the padded pane while the canvas measures
	// its own box, so the PTY gets one more row and column than the canvas draws; when
	// the reconnect call lands last that size sticks (reproduced live on a WebView reload).
	it("reconnects an existing session at the size the canvas draws", async () => {
		mount("existing-session");
		await waitFor(() => expect(resize).toHaveBeenCalled());
		const canvas = gridDimsForBox(CANVAS.width, CANVAS.height, 8, 16);
		expect(resize).toHaveBeenCalledWith("existing-session", canvas.rows, canvas.cols);
	});

	// Catches: a fresh PTY spawns at the padded pane size, so the agent starts at a size
	// no viewer shows and takes a second SIGWINCH as soon as the canvas measures.
	it("spawns a fresh session at the size the canvas will draw", async () => {
		mount(null);
		await waitFor(() => expect(createSession).toHaveBeenCalled());
		const canvas = gridDimsForBox(CANVAS.width, CANVAS.height, 8, 16);
		expect(createSession).toHaveBeenCalledWith(expect.objectContaining({ rows: canvas.rows, cols: canvas.cols }));
	});
});
