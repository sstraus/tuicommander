import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ErrorLogPanel } from "../../components/ErrorLogPanel/ErrorLogPanel";
import { appLogger } from "../../stores/appLogger";
import { errorLogStore } from "../../stores/errorLog";

const { writeClipboard } = vi.hoisted(() => ({ writeClipboard: vi.fn().mockResolvedValue(undefined) }));
vi.mock("../../utils/clipboard", () => ({ writeClipboard }));

/**
 * The panel is mounted for the whole app lifetime — App renders it
 * unconditionally and the `Show` inside gates only the JSX. Solid memos are
 * eager: they re-run whenever a dependency changes, whether or not anything
 * reads them. So a panel nobody has opened was still re-filtering the entire
 * log ring on every single log line, forever.
 *
 * The observable is `appLogger.getEntries` — the memos' one expensive call.
 */
describe("ErrorLogPanel memo gating", () => {
	beforeEach(() => {
		errorLogStore.close();
		appLogger.clear();
		vi.restoreAllMocks();
		vi.spyOn(console, "error").mockImplementation(() => {});
		vi.spyOn(console, "warn").mockImplementation(() => {});
	});

	afterEach(() => {
		cleanup();
		vi.restoreAllMocks();
	});

	it("does not filter the log ring while the panel is closed", () => {
		const view = render(() => <ErrorLogPanel />);
		const getEntries = vi.spyOn(appLogger, "getEntries");

		for (let i = 0; i < 10; i++) appLogger.error("app", `boom ${i}`);

		expect(getEntries).not.toHaveBeenCalled();
		expect(console.error).toHaveBeenCalledTimes(10);
		view.unmount();
	});

	/**
	 * The contrast that proves the test above is not vacuous: once opened, the
	 * same entries must be filtered and rendered.
	 */
	it("filters the log ring once the panel is opened", () => {
		const view = render(() => <ErrorLogPanel />);
		const { container } = view;
		appLogger.error("app", "visible failure");

		const getEntries = vi.spyOn(appLogger, "getEntries");
		errorLogStore.open();

		expect(getEntries).toHaveBeenCalled();
		expect(container.textContent).toContain("visible failure");
		expect(console.error).toHaveBeenCalledWith("[app]", "visible failure");
		view.unmount();
	});

	it("stops filtering again after the panel is closed", () => {
		const view = render(() => <ErrorLogPanel />);
		errorLogStore.open();
		errorLogStore.close();

		const getEntries = vi.spyOn(appLogger, "getEntries");
		for (let i = 0; i < 5; i++) appLogger.warn("app", `later ${i}`);

		expect(getEntries).not.toHaveBeenCalled();
		expect(console.warn).toHaveBeenCalledTimes(5);
		view.unmount();
	});

	it("cancels focus and auto-scroll frames when the panel closes", () => {
		const pending = new Set<number>();
		let nextFrame = 1;
		const request = vi.fn((_callback: FrameRequestCallback) => {
			const id = nextFrame++;
			pending.add(id);
			return id;
		});
		const cancel = vi.fn((id: number) => {
			pending.delete(id);
		});
		vi.stubGlobal("requestAnimationFrame", request);
		vi.stubGlobal("cancelAnimationFrame", cancel);

		const view = render(() => <ErrorLogPanel />);
		errorLogStore.open();
		expect(request).toHaveBeenCalledTimes(2);

		errorLogStore.close();
		expect(cancel).toHaveBeenCalledTimes(2);
		expect(pending.size).toBe(0);

		view.unmount();
		vi.unstubAllGlobals();
	});
});

it("catches shared filters losing severity selection and clipboard actions losing visible payloads", async () => {
	vi.spyOn(console, "error").mockImplementation(() => {});
	vi.spyOn(console, "warn").mockImplementation(() => {});
	appLogger.clear();
	appLogger.warn("app", "fixture warning");
	appLogger.error("git", "fixture error");
	expect(console.warn).toHaveBeenCalledWith("[app]", "fixture warning");
	expect(console.error).toHaveBeenCalledWith("[git]", "fixture error");
	errorLogStore.open();
	const view = render(() => <ErrorLogPanel />);
	fireEvent.click(view.getByRole("button", { name: "Error" }));
	expect(view.queryByText("fixture warning")).toBeNull();
	expect(view.getByText("fixture error")).toBeInTheDocument();
	fireEvent.click(view.getByRole("button", { name: "Copy All" }));
	await waitFor(() => expect(writeClipboard).toHaveBeenCalledWith(expect.stringContaining("fixture error")));
	expect(writeClipboard.mock.calls.at(-1)?.[0]).not.toContain("fixture warning");
	fireEvent.click(view.getByRole("button", { name: "Copy to clipboard" }));
	await waitFor(() => expect(writeClipboard).toHaveBeenCalledTimes(2));
	fireEvent.click(view.getByRole("button", { name: "Clear" }));
	expect(view.getByText("No log entries")).toBeInTheDocument();
	fireEvent.keyDown(document, { key: "Escape" });
	expect(errorLogStore.state.isOpen).toBe(false);
	cleanup();
	vi.restoreAllMocks();
});
