import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
const isTauri = vi.fn<() => boolean>(() => true);
const toastAdd = vi.fn();
const logger = { warn: vi.fn(), error: vi.fn(), debug: vi.fn() };

type Module = typeof import("../../utils/openUrl");

async function load(): Promise<Module> {
	vi.resetModules();
	vi.doMock("../../transport", () => ({ isTauri }));
	vi.doMock("../../invoke", () => ({ invoke }));
	vi.doMock("../../stores/appLogger", () => ({ appLogger: logger }));
	vi.doMock("../../stores/toasts", () => ({ toastsStore: { add: toastAdd } }));
	return await import("../../utils/openUrl");
}

describe("openConfirmedLink", () => {
	beforeEach(() => {
		invoke.mockReset().mockResolvedValue(undefined);
		isTauri.mockReturnValue(true);
		toastAdd.mockReset();
		for (const fn of Object.values(logger)) fn.mockReset();
	});

	// Catches: the owner cannot open a deliberate network document or editor-protocol link.
	it("owner_confirmed_network_or_custom_link_uses_handler", async () => {
		const mod = await load();
		const confirm = vi.fn().mockResolvedValue(true);
		mod.registerLinkConfirm(confirm);
		for (const target of [
			"editor://open?file=/repo/a.rs&line=3",
			"smb://nas/share/doc.pdf",
			"\\\\nas\\share\\doc.md",
		]) {
			invoke.mockClear();
			await mod.openConfirmedLink(target);
			expect(invoke).toHaveBeenCalledWith("open_default_app", { target, kind: "handler" });
		}
		// The dialog names the exact target the handler receives.
		expect(confirm.mock.calls[0][0].message).toContain("editor://open?file=/repo/a.rs&line=3");
		expect(confirm.mock.calls[0][0].defaultButton).toBe("cancel");

		// Web and mail links keep their direct click route; they ask nothing.
		confirm.mockClear();
		invoke.mockClear();
		await mod.openConfirmedLink("https://example.com/x");
		await mod.openConfirmedLink("mailto:a@example.com");
		expect(confirm).not.toHaveBeenCalled();
		expect(invoke).toHaveBeenCalledWith("open_default_app", { target: "https://example.com/x", kind: "url" });
		expect(invoke).toHaveBeenCalledWith("open_default_app", { target: "mailto:a@example.com", kind: "url" });
	});

	// Catches: downloaded content invokes a sensitive handler without owner action.
	it("untrusted_output_cannot_auto_open_or_skip_confirmation", async () => {
		const mod = await load();
		// The untrusted automatic route still refuses every non-web handler.
		mod.handleOpenUrl("editor://open?file=/etc/passwd");
		mod.handleOpenUrl("smb://attacker.example/share");
		expect(invoke).not.toHaveBeenCalled();

		// No confirmation surface registered: fail closed.
		await mod.openConfirmedLink("editor://open?file=/x");
		expect(invoke).not.toHaveBeenCalled();

		// The owner refuses: nothing opens.
		const refuse = vi.fn().mockResolvedValue(false);
		mod.registerLinkConfirm(refuse);
		await mod.openConfirmedLink("smb://attacker.example/share");
		expect(refuse).toHaveBeenCalledTimes(1);
		expect(invoke).not.toHaveBeenCalled();

		// Malformed input and WebView-internal schemes never reach a dialog or a handler.
		for (const target of [
			"javascript:alert(1)",
			"data:text/html,x",
			"file:///etc/passwd",
			"c:\\x",
			"nota link",
			"",
			"\\\\",
		]) {
			await mod.openConfirmedLink(target);
		}
		expect(refuse).toHaveBeenCalledTimes(1);
		expect(invoke).not.toHaveBeenCalled();
		expect(mod.isHandlerLink("Makefile:12")).toBe(false);
	});

	// Catches: a handler failure disappears into the log and the click looks dead.
	it("shows a visible error when the system handler fails", async () => {
		const mod = await load();
		mod.registerLinkConfirm(vi.fn().mockResolvedValue(true));
		invoke.mockRejectedValueOnce("No application knows how to open editor://x");
		await mod.openConfirmedLink("editor://x");
		expect(toastAdd).toHaveBeenCalledWith("Could not open link", expect.stringContaining("No application"), "error");
	});

	// Catches: browser mode passing a host network path to window.open, which cannot reach it.
	it("browser mode opens custom schemes on the client and explains UNC paths", async () => {
		const mod = await load();
		isTauri.mockReturnValue(false);
		const open = vi.spyOn(window, "open").mockReturnValue(null);
		mod.registerLinkConfirm(vi.fn().mockResolvedValue(true));
		await mod.openConfirmedLink("editor://x");
		expect(open).toHaveBeenCalledWith("editor://x", "_blank");
		await mod.openConfirmedLink("//nas/share/doc.md");
		expect(open).toHaveBeenCalledTimes(1);
		expect(toastAdd).toHaveBeenCalledWith("Could not open link", expect.stringContaining("desktop app"), "error");
		open.mockRestore();
	});
});
