import { fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const h = vi.hoisted(() => ({ invoke: vi.fn(), writeClipboard: vi.fn(), error: vi.fn() }));

vi.mock("../../invoke", () => ({ invoke: h.invoke }));
vi.mock("../../utils/clipboard", () => ({ writeClipboard: h.writeClipboard }));
vi.mock("../../stores/modalStack", () => ({ registerModal: vi.fn() }));
vi.mock("../../stores/appLogger", () => ({
	appLogger: { debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: h.error },
}));

import { GeneratorsModal } from "../../components/GeneratorsModal/GeneratorsModal";

describe("GeneratorsModal copy clipboard handling", () => {
	beforeEach(() => {
		h.invoke.mockReset().mockResolvedValue({ value: "s3cret-value" });
		h.writeClipboard.mockReset().mockResolvedValue(undefined);
		h.error.mockReset();
	});

	afterEach(() => {
		vi.useRealTimers();
	});

	it("shows Copied! after a successful write", async () => {
		vi.useFakeTimers();
		const { getByText } = render(() => <GeneratorsModal onClose={vi.fn()} />);
		await vi.waitFor(() => expect((getByText("Copy") as HTMLButtonElement).disabled).toBe(false));
		fireEvent.click(getByText("Copy"));
		await vi.waitFor(() => expect(getByText("Copied!")).toBeTruthy());
		await vi.advanceTimersByTimeAsync(2000);
	});

	it("logs a denied write, does not throw, and shows no false Copied! state", async () => {
		const unhandled = vi.fn();
		process.on("unhandledRejection", unhandled);
		try {
			const err = new DOMException("Write permission denied.", "NotAllowedError");
			h.writeClipboard.mockRejectedValue(err);
			const { getByText, queryByText } = render(() => <GeneratorsModal onClose={vi.fn()} />);
			await waitFor(() => expect((getByText("Copy") as HTMLButtonElement).disabled).toBe(false));
			fireEvent.click(getByText("Copy"));

			await waitFor(() => expect(h.error).toHaveBeenCalledWith("app", "Failed to copy generated value", err));
			await new Promise((r) => setTimeout(r, 20));
			expect(queryByText("Copied!")).toBeNull();
			expect(unhandled).not.toHaveBeenCalled();
		} finally {
			process.off("unhandledRejection", unhandled);
		}
	});
});
