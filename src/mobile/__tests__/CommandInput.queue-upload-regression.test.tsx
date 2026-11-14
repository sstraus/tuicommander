import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, expect, it, vi } from "vitest";
import { CommandInput } from "../components/CommandInput";

vi.mock("../../transport", () => ({
	buildHttpUrl: (path: string) => `http://localhost${path}`,
	rpc: vi.fn(async () => undefined),
	HttpRpcError: class extends Error {},
}));
vi.mock("../../services/uploadAttachment", () => ({ uploadAttachment: vi.fn() }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { debug: vi.fn(), warn: vi.fn(), error: vi.fn() } }));

import { uploadAttachment } from "../../services/uploadAttachment";
import { rpc } from "../../transport";

afterEach(() => {
	cleanup();
	vi.useRealTimers();
	vi.resetAllMocks();
});

// Catches: moving the attachment control disconnects its picker or permits Queue before the receipt arrives.
it("does not queue an incomplete prompt while its selected attachment is still uploading", async () => {
	vi.useFakeTimers();
	let finishUpload!: (receipt: { path: string; size: number }) => void;
	vi.mocked(uploadAttachment).mockReturnValue(
		new Promise((resolve) => {
			finishUpload = resolve;
		}),
	);
	vi.mocked(rpc).mockResolvedValue(undefined as never);
	const { container, getByRole } = render(() => <CommandInput sessionId="queue-upload" />);
	const input = container.querySelector("textarea")!;
	fireEvent.input(input, { target: { value: "Review this file" } });
	const picker = container.querySelector<HTMLInputElement>('input[type="file"]')!;
	const openPicker = vi.spyOn(picker, "click");
	fireEvent.click(getByRole("button", { name: "More composer actions" }));
	fireEvent.click(getByRole("menuitem", { name: "Attach file" }));
	expect(openPicker).toHaveBeenCalledOnce();
	fireEvent.change(picker, {
		target: { files: [new File(["important context"], "notes.txt", { type: "text/plain" })] },
	});
	expect(getByRole("status").textContent).toBe("Uploading notes.txt…");
	fireEvent.click(getByRole("button", { name: "More composer actions" }));
	expect(getByRole("menuitem", { name: "Attach file" })).toBeDisabled();
	fireEvent.click(getByRole("button", { name: "More composer actions" }));
	const send = getByRole("button", { name: "Send" });
	fireEvent.pointerDown(send, { button: 0, pointerType: "touch" });
	await vi.advanceTimersByTimeAsync(600);
	fireEvent.pointerUp(send, { button: 0, pointerType: "touch" });
	fireEvent.click(send);
	expect(getByRole("menuitem", { name: "Queue" })).toBeDisabled();
	fireEvent.click(getByRole("menuitem", { name: "Queue" }));
	await Promise.resolve();
	await Promise.resolve();
	// The selected attachment belongs to this prompt, not an unrelated producer.
	// Submitting now can run the agent without the file and erase its later receipt.
	const queuedBeforeUpload = vi.mocked(rpc).mock.calls.filter(([command]) => command === "enqueue_agent_command");
	// Settle the external operation before asserting, even on the failing path.
	finishUpload({ path: "/repo/.tuic/attachments/notes.txt", size: 17 });
	await vi.advanceTimersByTimeAsync(0);
	expect(container.querySelector('[role="status"]')).toBeNull();
	fireEvent.keyDown(document, { key: "Escape" });
	fireEvent.click(getByRole("button", { name: "More composer actions" }));
	expect(getByRole("menuitem", { name: "Attach file" })).not.toBeDisabled();
	expect(queuedBeforeUpload).toEqual([]);
	expect(input.value).toBe("Review this file @/repo/.tuic/attachments/notes.txt");
});

// Catches: an ego attachment is staged silently despite the backend's read-root limitation.
it("shows the access notice while retaining the uploaded attachment in the draft", async () => {
	const { toastsStore } = await import("../../stores/toasts");
	const toast = vi.spyOn(toastsStore, "add").mockReturnValue(1);
	vi.mocked(uploadAttachment).mockResolvedValue({
		path: "/tuic/attachments/pty-ego/file.txt",
		size: 4,
		notice: "Confined ego needs an existing user/profile read root",
	});
	const { container } = render(() => <CommandInput sessionId="ego-upload" />);
	fireEvent.change(container.querySelector<HTMLInputElement>('input[type="file"]')!, {
		target: { files: [new File(["file"], "file.txt")] },
	});
	await Promise.resolve();
	await Promise.resolve();
	expect(toast).toHaveBeenCalledWith(
		"Attachment access",
		"Confined ego needs an existing user/profile read root",
		"warn",
		true,
	);
	expect(container.querySelector("textarea")!.value).toBe("@/tuic/attachments/pty-ego/file.txt");
	toast.mockRestore();
});

// Catches: an upload that finishes after a session switch adds its path to the new session's draft.
it("keeps a late upload receipt out of the draft of a session selected during the upload", async () => {
	const { toastsStore } = await import("../../stores/toasts");
	const toast = vi.spyOn(toastsStore, "add").mockReturnValue(1);
	let finishUpload!: (receipt: { path: string; size: number }) => void;
	vi.mocked(uploadAttachment).mockReturnValue(
		new Promise((resolve) => {
			finishUpload = resolve;
		}),
	);
	vi.mocked(rpc).mockResolvedValue(undefined as never);
	const [session, setSession] = createSignal("upload-owner");
	const { container } = render(() => <CommandInput sessionId={session()} />);
	fireEvent.change(container.querySelector<HTMLInputElement>('input[type="file"]')!, {
		target: { files: [new File(["file"], "file.txt")] },
	});
	setSession("replacement");
	const input = container.querySelector("textarea")!;
	fireEvent.input(input, { target: { value: "other draft" } });
	vi.mocked(rpc).mockClear();
	finishUpload({ path: "/tuic/attachments/pty-upload-owner/file.txt", size: 4 });
	await Promise.resolve();
	await Promise.resolve();
	expect(uploadAttachment).toHaveBeenCalledWith(expect.any(File), { kind: "pty", id: "upload-owner" });
	expect(input.value).toBe("other draft");
	expect(vi.mocked(rpc).mock.calls.filter(([command]) => command === "write_pty")).toEqual([]);
	expect(toast).toHaveBeenCalledWith(
		"Attachment not added",
		expect.stringContaining("/tuic/attachments/pty-upload-owner/file.txt"),
		"warn",
		true,
	);
	toast.mockRestore();
});
