import { Component, createEffect, createSignal, on, onCleanup } from "solid-js";
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

/** Maximum offscreen canvas height in CSS pixels (browser safety limit) */
const MAX_OFFSCREEN_HEIGHT = 32768;

// ---------------------------------------------------------------------------
// Drawing helpers — accept both on-screen and offscreen 2D contexts
// ---------------------------------------------------------------------------

type Ctx2D = CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D;

function drawConnection(ctx: Ctx2D, conn: Connection): void {
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

function drawDot(ctx: Ctx2D, node: GraphNode): void {
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
  const [canvasRef, setCanvasRef] = createSignal<HTMLCanvasElement | undefined>(undefined);

  // Offscreen canvas holding the full pre-rendered graph (signal so Effect 2 re-runs on rebuild)
  const [offscreen, setOffscreen] = createSignal<OffscreenCanvas | null>(null);

  const maxCol = () =>
    props.nodes.length === 0
      ? 0
      : props.nodes.reduce((max, n) => Math.max(max, n.column), 0);

  const width = () => (maxCol() + 1) * LANE_WIDTH;

  // --- Effect 1: rebuild offscreen canvas when nodes change ---
  createEffect(
    on(
      () => [props.nodes, props.totalHeight] as const,
      () => {
        const nodes = props.nodes;
        const dpr = window.devicePixelRatio || 1;
        const w = width();
        // Cap height to browser maximum; graphs taller than this are rare
        const fullHeight = Math.min(props.totalHeight || 1, MAX_OFFSCREEN_HEIGHT);

        if (w === 0 || fullHeight === 0) {
          setOffscreen(null);
          return;
        }

        const oc = new OffscreenCanvas(w * dpr, fullHeight * dpr);
        const ctx = oc.getContext("2d");
        if (!ctx) {
          setOffscreen(null);
          return;
        }

        ctx.setTransform(dpr, 0, 0, dpr, 0, 0);

        // Draw all connections first (lines behind dots)
        for (const node of nodes) {
          for (const conn of node.connections) {
            drawConnection(ctx, conn);
          }
        }

        // Draw commit dots on top
        for (const node of nodes) {
          drawDot(ctx, node);
        }

        setOffscreen(oc);
      },
    ),
  );

  // --- Effect 2: blit visible slice on scroll — O(1) per frame ---
  createEffect(
    on(
      () => [props.scrollTop, props.viewportHeight, canvasRef(), offscreen()] as const,
      () => {
        const canvas = canvasRef();
        const oc = offscreen();
        if (!canvas || !oc) return;

        const ctx = canvas.getContext("2d");
        if (!ctx) return;

        const dpr = window.devicePixelRatio || 1;
        const w = width();
        const h = props.viewportHeight;

        // Resize visible canvas backing store for retina
        canvas.width = w * dpr;
        canvas.height = h * dpr;
        ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
        ctx.clearRect(0, 0, w, h);

        // Source rectangle in offscreen (in CSS pixels, scaled by dpr)
        const sy = props.scrollTop * dpr;
        const sh = h * dpr;
        const sw = w * dpr;

        // Clamp source to offscreen bounds
        const clampedSh = Math.min(sh, oc.height - sy);
        if (clampedSh <= 0 || sw <= 0) return;

        ctx.drawImage(oc, 0, sy, sw, clampedSh, 0, 0, w, clampedSh / dpr);
      },
    ),
  );

  onCleanup(() => {
    setOffscreen(null);
  });

  return (
    <canvas
      ref={setCanvasRef}
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
