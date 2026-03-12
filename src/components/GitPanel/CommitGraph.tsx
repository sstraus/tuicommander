import { Component, createEffect, on } from "solid-js";
import s from "./CommitGraph.module.css";

// ---------------------------------------------------------------------------
// Types — mirrors Rust GraphNode / Connection from git_graph.rs
// ---------------------------------------------------------------------------

export interface Connection {
  from_col: number;
  from_row: number;
  to_col: number;
  to_row: number;
  color_index: number;
}

export interface GraphNode {
  hash: string;
  column: number;
  row: number;
  color_index: number;
  parents: string[];
  refs: string[];
  connections: Connection[];
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/** Must match the virtualizer's estimateSize for collapsed rows in LogTab */
const ROW_HEIGHT = 48;

/** Horizontal spacing per lane column */
const LANE_WIDTH = 16;

/** Commit dot radius */
const DOT_RADIUS = 4;

/** 8 lane colors matching the style guide palette */
const COLORS = [
  "#4FC1FF", // blue
  "#7FE07F", // green
  "#FF7F7F", // red
  "#FFD700", // gold
  "#DA70D6", // orchid
  "#20B2AA", // teal
  "#FF8C00", // orange
  "#BA55D3", // purple
];

// ---------------------------------------------------------------------------
// Drawing helpers
// ---------------------------------------------------------------------------

function drawConnection(ctx: CanvasRenderingContext2D, conn: Connection): void {
  const x1 = conn.from_col * LANE_WIDTH + LANE_WIDTH / 2;
  const y1 = conn.from_row * ROW_HEIGHT + ROW_HEIGHT / 2;
  const x2 = conn.to_col * LANE_WIDTH + LANE_WIDTH / 2;
  const y2 = conn.to_row * ROW_HEIGHT + ROW_HEIGHT / 2;

  ctx.strokeStyle = COLORS[conn.color_index % COLORS.length];
  ctx.lineWidth = 2;
  ctx.beginPath();
  ctx.moveTo(x1, y1);

  if (x1 === x2) {
    ctx.lineTo(x2, y2);
  } else {
    // Cubic Bezier: starts vertical from source, bends to target
    const midY = (y1 + y2) / 2;
    ctx.bezierCurveTo(x1, midY, x2, midY, x2, y2);
  }

  ctx.stroke();
}

function drawDot(ctx: CanvasRenderingContext2D, node: GraphNode): void {
  const x = node.column * LANE_WIDTH + LANE_WIDTH / 2;
  const y = node.row * ROW_HEIGHT + ROW_HEIGHT / 2;
  const color = COLORS[node.color_index % COLORS.length];

  ctx.fillStyle = color;
  ctx.beginPath();
  ctx.arc(x, y, DOT_RADIUS, 0, Math.PI * 2);
  ctx.fill();
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export interface CommitGraphProps {
  nodes: GraphNode[];
  scrollTop: number;
  viewportHeight: number;
  totalHeight: number;
}

export const CommitGraph: Component<CommitGraphProps> = (props) => {
  let canvasRef: HTMLCanvasElement | undefined;

  const maxCol = () =>
    props.nodes.length === 0
      ? 0
      : props.nodes.reduce((max, n) => Math.max(max, n.column), 0);

  const width = () => (maxCol() + 1) * LANE_WIDTH;

  createEffect(
    on(
      () => [props.nodes, props.scrollTop, props.viewportHeight, props.totalHeight] as const,
      () => {
        const canvas = canvasRef;
        if (!canvas) return;

        const ctx = canvas.getContext("2d");
        if (!ctx) return;

        const dpr = window.devicePixelRatio || 1;
        const w = width();
        const h = props.viewportHeight;

        // Size the canvas backing store for retina
        canvas.width = w * dpr;
        canvas.height = h * dpr;
        ctx.setTransform(dpr, 0, 0, dpr, 0, 0);

        // Clear
        ctx.clearRect(0, 0, w, h);

        // Translate so we only render the visible viewport slice
        ctx.save();
        ctx.translate(0, -props.scrollTop);

        const viewTop = props.scrollTop;
        const viewBottom = props.scrollTop + h;

        // Draw connections first (lines behind dots)
        for (const node of props.nodes) {
          for (const conn of node.connections) {
            const minY = Math.min(conn.from_row, conn.to_row) * ROW_HEIGHT;
            const maxY = Math.max(conn.from_row, conn.to_row) * ROW_HEIGHT + ROW_HEIGHT;
            if (maxY < viewTop || minY > viewBottom) continue;
            drawConnection(ctx, conn);
          }
        }

        // Draw commit dots on top
        for (const node of props.nodes) {
          const y = node.row * ROW_HEIGHT + ROW_HEIGHT / 2;
          if (y < viewTop - ROW_HEIGHT || y > viewBottom + ROW_HEIGHT) continue;
          drawDot(ctx, node);
        }

        ctx.restore();
      },
    ),
  );

  return (
    <canvas
      ref={canvasRef}
      class={s.canvas}
      style={{ width: `${width()}px`, height: `${props.viewportHeight}px` }}
    />
  );
};

/** Width of the graph overlay in pixels, for use as left padding in the commit list */
export function graphWidth(nodes: GraphNode[]): number {
  if (nodes.length === 0) return 0;
  const maxCol = nodes.reduce((max, n) => Math.max(max, n.column), 0);
  return (maxCol + 1) * LANE_WIDTH;
}

export default CommitGraph;
