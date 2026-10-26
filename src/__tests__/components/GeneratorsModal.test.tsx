import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { GeneratorsModal } from "../../components/GeneratorsModal/GeneratorsModal";
import { mockInvoke } from "../mocks/tauri";
import "../mocks/tauri";

const { writeClipboard } = vi.hoisted(() => ({ writeClipboard: vi.fn() }));
vi.mock("../../utils/clipboard", () => ({ writeClipboard }));
beforeEach(() => {
	vi.clearAllMocks();
	mockInvoke.mockImplementation(async (command) =>
		command === "generate_value" ? { value: "fixture-value", extra: "fixture-public" } : undefined,
	);
	writeClipboard.mockResolvedValue(undefined);
});
afterEach(() => {
	cleanup();
	vi.useRealTimers();
});

it("catches shared option controls dropping password and Nano ID request changes", async () => {
	const view = render(() => <GeneratorsModal onClose={() => {}} />);
	await waitFor(() => expect(view.getByRole("button", { name: "Copy" })).not.toBeDisabled());
	fireEvent.input(view.getByRole("slider", { name: "Password length" }), { target: { value: "48" } });
	fireEvent.click(view.getByRole("checkbox", { name: "A–Z" }));
	await waitFor(() =>
		expect(mockInvoke).toHaveBeenCalledWith("generate_value", {
			request: { type: "password", length: 48, uppercase: false, lowercase: true, numbers: true, symbols: true },
		}),
	);
	fireEvent.click(view.getByRole("button", { name: "Nano ID" }));
	fireEvent.input(view.getByRole("spinbutton", { name: "Nano ID length" }), { target: { value: "30" } });
	await waitFor(() =>
		expect(mockInvoke).toHaveBeenCalledWith("generate_value", { request: { type: "nano_id", length: 30 } }),
	);
	expect(view.getByRole("button", { name: "Nano ID" })).toHaveAttribute("aria-pressed", "true");
});

it("catches copy actions losing keypair values and Close losing its callback", async () => {
	const onClose = vi.fn();
	const view = render(() => <GeneratorsModal onClose={onClose} />);
	fireEvent.click(view.getByRole("button", { name: "Ed25519 Key" }));
	await waitFor(() => expect(view.getByRole("button", { name: "Copy Private" })).not.toBeDisabled());
	vi.useFakeTimers();
	fireEvent.click(view.getByRole("button", { name: "Copy Private" }));
	await vi.advanceTimersByTimeAsync(0);
	expect(writeClipboard).toHaveBeenCalledWith("fixture-value");
	fireEvent.click(view.getByRole("button", { name: "Copy Public" }));
	await vi.advanceTimersByTimeAsync(0);
	expect(writeClipboard).toHaveBeenCalledWith("fixture-public");
	expect(view.getAllByRole("button", { name: "Copied!" })).toHaveLength(2);
	await vi.advanceTimersByTimeAsync(2000);
	expect(view.getByRole("button", { name: "Copy Private" })).not.toBeDisabled();
	expect(view.getByRole("button", { name: "Copy Public" })).not.toBeDisabled();
	for (const output of view.getAllByRole("textbox")) expect(output).toHaveAttribute("readonly");
	fireEvent.click(view.getByRole("button", { name: "Close" }));
	expect(onClose).toHaveBeenCalledOnce();
});

it("catches Regenerate and Copy becoming enabled before a generation settles", async () => {
	let settle!: (result: { value: string }) => void;
	mockInvoke.mockReturnValueOnce(
		new Promise((resolve) => {
			settle = resolve;
		}),
	);
	const view = render(() => <GeneratorsModal onClose={() => {}} />);
	expect(view.getByRole("button", { name: "Regenerate" })).toBeDisabled();
	expect(view.getByRole("button", { name: "Copy" })).toBeDisabled();
	settle({ value: "completed" });
	await waitFor(() => expect(view.getByRole("button", { name: "Regenerate" })).not.toBeDisabled());
	fireEvent.click(view.getByRole("button", { name: "Regenerate" }));
	await waitFor(() => expect(mockInvoke).toHaveBeenCalledTimes(2));
});
