// @vitest-environment jsdom

import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { SubscribePtyOptions } from "../../transport";
import { OutputView } from "../components/OutputView";

const { subscribePty } = vi.hoisted(() => ({ subscribePty: vi.fn() }));
vi.mock("../../transport", () => ({ subscribePty }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { warn: vi.fn(), error: vi.fn() } }));

let frames: FrameRequestCallback[];
let stream: SubscribePtyOptions;
let resizeCallbacks: Map<Element, ResizeObserverCallback>;

beforeEach(() => {
	frames = [];
	resizeCallbacks = new Map();
	vi.stubGlobal(
		"ResizeObserver",
		class {
			constructor(private callback: ResizeObserverCallback) {}
			observe(target: Element) {
				resizeCallbacks.set(target, this.callback);
			}
			disconnect() {
				resizeCallbacks.clear();
			}
		},
	);
	subscribePty.mockClear();
	vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
		frames.push(callback);
		return frames.length;
	});
	vi.stubGlobal(
		"fetch",
		vi.fn(async () => ({
			ok: true,
			json: async () => ({ lines: ["initial"], total_lines: 1, offset: 0 }),
		})),
	);
	subscribePty.mockImplementation(async (_id, _output, _exit, options) => {
		stream = options;
		return () => {};
	});
});

afterEach(() => {
	cleanup();
	vi.unstubAllGlobals();
});

function paint() {
	const pending = frames.splice(0);
	for (const callback of pending) callback(0);
}

async function mountOutput() {
	const view = render(() => <OutputView sessionId="scroll-session" />);
	const viewport = view.container.firstElementChild as HTMLDivElement;
	let height = 2000;
	let clientHeight = 400;
	let top = 0;
	Object.defineProperties(viewport, {
		clientHeight: { get: () => clientHeight },
		scrollHeight: { get: () => height },
		scrollTop: {
			get: () => top,
			set: (value: number) => {
				top = Math.max(0, Math.min(value, height - clientHeight));
			},
		},
	});
	await waitFor(() => expect(subscribePty).toHaveBeenCalled());
	paint();
	fireEvent.scroll(viewport);
	return {
		viewport,
		resize(nextHeight: number, contentHeight = height) {
			clientHeight = nextHeight;
			height = contentHeight;
			viewport.scrollTop = Math.min(top, height - clientHeight); // browser clamps on expansion
			resizeCallbacks.get(viewport)?.([], {} as ResizeObserver);
		},
		chunk() {
			height += 20;
			stream.onLogLines?.([{ spans: [{ text: "next line" }] }]);
			stream.onScreenRows?.(["live output"]);
		},
	};
}

function dragUp(viewport: HTMLDivElement, pixels: number) {
	fireEvent.touchStart(viewport, { touches: [{ clientY: 200 }] });
	viewport.scrollTop -= pixels;
	fireEvent.touchMove(viewport, { touches: [{ clientY: 200 - pixels }] });
	fireEvent.scroll(viewport);
	fireEvent.touchEnd(viewport, { touches: [] });
}

describe("mobile output file links", () => {
	const path = "stories/archive/2011-86b5-complete-P2-mobile-cli-file-links-wrap-centered-with-surrounding-text.md";

	it("keeps wrapped file links in inline text instead of native centered atomic buttons", async () => {
		vi.mocked(fetch).mockResolvedValue({
			ok: true,
			json: async () => ({ lines: [`  Read ${path} next`], total_lines: 1, offset: 0 }),
		} as Response);
		const view = render(() => <OutputView sessionId="links" onOpenFileLink={vi.fn()} />);
		const link = await view.findByRole("button", { name: path });
		// Native buttons remain atomic inline-blocks even with display:inline in Chrome.
		expect(link.tagName).not.toBe("BUTTON");
		expect(link.tabIndex).toBe(0);
		expect(link.parentElement?.style.getPropertyValue("--wrap-indent")).toBe("2ch");
		expect(link.parentElement?.textContent).toBe(`  Read ${path} next`);
	});

	it("keeps inline file actions tappable and keyboard accessible without scrolling on Space", async () => {
		vi.mocked(fetch).mockResolvedValue({
			ok: true,
			json: async () => ({ lines: [path], total_lines: 1, offset: 0 }),
		} as Response);
		const open = vi.fn();
		const view = render(() => <OutputView sessionId="links" onOpenFileLink={open} />);
		const link = await view.findByRole("button", { name: path });
		fireEvent.click(link);
		fireEvent.keyDown(link, { key: "Enter" });
		expect(fireEvent.keyDown(link, { key: " " })).toBe(false);
		fireEvent.keyDown(link, { key: "Escape" });
		expect(open.mock.calls).toEqual([
			[path, undefined],
			[path, undefined],
			[path, undefined],
		]);
	});
});

describe("mobile output follow", () => {
	it("keeps newest output visible after viewport resize while following", async () => {
		const { viewport, resize } = await mountOutput();
		paint(); // Drain the initial font-effect frame before testing resize alone.
		// Catches: keyboard shrink leaves the newest 200px below the clip.
		resize(200);
		paint();
		expect(viewport.scrollTop).toBe(1800);
		resize(400);
		fireEvent.scroll(viewport);
		paint();
		expect(viewport.scrollTop).toBe(1600);
		// Catches: rotation rewrap grows content without a new output frame.
		resize(400, 2400);
		paint();
		expect(viewport.scrollTop).toBe(2000);
	});

	it("does not pull a detached reader to bottom on viewport resize", async () => {
		const { viewport, resize } = await mountOutput();
		dragUp(viewport, 240);
		resize(200);
		paint();
		expect(viewport.scrollTop).toBe(1360);
		resize(400);
		paint();
		expect(viewport.scrollTop).toBe(1360);
	});

	it("does not let queued resize follow overwrite a later upward gesture", async () => {
		const { viewport, resize } = await mountOutput();
		resize(200);
		dragUp(viewport, 240);
		paint();
		expect(viewport.scrollTop).toBe(1360);
	});

	it("keeps the line being read when streaming crosses the tail cap instead of evicting detached content", async () => {
		const { viewport, append, visibleLine } = await mountCappedOutput();
		dragUp(viewport, 240);
		const reading = visibleLine();
		append(600, 650);
		paint();
		expect(visibleLine()).toBe(reading);
	});

	it("reveals older history at the cap instead of discarding the fetched page and resetting scroll", async () => {
		const { viewport, visibleLine, append } = await mountCappedOutput();
		viewport.scrollTop = 100;
		const reading = visibleLine();
		fireEvent.scroll(viewport);
		await waitFor(() => expect(viewport.textContent).toContain("history 0"));
		append(600, 610); // live output between prepend and paint must not shift the anchor
		paint();
		expect(visibleLine()).toBe(reading);
		expect(viewport.scrollTop).toBe(2100);
		viewport.scrollTop += 200;
		fireEvent.scroll(viewport);
		paint();
		expect(viewport.scrollTop).toBe(2300);
	});

	it("restores the bounded live tail after a detached reader returns to bottom", async () => {
		const { viewport, append, rows } = await mountCappedOutput();
		dragUp(viewport, 240);
		append(600, 650);
		paint();
		viewport.scrollTop = viewport.scrollHeight;
		fireEvent.scroll(viewport);
		append(650, 651);
		paint();
		expect(rows()).toHaveLength(500);
		expect(rows()[0].textContent).toBe("history 151");
		expect(viewport.scrollTop).toBe(viewport.scrollHeight - viewport.clientHeight);
	});

	it("does not evict the reading window while a stationary touch guards output at the cap", async () => {
		const { viewport, append, visibleLine } = await mountCappedOutput();
		fireEvent.touchStart(viewport);
		const reading = visibleLine();
		append(600, 650);
		paint();
		expect(visibleLine()).toBe(reading);
		fireEvent.touchCancel(viewport);
		append(650, 651);
		paint();
		expect(viewport.scrollTop).toBe(viewport.scrollHeight - viewport.clientHeight);
	});

	it("keeps a small upward touch drag detached across output chunks instead of treating it as bottom", async () => {
		const { viewport, chunk } = await mountOutput();
		dragUp(viewport, 40);
		const detachedTop = viewport.scrollTop;
		for (let i = 0; i < 3; i++) {
			chunk();
			paint();
			expect(viewport.scrollTop).toBe(detachedTop);
		}
	});

	it("does not let an output frame queued before the upward drag overwrite the gesture", async () => {
		const { viewport, chunk } = await mountOutput();
		chunk();
		dragUp(viewport, 240);
		const detachedTop = viewport.scrollTop;
		paint();
		expect(viewport.scrollTop).toBe(detachedTop);
	});

	it("resumes following only after returning to the bottom, including fractional scroll positions", async () => {
		const { viewport, chunk } = await mountOutput();
		dragUp(viewport, 40);
		viewport.scrollTop = viewport.scrollHeight - viewport.clientHeight - 0.5;
		fireEvent.scroll(viewport);
		chunk();
		paint();
		expect(viewport.scrollTop).toBe(viewport.scrollHeight - viewport.clientHeight);
	});

	it("keeps a stationary touch from scrolling output and releases the guard on touch cancel", async () => {
		const { viewport, chunk } = await mountOutput();
		const before = viewport.scrollTop;
		fireEvent.touchStart(viewport);
		chunk();
		paint();
		expect(viewport.scrollTop).toBe(before);
		fireEvent.touchCancel(viewport);
		chunk();
		paint();
		expect(viewport.scrollTop).toBe(viewport.scrollHeight - viewport.clientHeight);
	});
	it("keeps following when output grows between its own scroll-to-bottom and the scroll event it caused", async () => {
		const { viewport, chunk } = await mountOutput();
		chunk();
		paint(); // follow scroll: scrollTop = bottom, scroll event not yet delivered
		chunk(); // next chunk lands before the browser dispatches that scroll event
		fireEvent.scroll(viewport); // the follow scroll's own event, now measured against grown content
		chunk();
		paint();
		expect(viewport.scrollTop).toBe(viewport.scrollHeight - viewport.clientHeight);
	});
});

/** Fixed-height DOM rows model layout; assertions read the actual rendered content. */
async function mountCappedOutput() {
	vi.mocked(fetch).mockImplementation(async (input) => {
		const url = new URL(String(input), "http://localhost");
		const start = Number(url.searchParams.get("offset") ?? 100);
		const lines = Array.from({ length: 100 }, (_, i) => `history ${start + i}`);
		return { ok: true, json: async () => ({ lines, total_lines: 200, offset: start }) } as Response;
	});
	const view = render(() => <OutputView sessionId="scroll-session" />);
	const viewport = view.container.firstElementChild as HTMLDivElement;
	const rows = () => Array.from(viewport.querySelectorAll<HTMLDivElement>("pre > div"));
	let top = 0;
	Object.defineProperties(viewport, {
		clientHeight: { get: () => 400 },
		scrollHeight: { get: () => rows().length * 20 },
		scrollTop: {
			get: () => top,
			set: (value: number) => {
				top = Math.max(0, Math.min(value, viewport.scrollHeight - 400));
			},
		},
	});
	await waitFor(() => expect(subscribePty).toHaveBeenCalled());
	const append = (start: number, end: number) => {
		stream.onLogLines?.(Array.from({ length: end - start }, (_, i) => ({ spans: [{ text: `history ${start + i}` }] })));
	};
	append(200, 600);
	paint();
	fireEvent.scroll(viewport);
	return { viewport, rows, append, visibleLine: () => rows()[Math.floor(viewport.scrollTop / 20)]?.textContent };
}

// Catches: display:none leaves CLI detached or remounting loses the existing PTY stream.
it("returns the mounted output to bottom after hidden Chat output", async () => {
	const [visible, setVisible] = createSignal(true);
	const view = render(() => <OutputView sessionId="hidden-output" visible={visible()} />);
	const viewport = view.container.firstElementChild as HTMLDivElement;
	let top = 0;
	Object.defineProperties(viewport, {
		clientHeight: { value: 400 },
		scrollHeight: { value: 2000 },
		scrollTop: {
			get: () => top,
			set: (value: number) => {
				top = Math.min(value, 1600);
			},
		},
	});
	await waitFor(() => expect(subscribePty).toHaveBeenCalled());
	paint();
	fireEvent.scroll(viewport);
	dragUp(viewport, 240);
	setVisible(false);
	stream.onScreenRows?.(["output during Chat"]);
	paint();
	expect(viewport.style.display).toBe("none");
	setVisible(true);
	paint();
	expect(view.container.firstElementChild).toBe(viewport);
	expect(subscribePty).toHaveBeenCalledTimes(1);
	expect(viewport.scrollTop).toBe(1600);
	expect(viewport.textContent).toContain("output during Chat");
});

// Catches: changing font metrics while reading history loses the visible row.
it("keeps the detached reading row anchored when the font size changes", async () => {
	const { mobileFontSizes } = await import("../mobileFontSize");
	while (mobileFontSizes.terminal() > 14) mobileFontSizes.step("terminal", -1);
	const { viewport } = await mountOutput();
	dragUp(viewport, 600);
	const row = viewport.querySelector("pre > div") as HTMLElement;
	const text = viewport.querySelector("pre") as HTMLPreElement;
	Object.defineProperty(viewport, "getBoundingClientRect", { value: () => ({ top: 0 }) });
	// Model a row whose layout moves 100px as the font grows; the scroll owner
	// must compensate that delta rather than keep the stale pixel scrollTop.
	row.getBoundingClientRect = () =>
		({
			top: 1200 + (Number.parseInt(text.style.fontSize, 10) - 14) * 100 - viewport.scrollTop,
			bottom: 1250 + (Number.parseInt(text.style.fontSize, 10) - 14) * 100 - viewport.scrollTop,
		}) as DOMRect;
	const before = row.getBoundingClientRect().top;
	mobileFontSizes.step("terminal", 1);
	paint();
	expect(text.style.fontSize).toBe("15px");
	expect(row.getBoundingClientRect().top).toBe(before);
	mobileFontSizes.step("terminal", -1);
	paint();
});
