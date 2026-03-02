import { describe, it, expect } from "vitest";
import { parseFrontmatter, extractPlanMetadata } from "../../utils/frontmatter";

describe("parseFrontmatter", () => {
  it("parses valid frontmatter with string values", () => {
    const input = `---
title: My Plan
status: in-progress
---
# Content`;
    const result = parseFrontmatter(input);
    expect(result.data.title).toBe("My Plan");
    expect(result.data.status).toBe("in-progress");
    expect(result.content).toBe("# Content");
  });

  it("returns empty data and full content when no frontmatter", () => {
    const input = "# Just Content\nNo frontmatter here";
    const result = parseFrontmatter(input);
    expect(result.data).toEqual({});
    expect(result.content).toBe(input);
  });

  it("handles empty file", () => {
    const result = parseFrontmatter("");
    expect(result.data).toEqual({});
    expect(result.content).toBe("");
  });

  it("parses boolean values", () => {
    const input = `---
draft: true
published: false
---
Body`;
    const result = parseFrontmatter(input);
    expect(result.data.draft).toBe(true);
    expect(result.data.published).toBe(false);
  });

  it("parses numeric values", () => {
    const input = `---
priority: 3
score: 95.5
---
Body`;
    const result = parseFrontmatter(input);
    expect(result.data.priority).toBe(3);
    expect(result.data.score).toBe(95.5);
  });

  it("handles quoted string values", () => {
    const input = `---
title: "My Plan: A Journey"
subtitle: 'Single quoted'
---
Body`;
    const result = parseFrontmatter(input);
    expect(result.data.title).toBe("My Plan: A Journey");
    expect(result.data.subtitle).toBe("Single quoted");
  });

  it("handles empty values", () => {
    const input = `---
title:
status:
---
Body`;
    const result = parseFrontmatter(input);
    expect(result.data.title).toBe("");
    expect(result.data.status).toBe("");
  });

  it("skips comment lines in frontmatter", () => {
    const input = `---
# This is a comment
title: My Plan
---
Body`;
    const result = parseFrontmatter(input);
    expect(result.data.title).toBe("My Plan");
    expect(result.data["# This is a comment"]).toBeUndefined();
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
    const result = parseFrontmatter(input);
    expect(result.data.title).toBe("Plan");
    expect(result.content).toContain("This is a horizontal rule");
    expect(result.content).toContain("More content");
  });

  it("handles malformed frontmatter gracefully (no closing ---)", () => {
    const input = `---
title: Plan
# Content starts without closing fence`;
    const result = parseFrontmatter(input);
    // Treat as no valid frontmatter — return everything as content
    expect(result.data).toEqual({});
    expect(result.content).toBe(input);
  });

  it("handles frontmatter not at start of file", () => {
    const input = `Some text before
---
title: Plan
---
Body`;
    const result = parseFrontmatter(input);
    // Frontmatter must be at the very start — this is not frontmatter
    expect(result.data).toEqual({});
    expect(result.content).toBe(input);
  });

  it("preserves blank line between frontmatter and content", () => {
    const input = `---
title: Plan
---

# Content with leading blank line`;
    const result = parseFrontmatter(input);
    expect(result.content).toBe("\n# Content with leading blank line");
  });

  it("handles keys with underscores and hyphens", () => {
    const input = `---
estimated_effort: M
my-key: value
---
Body`;
    const result = parseFrontmatter(input);
    expect(result.data.estimated_effort).toBe("M");
    expect(result.data["my-key"]).toBe("value");
  });

  it("does not parse date strings as special types", () => {
    const input = `---
created: 2026-03-01
---
Body`;
    const result = parseFrontmatter(input);
    // Should remain a string, not a Date object
    expect(result.data.created).toBe("2026-03-01");
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
});
