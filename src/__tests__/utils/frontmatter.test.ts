import { describe, it, expect } from "vitest";
import { stripFrontmatter, extractPlanMetadata } from "../../utils/frontmatter";

describe("stripFrontmatter", () => {
  it("strips valid frontmatter and returns body content", () => {
    const input = `---
title: My Plan
status: in-progress
---
# Content`;
    expect(stripFrontmatter(input)).toBe("# Content");
  });

  it("returns full content unchanged when no frontmatter", () => {
    const input = "# Just Content\nNo frontmatter here";
    expect(stripFrontmatter(input)).toBe(input);
  });

  it("handles empty file", () => {
    expect(stripFrontmatter("")).toBe("");
  });

  it("strips frontmatter with boolean values", () => {
    const input = `---
draft: true
published: false
---
Body`;
    expect(stripFrontmatter(input)).toBe("Body");
  });

  it("strips frontmatter with numeric values", () => {
    const input = `---
priority: 3
score: 95.5
---
Body`;
    expect(stripFrontmatter(input)).toBe("Body");
  });

  it("strips frontmatter with quoted string values", () => {
    const input = `---
title: "My Plan: A Journey"
subtitle: 'Single quoted'
---
Body`;
    expect(stripFrontmatter(input)).toBe("Body");
  });

  it("strips frontmatter with empty values", () => {
    const input = `---
title:
status:
---
Body`;
    expect(stripFrontmatter(input)).toBe("Body");
  });

  it("strips frontmatter that contains comment lines", () => {
    const input = `---
# This is a comment
title: My Plan
---
Body`;
    expect(stripFrontmatter(input)).toBe("Body");
  });

  it("handles extra --- delimiters in content body", () => {
    const input = `---
title: Plan
---
# Content
---
This is a horizontal rule, not frontmatter
---
More content`;
    const result = stripFrontmatter(input);
    expect(result).toContain("This is a horizontal rule");
    expect(result).toContain("More content");
  });

  it("returns full input for malformed frontmatter (no closing ---)", () => {
    const input = `---
title: Plan
# Content starts without closing fence`;
    expect(stripFrontmatter(input)).toBe(input);
  });

  it("returns full input when frontmatter is not at start of file", () => {
    const input = `Some text before
---
title: Plan
---
Body`;
    expect(stripFrontmatter(input)).toBe(input);
  });

  it("preserves blank line between frontmatter and content", () => {
    const input = `---
title: Plan
---

# Content with leading blank line`;
    expect(stripFrontmatter(input)).toBe("\n# Content with leading blank line");
  });

  it("strips frontmatter with keys that have underscores and hyphens", () => {
    const input = `---
estimated_effort: M
my-key: value
---
Body`;
    expect(stripFrontmatter(input)).toBe("Body");
  });

  it("strips frontmatter with date-like string values", () => {
    const input = `---
created: 2026-03-01
---
Body`;
    expect(stripFrontmatter(input)).toBe("Body");
  });
});

// ---------------------------------------------------------------------------
// extractPlanMetadata
// ---------------------------------------------------------------------------

describe("extractPlanMetadata", () => {
  it("extracts H1 title from markdown", () => {
    const input = "# Implementation Plan: Auto-delete branch on PR close\n\n**Status:** Draft";
    const meta = extractPlanMetadata(input);
    expect(meta.title).toBe("Auto-delete branch on PR close");
  });

  it("strips common plan title prefixes", () => {
    const input = "# Implementation Plan: My Feature\n\nBody";
    expect(extractPlanMetadata(input).title).toBe("My Feature");
  });

  it("strips 'Plan:' prefix", () => {
    const input = "# Plan: Something Cool\n\nBody";
    expect(extractPlanMetadata(input).title).toBe("Something Cool");
  });

  it("keeps title as-is when no prefix matches", () => {
    const input = "# Frontend Performance Optimization Plan\n\nBody";
    expect(extractPlanMetadata(input).title).toBe("Frontend Performance Optimization Plan");
  });

  it("returns null title when no H1 found", () => {
    const input = "No heading here\n\nJust body text";
    expect(extractPlanMetadata(input).title).toBeNull();
  });

  it("extracts status from inline bold markdown", () => {
    const input = "# Plan\n\n**Status:** In Progress\n**Effort:** M";
    const meta = extractPlanMetadata(input);
    expect(meta.status).toBe("In Progress");
  });

  it("extracts status from YAML frontmatter (takes precedence)", () => {
    const input = "---\nstatus: completed\n---\n# Plan\n\n**Status:** Draft";
    const meta = extractPlanMetadata(input);
    expect(meta.status).toBe("completed");
  });

  it("extracts effort from inline bold markdown", () => {
    const input = "# Plan\n\n**Estimated Effort:** L-XL";
    const meta = extractPlanMetadata(input);
    expect(meta.effort).toBe("L-XL");
  });

  it("extracts priority from inline bold markdown", () => {
    const input = "# Plan\n\n**Priority:** P1";
    const meta = extractPlanMetadata(input);
    expect(meta.priority).toBe("P1");
  });

  it("extracts story from inline bold markdown", () => {
    const input = "# Plan\n\n**Story:** 420-e0ea";
    const meta = extractPlanMetadata(input);
    expect(meta.story).toBe("420-e0ea");
  });

  it("extracts created date from inline bold markdown", () => {
    const input = "# Plan\n\n**Created:** 2026-02-28";
    const meta = extractPlanMetadata(input);
    expect(meta.created).toBe("2026-02-28");
  });

  it("extracts created date from YAML frontmatter", () => {
    const input = "---\ncreated: 2026-02-28\n---\n# Plan";
    const meta = extractPlanMetadata(input);
    expect(meta.created).toBe("2026-02-28");
  });

  it("handles a full plan file with both frontmatter and inline metadata", () => {
    const input = `---
status: completed
created: 2026-02-27
completed_at: "2026-02-28T15:46:40.969Z"
---
# Implementation Plan: Auto-delete local branch when PR merged/closed

**Created:** 2026-02-27
**Status:** Draft
**Story:** 420-e0ea
**Estimated Effort:** M

## Summary

When the GitHub polling loop detects a PR...`;
    const meta = extractPlanMetadata(input);
    expect(meta.title).toBe("Auto-delete local branch when PR merged/closed");
    expect(meta.status).toBe("completed"); // frontmatter wins
    expect(meta.effort).toBe("M");
    expect(meta.story).toBe("420-e0ea");
    expect(meta.created).toBe("2026-02-27"); // frontmatter wins over inline
  });

  it("returns all null for empty input", () => {
    const meta = extractPlanMetadata("");
    expect(meta.title).toBeNull();
    expect(meta.status).toBeNull();
    expect(meta.effort).toBeNull();
    expect(meta.priority).toBeNull();
    expect(meta.story).toBeNull();
    expect(meta.created).toBeNull();
  });

  it("handles file with only frontmatter, no body", () => {
    const input = "---\nstatus: draft\n---\n";
    const meta = extractPlanMetadata(input);
    expect(meta.status).toBe("draft");
    expect(meta.title).toBeNull();
  });

  it("parses boolean frontmatter values via extractPlanMetadata", () => {
    const input = `---
status: true
---
Body`;
    // boolean true coerced to string "true" by stringOrNull
    expect(extractPlanMetadata(input).status).toBe("true");
  });

  it("parses numeric frontmatter values via extractPlanMetadata", () => {
    const input = `---
created: 2026
---
Body`;
    // numeric 2026 coerced to string "2026"
    expect(extractPlanMetadata(input).created).toBe("2026");
  });

  it("does not parse date strings as special types", () => {
    const input = `---
created: 2026-03-01
---
Body`;
    // Should remain a string, not a Date object
    expect(extractPlanMetadata(input).created).toBe("2026-03-01");
  });
});
