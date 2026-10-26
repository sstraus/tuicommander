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
