import { describe, expect, it } from "vitest";
import { type FilePage, PagedReadCancelled, readTextInPages } from "../../utils/pagedTextRead";

/** Serve `bytes` the way the Rust page reader does, `pageSize` raw bytes per page. */
function pagesOf(bytes: Uint8Array, pageSize: number) {
	const offsets: number[] = [];
	const readPage = async (offset: number): Promise<FilePage> => {
		offsets.push(offset);
		const slice = bytes.subarray(offset, offset + pageSize);
		return {
			data: btoa(String.fromCharCode(...slice)),
			offset,
			bytesRead: slice.length,
			totalSize: bytes.length,
			hasMore: offset + slice.length < bytes.length,
		};
	};
	return { readPage, offsets };
}

const stable = { isCancelled: () => false };

describe("readTextInPages", () => {
	// Catches: a multi-byte character split across a page boundary is corrupted or the final page is lost.
	it("rebuilds the exact text across page boundaries up to the final page", async () => {
		const text = "aé€😀z".repeat(5);
		const { readPage, offsets } = pagesOf(new TextEncoder().encode(text), 3);
		expect(await readTextInPages(readPage, stable)).toBe(text);
		expect(offsets[0]).toBe(0);
		expect(offsets.every((offset, i) => i === 0 || offset === offsets[i - 1] + 3)).toBe(true);
	});

	// Catches: an empty file loops forever or yields an error instead of an empty document.
	it("reads an empty file as one empty final page", async () => {
		const { readPage, offsets } = pagesOf(new Uint8Array(), 4);
		expect(await readTextInPages(readPage, stable)).toBe("");
		expect(offsets).toEqual([0]);
	});

	// Catches: invalid bytes are silently replaced with U+FFFD and opened as editable text.
	it("rejects bytes that are not UTF-8 with the editor's not-displayable wording", async () => {
		const { readPage } = pagesOf(Uint8Array.from([0x61, 0xff, 0x62]), 2);
		await expect(readTextInPages(readPage, stable)).rejects.toThrow(/valid UTF-8/i);
		// A sequence cut off by EOF is invalid too, not held back forever.
		const truncated = pagesOf(Uint8Array.from([0x61, 0xc3]), 1);
		await expect(readTextInPages(truncated.readPage, stable)).rejects.toThrow(/valid UTF-8/i);
	});

	// Catches: a cancelled load keeps pulling pages after the tab switched to another file.
	it("stops requesting pages once cancelled", async () => {
		let cancelled = false;
		const { readPage, offsets } = pagesOf(new TextEncoder().encode("abcdefgh"), 2);
		const reading = readTextInPages(
			async (offset) => {
				const page = await readPage(offset);
				if (offset === 2) cancelled = true;
				return page;
			},
			{ ...stable, isCancelled: () => cancelled },
		);
		await expect(reading).rejects.toBeInstanceOf(PagedReadCancelled);
		expect(offsets).toEqual([0, 2]);
	});

	// Catches: pages from before and after a truncating rewrite are stitched into one document.
	it("rejects a file that shrinks during the read", async () => {
		const first = new TextEncoder().encode("abcdef");
		const rewritten = new TextEncoder().encode("abcd");
		const shrinking = async (offset: number) =>
			(await pagesOf(offset === 0 ? first : rewritten, 2).readPage(offset)) as FilePage;
		await expect(readTextInPages(shrinking, stable)).rejects.toThrow(/changed on disk/);
	});

	// Catches: a page that answers a different offset or makes no progress spins forever.
	it("rejects pages that do not advance at the requested offset", async () => {
		const stalled = async (offset: number): Promise<FilePage> => ({
			data: "",
			offset,
			bytesRead: 0,
			totalSize: 4,
			hasMore: true,
		});
		await expect(readTextInPages(stalled, stable)).rejects.toThrow(/changed on disk/);
		const shifted = pagesOf(new TextEncoder().encode("abcd"), 2);
		await expect(
			readTextInPages(async (offset) => ({ ...(await shifted.readPage(offset)), offset: offset + 1 }), stable),
		).rejects.toThrow(/changed on disk/);
	});
});

describe("readTextInPages byte fidelity", () => {
	// Catches: the paged decoder strips a UTF-8 BOM that the whole read
	// (Rust read_to_string) keeps, so saving a file opened in pages silently
	// rewrites it without its first three bytes.
	it("keeps a leading UTF-8 byte order mark like the whole-file read", async () => {
		const bytes = new Uint8Array([0xef, 0xbb, 0xbf, 0x61, 0x62]);
		const page: FilePage = {
			data: btoa(String.fromCharCode(...bytes)),
			offset: 0,
			bytesRead: bytes.length,
			totalSize: bytes.length,
			hasMore: false,
		};
		const text = await readTextInPages(async () => page, { isCancelled: () => false });
		expect(text).toBe("\uFEFFab");
	});
});

describe("readTextInPages on a log that grows while it loads", () => {
	// catches: an append-only file (an agent or server log) is rejected as "changed",
	// so the phone can never open an active large log that the desktop editor opens.
	it("returns the content instead of rejecting when the file only grew", async () => {
		const encoder = new TextEncoder();
		const original = "line 1\nline 2\nline 3\n";
		let bytes = encoder.encode(original);
		let reads = 0;
		const readPage = async (offset: number): Promise<FilePage> => {
			reads += 1;
			const slice = bytes.slice(offset, offset + 8);
			const page: FilePage = {
				data: btoa(String.fromCharCode(...slice)),
				offset,
				bytesRead: slice.length,
				totalSize: bytes.length,
				hasMore: offset + slice.length < bytes.length,
			};
			// The log writer appends one line after the first page was served.
			if (reads === 1) bytes = encoder.encode(`${original}line 4\n`);
			return page;
		};
		const text = await readTextInPages(readPage, { isCancelled: () => false });

		expect(text.startsWith(original)).toBe(true);
	});
});
