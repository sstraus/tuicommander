import { measureFont, type CellMetrics } from "./canvasTerminalUtils";

interface CacheConfig {
  fontSize: number;
  fontFamily: string;
  fontWeight: number;
  dpr: number;
  lineHeight: number;
}

let config: CacheConfig | null = null;
let sharedMetrics: CellMetrics | null = null;
let measureCtx: CanvasRenderingContext2D | null = null;
let refCount = 0;

function configMatches(a: CacheConfig, b: CacheConfig): boolean {
  return a.fontSize === b.fontSize
    && a.fontFamily === b.fontFamily
    && a.fontWeight === b.fontWeight
    && a.dpr === b.dpr
    && a.lineHeight === b.lineHeight;
}

function ensureMeasureCtx(): void {
  if (measureCtx) return;
  const c = document.createElement("canvas");
  c.width = 1;
  c.height = 1;
  measureCtx = c.getContext("2d")!;
}

function destroyMeasureCtx(): void {
  measureCtx = null;
}

export function getSharedMetrics(
  fontSize: number,
  fontFamily: string,
  dpr: number,
  lineHeight: number,
  fontWeight: number,
): CellMetrics {
  const cfg: CacheConfig = { fontSize, fontFamily, fontWeight, dpr, lineHeight };
  if (sharedMetrics && config && configMatches(config, cfg)) {
    return sharedMetrics;
  }

  config = cfg;
  ensureMeasureCtx();
  sharedMetrics = measureFont(measureCtx!, fontSize, fontFamily, dpr, lineHeight, fontWeight);
  return sharedMetrics;
}

export function acquireCache(): void {
  refCount++;
}

export function releaseCache(): void {
  refCount = Math.max(0, refCount - 1);
  if (refCount === 0) {
    destroyMeasureCtx();
    config = null;
    sharedMetrics = null;
  }
}

export function invalidateGlyphCache(): void {
  config = null;
  sharedMetrics = null;
}
