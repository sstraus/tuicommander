import { describe, it, expect } from "vitest";
import { interpolateTemplate } from "../../utils/templateInterpolation";

describe("interpolateTemplate", () => {
  it("substitutes known variables", () => {
    const result = interpolateTemplate("review {pr_number} on {branch}", {
      pr_number: "42",
      branch: "feature/login",
    });
    expect(result).toBe("review 42 on feature/login");
  });

  it("replaces null values with empty string", () => {
    const result = interpolateTemplate("pr {pr_number} url {pr_url}", {
      pr_number: "42",
      pr_url: null,
    });
    expect(result).toBe("pr 42 url ");
  });

  it("leaves unknown variables as-is", () => {
    const result = interpolateTemplate("{known} and {unknown}", {
      known: "yes",
    });
    expect(result).toBe("yes and {unknown}");
  });

  it("handles multiple occurrences of the same variable", () => {
    const result = interpolateTemplate("{x} then {x}", { x: "val" });
    expect(result).toBe("val then val");
  });

  it("returns template unchanged when no vars match", () => {
    const result = interpolateTemplate("no vars here", { x: "val" });
    expect(result).toBe("no vars here");
  });

  it("handles all PR review variables", () => {
    const result = interpolateTemplate(
      "gh pr review {pr_number} --repo {repo} --comment -b 'Review for {branch} targeting {base_branch}: {pr_url}'",
      {
        pr_number: "99",
        branch: "feat/auth",
        base_branch: "main",
        repo: "owner/repo",
        pr_url: "https://github.com/owner/repo/pull/99",
      },
    );
    expect(result).toBe(
      "gh pr review 99 --repo owner/repo --comment -b 'Review for feat/auth targeting main: https://github.com/owner/repo/pull/99'",
    );
  });

  it("handles empty template", () => {
    expect(interpolateTemplate("", { x: "val" })).toBe("");
  });

  it("handles empty vars", () => {
    expect(interpolateTemplate("{x}", {})).toBe("{x}");
  });
});
