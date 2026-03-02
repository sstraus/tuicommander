import { describe, it, expect } from "vitest";
import { parseFrontmatter } from "../../utils/frontmatter";

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
