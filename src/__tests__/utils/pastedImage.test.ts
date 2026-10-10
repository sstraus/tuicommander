import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../invoke", () => ({ invoke: vi.fn() }));
vi.mock("../../stores/appLogger", () => ({
	appLogger: { error: vi.fn(), warn: vi.fn(), info: vi.fn(), debug: vi.fn() },
}));

import { invoke } from "../../invoke";
import { savePastedImage } from "../../utils/pastedImage";

type Item = { kind: string; type: string; getAsFile: () => File | null };

const file = (type: string) => new File([new Uint8Array([1, 2, 3])], "x", { type });
const imageItem = (type: string): Item => ({ kind: "file", type, getAsFile: () => file(type) });
const textItem = (): Item => ({ kind: "string", type: "text/plain", getAsFile: () => null });

function pasteEvent(items: Item[]): ClipboardEvent {
	const e = new Event("paste", { bubbles: true, cancelable: true }) as unknown as ClipboardEvent;
	Object.defineProperty(e, "clipboardData", { value: { items } });
	return e;
}

beforeEach(() => {
	vi.mocked(invoke).mockReset();
	vi.mocked(invoke).mockResolvedValue("/saved/a.png" as never);
});

describe("savePastedImage (critic 1350)", () => {
	it("does not claim a paste whose item type is an Object.prototype key — catches `in` lookup on a plain object", async () => {
		const getNoteId = vi.fn(() => "n1");
		const e = pasteEvent([{ kind: "file", type: "constructor", getAsFile: () => file("constructor") }]);
		const out = await savePastedImage(e, getNoteId);
		expect(out).toBeNull();
		expect(e.defaultPrevented).toBe(false);
		expect(invoke).not.toHaveBeenCalled();
	});

	it("leaves the default text paste alone when the image item yields no file — catches preventDefault before the getAsFile null check", async () => {
		const e = pasteEvent([{ kind: "file", type: "image/png", getAsFile: () => null }, textItem()]);
		const out = await savePastedImage(e, () => "n1");
		expect(out).toBeNull();
		expect(e.defaultPrevented).toBe(false);
	});

	it("saves only the first of several images — catches a loop that saves each and loses all but one path", async () => {
		const e = pasteEvent([imageItem("image/png"), imageItem("image/jpeg")]);
		await savePastedImage(e, () => "n1");
		expect(invoke).toHaveBeenCalledTimes(1);
		expect(invoke).toHaveBeenCalledWith("save_note_image", expect.objectContaining({ noteId: "n1", extension: "png" }));
	});

	it("finds an image behind a non-plain-text item in a mixed clipboard — catches stopping at the first item", async () => {
		const e = pasteEvent([{ kind: "string", type: "text/html", getAsFile: () => null }, imageItem("image/gif")]);
		const out = await savePastedImage(e, () => "n1");
		expect(out).toBe("/saved/a.png");
		expect(invoke).toHaveBeenCalledWith("save_note_image", expect.objectContaining({ extension: "gif" }));
	});

	it("maps jpeg to jpg and sends the base64 of the bytes — catches a wrong extension or encoding", async () => {
		const e = pasteEvent([imageItem("image/jpeg")]);
		await savePastedImage(e, () => "n1");
		expect(invoke).toHaveBeenCalledWith("save_note_image", {
			noteId: "n1",
			dataBase64: btoa("\x01\x02\x03"),
			extension: "jpg",
		});
	});

	it("returns null and still cancels the default when saving fails — catches a rejection escaping to the caller", async () => {
		vi.mocked(invoke).mockRejectedValue(new Error("disk full"));
		const e = pasteEvent([imageItem("image/png")]);
		await expect(savePastedImage(e, () => "n1")).resolves.toBeNull();
		expect(e.defaultPrevented).toBe(true);
	});

	it("does not call getNoteId for a non-image paste — catches allocating a note id (and Ideas pendingIdeaId) on text paste", async () => {
		const getNoteId = vi.fn(() => "n1");
		const e = pasteEvent([
			textItem(),
			{ kind: "file", type: "application/pdf", getAsFile: () => file("application/pdf") },
		]);
		expect(await savePastedImage(e, getNoteId)).toBeNull();
		expect(getNoteId).not.toHaveBeenCalled();
		expect(e.defaultPrevented).toBe(false);
	});

	it("cancels the default synchronously, before the save settles — catches moving preventDefault after the first await", async () => {
		let finishSave!: (path: string) => void;
		vi.mocked(invoke).mockReturnValue(
			new Promise<string>((resolve) => {
				finishSave = resolve;
			}) as never,
		);
		const e = pasteEvent([imageItem("image/png")]);
		const saving = savePastedImage(e, () => "n1");
		expect(e.defaultPrevented).toBe(true);
		// Let encoding reach the controlled IPC request, then release it.
		await vi.waitFor(() => expect(invoke).toHaveBeenCalled());
		finishSave("/saved/a.png");
		await saving;
	});

	it("returns null when clipboardData is missing — catches a throw on synthetic paste events", async () => {
		const e = new Event("paste", { cancelable: true }) as unknown as ClipboardEvent;
		await expect(savePastedImage(e, () => "n1")).resolves.toBeNull();
	});
});

it("saves Finder file-name plus image clipboard data rather than rejecting it as text", async () => {
	// Catches: shared clipboard selection treats Finder's filename as substantive text.
	const event = pasteEvent([textItem(), imageItem("image/png")]);
	Object.defineProperty(event.clipboardData, "getData", { value: () => "/Users/Boss/Pictures/clip.png" });
	await expect(savePastedImage(event, () => "finder")).resolves.toBe("/saved/a.png");
	expect(event.defaultPrevented).toBe(true);
});
