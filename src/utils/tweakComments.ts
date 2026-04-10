/**
 * Inline review comments for markdown files.
 *
 * Comments are stored directly inside the .md source as HTML comments,
 * which are invisible to any markdown renderer but readable by humans and LLMs:
 *
 *     prefix <!--tweak:begin:ID-->highlighted text<!--tweak:end:ID @<ISO-TIMESTAMP>
 *     comment body, free text, may span multiple lines
 *     --> suffix
 *
 * The body is plain text. The only forbidden sequence is `-->` (which would
 * close the enclosing HTML comment prematurely); it is escaped to `--&gt;`
 * on write and restored on read. No other escaping is performed — quotes,
 * newlines, unicode, `<`, `&` are all kept verbatim.
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
  "     Format: [tweak:begin:ID]highlighted text[tweak:end:ID @ISO-TIMESTAMP\n" +
  "     comment body (free text, may span multiple lines)\n" +
  "     ] — where [ ] are the HTML comment delimiters <!-- -->.\n" +
  "     The only escape is '-->' → '--&gt;' inside the comment body.\n" +
  "     Read each comment, apply the feedback to the highlighted text,\n" +
  "     then remove the tweak markers. -->\n\n";

// Matches a full tweak comment span: begin marker, highlighted content,
// end marker with timestamp + body. Lazy matching is safe because the body
// cannot contain `-->` (escaped at write time).
const FULL_RE =
  /<!--tweak:begin:([A-Za-z0-9_-]+)-->([\s\S]*?)<!--tweak:end:\1 @(\S+)\s([\s\S]*?)-->/g;

/** Escape the only sequence that would break the enclosing HTML comment. */
function escapeBody(body: string): string {
  return body.replace(/-->/g, "--&gt;");
}

/** Reverse escapeBody. */
function unescapeBody(body: string): string {
  return body.replace(/--&gt;/g, "-->");
}

/** Serialize a comment into its inline marker form (does not insert into source). */
export function serializeTweakComment(c: TweakComment): string {
  return `<!--tweak:begin:${c.id}-->${c.highlighted}<!--tweak:end:${c.id} @${c.createdAt}\n${escapeBody(c.comment)}-->`;
}

/** Parse all tweak comments from a markdown source, in document order. */
export function parseTweakComments(source: string): TweakComment[] {
  const results: TweakComment[] = [];
  FULL_RE.lastIndex = 0;
  let match: RegExpExecArray | null;
  while ((match = FULL_RE.exec(source)) !== null) {
    const [, id, highlighted, createdAt, body] = match;
    results.push({
      id,
      highlighted,
      comment: unescapeBody(body),
      createdAt,
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
    let end = trimmed.length;
    while (end < source.length && (source[end] === "\n" || source[end] === "\r")) end++;
    return source.slice(end);
  }
  return source;
}

/**
 * Insert a tweak comment into the source by wrapping the first occurrence
 * of `highlighted` with begin/end markers. Throws if the text is not found.
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
  const escapedId = id.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const re = new RegExp(
    `<!--tweak:begin:${escapedId}-->([\\s\\S]*?)<!--tweak:end:${escapedId} @\\S+\\s[\\s\\S]*?-->`,
    "g",
  );
  let changed = false;
  const out = source.replace(re, (_, highlighted) => {
    changed = true;
    return highlighted;
  });
  if (!changed) return source;
  if (parseTweakComments(out).length === 0) {
    return removeConventionHeader(out);
  }
  return out;
}

/** Update the comment text of an existing tweak comment, preserving its id and highlighted text. */
export function updateTweakComment(source: string, id: string, newComment: string): string {
  const escapedId = id.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const re = new RegExp(
    `<!--tweak:begin:${escapedId}-->([\\s\\S]*?)<!--tweak:end:${escapedId} @(\\S+)\\s[\\s\\S]*?-->`,
    "g",
  );
  return source.replace(re, (_, highlighted, createdAt) =>
    serializeTweakComment({ id, highlighted, comment: newComment, createdAt }),
  );
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
  let out = source.startsWith(CONVENTION_HEADER)
    ? source.slice(CONVENTION_HEADER.length)
    : source;

  // Replace each tweak marker pair with a highlight span.
  // The comment body is already free of `-->`; for the attribute context we
  // additionally escape `&` and `"` so the attribute parses correctly. The
  // browser auto-decodes these when we read via `dataset.tweakComment`.
  FULL_RE.lastIndex = 0;
  out = out.replace(FULL_RE, (_, id, highlighted, createdAt, body) => {
    const plain = unescapeBody(body);
    const attrComment = plain.replace(/&/g, "&amp;").replace(/"/g, "&quot;");
    return `<span class="tweak-highlight" data-tweak-id="${id}" data-tweak-at="${createdAt}" data-tweak-comment="${attrComment}">${highlighted}</span>`;
  });
  return out;
}

/** Generate a unique id for a new comment (short, URL-safe). */
export function generateTweakCommentId(): string {
  const rand = Math.random().toString(36).slice(2, 8);
  const ts = Date.now().toString(36);
  return `c_${ts}${rand}`;
}

// ---- GFM Task-List Checkbox Toggle ----


/**
 * Set the checkbox on the given source line to the specified mark.
 * `sourceLine` is the 0-based line number in the raw markdown source,
 * injected as `data-source-line` by the MarkdownRenderer preprocessor.
 * `mark` is one of: `" "` (unchecked), `"x"` (checked), `"~"` (in-progress).
 */
export function toggleCheckbox(source: string, sourceLine: number, mark: " " | "x" | "~"): string {
  const lines = source.split("\n");
  if (sourceLine < 0 || sourceLine >= lines.length) return source;
  const line = lines[sourceLine];
  const m = /^(\s*[-*+]\s+)\[([ xX~])\]/.exec(line);
  if (!m) return source;
  lines[sourceLine] = `${m[1]}[${mark}]${line.slice(m[0].length)}`;
  return lines.join("\n");
}
