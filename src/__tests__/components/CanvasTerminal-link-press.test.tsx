import { render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { VisibleIntersectionObserver } from "../helpers/visibleIntersectionObserver";

/**
 * A left press on an underlined path while the app reports the mouse (Claude
 * Code fullscreen). The contract is the user's: the underlined name opens when
 * clicked, every other press still belongs to the app. The last case is the same
 * contract without mouse reporting (Claude with the alternate screen disabled).
 */

const { invoke, frameSink, openConfirmedLink } = vi.hoisted(() => ({
	invoke: vi.fn(),
	frameSink: { current: null as ((data: ArrayBuffer) => void) | null },
	openConfirmedLink: vi.fn(),
}));

vi.mock("../../utils/openUrl", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../utils/openUrl")>()),
	openConfirmedLink,
}));

vi.mock("../../components/Terminal/canvasTerminalTransport", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../components/Terminal/canvasTerminalTransport")>()),
	createTransport: () => ({
		onEvent: vi.fn().mockResolvedValue(undefined),
		subscribe: vi.fn(async (onFrame: (data: ArrayBuffer) => void) => {
			frameSink.current = onFrame;
		}),
		resubscribe: vi.fn().mockResolvedValue(undefined),
		unsubscribe: vi.fn(),
		ackFrame: vi.fn(),
		invoke,
	}),
}));
vi.mock("../../components/Terminal/gridRenderer", () => ({
	createGridRenderer: () => ({ setTheme: vi.fn(), invalidateCaches: vi.fn(), paintGrid: vi.fn(), paintRow: vi.fn() }),
}));

import CanvasTerminal from "../../components/Terminal/CanvasTerminal";

const ROWS = 2;
const COLS = 30;
const HEADER_SIZE = 26;
const CELL_SIZE = 11;
const GUTTER = 6;
const CELL_W = 8;
const ROW0 = "  followups.md  plain words";
/** `followups.md` occupies columns 2-13 of row 0. */
const NAME_COL = 5;
const OTHER_COL = 20;
/** Tagged by the app as an OSC 8 hyperlink to `followups.md`; the text itself is no path. Columns 6-8. */
const OSC8_ROW = "  see doc  plain words";
const OSC8_COL = 7;
/** Names a tool call prints before the file it writes exists. */
const LATE_ROW = "  later.txt  plain words";

/** A full screen whose row 0 is `row0`, with mouse reporting (SGR, button-event tracking) on unless `mouseMode` is 0. */
function frame(row0 = ROW0, mouseMode = 2): ArrayBuffer {
	const buffer = new ArrayBuffer(HEADER_SIZE + ROWS * (4 + COLS * CELL_SIZE));
	const view = new DataView(buffer);
	view.setUint16(0, ROWS, true);
	view.setUint8(6, 1);
	view.setUint8(17, mouseMode === 0 ? 0 : (mouseMode << 3) | 0x20);
	view.setUint16(18, ROWS, true);
	view.setUint16(20, COLS, true);
	let offset = HEADER_SIZE;
	for (let r = 0; r < ROWS; r++) {
		view.setUint16(offset, r, true);
		view.setUint16(offset + 2, COLS, true);
		offset += 4;
		for (const char of (r === 0 ? row0 : "").padEnd(COLS, " ")) {
			view.setUint32(offset, char.codePointAt(0) ?? 0, true);
			offset += CELL_SIZE;
		}
	}
	return buffer;
}

const x = (col: number) => GUTTER + col * CELL_W + CELL_W / 2;

function fire(target: Element, type: string, col: number, init: MouseEventInit = {}) {
	target.dispatchEvent(
		new MouseEvent(type, { bubbles: true, cancelable: true, clientX: x(col), clientY: 2, button: 0, ...init }),
	);
}

function click(target: Element, pressCol: number, releaseCol: number, init: MouseEventInit = {}) {
	fire(target, "mousedown", pressCol, { buttons: 1, ...init });
	fire(target, "mouseup", releaseCol, init);
	fire(target, "click", releaseCol, init);
}

const ptyWrites = () =>
	invoke.mock.calls.filter(([cmd]) => cmd === "write_pty").map(([, a]) => (a as { data: string }).data);

describe("CanvasTerminal link press under mouse reporting", () => {
	const onOpen = vi.fn();
	let screenRow0 = ROW0;
	let lateFileExists = false;
	let osc8Active = false;
	let osc8Uri = "followups.md";
	let canvas: Element;
	let unmount: () => void;

	beforeEach(async () => {
		onOpen.mockClear();
		invoke.mockReset();
		screenRow0 = ROW0;
		lateFileExists = false;
		osc8Active = false;
		osc8Uri = "followups.md";
		openConfirmedLink.mockClear();
		invoke.mockImplementation(
			async (cmd: string, args: { candidate?: string; candidates?: string[]; col?: number }) => {
				if (cmd === "terminal_hyperlink_span") {
					const col = args.col ?? -1;
					return osc8Active && col >= 6 && col < 9 ? [6, 9, osc8Uri] : null;
				}
				if (cmd === "terminal_get_row_text") return screenRow0.trimEnd();
				if (cmd === "terminal_get_logical_line") return [0, screenRow0.trimEnd()];
				// Open revalidates the absolute path returned by the initial lookup.
				// The backend accepts both forms; rejecting the second hides valid clicks.
				const resolve = (candidate: string) => {
					const name = candidate.startsWith("/cwd/") ? candidate.slice(5) : candidate;
					return name === "followups.md" || (name === "later.txt" && lateFileExists)
						? { absolute_path: `/cwd/${name}`, is_directory: false }
						: null;
				};
				if (cmd === "resolve_terminal_path") return resolve(args.candidate ?? "");
				if (cmd === "resolve_terminal_paths") return (args.candidates ?? []).map(resolve);
				return null;
			},
		);
		frameSink.current = null;
		Object.defineProperty(document, "fonts", {
			configurable: true,
			value: { load: () => Promise.resolve([]), ready: Promise.resolve() },
		});
		vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(
			new Proxy(
				{},
				{
					get: (_t, prop) =>
						prop === "measureText"
							? () => ({ width: CELL_W, fontBoundingBoxAscent: 10, fontBoundingBoxDescent: 3 })
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

		const view = render(() => <CanvasTerminal sessionId="link-1324" terminalId="link-1324" onOpenFilePath={onOpen} />);
		unmount = view.unmount;
		await waitFor(() => expect(frameSink.current).not.toBeNull());
		// The reset above narrows `current` to null for the compiler; the subscribe mock refills it.
		(frameSink.current as ((data: ArrayBuffer) => void) | null)?.(frame());
		const found = view.container.querySelector('canvas[tabindex="0"]');
		if (!found) throw new Error("terminal canvas not mounted");
		canvas = found;
		// The underline exists once verification has resolved the name.
		await waitFor(() => expect(invoke.mock.calls.map(([c]) => c)).toContain("resolve_terminal_paths"));
		await new Promise((r) => setTimeout(r, 20));
		invoke.mockClear();
	});

	afterEach(() => {
		unmount();
		vi.restoreAllMocks();
		vi.unstubAllGlobals();
	});

	// Catches: the underlined name being dead under mouse reporting (press forwarded, no hover probe ran).
	it("opens an underlined name on a plain click and keeps the press from the app", async () => {
		click(canvas, NAME_COL, NAME_COL);
		await waitFor(() => expect(onOpen).toHaveBeenCalledWith("/cwd/followups.md", undefined, undefined));
		expect(ptyWrites()).toEqual([]);
	});

	// Catches: a drag-select in the app ending on a path opening the file from a leftover hover.
	it("does not open a path where a forwarded press ended", async () => {
		click(canvas, OTHER_COL, NAME_COL);
		await new Promise((r) => setTimeout(r, 200));
		expect(onOpen).not.toHaveBeenCalled();
		expect(ptyWrites()).toContainEqual(expect.stringMatching(/^\x1b\[<0;\d+;\d+M$/));
	});

	// Catches: a claimed press released away from its span still opening it.
	it("does not open when the claimed press is released off the span", async () => {
		click(canvas, NAME_COL, OTHER_COL);
		await new Promise((r) => setTimeout(r, 200));
		expect(onOpen).not.toHaveBeenCalled();
	});

	// Catches: a double-click on a path opening the file twice (one open per click event).
	it("opens once for a double-click", async () => {
		click(canvas, NAME_COL, NAME_COL, { detail: 1 });
		click(canvas, NAME_COL, NAME_COL, { detail: 2 });
		await waitFor(() => expect(onOpen).toHaveBeenCalledTimes(1));
		await new Promise((r) => setTimeout(r, 200));
		expect(onOpen).toHaveBeenCalledTimes(1);
		expect(ptyWrites()).toEqual([]);
	});

	// Catches: the app receiving a drag (button 32+) for a press the link swallowed.
	it("reports no drag motion for a claimed press", () => {
		fire(canvas, "mousedown", NAME_COL, { buttons: 1 });
		fire(document.body, "mousemove", OTHER_COL, { buttons: 1 });
		expect(ptyWrites().filter((d) => /^\x1b\[<3[2-9];/.test(d))).toEqual([]);
	});

	// Catches: the drag guard also muting motion of a press the app owns.
	it("still reports drag motion for a press the app owns", () => {
		fire(canvas, "mousedown", OTHER_COL, { buttons: 1 });
		fire(document.body, "mousemove", OTHER_COL + 1, { buttons: 1 });
		expect(ptyWrites().some((d) => /^\x1b\[<32;/.test(d))).toBe(true);
	});

	// Catches: Shift losing its documented bypass of mouse reporting over a link.
	it("opens with Shift held and sends nothing to the app", async () => {
		click(canvas, NAME_COL, NAME_COL, { shiftKey: true });
		await waitFor(() => expect(onOpen).toHaveBeenCalledWith("/cwd/followups.md", undefined, undefined));
		expect(ptyWrites()).toEqual([]);
	});

	// Catches: a hover probe made before the file exists caching "no link" for that row text forever,
	// so the name underlined by the 3 s re-verification does nothing on click (#1330).
	it("opens a name whose file appeared after the pointer first hovered it, without mouse reporting", async () => {
		vi.useFakeTimers({ toFake: ["Date"] });
		try {
			screenRow0 = LATE_ROW;
			const send = frameSink.current as ((data: ArrayBuffer) => void) | null;
			send?.(frame(LATE_ROW, 0));
			await waitFor(() => expect(invoke.mock.calls.map(([c]) => c)).toContain("resolve_terminal_paths"));
			// Hover while the file is missing: the probe resolves nothing.
			fire(document.body, "mousemove", NAME_COL);
			await waitFor(() => expect(invoke.mock.calls.map(([c]) => c)).toContain("resolve_terminal_path"));
			lateFileExists = true;
			vi.setSystemTime(Date.now() + 4_000);
			// A new frame schedules the re-verification that underlines the name.
			send?.(frame(LATE_ROW, 0));
			await waitFor(() => expect(invoke.mock.calls.filter(([c]) => c === "resolve_terminal_paths")).toHaveLength(2));
			await new Promise((r) => setTimeout(r, 50));
			click(canvas, NAME_COL, NAME_COL);
			await waitFor(() => expect(onOpen).toHaveBeenCalledWith("/cwd/later.txt", undefined, undefined));
		} finally {
			vi.useRealTimers();
		}
	});

	// Catches: 1324 requiring an underlined span at the press, which killed links the hover probe
	// resolves but the dashed underline never covers (OSC 8 over non-path text).
	it.each([
		["without mouse reporting", 0],
		["under mouse reporting", 2],
	])("opens an OSC 8 link over non-path text hovered before the click, %s", async (_name, mouseMode) => {
		osc8Active = true;
		screenRow0 = OSC8_ROW;
		(frameSink.current as ((data: ArrayBuffer) => void) | null)?.(frame(OSC8_ROW, mouseMode));
		await new Promise((r) => setTimeout(r, 20));
		fire(document.body, "mousemove", OSC8_COL);
		await waitFor(() => expect(canvas.getAttribute("style") ?? "").toContain("pointer"));
		click(canvas, OSC8_COL, OSC8_COL);
		await waitFor(() => expect(onOpen).toHaveBeenCalledWith("/cwd/followups.md", undefined, undefined));
		expect(ptyWrites()).toEqual([]);
	});

	// Catches: an OSC 8 editor-protocol link from untrusted output opening without a click,
	// or a click bypassing the owner-confirmed handler route.
	it("sends a clicked OSC 8 custom-scheme link to the confirmed handler route only", async () => {
		osc8Active = true;
		osc8Uri = "editor://open?file=/cwd/a.rs";
		screenRow0 = OSC8_ROW;
		(frameSink.current as ((data: ArrayBuffer) => void) | null)?.(frame(OSC8_ROW, 0));
		fire(document.body, "mousemove", OSC8_COL);
		await waitFor(() => expect(canvas.getAttribute("style") ?? "").toContain("pointer"));
		expect(openConfirmedLink).not.toHaveBeenCalled();
		click(canvas, OSC8_COL, OSC8_COL);
		await waitFor(() => expect(openConfirmedLink).toHaveBeenCalledWith("editor://open?file=/cwd/a.rs"));
		expect(openConfirmedLink).toHaveBeenCalledTimes(1);
		expect(onOpen).not.toHaveBeenCalled();
	});

	// Catches: the hover fallback claiming a press over plain text that was never a link.
	it("does not open or swallow a press on plain text next to an OSC 8 link", async () => {
		osc8Active = true;
		screenRow0 = OSC8_ROW;
		(frameSink.current as ((data: ArrayBuffer) => void) | null)?.(frame(OSC8_ROW, 2));
		await new Promise((r) => setTimeout(r, 20));
		fire(document.body, "mousemove", OSC8_COL);
		await waitFor(() => expect(canvas.getAttribute("style") ?? "").toContain("pointer"));
		click(canvas, OTHER_COL, OTHER_COL);
		await new Promise((r) => setTimeout(r, 100));
		expect(onOpen).not.toHaveBeenCalled();
		expect(ptyWrites().length).toBeGreaterThan(0);
	});
});
