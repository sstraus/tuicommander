import { filePreviewRegistry } from "../plugins/filePreviewRegistry";
import { diffTabsStore } from "../stores/diffTabs";
import { editorTabsStore } from "../stores/editorTabs";
import { mdTabsStore } from "../stores/mdTabs";
import { terminalsStore } from "../stores/terminals";

/** Classification of how a file should be opened in the UI. */
export type FileOpenTarget = "markdown" | "preview" | "editor";

const MD_EXTS = new Set(["md", "mdx"]);
const PREVIEW_EXTS = new Set([
	// Documents
	"pdf",
	"html",
	"htm",
	// Images
	"png",
	"jpg",
	"jpeg",
	"gif",
	"webp",
	"svg",
	"avif",
	"ico",
	"bmp",
	// Video
	"mp4",
	"webm",
	"mov",
	"ogg",
	// Audio
	"mp3",
	"wav",
	"flac",
	"aac",
	"m4a",
]);

const IMAGE_EXTS = new Set(["png", "jpg", "jpeg", "gif", "webp", "avif", "bmp", "tiff", "tif"]);

/** Whether a file path has an image extension suitable for OSC 1337 inline display. */
export function isImageFile(filePath: string): boolean {
	return IMAGE_EXTS.has(extOf(filePath));
}

/** Extract lowercase extension from a file path (without the dot). */
function extOf(filePath: string): string {
	const dot = filePath.lastIndexOf(".");
	return dot === -1 ? "" : filePath.slice(dot + 1).toLowerCase();
}

/** Classify how a file should be opened based on its extension. */
export function classifyFile(filePath: string): FileOpenTarget {
	const ext = extOf(filePath);
	if (MD_EXTS.has(ext)) return "markdown";
	if (PREVIEW_EXTS.has(ext)) return "preview";
	return "editor";
}

/**
 * Open a file using the best available handler:
 * 1. Plugin file preview (if registered and no line target)
 * 2. Markdown tab for .md/.mdx
 * 3. HTML preview for media/document files
 * 4. CodeMirror editor (fallback)
 */
export function openFileAction(
	filePath: string,
	repoPath: string,
	fsRoot?: string,
	line?: number,
	onEditorTab?: (tabId: string) => void,
): void {
	if (line === undefined) {
		const handler = filePreviewRegistry.getHandler(filePath);
		if (handler) {
			handler.onOpen({ filePath, repoPath, fsRoot: fsRoot || repoPath });
			return;
		}
	}

	const target = classifyFile(filePath);
	if (target === "markdown" && line === undefined) {
		mdTabsStore.add(repoPath, filePath, fsRoot);
		terminalsStore.setActive(null);
		diffTabsStore.setActive(null);
		editorTabsStore.setActive(null);
	} else if (target === "preview" && line === undefined) {
		mdTabsStore.addHtmlPreview(repoPath, filePath, fsRoot);
		terminalsStore.setActive(null);
		diffTabsStore.setActive(null);
		editorTabsStore.setActive(null);
	} else {
		const tabId = editorTabsStore.add(repoPath, filePath, line, { fsRoot: fsRoot || repoPath });
		terminalsStore.setActive(null);
		diffTabsStore.setActive(null);
		mdTabsStore.setActive(null);
		onEditorTab?.(tabId);
	}
}
