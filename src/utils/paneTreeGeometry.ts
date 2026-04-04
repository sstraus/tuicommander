import type { PaneNode } from "../stores/paneLayout";

/** Normalized rectangle representing a leaf pane's position in the layout */
export interface LeafRect {
  groupId: string;
  x: number;
  y: number;
  w: number;
  h: number;
}

/** Compute normalized (0–1) rectangles for every leaf in a pane tree */
export function computeLeafRects(node: PaneNode): LeafRect[] {
  return recurse(node, 0, 0, 1, 1);
}

function recurse(node: PaneNode, x: number, y: number, w: number, h: number): LeafRect[] {
  if (node.type === "leaf") {
    return [{ groupId: node.id, x, y, w, h }];
  }

  const results: LeafRect[] = [];
  let offset = 0;
  for (let i = 0; i < node.children.length; i++) {
    const ratio = node.ratios[i];
    if (node.direction === "vertical") {
      results.push(...recurse(node.children[i], x + offset * w, y, ratio * w, h));
    } else {
      results.push(...recurse(node.children[i], x, y + offset * h, w, ratio * h));
    }
    offset += ratio;
  }
  return results;
}
