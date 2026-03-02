/**
 * Lightweight YAML frontmatter parser for plan files.
 *
 * Handles flat key-value pairs only (no nested structures, arrays, or multiline values).
 * Zero dependencies — plan frontmatter is intentionally simple.
 */

export interface FrontmatterResult {
  /** Parsed key-value pairs from the frontmatter block */
  data: Record<string, string | number | boolean>;
  /** Markdown content after the frontmatter block (or full content if no frontmatter) */
  content: string;
}

/**
 * Parse YAML frontmatter from a markdown string.
 *
 * Frontmatter must start at the very first line with `---` and end with a
 * matching `---`. Content before the first `---` or missing closing `---`
 * results in no frontmatter being parsed.
 */
export function parseFrontmatter(input: string): FrontmatterResult {
  if (!input.startsWith("---")) {
    return { data: {}, content: input };
  }

  // Find the closing --- (skip the opening one)
  const closingIdx = input.indexOf("\n---", 3);
  if (closingIdx < 0) {
    return { data: {}, content: input };
  }

  const frontmatterBlock = input.slice(4, closingIdx); // skip "---\n"
  // Skip "\n---" and strip exactly one leading newline from content
  const rawContent = input.slice(closingIdx + 4);
  const content = rawContent.startsWith("\n") ? rawContent.slice(1) : rawContent;

  const data: Record<string, string | number | boolean> = {};

  for (const line of frontmatterBlock.split("\n")) {
    const trimmed = line.trim();
    // Skip empty lines and comments
    if (!trimmed || trimmed.startsWith("#")) continue;

    const colonIdx = trimmed.indexOf(":");
    if (colonIdx < 0) continue;

    const key = trimmed.slice(0, colonIdx).trim();
    const rawValue = trimmed.slice(colonIdx + 1).trim();

    data[key] = parseValue(rawValue);
  }

  return { data, content };
}

/** Coerce a raw string value to the appropriate JS type. */
function parseValue(raw: string): string | number | boolean {
  if (raw === "") return "";
  if (raw === "true") return true;
  if (raw === "false") return false;

  // Quoted strings — strip quotes
  if ((raw.startsWith('"') && raw.endsWith('"')) || (raw.startsWith("'") && raw.endsWith("'"))) {
    return raw.slice(1, -1);
  }

  // Numbers (integers and floats, but not date-like strings)
  if (/^-?\d+(\.\d+)?$/.test(raw)) {
    return Number(raw);
  }

  return raw;
}
