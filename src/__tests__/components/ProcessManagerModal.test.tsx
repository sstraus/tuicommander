import { fireEvent, render } from "@solidjs/testing-library";
import { expect, it, vi } from "vitest";
import { ProcessManagerModal } from "../../components/ProcessManagerModal/ProcessManagerModal";
import { mockInvoke } from "../mocks/tauri";

it("catches the shared close action losing its callback or submitting a surrounding form", () => {
	mockInvoke.mockResolvedValue([]);
	const onClose = vi.fn();
	const { getByRole } = render(() => <ProcessManagerModal onClose={onClose} />);
	const close = getByRole("button", { name: "Close" });
	expect(close.getAttribute("type")).toBe("button");
	fireEvent.click(close);
	expect(onClose).toHaveBeenCalledTimes(1);
});

// catches: a 2 s refresh with identical stats remounts every row
it("process_manager_identical_refresh_keeps_rows", async () => {
	vi.useFakeTimers();
	try {
		const stats = () => [
			{ session_id: null, name: "tuicommander", pid: 1, rss_kb: 2048, cpu_pct: 1.5 },
			{ session_id: "s1", name: "claude", pid: 2, rss_kb: 4096, cpu_pct: 3 },
		];
		mockInvoke.mockClear();
		mockInvoke.mockImplementation(() => Promise.resolve(stats()));
		const view = render(() => <ProcessManagerModal onClose={() => {}} />);
		await vi.advanceTimersByTimeAsync(0);
		const before = [...view.container.querySelectorAll("tbody tr")];
		expect(before).toHaveLength(2);

		await vi.advanceTimersByTimeAsync(2000);
		expect(mockInvoke).toHaveBeenCalledTimes(2);
		const after = [...view.container.querySelectorAll("tbody tr")];
		expect(after).toHaveLength(2);
		expect(after[0]).toBe(before[0]);
		expect(after[1]).toBe(before[1]);
		view.unmount();
	} finally {
		mockInvoke.mockReset();
		vi.useRealTimers();
	}
});
