import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mockInvoke } from "../mocks/tauri";
import "../mocks/tauri";
import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";

const { mockWriteClipboard, mockDownloadText } = vi.hoisted(() => ({
	mockWriteClipboard: vi.fn<() => Promise<void>>(() => Promise.resolve()),
	mockDownloadText: vi.fn(),
}));

vi.mock("../../utils/clipboard", () => ({
	writeClipboard: mockWriteClipboard,
}));

vi.mock("../../utils/downloadText", () => ({
	downloadText: mockDownloadText,
}));

import { ChangelogModal } from "../../components/ChangelogModal/ChangelogModal";
import { appLogger } from "../../stores/appLogger";

describe("ChangelogModal", () => {
	const defaultProps = {
		repoPath: "/repo",
		onClose: vi.fn(),
	};

	beforeEach(() => {
		vi.clearAllMocks();
		mockInvoke.mockResolvedValue({ markdown: "## CL\n- x", json: {} });
	});

	afterEach(() => {
		cleanup();
		vi.restoreAllMocks();
	});

	it("shows loading state then renders the markdown", async () => {
		let resolveInvoke!: (value: unknown) => void;
		mockInvoke.mockReturnValueOnce(new Promise((res) => (resolveInvoke = res)));

		const { container } = render(() => <ChangelogModal {...defaultProps} />);
		// Loading state visible before the promise resolves
		expect(container.querySelector(".spinner")).not.toBeNull();

		resolveInvoke({ markdown: "## CL\n- x", json: {} });

		await waitFor(() => {
			expect(container.querySelector(".markdown")).not.toBeNull();
		});
		expect(container.querySelector(".markdown")!.textContent).toContain("## CL");
		expect(container.querySelector(".spinner")).toBeNull();
	});

	it("asks the backend for this repo, and carries no provider or model with it", async () => {
		render(() => <ChangelogModal {...defaultProps} />);
		await waitFor(() => expect(mockInvoke).toHaveBeenCalled());
		// Which model writes the changelog is ego's configuration (criterion 4).
		expect(mockInvoke).toHaveBeenCalledWith("generate_changelog", { repoPath: "/repo" });
	});

	it("renders ego's own sentence when ego is not reachable, never an empty changelog", async () => {
		// Verbatim from `acp::oneshot::ask` when the ACP connection cannot be made.
		const egoSays = "ego could not run this turn: no ego executable is configured";
		const loggedError = vi.spyOn(appLogger, "error").mockImplementation(() => {});
		mockInvoke.mockRejectedValueOnce(egoSays);

		const { container } = render(() => <ChangelogModal {...defaultProps} />);

		await waitFor(() => {
			expect(container.querySelector(".error")).not.toBeNull();
		});
		expect(container.querySelector(".error")!.textContent).toContain("no ego executable is configured");
		expect(loggedError).toHaveBeenCalledWith("github", "generate_changelog failed", { error: egoSays });
		// Not a blank body that reads as "this repo changed nothing".
		expect(container.querySelector(".markdown")).toBeNull();
		expect(container.querySelector(".spinner")).toBeNull();
	});

	it("disables Copy and Save while there is nothing to copy or save", async () => {
		const loggedError = vi.spyOn(appLogger, "error").mockImplementation(() => {});
		mockInvoke.mockRejectedValueOnce("ego could not run this turn: spawn failed");
		const { container } = render(() => <ChangelogModal {...defaultProps} />);

		await waitFor(() => {
			expect(container.querySelector(".error")).not.toBeNull();
		});
		expect(screen.getByRole("button", { name: "Copy" })).toBeDisabled();
		expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
		expect(loggedError).toHaveBeenCalledWith("github", "generate_changelog failed", {
			error: "ego could not run this turn: spawn failed",
		});
	});

	it("calls writeClipboard when Copy is clicked", async () => {
		const { container } = render(() => <ChangelogModal {...defaultProps} />);

		await waitFor(() => {
			expect(container.querySelector(".markdown")).not.toBeNull();
		});

		const copyBtn = screen.getByRole("button", { name: "Copy" });
		fireEvent.click(copyBtn);
		expect(mockWriteClipboard).toHaveBeenCalledWith("## CL\n- x");
	});

	it("calls downloadText when Save is clicked", async () => {
		const { container } = render(() => <ChangelogModal {...defaultProps} />);

		await waitFor(() => {
			expect(container.querySelector(".markdown")).not.toBeNull();
		});

		const saveBtn = screen.getByRole("button", { name: "Save" });
		fireEvent.click(saveBtn);
		expect(mockDownloadText).toHaveBeenCalledWith("CHANGELOG-ai.md", "## CL\n- x");
	});
});
