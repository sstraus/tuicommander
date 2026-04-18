/** Classification of how a file should be opened in the UI. */
export type FileOpenTarget = "markdown" | "preview" | "editor";

const MD_EXTS = new Set(["md", "mdx"]);
const PREVIEW_EXTS = new Set([
  // Documents
  "pdf", "html", "htm",
  // Images
  "png", "jpg", "jpeg", "gif", "webp", "svg", "avif", "ico", "bmp",
  // Video
  "mp4", "webm", "mov", "ogg",
  // Audio
  "mp3", "wav", "flac", "aac", "m4a",
]);

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
