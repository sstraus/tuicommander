/** One bounded page from `read_editor_file_page` / `read_editor_file_external_page`. */
export interface FilePage {
	/** Standard base64 of the raw page bytes. */
	data: string;
	offset: number;
	bytesRead: number;
	totalSize: number;
	hasMore: boolean;
}

/** Thrown when the caller stopped the read between pages (file or tab changed). */
export class PagedReadCancelled extends Error {
	constructor() {
		super("Paged read cancelled");
	}
}

/** Matched by the editor's "too large"/"not displayable" notices; keep the wording stable. */
const NOT_UTF8 = "File is not valid UTF-8 text";
const CHANGED = "File changed on disk while it was loading. Reload to try again.";

export interface PagedTextReadOptions {
	/** Polled before each page and after the last one. */
	isCancelled: () => boolean;
}

function decodeBase64(data: string): Uint8Array {
	return Uint8Array.from(atob(data), (char) => char.charCodeAt(0));
}

/**
 * Read a UTF-8 text file one bounded page at a time, pulling the next page only
 * after the previous one is decoded. The size reported by the first page is the
 * end of the read: bytes appended later (an active log) are left for the next
 * re-read, which the editor's disk-stat poll starts. A file that shrinks or
 * answers another offset rejects instead of returning mixed content.
 */
export async function readTextInPages(
	readPage: (offset: number) => Promise<FilePage>,
	{ isCancelled }: PagedTextReadOptions,
): Promise<string> {
	const decoder = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true });
	const parts: string[] = [];
	let offset = 0;
	let totalSize: number | null = null;
	for (;;) {
		if (isCancelled()) throw new PagedReadCancelled();
		const page = await readPage(offset);
		totalSize ??= page.totalSize;
		// Growth past the first size is tolerated; shrinking is a rewrite.
		if (page.totalSize < totalSize || page.offset !== offset) throw new Error(CHANGED);
		const all = decodeBase64(page.data);
		// An empty page that still reports more data would never advance.
		if (all.length !== page.bytesRead || (page.hasMore && page.bytesRead === 0)) throw new Error(CHANGED);
		const bytes = all.subarray(0, Math.min(all.length, totalSize - offset));
		const last = offset + bytes.length >= totalSize || !page.hasMore;
		try {
			parts.push(decoder.decode(bytes, { stream: !last }));
		} catch {
			throw new Error(NOT_UTF8);
		}
		offset += bytes.length;
		if (last) break;
	}
	if (offset !== totalSize) throw new Error(CHANGED);
	if (isCancelled()) throw new PagedReadCancelled();
	return parts.join("");
}
