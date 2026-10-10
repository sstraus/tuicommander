import { beforeEach, describe, expect, it, vi } from "vitest";
import { copyPathToClipboard, readClipboard, writeClipboard } from "../../utils/clipboard";
import { mockInvoke } from "../mocks/tauri";

describe("copyPathToClipboard", () => {
	beforeEach(() => {
		mockInvoke.mockClear();
	});

	// In Tauri mode writeClipboard() routes through the native clipboard
	// command (WKWebView rejects navigator.clipboard.writeText), so assert on the invoke.
	const copiedText = () =>
		mockInvoke.mock.calls.find(([cmd]) => cmd === "write_clipboard_text")?.[1].text;

	it("compresses the user's home directory to ~", async () => {
		copyPathToClipboard("/Users/someone/Gits/project/src/main.rs");
		await vi.waitFor(() => expect(copiedText()).toBe("~/Gits/project/src/main.rs"));
	});

	it("leaves a path outside the home directory untouched", async () => {
		copyPathToClipboard("/etc/hosts");
		await vi.waitFor(() => expect(copiedText()).toBe("/etc/hosts"));
	});
});


describe("native clipboard commands", () => {
	beforeEach(() => {
		mockInvoke.mockReset();
	});

	it("writes through the native command so removing the plugin cannot break copy", async () => {
		await writeClipboard("hello\nworld");
		expect(mockInvoke).toHaveBeenCalledExactlyOnceWith("write_clipboard_text", { text: "hello\nworld" });
	});

	it("reads through the native command so paste does not use the removed plugin", async () => {
		mockInvoke.mockResolvedValueOnce("pasted text");
		await expect(readClipboard()).resolves.toBe("pasted text");
		expect(mockInvoke).toHaveBeenCalledExactlyOnceWith("read_clipboard_text", undefined);
	});

	it("rejects native write failures so callers can show copy failed", async () => {
		mockInvoke.mockRejectedValueOnce("clipboard unavailable");
		await expect(writeClipboard("text")).rejects.toBe("clipboard unavailable");
	});

	it("rejects native read failures instead of returning empty text", async () => {
		mockInvoke.mockRejectedValueOnce("clipboard unavailable");
		await expect(readClipboard()).rejects.toBe("clipboard unavailable");
	});
});
