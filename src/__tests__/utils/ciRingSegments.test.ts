import { describe, it, expect } from "vitest";
import { computeCiRingSegments } from "../../utils/ciRingSegments";

const RADIUS = 7;
const CIRCUMFERENCE = 2 * Math.PI * RADIUS;

const COLORS = {
  failed: "#f85149",
  pending: "#d29922",
  passed: "#3fb950",
};

describe("computeCiRingSegments", () => {
  it("returns empty array when all counts are zero", () => {
    expect(computeCiRingSegments(0, 0, 0, CIRCUMFERENCE, COLORS)).toEqual([]);
  });

  it("returns single segment covering full ring when only failed", () => {
    const result = computeCiRingSegments(5, 0, 0, CIRCUMFERENCE, COLORS);
    expect(result).toHaveLength(1);
    expect(result[0].color).toBe(COLORS.failed);
    expect(result[0].className).toBe("ci-ring-failed");
    expect(result[0].dashOffset).toBe(0);
    expect(result[0].dashArray).toBe(`${CIRCUMFERENCE} 0`);
  });

  it("returns single segment covering full ring when only passed", () => {
    const result = computeCiRingSegments(0, 0, 3, CIRCUMFERENCE, COLORS);
    expect(result).toHaveLength(1);
    expect(result[0].color).toBe(COLORS.passed);
    expect(result[0].className).toBe("ci-ring-passed");
    expect(result[0].dashOffset).toBe(0);
  });

  it("returns single segment covering full ring when only pending", () => {
    const result = computeCiRingSegments(0, 4, 0, CIRCUMFERENCE, COLORS);
    expect(result).toHaveLength(1);
    expect(result[0].color).toBe(COLORS.pending);
    expect(result[0].className).toBe("ci-ring-pending");
    expect(result[0].dashOffset).toBe(0);
  });

  it("returns segments in order: failed, pending, passed", () => {
    const result = computeCiRingSegments(1, 1, 1, CIRCUMFERENCE, COLORS);
    expect(result).toHaveLength(3);
    expect(result[0].className).toBe("ci-ring-failed");
    expect(result[1].className).toBe("ci-ring-pending");
    expect(result[2].className).toBe("ci-ring-passed");
  });

  it("computes correct proportions for mixed counts", () => {
    // 2 failed, 3 pending, 5 passed = 10 total
    // failed: 20%, pending: 30%, passed: 50%
    const result = computeCiRingSegments(2, 3, 5, CIRCUMFERENCE, COLORS);
    expect(result).toHaveLength(3);

    const failedLen = (2 / 10) * CIRCUMFERENCE;
    const pendingLen = (3 / 10) * CIRCUMFERENCE;
    const passedLen = (5 / 10) * CIRCUMFERENCE;

    // Failed segment: starts at offset 0
    expect(result[0].dashArray).toBe(`${failedLen} ${CIRCUMFERENCE - failedLen}`);
    expect(result[0].dashOffset).toBe(0);

    // Pending segment: offset by failed length
    expect(result[1].dashArray).toBe(`${pendingLen} ${CIRCUMFERENCE - pendingLen}`);
    expect(result[1].dashOffset).toBe(-failedLen);

    // Passed segment: offset by failed + pending
    expect(result[2].dashArray).toBe(`${passedLen} ${CIRCUMFERENCE - passedLen}`);
    expect(result[2].dashOffset).toBe(-(failedLen + pendingLen));
  });

  it("skips segments with zero count", () => {
    // Only failed and passed, no pending
    const result = computeCiRingSegments(3, 0, 7, CIRCUMFERENCE, COLORS);
    expect(result).toHaveLength(2);
    expect(result[0].className).toBe("ci-ring-failed");
    expect(result[1].className).toBe("ci-ring-passed");

    const failedLen = (3 / 10) * CIRCUMFERENCE;
    expect(result[1].dashOffset).toBe(-failedLen);
  });

  it("works with a different circumference value", () => {
    const circ = 100;
    const result = computeCiRingSegments(1, 0, 1, circ, COLORS);
    expect(result).toHaveLength(2);
    expect(result[0].dashArray).toBe("50 50");
    expect(result[1].dashArray).toBe("50 50");
    expect(result[1].dashOffset).toBe(-50);
  });
});
