import { describe, it, expect } from "vitest";
import { computeLeafRects } from "../../utils/paneTreeGeometry";
import type { PaneNode } from "../../stores/paneLayout";

describe("computeLeafRects", () => {
  it("returns single rect for a leaf node", () => {
    const node: PaneNode = { type: "leaf", id: "g1" };
    const rects = computeLeafRects(node);
    expect(rects).toEqual([{ groupId: "g1", x: 0, y: 0, w: 1, h: 1 }]);
  });

  it("splits horizontally into two rects (top/bottom)", () => {
    const node: PaneNode = {
      type: "branch",
      direction: "horizontal",
      children: [
        { type: "leaf", id: "g1" },
        { type: "leaf", id: "g2" },
      ],
      ratios: [0.5, 0.5],
    };
    const rects = computeLeafRects(node);
    expect(rects).toHaveLength(2);
    expect(rects[0]).toEqual({ groupId: "g1", x: 0, y: 0, w: 1, h: 0.5 });
    expect(rects[1]).toEqual({ groupId: "g2", x: 0, y: 0.5, w: 1, h: 0.5 });
  });

  it("splits vertically into two rects (left/right)", () => {
    const node: PaneNode = {
      type: "branch",
      direction: "vertical",
      children: [
        { type: "leaf", id: "g1" },
        { type: "leaf", id: "g2" },
      ],
      ratios: [0.5, 0.5],
    };
    const rects = computeLeafRects(node);
    expect(rects).toHaveLength(2);
    expect(rects[0]).toEqual({ groupId: "g1", x: 0, y: 0, w: 0.5, h: 1 });
    expect(rects[1]).toEqual({ groupId: "g2", x: 0.5, y: 0, w: 0.5, h: 1 });
  });

  it("handles uneven ratios", () => {
    const node: PaneNode = {
      type: "branch",
      direction: "vertical",
      children: [
        { type: "leaf", id: "g1" },
        { type: "leaf", id: "g2" },
      ],
      ratios: [0.3, 0.7],
    };
    const rects = computeLeafRects(node);
    expect(rects[0].w).toBeCloseTo(0.3);
    expect(rects[1].x).toBeCloseTo(0.3);
    expect(rects[1].w).toBeCloseTo(0.7);
  });

  it("handles three-way vertical split", () => {
    const r = 1 / 3;
    const node: PaneNode = {
      type: "branch",
      direction: "vertical",
      children: [
        { type: "leaf", id: "g1" },
        { type: "leaf", id: "g2" },
        { type: "leaf", id: "g3" },
      ],
      ratios: [r, r, r],
    };
    const rects = computeLeafRects(node);
    expect(rects).toHaveLength(3);
    expect(rects[0].x).toBeCloseTo(0);
    expect(rects[0].w).toBeCloseTo(r);
    expect(rects[1].x).toBeCloseTo(r);
    expect(rects[2].x).toBeCloseTo(2 * r);
  });

  it("handles nested split (vertical > horizontal)", () => {
    const node: PaneNode = {
      type: "branch",
      direction: "vertical",
      children: [
        { type: "leaf", id: "g1" },
        {
          type: "branch",
          direction: "horizontal",
          children: [
            { type: "leaf", id: "g2" },
            { type: "leaf", id: "g3" },
          ],
          ratios: [0.5, 0.5],
        },
      ],
      ratios: [0.5, 0.5],
    };
    const rects = computeLeafRects(node);
    expect(rects).toHaveLength(3);
    // g1: left half
    expect(rects[0]).toEqual({ groupId: "g1", x: 0, y: 0, w: 0.5, h: 1 });
    // g2: right half, top
    expect(rects[1]).toEqual({ groupId: "g2", x: 0.5, y: 0, w: 0.5, h: 0.5 });
    // g3: right half, bottom
    expect(rects[2]).toEqual({ groupId: "g3", x: 0.5, y: 0.5, w: 0.5, h: 0.5 });
  });
});
