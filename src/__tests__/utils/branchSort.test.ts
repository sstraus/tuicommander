import { describe, it, expect } from "vitest";
import { compareBranches, type SortableBranch } from "../../utils/branchSort";

function makeBranch(name: string, isMain: boolean): SortableBranch {
  return { name, isMain };
}

/** Sort an array of branches using compareBranches with optional PR state lookup */
function sortBranches(
  branches: SortableBranch[],
  prStates: Record<string, { state?: string }> = {},
): SortableBranch[] {
  return [...branches].sort((a, b) =>
    compareBranches(a, b, prStates[a.name] ?? null, prStates[b.name] ?? null),
  );
}

function names(branches: SortableBranch[]): string[] {
  return branches.map((b) => b.name);
}

describe("compareBranches", () => {
  describe("main branches first", () => {
    it("sorts main before feature branches", () => {
      const result = sortBranches([
        makeBranch("feature/z", false),
        makeBranch("main", true),
        makeBranch("feature/a", false),
      ]);
      expect(names(result)).toEqual(["main", "feature/a", "feature/z"]);
    });

    it("sorts multiple main branches alphabetically among themselves", () => {
      const result = sortBranches([
        makeBranch("feature/x", false),
        makeBranch("master", true),
        makeBranch("develop", true),
        makeBranch("main", true),
      ]);
      expect(names(result)).toEqual(["develop", "main", "master", "feature/x"]);
    });
  });

  describe("merged/closed PRs to bottom", () => {
    it("sorts merged PR branches after active branches", () => {
      const result = sortBranches(
        [
          makeBranch("feature/merged", false),
          makeBranch("feature/active", false),
        ],
        {
          "feature/merged": { state: "MERGED" },
          "feature/active": { state: "OPEN" },
        },
      );
      expect(names(result)).toEqual(["feature/active", "feature/merged"]);
    });

    it("sorts closed PR branches after active branches", () => {
      const result = sortBranches(
        [
          makeBranch("feature/closed", false),
          makeBranch("feature/active", false),
        ],
        {
          "feature/closed": { state: "CLOSED" },
          "feature/active": { state: "OPEN" },
        },
      );
      expect(names(result)).toEqual(["feature/active", "feature/closed"]);
    });

    it("sorts merged and closed alphabetically among themselves", () => {
      const result = sortBranches(
        [
          makeBranch("feature/z-merged", false),
          makeBranch("feature/a-closed", false),
        ],
        {
          "feature/z-merged": { state: "MERGED" },
          "feature/a-closed": { state: "CLOSED" },
        },
      );
      expect(names(result)).toEqual(["feature/a-closed", "feature/z-merged"]);
    });
  });

  describe("full sorting: main first, active middle, merged/closed bottom", () => {
    it("sorts all three tiers correctly", () => {
      const result = sortBranches(
        [
          makeBranch("main", true),
          makeBranch("feature/active", false),
          makeBranch("feature/merged", false),
          makeBranch("feature/closed", false),
        ],
        {
          "feature/active": { state: "OPEN" },
          "feature/merged": { state: "MERGED" },
          "feature/closed": { state: "CLOSED" },
        },
      );
      expect(names(result)).toEqual([
        "main",
        "feature/active",
        "feature/closed",
        "feature/merged",
      ]);
    });
  });

  describe("edge cases", () => {
    it("handles empty array", () => {
      expect(sortBranches([])).toEqual([]);
    });

    it("handles single branch", () => {
      const result = sortBranches([makeBranch("main", true)]);
      expect(names(result)).toEqual(["main"]);
    });

    it("handles branches with no PR state", () => {
      const result = sortBranches([
        makeBranch("feature/b", false),
        makeBranch("feature/a", false),
      ]);
      expect(names(result)).toEqual(["feature/a", "feature/b"]);
    });

    it("handles null and undefined PR states", () => {
      // compareBranches should handle null/undefined gracefully
      expect(compareBranches(makeBranch("a", false), makeBranch("b", false), null, undefined)).toBeLessThan(0);
    });
  });
});
