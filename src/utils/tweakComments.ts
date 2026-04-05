/**
 * Inline review comments for markdown files.
 *
 * Comments are stored directly inside the .md source as HTML comments,
 * which are invisible to any markdown renderer but readable by humans and LLMs:
 *
 *     prefix <!--tweak:begin:ID-->highlighted text<!--tweak:end:ID:BASE64_JSON--> suffix
 *
 * BASE64_JSON = btoa(JSON.stringify({ comment, created_at }))
 *
 * The first time a comment is added to a file, a convention header is prepended
 * so that any LLM reading the file understands the format without external context.
 */

export interface TweakComment {
  id: string;
  highlighted: string;
  comment: string;
  createdAt: string;
}

export const CONVENTION_HEADER =
  "<!-- tweak-comments v1: inline review comments.\n" +
  "     Format: <!--tweak:begin:ID-->highlighted text<!--tweak:end:ID:BASE64_JSON-->\n" +
  "     BASE64_JSON decodes to {\"comment\":\"...\",\"created_at\":\"...\"}.\n" +
  "     Read each comment, evaluate the feedback against the highlighted text,\n" +
  "     apply the requested changes, then remove the tweak markers. -->\n\n";

// Matches a full tweak comment span: begin marker, highlighted content, end marker with payload.
// [\s\S]*? = non-greedy across newlines (highlighted text is single-block but may contain inline formatting).
const FULL_RE = /<!--tweak:begin:([A-Za-z0-9_-]+)-->([\s\S]*?)<!--tweak:end:\1:([A-Za-z0-9+/=]+)-->/g;

interface Payload {
  comment: string;
  created_at: string;
}

function decodePayload(b64: string): Payload | null {
  try {
    const json = atob(b64);
    const parsed = JSON.parse(json);
    if (typeof parsed?.comment !== "string" || typeof parsed?.created_at !== "string") {
      return null;
    }
    return parsed as Payload;
  } catch {
    return null;
  }
}

function encodePayload(comment: string, createdAt: string): string {
  return btoa(JSON.stringify({ comment, created_at: createdAt }));
}

/** Serialize a comment into its inline marker form (does not insert into source). */
export function serializeTweakComment(c: TweakComment): string {
  const payload = encodePayload(c.comment, c.createdAt);
  return `<!--tweak:begin:${c.id}-->${c.highlighted}<!--tweak:end:${c.id}:${payload}-->`;
}

/** Parse all tweak comments from a markdown source, in document order. */
export function parseTweakComments(source: string): TweakComment[] {
  const results: TweakComment[] = [];
  // Reset regex state (global regex keeps lastIndex between calls).
  FULL_RE.lastIndex = 0;
  let match: RegExpExecArray | null;
  while ((match = FULL_RE.exec(source)) !== null) {
    const [, id, highlighted, b64] = match;
    const payload = decodePayload(b64);
    if (!payload) continue;
    results.push({
      id,
      highlighted,
      comment: payload.comment,
      createdAt: payload.created_at,
    });
  }
  return results;
}

/** Prepend the convention header if not already present. */
export function ensureConventionHeader(source: string): string {
  if (source.startsWith(CONVENTION_HEADER)) return source;
  // Also treat a stripped (whitespace-trimmed) match as present to be resilient
  // to trailing-newline normalization by editors.
  if (source.includes(CONVENTION_HEADER.trimEnd())) return source;
  return CONVENTION_HEADER + source;
}

/** Remove the convention header if present (used when last comment is removed). */
function removeConventionHeader(source: string): string {
  if (source.startsWith(CONVENTION_HEADER)) {
    return source.slice(CONVENTION_HEADER.length);
  }
  const trimmed = CONVENTION_HEADER.trimEnd();
  const idx = source.indexOf(trimmed);
  if (idx === 0) {
    // Remove header + any trailing whitespace up to the next non-blank line.
    let end = trimmed.length;
    while (end < source.length && (source[end] === "\n" || source[end] === "\r")) end++;
    return source.slice(end);
  }
  return source;
}

/**
 * Insert a tweak comment into the source by wrapping the first occurrence
 * of `highlighted` with begin/end markers. Throws if the text is not found.
 *
 * The caller is responsible for passing a `highlighted` string that matches
 * verbatim in the source — which is guaranteed when the highlight comes from
 * a live text selection on the rendered DOM (the selection text exists in
 * the source too, barring inline markdown syntax that we avoid by scoping
 * selections to plain-text ranges — see the UI layer).
 */
export function insertTweakComment(source: string, comment: TweakComment): string {
  const idx = source.indexOf(comment.highlighted);
  if (idx === -1) {
    throw new Error(
      `insertTweakComment: highlighted text not found in source: "${comment.highlighted.slice(0, 40)}..."`,
    );
  }
  const wrapped = serializeTweakComment(comment);
  const replaced =
    source.slice(0, idx) + wrapped + source.slice(idx + comment.highlighted.length);
  return ensureConventionHeader(replaced);
}

/** Remove a comment by id, keeping the highlighted text in place. */
export function removeTweakComment(source: string, id: string): string {
  // Escape id for regex (ids are alphanumeric+_- so this is defensive).
  const escapedId = id.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const re = new RegExp(
    `<!--tweak:begin:${escapedId}-->([\\s\\S]*?)<!--tweak:end:${escapedId}:[A-Za-z0-9+/=]+-->`,
    "g",
  );
  let changed = false;
  const out = source.replace(re, (_, highlighted) => {
    changed = true;
    return highlighted;
  });
  if (!changed) return source;
  // If no comments remain, strip the convention header too.
  if (parseTweakComments(out).length === 0) {
    return removeConventionHeader(out);
  }
  return out;
}

/** Update the comment text of an existing tweak comment, preserving its id and highlighted text. */
export function updateTweakComment(source: string, id: string, newComment: string): string {
  const escapedId = id.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const re = new RegExp(
    `<!--tweak:begin:${escapedId}-->([\\s\\S]*?)<!--tweak:end:${escapedId}:([A-Za-z0-9+/=]+)-->`,
    "g",
  );
  return source.replace(re, (_, highlighted, b64) => {
    const existing = decodePayload(b64);
    const createdAt = existing?.created_at ?? new Date().toISOString();
    return serializeTweakComment({ id, highlighted, comment: newComment, createdAt });
  });
}

/**
 * Pre-process markdown source before passing to `marked`:
 * converts tweak markers into `<span>` HTML so they survive the markdown
 * pipeline and are available in the rendered DOM for highlight styling
 * and click interactions.
 *
 * The convention header (an HTML comment) is stripped at this stage so
 * it never appears in rendered output.
 */
export function applyTweakHighlights(source: string): string {
  // Remove the convention header — it's for humans/LLMs reading the raw file,
  // not for the rendered view.
  let out = source.startsWith(CONVENTION_HEADER)
    ? source.slice(CONVENTION_HEADER.length)
    : source;

  // Replace each tweak marker pair with a highlight span.
  // The data-tweak-comment-b64 attribute carries the payload for the click handler.
  FULL_RE.lastIndex = 0;
  out = out.replace(
    FULL_RE,
    (_, id, highlighted, b64) =>
      `<span class="tweak-highlight" data-tweak-id="${id}" data-tweak-comment-b64="${b64}">${highlighted}</span>`,
  );
  return out;
}

/** Generate a unique id for a new comment (short, URL-safe). */
export function generateTweakCommentId(): string {
  const rand = Math.random().toString(36).slice(2, 8);
  const ts = Date.now().toString(36);
  return `c_${ts}${rand}`;
}
