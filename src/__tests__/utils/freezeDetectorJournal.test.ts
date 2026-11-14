import { afterEach, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ warn: vi.fn() }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { debug: vi.fn(), diag: { warn: mocks.warn } } }));
vi.mock("../../utils/perfDebug", () => ({ isPerfDebug: () => true }));
vi.mock("../../utils/perfTrace", () => ({ getLastCrumb: () => ({ label: "private/path", ageMs: 5 }) }));

afterEach(() => {
	vi.clearAllTimers();
	vi.useRealTimers();
	vi.restoreAllMocks();
	vi.unstubAllGlobals();
	mocks.warn.mockClear();
});

it("includes a numeric freeze gap so persistence need not store or parse breadcrumb strings", async () => {
	vi.resetModules();
	vi.useFakeTimers();
	vi.spyOn(document, "hasFocus").mockReturnValue(true);
	vi.spyOn(document, "hidden", "get").mockReturnValue(false);
	const { startFreezeDetector } = await import("../../utils/freezeDetector");
	let now = 0;
	vi.spyOn(performance, "now").mockImplementation(() => now);
	startFreezeDetector();
	now = 3000;
	vi.advanceTimersByTime(50);
	expect(mocks.warn).toHaveBeenCalledWith("app", "UI freeze: 3000ms main-thread block", {
		label: "private/path",
		ageMs: 5,
		gap_ms: 3000,
	});
});
