import { createSignal } from "solid-js";
import maxImageBytes from "../../shared/acp-image-limit.json";
import type { AcpContentBlock } from "../../types/acp";

export const MAX_PASTED_IMAGE_BYTES = maxImageBytes;
const IMAGE_TYPES = new Set(["image/png", "image/jpeg", "image/gif", "image/webp"]);

export interface StagedImage {
	src: string;
	size: number;
	block: Extract<AcpContentBlock, { type: "image" }>;
}

export interface StagedFile {
	name: string;
	path: string;
}

/**
 * What the person has typed but not sent yet.
 *
 * Module scope rather than component state so the panel can be closed and
 * reopened without losing a half-written question, and so something outside the
 * panel — the terminal context menu — can hand it a selection to ask about.
 */
const [text, setText] = createSignal("");
const [images, setImages] = createSignal<StagedImage[]>([]);
const [files, setFiles] = createSignal<StagedFile[]>([]);
interface DraftContent {
	text: string;
	images: StagedImage[];
	files: StagedFile[];
}
interface StoredDraft extends DraftContent {
	pastes: [string, string][];
}
const drafts = new Map<string, DraftContent>();
const [parked, setParked] = createSignal<DraftContent | null>(null);
const [storageError, setStorageError] = createSignal(false);
const parkedDrafts = new Map<string, DraftContent>();
const pastedText = new Map<string, string>();
const PARKED_DB = "tuic-ai-chat-parked-drafts";
const PARKED_STORE = "drafts";
let database: Promise<IDBDatabase | null> | undefined;
let activeSession = "";
let revision = 0;
let pendingBytes = 0;
let pasteNumber = 0;

function openDatabase(): Promise<IDBDatabase | null> {
	if (database) return database;
	if (typeof indexedDB === "undefined") return Promise.resolve(null);
	database = new Promise((resolve) => {
		try {
			const request = indexedDB.open(PARKED_DB, 1);
			request.onupgradeneeded = () => request.result.createObjectStore(PARKED_STORE);
			request.onsuccess = () => resolve(request.result);
			request.onerror = () => resolve(null);
			request.onblocked = () => resolve(null);
		} catch {
			resolve(null);
		}
	});
	return database;
}

function persistParked(session: string, draft: DraftContent | null): void {
	void openDatabase().then((db) => {
		if (!db) {
			if (draft) setStorageError(true);
			return;
		}
		try {
			const transaction = db.transaction(PARKED_STORE, "readwrite");
			transaction.oncomplete = () => setStorageError(false);
			transaction.onerror = () => setStorageError(true);
			transaction.onabort = () => setStorageError(true);
			const store = transaction.objectStore(PARKED_STORE);
			if (draft) {
				const pastes = [...pastedText].filter(([marker]) => draft.text.includes(marker));
				store.put({ ...draft, pastes } satisfies StoredDraft, session);
			} else {
				store.delete(session);
			}
		} catch {
			setStorageError(true);
		}
	});
}

function hydrateParked(session: string, atRevision: number): void {
	void openDatabase().then((db) => {
		if (!db) return;
		const request = db.transaction(PARKED_STORE, "readonly").objectStore(PARKED_STORE).get(session);
		request.onsuccess = () => {
			const saved = request.result as StoredDraft | undefined;
			if (!saved || activeSession !== session || revision !== atRevision || parked()) return;
			for (const [marker, value] of saved.pastes) {
				pastedText.set(marker, value);
				pasteNumber = Math.max(pasteNumber, Number(marker.match(/#(\d+)/)?.[1] ?? 0));
			}
			setParked({ text: saved.text, images: saved.images, files: saved.files ?? [] });
			parkedDrafts.set(session, { text: saved.text, images: saved.images, files: saved.files ?? [] });
		};
	});
}

function readImage(file: File): Promise<string> {
	return new Promise((resolve, reject) => {
		const reader = new FileReader();
		reader.onload = () => resolve(String(reader.result));
		reader.onerror = () => reject(reader.error ?? new Error("Could not read image"));
		reader.readAsDataURL(file);
	});
}

export const aiChatDraft = {
	text,
	set: setText,
	expandedText(): string {
		return text().replace(/\[Pasted text #\d+ \+\d+ words\]/g, (marker) => pastedText.get(marker) ?? marker);
	},
	stageTextPaste(value: string, start: number, end: number): number | null {
		const words = value.trim().split(/\s+/).filter(Boolean).length;
		if (words <= 200) return null;
		const marker = `[Pasted text #${++pasteNumber} +${words} words]`;
		pastedText.set(marker, value);
		setText((current) => current.slice(0, start) + marker + current.slice(end));
		return start + marker.length;
	},
	images,
	files,
	parked,
	storageError,
	activate(session: string): void {
		if (session === activeSession) {
			if (!session && !parked()) hydrateParked(session, revision);
			return;
		}
		const previousSession = activeSession;
		const previous = { text: text(), images: images(), files: files() };
		if (activeSession) drafts.set(activeSession, previous);
		const next = drafts.get(session) ?? (activeSession === "" && session ? previous : undefined);
		const nextParked = parkedDrafts.get(session) ?? (activeSession === "" && session ? parked() : null);
		activeSession = session;
		setText(next?.text ?? "");
		setImages(next?.images ?? []);
		setFiles(next?.files ?? []);
		setParked(nextParked);
		revision += 1;
		if (!nextParked) hydrateParked(session, revision);
		if (!previousSession && session && nextParked) {
			persistParked(session, nextParked);
			persistParked("", null);
		}
	},
	parkOrSwap(): void {
		const current = { text: text(), images: images(), files: files() };
		const previous = parked();
		if (!previous && !current.text.trim() && current.images.length === 0 && current.files.length === 0) return;
		setText(previous?.text ?? "");
		setImages(previous?.images ?? []);
		setFiles(previous?.files ?? []);
		setParked(
			previous && (current.text.trim() || current.images.length || current.files.length)
				? current
				: previous
					? null
					: current,
		);
		revision += 1;
		if (activeSession) {
			const saved = parked();
			if (saved) parkedDrafts.set(activeSession, saved);
			else parkedDrafts.delete(activeSession);
		}
		persistParked(activeSession, parked());
	},
	restoreAfterSend(): void {
		const saved = parked();
		this.clear();
		if (!saved) return;
		setText(saved.text);
		setImages(saved.images);
		setFiles(saved.files);
		setParked(null);
		setStorageError(false);
		parkedDrafts.delete(activeSession);
		persistParked(activeSession, null);
	},

	/** Keep async startup bound to its draft, allowing the first session handoff. */
	captureOwnership(): (session: string) => boolean {
		const atRevision = revision;
		const session = activeSession;
		return (startedSession) =>
			revision === atRevision ||
			(!!startedSession && !session && revision === atRevision + 1 && activeSession === startedSession);
	},

	/** Validate bytes before FileReader expands them into base64. */
	async stageImage(file: File, supported: boolean): Promise<string | null> {
		const stagedAt = revision;
		if (!supported) return "This agent does not support images.";
		if (!IMAGE_TYPES.has(file.type)) return `Unsupported image type: ${file.type || "unknown"}.`;
		if (file.size > MAX_PASTED_IMAGE_BYTES) {
			return `Image is ${(file.size / (1024 * 1024)).toFixed(1)} MiB; the limit is ${MAX_PASTED_IMAGE_BYTES / (1024 * 1024)} MiB.`;
		}
		if (images().reduce((sum, image) => sum + image.size, pendingBytes) + file.size > MAX_PASTED_IMAGE_BYTES) {
			return `Image is ${(file.size / (1024 * 1024)).toFixed(1)} MiB; the total limit is ${MAX_PASTED_IMAGE_BYTES / (1024 * 1024)} MiB.`;
		}
		pendingBytes += file.size;
		try {
			const src = await readImage(file);
			if (stagedAt !== revision) return null;
			const data = src.slice(src.indexOf(",") + 1);
			setImages((current) => [
				...current,
				{ src, size: file.size, block: { type: "image", mimeType: file.type, data } },
			]);
			return null;
		} catch {
			return "Could not read image from clipboard.";
		} finally {
			pendingBytes -= file.size;
		}
	},

	removeImage(image: StagedImage): void {
		setImages((current) => current.filter((item) => item !== image));
	},

	stageFile(file: StagedFile): void {
		setFiles((current) => [...current, file]);
	},

	removeFile(file: StagedFile): void {
		setFiles((current) => current.filter((item) => item !== file));
	},

	/** Add text to the draft and leave the cursor after it. */
	append(addition: string): void {
		const current = text();
		setText(current ? `${current.replace(/\s+$/, "")}\n\n${addition}` : addition);
	},

	clear(): void {
		revision += 1;
		for (const marker of text().match(/\[Pasted text #\d+ \+\d+ words\]/g) ?? []) pastedText.delete(marker);
		setText("");
		setImages([]);
		setFiles([]);
		drafts.delete(activeSession);
	},

	reset(): void {
		drafts.clear();
		parkedDrafts.clear();
		setParked(null);
		setStorageError(false);
		pastedText.clear();
		pasteNumber = 0;
		activeSession = "";
		revision += 1;
		setText("");
		setImages([]);
		setFiles([]);
		void openDatabase().then((db) => {
			if (db) db.transaction(PARKED_STORE, "readwrite").objectStore(PARKED_STORE).clear();
		});
	},
};
