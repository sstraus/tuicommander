import { Component, Show } from "solid-js";
import { computeCiRingSegments } from "../../utils/ciRingSegments";

export interface CiRingProps {
  passed: number;
  failed: number;
  pending: number;
  onClick?: () => void;
}

// SVG circle arc parameters
const SIZE = 16;
const STROKE_WIDTH = 2;
const RADIUS = (SIZE - STROKE_WIDTH) / 2;
const CIRCUMFERENCE = 2 * Math.PI * RADIUS;

// GitHub-style colors
const COLORS = {
  passed: "#3fb950",
  failed: "#f85149",
  pending: "#d29922",
};

/** CI status ring showing pass/fail/pending proportions */
export const CiRing: Component<CiRingProps> = (props) => {
  const total = () => props.passed + props.failed + props.pending;

  const segments = () =>
    computeCiRingSegments(props.failed, props.pending, props.passed, CIRCUMFERENCE, COLORS);

  return (
    <Show when={total() > 0}>
      <svg
        width={SIZE}
        height={SIZE}
        viewBox={`0 0 ${SIZE} ${SIZE}`}
        class="ci-ring"
        style={{ cursor: props.onClick ? "pointer" : "default", "flex-shrink": "0" }}
        onClick={props.onClick}
      >
        {/* Background circle */}
        <circle
          cx={SIZE / 2}
          cy={SIZE / 2}
          r={RADIUS}
          fill="none"
          stroke="var(--border-color, #333)"
          stroke-width={STROKE_WIDTH}
        />
        {/* Colored arc segments */}
        {segments().map((seg) => (
          <circle
            cx={SIZE / 2}
            cy={SIZE / 2}
            r={RADIUS}
            fill="none"
            stroke={seg.color}
            stroke-width={STROKE_WIDTH}
            stroke-dasharray={seg.dashArray}
            stroke-dashoffset={seg.dashOffset}
            stroke-linecap="butt"
            class={seg.className}
            transform={`rotate(-90 ${SIZE / 2} ${SIZE / 2})`}
          />
        ))}
      </svg>
    </Show>
  );
};
