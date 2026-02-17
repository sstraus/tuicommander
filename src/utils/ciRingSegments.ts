/** A single arc segment for the CI status ring SVG */
export interface CiRingSegment {
  color: string;
  dashArray: string;
  dashOffset: number;
  className: string;
}

interface CiRingColors {
  failed: string;
  pending: string;
  passed: string;
}

/**
 * Compute SVG stroke-dasharray segments for a CI status ring.
 * Segments are ordered by priority: failed > pending > passed.
 * Each segment's dashOffset positions it after the previous segments.
 */
export function computeCiRingSegments(
  failed: number,
  pending: number,
  passed: number,
  circumference: number,
  colors: CiRingColors,
): CiRingSegment[] {
  const total = failed + pending + passed;
  if (total === 0) return [];

  const result: CiRingSegment[] = [];
  let offset = 0;

  const entries: Array<{ count: number; color: string; className: string }> = [
    { count: failed, color: colors.failed, className: "ci-ring-failed" },
    { count: pending, color: colors.pending, className: "ci-ring-pending" },
    { count: passed, color: colors.passed, className: "ci-ring-passed" },
  ];

  for (const entry of entries) {
    if (entry.count > 0) {
      const len = (entry.count / total) * circumference;
      result.push({
        color: entry.color,
        dashArray: `${len} ${circumference - len}`,
        dashOffset: offset === 0 ? 0 : -offset,
        className: entry.className,
      });
      offset += len;
    }
  }

  return result;
}
