// @vitest-environment jsdom

import { cleanup, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { mockInvoke } = vi.hoisted(() => ({ mockInvoke: vi.fn() }));

vi.mock("../../invoke", () => ({ invoke: mockInvoke }));
vi.mock("@tauri-apps/api/event", () => ({
	emitTo: vi.fn().mockResolvedValue(undefined),
	listen: vi.fn().mockResolvedValue(vi.fn()),
	emit: vi.fn().mockResolvedValue(undefined),
}));

import { MarkdownTab } from "../../components/MarkdownTab/MarkdownTab";
import { type FileTab, mdTabsStore } from "../../stores/mdTabs";

/** Manual animation-frame queue so the tests decide exactly when a frame fires. */
let frames: Map<number, FrameRequestCallback>;
let nextId: number;
let cancelSpy: ReturnType<typeof vi.spyOn>;
let focusSpy: ReturnType<typeof vi.spyOn>;

function flushFrames() {
	const pending = [...frames.entries()];
	frames.clear();
	for (const [, cb] of pending) cb(0);
}

describe("MarkdownTab focus-frame lifecycle", () => {
	beforeEach(() => {
		frames = new Map();
		nextId = 1;
		mockInvoke.mockReset();
		mockInvoke.mockResolvedValue("# Doc");
		vi.spyOn(window, "requestAnimationFrame").mockImplementation((cb: FrameRequestCallback) => {
			const id = nextId++;
			frames.set(id, cb);
			return id;
		});
		cancelSpy = vi.spyOn(window, "cancelAnimationFrame").mockImplementation((id: number) => {
			frames.delete(id);
		});
		focusSpy = vi.spyOn(HTMLElement.prototype, "focus").mockImplementation(() => {});
		mdTabsStore.clearAll();
	});

	afterEach(() => {
		cleanup();
		vi.restoreAllMocks();
		mdTabsStore.clearAll();
	});

	const addTab = (name: string) => mdTabsStore.addMcpFile(name, "/repo", `/docs/${name}.md`, false, false);

	async function mountActive() {
		const id = addTab("a");
		const view = render(() => <MarkdownTab tab={mdTabsStore.get(id) as FileTab} />);
		await waitFor(() => expect(view.container.querySelector("[tabindex]")).not.toBeNull());
		return { id, ...view };
	}

	it("focuses the wrapper with preventScroll when the frame fires", async () => {
		await mountActive();
		focusSpy.mockClear();
		flushFrames();
		expect(focusSpy).toHaveBeenCalledWith({ preventScroll: true });
	});

	it("cancels the pending focus frame on unmount so nothing focuses a detached wrapper", async () => {
		const { unmount } = await mountActive();
		focusSpy.mockClear();
		cancelSpy.mockClear();
		const pending = [...frames.keys()];
		expect(pending.length).toBeGreaterThan(0);

		unmount();

		expect(cancelSpy.mock.calls.map((c: unknown[]) => c[0])).toEqual(expect.arrayContaining(pending));
		flushFrames();
		expect(focusSpy).not.toHaveBeenCalled();
	});

	it("leaves a single pending focus frame after rapid re-activation", async () => {
		const { id } = await mountActive();
		flushFrames();
		focusSpy.mockClear();

		// Flip the active tab back and forth without letting a frame run in between.
		const other = addTab("b");
		mdTabsStore.setActive(id);
		mdTabsStore.setActive(other);
		mdTabsStore.setActive(id);

		flushFrames();
		expect(focusSpy).toHaveBeenCalledTimes(1);
		expect(focusSpy).toHaveBeenCalledWith({ preventScroll: true });
	});
});
