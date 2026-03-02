/**
 * Lightweight YAML frontmatter parser for plan files.
 *
 * Handles flat key-value pairs only (no nested structures, arrays, or multiline values).
 * Zero dependencies — plan frontmatter is intentionally simple.
 */

interface FrontmatterResult {
  /** Parsed key-value pairs from the frontmatter block */
  data: Record<string, string | number | boolean>;
  /** Markdown content after the frontmatter block (or full content if no frontmatter) */
  content: string;
}

/**
 * Strip YAML frontmatter from a markdown string and return the body content.
 *
 * Frontmatter must start at the very first line with `---` and end with a
 * matching `---`. Returns the full input unchanged if no valid frontmatter found.
 */
export function stripFrontmatter(input: string): string {
  return parseFrontmatter(input).content;
}

/**
 * Parse YAML frontmatter from a markdown string.
 *
 * Returns parsed key-value data and the body content after the frontmatter block.
 * Frontmatter must start at the very first line with `---` and end with a
 * matching `---`. Content before the first `---` or missing closing `---`
 * results in no frontmatter being parsed.
 */
function parseFrontmatter(input: string): FrontmatterResult {
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

// ---------------------------------------------------------------------------
// Plan metadata extraction
// ---------------------------------------------------------------------------

/** Extracted metadata from a plan file (frontmatter + inline bold markdown). */
export interface PlanMetadata {
  /** H1 title with common prefixes stripped, or null if no H1 found */
  title: string | null;
  /** Plan status (e.g. "Draft", "completed") — frontmatter takes precedence */
  status: string | null;
  /** Estimated effort (e.g. "S", "M", "L-XL") */
  effort: string | null;
  /** Priority (e.g. "P1") */
  priority: string | null;
  /** Story reference (e.g. "420-e0ea") */
  story: string | null;
  /** Created date string (e.g. "2026-02-28") */
  created: string | null;
}

/** Prefixes stripped from H1 titles (case-insensitive, colon-separated). */
const TITLE_PREFIXES = ["Implementation Plan:", "Plan:"];

/**
 * Extract plan metadata from a raw markdown file.
 *
 * Sources (in priority order):
 * 1. YAML frontmatter (status, created) — overrides inline values
 * 2. Inline bold markdown: `**Key:** Value` patterns
 * 3. First H1 heading for the title
 */
export function extractPlanMetadata(input: string): PlanMetadata {
  const result: PlanMetadata = {
    title: null,
    status: null,
    effort: null,
    priority: null,
    story: null,
    created: null,
  };

  if (!input) return result;

  // Parse frontmatter first
  const { data: fm, content } = parseFrontmatter(input);

  // Extract H1 title from the markdown content (or full input if no frontmatter)
  const h1Match = content.match(/^#\s+(.+)$/m);
  if (h1Match) {
    let title = h1Match[1].trim();
    for (const prefix of TITLE_PREFIXES) {
      if (title.toLowerCase().startsWith(prefix.toLowerCase())) {
        title = title.slice(prefix.length).trim();
        break;
      }
    }
    result.title = title;
  }

  // Extract inline bold markdown values: **Key:** Value
  const inlineFields = new Map<string, string>();
  const boldPattern = /^\*\*([^*]+):\*\*\s*(.+)$/gm;
  let match: RegExpExecArray | null;
  while ((match = boldPattern.exec(content)) !== null) {
    inlineFields.set(match[1].trim().toLowerCase(), match[2].trim());
  }

  // Map fields — frontmatter takes precedence over inline
  result.status = stringOrNull(fm.status) ?? inlineFields.get("status") ?? null;
  result.effort = stringOrNull(fm.effort) ?? inlineFields.get("estimated effort") ?? null;
  result.priority = stringOrNull(fm.priority) ?? inlineFields.get("priority") ?? null;
  result.story = stringOrNull(fm.story) ?? inlineFields.get("story") ?? null;
  result.created = stringOrNull(fm.created) ?? inlineFields.get("created") ?? null;

  return result;
}

/** Convert a frontmatter value to string or null. */
function stringOrNull(val: unknown): string | null {
  if (val === undefined || val === null || val === "") return null;
  return String(val);
}

// ---------------------------------------------------------------------------
// Value parsing
// ---------------------------------------------------------------------------

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
