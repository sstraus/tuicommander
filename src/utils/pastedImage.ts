import { invoke } from "../invoke";
import { appLogger } from "../stores/appLogger";

const IMAGE_EXTENSIONS = new Map([
	["image/png", "png"],
	["image/jpeg", "jpg"],
	["image/webp", "webp"],
	["image/gif", "gif"],
]);

/** Convert a Blob to a base64 string */
async function blobToBase64(blob: Blob): Promise<string> {
	const buffer = await blob.arrayBuffer();
	const bytes = new Uint8Array(buffer);
	let binary = "";
	for (const byte of bytes) binary += String.fromCharCode(byte);
	return btoa(binary);
}

const IMAGE_FILE_NAME = /^[^\r\n]*\.(png|jpe?g|webp|gif)$/i;

/** Finder puts the copied image file's name or path on the clipboard as text/plain
 *  next to the image itself; that text is not something the user means to paste. */
function isImageFileName(e: ClipboardEvent): boolean {
	return IMAGE_FILE_NAME.test(e.clipboardData?.getData?.("text/plain").trim() ?? "");
}

/** Text wins over incidental images; Finder's image filename is not substantive text. */
export function isImagePaste(e: ClipboardEvent): boolean {
	const items = e.clipboardData?.items;
	if (!items) return false;
	if (Array.from(items).some((item) => item.type === "text/plain") && !isImageFileName(e)) return false;
	return Array.from(items).some((item) => item.type.startsWith("image/"));
}

/** Shared clipboard precedence for file-backed and ACP image attachments. */
export function pastedImageFiles(e: ClipboardEvent): File[] {
	const items = e.clipboardData?.items;
	if (!items || !isImagePaste(e)) return [];
	return Array.from(items)
		.filter((item) => IMAGE_EXTENSIONS.has(item.type))
		.map((item) => item.getAsFile())
		.filter((file): file is File => file !== null);
}

/** Save the first accepted image on a paste event and return its path.
 *
 *  Returns null when the paste carries no accepted image (the default text paste
 *  proceeds), when it also carries text/plain that is more than the image's own
 *  file name (a spreadsheet cell copy puts a rendered picture next to its text;
 *  the text wins), or when saving failed
 *  (logged). The default action is cancelled
 *  synchronously, before the first await, so the caller may rely on
 *  `event.defaultPrevented` to know the paste was claimed.
 *  `getNoteId` is only called once an image is found: it names the asset
 *  directory (`note-images/<id>/`) on the backend. */
export async function savePastedImage(e: ClipboardEvent, getNoteId: () => string): Promise<string | null> {
	for (const blob of pastedImageFiles(e)) {
		const extension = IMAGE_EXTENSIONS.get(blob.type);
		if (!extension) continue;
		e.preventDefault();

		const noteId = getNoteId();
		try {
			const dataBase64 = await blobToBase64(blob);
			// `noteId` is the backend argument name and the on-disk asset
			// directory (`note-images/<id>/`). It keeps the old vocabulary
			// on purpose — see the boundary note in `stores/ideas.ts`.
			return await invoke<string>("save_note_image", {
				noteId,
				dataBase64,
				extension,
			});
		} catch (err) {
			appLogger.error("store", "Failed to save pasted image", err);
			return null;
		}
	}
	return null;
}
