import { type Component, createEffect, createSignal, onCleanup, onMount } from "solid-js";
import { isMacOS, isWindows } from "../../platform";
import { pluginRegistry } from "../../plugins/pluginRegistry";
import { appLogger } from "../../stores/appLogger";
import { settingsStore } from "../../stores/settings";
import { terminalsStore } from "../../stores/terminals";
import { filterMatchesToBlock } from "../../utils/blockSearchFilter";
import { formatRelativeTime } from "../../utils/formatRelativeTime";
import { handleOpenUrl } from "../../utils/openUrl";
import { markPerf, noteFrameRequest } from "../../utils/perfTrace";
import { ContextMenu, createContextMenu } from "../ContextMenu/ContextMenu";
import { installTouchHandlers } from "./canvasTerminalTouch";
import { createTransport, type TerminalTransport } from "./canvasTerminalTransport";
import {
	type CellMetrics,
	type CursorShape,
	computeCursorRect,
	type DecodedFrame,
	decodeBinaryFrame,
	GUTTER_PX,
	resolveDefaultTerminalColors,
	SCROLLBAR_PX,
	snapLineHeight,
} from "./canvasTerminalUtils";
import { fetchFontPayloads, resolveWorkerFontFaces } from "./fontAssets";
import {
	installFrameTimingDebugHook,
	isFrameTimingEnabled,
	onFrameTimingEnabledChange,
	recordFrameTiming,
	resetFrameTiming,
} from "./frameTiming";
import { acquireCache, getSharedMetrics, invalidateGlyphCache, releaseCache } from "./glyphCache";
import { createGridRenderer, type GridRenderer } from "./gridRenderer";
import { kittySequenceForKey } from "./kittyKeyboard";
import { filePathRegex, fileUrlRegex } from "./linkProvider";
import { continuationRowsAfterSuggest, isSuggestBlock } from "./suggestOverlay";
import { altSequenceFromCode, createCompositionState, keyToSequence } from "./terminalInput";
import { decideFrameGrid } from "./workerGridState";
import { chooseRenderer, receiveFrame, WorkerRenderer } from "./workerProtocol";

// Re-export for external consumers
export type { CellMetrics, CursorShape, DecodedFrame };

export interface CanvasTerminalRef {
	focus: () => void;
	refresh: () => void;
	resubscribe: () => Promise<void>;
	getSelectionText: () => string;
	searchFind: (query: string, blockScope?: boolean) => Promise<{ index: number; count: number }>;
	searchNext: () => { index: number; count: number };
	searchPrev: () => { index: number; count: number };
	searchClear: () => void;
	/** Paste text with correct bracketed paste wrapping based on current terminal state */
	paste: (text: string) => void;
}

export interface CanvasTerminalProps {
	sessionId: string;
	terminalId: string;
	onOpenFilePath?: (path: string, line?: number, col?: number) => void;
	onSearchOpen?: () => void;
	onSearchClose?: () => void;
	searchVisible?: boolean;
	onResume?: () => void;
	onResumeDismiss?: () => void;
	hasPendingResume?: boolean;
	onCwdChange?: (id: string, cwd: string) => void;
	onFocus?: () => void;
	onRef?: (ref: CanvasTerminalRef) => void;
	onBell?: () => void;
}

const SUGGEST_ANCHOR_RE = /^[\s●⏺]*suggest:\s+\S/;
const INTENT_RE = /^[\s●⏺]*intent:\s+/;

const CanvasTerminal: Component<CanvasTerminalProps> = (props) => {
	let canvasRef!: HTMLCanvasElement;
	let overlayCanvasRef!: HTMLCanvasElement;
	let touchTextareaRef!: HTMLTextAreaElement;
	let keyInputRef!: HTMLInputElement;
	let scrollbarRef!: HTMLDivElement;
	let scrollThumbRef!: HTMLDivElement;
	let overlayRef!: HTMLDivElement;
	let containerRef!: HTMLDivElement;
	let ctx!: CanvasRenderingContext2D;
	let octx!: CanvasRenderingContext2D;
	// Shared base-grid renderer (the single canvas2d paint implementation,
	// also used by the render worker). Created in onMount once ctx exists.
	let gridRenderer!: GridRenderer;
	// Renderer mode is decided once at mount (chooseRenderer). "worker" transfers
	// the base canvas to a Web Worker; "main" uses gridRenderer on this thread.
	// Flipping the setting only affects terminals opened afterwards.
	let rendererMode: "worker" | "main" = "main";
	let renderWorker: Worker | null = null;
	let workerRenderer: WorkerRenderer | null = null;
	let unregisterTimingListener: (() => void) | null = null;

	const [metrics, setMetrics] = createSignal<CellMetrics | null>(null);
	const [focused, setFocused] = createSignal(false);
	let currentFrame: DecodedFrame | null = null;
	let lastDisplayOffset = -1;
	let lastScreenRows = -1;
	let lastScreenCols = -1;
	let searchMatches: { row: number; col_start: number; col_end: number }[] = [];
	let activeSearchIndex = -1;
	let cursorBlinkOn = true;
	let blinkInterval: ReturnType<typeof setInterval> | undefined;
	let blinkResetAt = 0;
	let unsubscribe: (() => void) | undefined;
	let resizeObserver: ResizeObserver | undefined;
	let visibilityObserver: IntersectionObserver | undefined;
	let lastResizeCols = 0;
	let lastResizeRows = 0;
	let transport: TerminalTransport | undefined;
	let invokeRef: ((cmd: string, args: Record<string, unknown>) => Promise<unknown>) | undefined;
	let rafId: number | undefined;
	// Main-mode render-scheduling stamp: when a repaint is first requested, the gap
	// to the rAF callback is the main-thread analog of the worker's "sched" metric
	// (only meaningful in main mode; 0 = no pending request). Lets us compare main
	// vs worker scheduling latency under CPU load. Gated by isFrameTimingEnabled().
	let mainDirtySince = 0;
	let resizeDebounce: ReturnType<typeof setTimeout> | undefined;
	let dprMediaQuery: MediaQueryList | undefined;
	let dprChangeHandler: (() => void) | undefined;
	let cleanupTouch: (() => void) | undefined;
	let alive = true;
	let linkCheckGeneration = 0;
	const ipcErr = (cmd: string) => (e: unknown) =>
		appLogger.debug("terminal", `${cmd} failed`, { sessionId: props.sessionId, error: e });

	// Selection state — row coordinates are absolute (viewportTop + viewportRow)
	// so the highlight stays anchored to the original content when scrolling.
	let selecting = false;
	let selectionStart: { col: number; row: number } | null = null;
	let selectionEnd: { col: number; row: number } | null = null;
	let cachedSelectionText = "";
	let selectionScrollTimer: ReturnType<typeof setInterval> | null = null;
	let selectionScrollDelta = 0;

	// Link detection
	const linkCache = new Map<
		string,
		{ text: string; path: string; line?: number; col?: number; index: number }[] | null
	>();
	let hoveredLink: {
		row: number;
		colStart: number;
		colEnd: number;
		path: string;
		line?: number;
		col?: number;
		spans?: { row: number; colStart: number; colEnd: number }[];
	} | null = null;
	const detectedLinks = new Map<number, { colStart: number; colEnd: number }[]>();

	// Link context menu: right-clicking a detected link offers Open / Copy link.
	// TUIC is UI-first — opening a link (e.g. a markdown file) is a primary action,
	// so plain left-click still opens; this menu adds a non-destructive way to copy
	// the target without opening it.
	type LinkTarget = { path: string; line?: number; col?: number };
	const linkMenu = createContextMenu();
	const [linkMenuTarget, setLinkMenuTarget] = createSignal<LinkTarget | null>(null);

	const openLink = (link: LinkTarget) => {
		if (link.path.startsWith("http://") || link.path.startsWith("https://")) {
			handleOpenUrl(link.path);
		} else {
			const path = link.path.startsWith("file://") ? link.path.slice(7) : link.path;
			props.onOpenFilePath?.(path, link.line, link.col);
		}
	};

	const copyLink = (link: LinkTarget) => {
		const text = link.path.startsWith("file://") ? link.path.slice(7) : link.path;
		navigator.clipboard.writeText(text).catch(() => {});
	};

	// Cached CSS custom properties (re-read on remeasure, not every frame)
	let cachedBgDefault = "#1e1e1e";
	let cachedFgDefault = "#d4d4d4";

	// Pixel scroll accumulator: shared by wheel + touch handlers.
	// Accumulates raw pixel delta; when it crosses cellHeight, emits a line scroll to backend.
	let scrollAccumPx = 0;
	let scrollGestureDistPx = 0;

	// Row index → row data lookup (persistent, updated incrementally)
	const rowMap = new Map<number, DecodedFrame["rows"][0]>();
	// Rows that arrived in the latest onFrame batch (drives incremental repaint)
	const pendingDirtyRows = new Set<number>();
	// When true, next paint must redraw everything (scroll, resize, clear)
	let fullRepaintNeeded = true;
	let hidden = false;
	let lastHistorySize = -1;

	function writePtyNoScroll(data: string) {
		invokeRef?.("write_pty", { sessionId: props.sessionId, data }).catch((e) => {
			appLogger.warn("terminal", "PTY write failed", { sessionId: props.sessionId, error: e });
		});
	}

	function writePty(data: string) {
		if (currentFrame && currentFrame.displayOffset > 0) {
			invokeRef?.("terminal_scroll", { sessionId: props.sessionId, delta: -currentFrame.displayOffset }).catch(
				ipcErr("terminal_scroll"),
			);
		}
		writePtyNoScroll(data);
	}

	function scheduleRepaint() {
		if (rafId !== undefined || hidden || !alive) return;
		// Stamp the first repaint request of this cycle (main-mode "sched" — see decl).
		if (mainDirtySince === 0 && isFrameTimingEnabled() && rendererMode === "main") {
			mainDirtySince = performance.now();
		}
		rafId = requestAnimationFrame(() => {
			rafId = undefined;
			if (!alive || hidden) return;
			const m = metrics();
			if (currentFrame && m) {
				const dirty = pendingDirtyRows.size > 0 ? new Set(pendingDirtyRows) : undefined;
				pendingDirtyRows.clear();
				// "paint" timing measures the BASE grid paint, which only happens on main.
				// In worker mode the worker paints the base, so we don't record it here
				// (paintFrame paints only the cheap overlay) — keeps paint.count a true
				// signal that the base is NOT being painted on the main thread.
				const timing = isFrameTimingEnabled() && rendererMode === "main";
				// "sched": request->rAF-callback delay. Records the main-thread scheduling
				// latency to mirror the worker's sched and expose vsync rAF priority.
				if (timing && mainDirtySince) {
					recordFrameTiming(props.sessionId, "sched", performance.now() - mainDirtySince);
				}
				const paintT0 = timing ? performance.now() : 0;
				paintFrame(currentFrame, m, dirty);
				if (timing) recordFrameTiming(props.sessionId, "paint", performance.now() - paintT0);
			}
			mainDirtySince = 0;
		});
	}

	function startSelectionScroll(delta: number) {
		if (selectionScrollTimer !== null && selectionScrollDelta === delta) return;
		stopSelectionScroll();
		selectionScrollDelta = delta;
		const speed = Math.min(Math.abs(delta), 5);
		const interval = Math.max(20, 80 - speed * 12);
		selectionScrollTimer = setInterval(() => {
			if (!selecting || !selectionStart || !currentFrame || !invokeRef) {
				stopSelectionScroll();
				return;
			}
			const scrollDir = delta > 0 ? 1 : -1;
			invokeRef("terminal_scroll", { sessionId: props.sessionId, delta: scrollDir }).catch(ipcErr("terminal_scroll"));
			const edgeRow = scrollDir > 0 ? 0 : (currentFrame.screenRows || lastResizeRows) - 1;
			const absRow = viewportRowToAbs(edgeRow);
			if (absRow !== null) {
				selectionEnd = { col: scrollDir > 0 ? 0 : 9999, row: absRow + scrollDir };
				const m = metrics();
				if (m) paintFrame(currentFrame, m);
			}
		}, interval);
	}

	function stopSelectionScroll() {
		if (selectionScrollTimer !== null) {
			clearInterval(selectionScrollTimer);
			selectionScrollTimer = null;
			selectionScrollDelta = 0;
		}
	}

	function canvasToGrid(e: MouseEvent): { col: number; row: number } {
		const m = metrics();
		if (!m) return { col: 0, row: 0 };
		const rect = canvasRef.getBoundingClientRect();
		const x = e.clientX - rect.left - GUTTER_PX;
		const y = e.clientY - rect.top;
		const maxCol = Math.max(0, Math.floor((rect.width - GUTTER_PX) / m.cellWidth) - 1);
		const maxRow = Math.max(0, Math.floor(rect.height / m.cellHeight) - 1);
		return {
			col: Math.max(0, Math.min(Math.floor(x / m.cellWidth), maxCol)),
			row: Math.max(0, Math.min(Math.floor(y / m.cellHeight), maxRow)),
		};
	}

	function mouseModifiers(e: MouseEvent): number {
		return (e.shiftKey ? 4 : 0) | (e.altKey ? 8 : 0) | (e.ctrlKey ? 16 : 0);
	}

	function sgrMouseSequence(button: number, col: number, row: number, press: boolean, e?: MouseEvent): string {
		const cb = button + (e ? mouseModifiers(e) : 0);
		return `\x1b[<${cb};${col + 1};${row + 1}${press ? "M" : "m"}`;
	}

	function viewportRowToAbs(viewportRow: number): number | null {
		if (!currentFrame) return null;
		return currentFrame.historySize - currentFrame.displayOffset + viewportRow;
	}

	function remeasure() {
		// Worker mode owns the base canvas (no main `ctx`); it only needs octx + worker.
		if (rendererMode === "main" && !ctx) return;
		if (rendererMode === "worker" && !workerRenderer) return;
		const rect = containerRef.getBoundingClientRect();
		if (rect.width <= 0 || rect.height <= 0) return;

		const dpr = window.devicePixelRatio || 1;
		const perTerminalSize = terminalsStore.state.terminals[props.terminalId]?.fontSize;
		const fontSize = perTerminalSize ?? settingsStore.state.defaultFontSize;
		const fontFamily = settingsStore.getFontFamily();
		const fontWeight = settingsStore.state.fontWeight;
		const m = getSharedMetrics(fontSize, fontFamily, dpr, snapLineHeight(fontSize), fontWeight);
		setMetrics(m);

		const defaultColors = resolveDefaultTerminalColors(canvasRef);
		cachedBgDefault = defaultColors.bg;
		cachedFgDefault = defaultColors.fg;
		if (rendererMode === "main") gridRenderer.setTheme(cachedBgDefault, cachedFgDefault);

		const cols = Math.floor((rect.width - GUTTER_PX - SCROLLBAR_PX) / m.cellWidth);
		const rows = Math.floor(rect.height / m.cellHeight);
		if (cols <= 0 || rows <= 0) return;
		const logicalW = cols * m.cellWidth + GUTTER_PX;
		const logicalH = rows * m.cellHeight;
		if (rendererMode === "main") {
			// Base canvas lives on this thread.
			canvasRef.width = logicalW * dpr;
			canvasRef.height = logicalH * dpr;
			ctx.scale(dpr, dpr);
			ctx.translate(GUTTER_PX, 0);
		} else {
			// Worker owns the base canvas size/transform via postResize below; the
			// element still needs its CSS box set on this thread.
			workerRenderer?.postResize({
				w: logicalW,
				h: logicalH,
				dpr,
				cols,
				rows,
				metrics: m,
				bgDefault: cachedBgDefault,
				fgDefault: cachedFgDefault,
				fontFamily,
				fontWeight,
			});
		}
		canvasRef.style.width = `${logicalW}px`;
		canvasRef.style.height = `${logicalH}px`;
		overlayCanvasRef.width = logicalW * dpr;
		overlayCanvasRef.height = logicalH * dpr;
		overlayCanvasRef.style.width = `${logicalW}px`;
		overlayCanvasRef.style.height = `${logicalH}px`;
		octx.scale(dpr, dpr);
		octx.translate(GUTTER_PX, 0);
		if (
			cols > 0 &&
			rows > 0 &&
			logicalW > 0 &&
			logicalH > 0 &&
			invokeRef &&
			(cols !== lastResizeCols || rows !== lastResizeRows)
		) {
			lastResizeCols = cols;
			lastResizeRows = rows;

			rowMap.clear();
			detectedLinks.clear();
			fullRepaintNeeded = true;
			lastDisplayOffset = -1;
			invokeRef("resize_pty", { sessionId: props.sessionId, rows, cols }).catch(ipcErr("resize_pty"));
		}

		if (currentFrame) {
			fullRepaintNeeded = true;
			paintFrame(currentFrame, m);
		}
	}

	function paintFrame(frame: DecodedFrame, m: CellMetrics, dirtyIndices?: Set<number>) {
		// Base grid: main thread only. In worker mode the base is painted off-thread
		// (Option A: main decodes + overlays, worker paints the base), so the main
		// thread paints just the overlay here — that's what makes the cursor/selection/
		// search/links/scrollbar interactive in worker mode.
		if (rendererMode === "main") {
			// Base grid → shared renderer (same impl the worker uses).
			gridRenderer.paintGrid(rowMap, m, { fullRepaint: fullRepaintNeeded, dirtyIndices });
		}
		fullRepaintNeeded = false;

		// Overlay (cursor/selection/search/links/scrollbar/suggest) always stays on main.
		repaintOverlay(frame, m);

		updateScrollbar(frame);
		updateSuggestOverlay(frame, m, dirtyIndices);
	}

	function repaintOverlay(frame: DecodedFrame, m: CellMetrics) {
		octx.clearRect(-GUTTER_PX, 0, overlayCanvasRef.width / m.dpr, overlayCanvasRef.height / m.dpr);
		paintSelection(m);
		paintSearchHighlights(m);
		paintLinkUnderline(frame, m);
		paintGutterMarkers(m);
		paintBlockTimestamps(m);
		paintFoldedBlocks(m);
		paintCursor(frame, m);
	}

	function paintLinkUnderline(_frame: DecodedFrame, m: CellMetrics) {
		const maxRow = currentFrame?.screenRows || lastResizeRows;
		octx.strokeStyle = cachedFgDefault;
		octx.lineWidth = 1;

		// Dashed underline for all detected links
		if (detectedLinks.size > 0) {
			octx.globalAlpha = 0.4;
			octx.setLineDash([2, 3]);
			octx.beginPath();
			for (const [row, spans] of detectedLinks) {
				if (row < 0 || row >= maxRow) continue;
				const y = row * m.cellHeight + m.cellHeight - 1 + 0.5;
				for (const span of spans) {
					octx.moveTo(span.colStart * m.cellWidth, y);
					octx.lineTo(span.colEnd * m.cellWidth, y);
				}
			}
			octx.stroke();
			octx.setLineDash([]);
			octx.globalAlpha = 1;
		}

		// Solid underline for hovered link
		if (hoveredLink) {
			const rowSpans = hoveredLink.spans || [
				{ row: hoveredLink.row, colStart: hoveredLink.colStart, colEnd: hoveredLink.colEnd },
			];
			for (const span of rowSpans) {
				if (span.row >= 0 && span.row < maxRow) {
					const x = span.colStart * m.cellWidth;
					const w = (span.colEnd - span.colStart) * m.cellWidth;
					const y = span.row * m.cellHeight + m.cellHeight - 1 + 0.5;
					octx.beginPath();
					octx.moveTo(x, y);
					octx.lineTo(x + w, y);
					octx.stroke();
				}
			}
		}
	}

	function findNearestVisibleMatch(matches: typeof searchMatches): number {
		if (!currentFrame) return 0;
		const viewportTop = currentFrame.historySize - currentFrame.displayOffset;
		const screenLines = currentFrame.screenRows || lastResizeRows;
		const viewportBottom = viewportTop + screenLines;
		// Prefer the last visible match (closest to cursor / bottom of viewport)
		let best = 0;
		for (let i = matches.length - 1; i >= 0; i--) {
			if (matches[i].row >= viewportTop && matches[i].row < viewportBottom) {
				best = i;
				break;
			}
		}
		return best;
	}

	function scrollToMatch(match: { row: number; col_start: number; col_end: number }) {
		if (!currentFrame || !invokeRef) return;
		const viewportTop = currentFrame.historySize - currentFrame.displayOffset;
		const screenLines = currentFrame.screenRows || lastResizeRows;
		const viewportBottom = viewportTop + screenLines;
		if (match.row >= viewportTop && match.row < viewportBottom) return;
		const targetOffset = currentFrame.historySize - match.row + Math.floor(screenLines / 2);
		const clamped = Math.max(0, Math.min(targetOffset, currentFrame.historySize));
		const delta = clamped - currentFrame.displayOffset;
		if (delta !== 0) {
			invokeRef("terminal_scroll", { sessionId: props.sessionId, delta }).catch(ipcErr("terminal_scroll"));
		}
	}

	function absRowToViewport(absRow: number): number | null {
		if (!currentFrame) return null;
		const viewportTop = currentFrame.historySize - currentFrame.displayOffset;
		const viewportRow = absRow - viewportTop;
		if (viewportRow < 0 || viewportRow >= (currentFrame.screenRows || lastResizeRows)) return null;
		return viewportRow;
	}

	function paintSearchHighlights(m: CellMetrics) {
		if (searchMatches.length === 0) return;
		for (let i = 0; i < searchMatches.length; i++) {
			const match = searchMatches[i];
			const vpRow = absRowToViewport(match.row);
			if (vpRow === null) continue;
			const isActive = i === activeSearchIndex;
			const x = match.col_start * m.cellWidth;
			const y = vpRow * m.cellHeight;
			const w = (match.col_end - match.col_start) * m.cellWidth;
			octx.fillStyle = "rgba(255, 180, 50, 0.2)";
			octx.fillRect(x, y, w, m.cellHeight);
			if (isActive) {
				octx.fillStyle = "#e8984c";
				octx.fillRect(x, y + m.cellHeight - 2, w, 2);
			}
		}
	}

	function paintGutterMarkers(m: CellMetrics) {
		const term = terminalsStore.get(props.terminalId);
		if (!term) return;
		const blocks = term.commandBlocks;
		if (blocks.length === 0) return;
		for (const block of blocks) {
			if (block.exitCode === null || block.exitCode === 0) continue;
			const vpRow = absRowToViewport(block.promptLine);
			if (vpRow === null) continue;
			octx.fillStyle = "#f85149";
			octx.fillRect(-GUTTER_PX, vpRow * m.cellHeight, 3, m.cellHeight);
		}
	}

	function paintFoldedBlocks(m: CellMetrics) {
		const term = terminalsStore.get(props.terminalId);
		if (!term || term.foldedBlocks.size === 0) return;
		const fontFamily = settingsStore.getFontFamily();
		const painted = new Set<number>();
		for (const promptLine of term.foldedBlocks) {
			if (painted.has(promptLine)) continue;
			painted.add(promptLine);
			const block = term.commandBlocks.find((b) => b.promptLine === promptLine);
			if (!block?.endLine) continue;
			const foldStart = (block.executionLine ?? block.promptLine) + 1;
			const foldEnd = block.endLine;
			const foldedCount = foldEnd - foldStart;
			if (foldedCount <= 0) continue;
			const startVp = absRowToViewport(foldStart);
			if (startVp === null) continue;
			const endVp = absRowToViewport(foldEnd - 1);
			const lastVp = endVp ?? lastResizeRows - 1;
			const y = startVp * m.cellHeight;
			const h = (lastVp - startVp + 1) * m.cellHeight;
			octx.fillStyle = cachedBgDefault;
			octx.globalAlpha = 0.85;
			octx.fillRect(-GUTTER_PX, y, overlayCanvasRef.width / m.dpr, h);
			octx.globalAlpha = 1.0;
			const label = `  ··· ${foldedCount} lines folded ···`;
			octx.font = `${Math.round(m.cellHeight * 0.7)}px ${fontFamily}`;
			octx.fillStyle = "rgba(150,150,150,0.6)";
			octx.fillText(label, 4, y + m.cellHeight * 0.75);
			// Fold gutter indicator
			octx.fillStyle = "rgba(88,166,255,0.5)";
			const gutterVp = absRowToViewport(block.promptLine);
			if (gutterVp !== null) {
				octx.fillRect(-GUTTER_PX, gutterVp * m.cellHeight, 3, m.cellHeight);
			}
		}
	}

	let blockTimestampsVisible = false;

	function paintBlockTimestamps(m: CellMetrics) {
		if (!blockTimestampsVisible || !settingsStore.state.showBlockTimestamps) return;
		const term = terminalsStore.get(props.terminalId);
		if (!term) return;
		const all = term.activeBlock ? [...term.commandBlocks, term.activeBlock] : term.commandBlocks;
		if (all.length === 0) return;
		const fontFamily = settingsStore.getFontFamily();
		const fontSize = Math.round(m.cellHeight * 0.7);
		octx.font = `${fontSize}px ${fontFamily}`;
		octx.fillStyle = "rgba(150,150,150,0.5)";
		const canvasW = overlayCanvasRef.width / m.dpr;
		let lastLabelBottom = -Infinity;
		for (const block of all) {
			const vpRow = absRowToViewport(block.promptLine);
			if (vpRow === null) continue;
			const y = vpRow * m.cellHeight;
			if (y < lastLabelBottom) continue;
			const label = formatRelativeTime(Date.now() - block.startedAt);
			const tw = octx.measureText(label).width;
			octx.fillText(label, canvasW - tw - 8, y + m.cellHeight * 0.75);
			lastLabelBottom = y + m.cellHeight;
		}
	}

	function paintSelection(m: CellMetrics) {
		if (!selectionStart || !selectionEnd) return;
		const absStartRow = Math.min(selectionStart.row, selectionEnd.row);
		const absEndRow = Math.max(selectionStart.row, selectionEnd.row);

		octx.fillStyle = "rgba(58, 130, 220, 0.35)";

		for (let absRi = absStartRow; absRi <= absEndRow; absRi++) {
			const vpRow = absRowToViewport(absRi);
			if (vpRow === null) continue;
			const row = rowMap.get(vpRow);
			if (!row) continue;
			const y = vpRow * m.cellHeight;

			if (absStartRow === absEndRow) {
				const c0 = Math.min(selectionStart.col, selectionEnd.col);
				const c1 = Math.max(selectionStart.col, selectionEnd.col);
				octx.fillRect(c0 * m.cellWidth, y, (c1 - c0 + 1) * m.cellWidth, m.cellHeight);
			} else if (absRi === absStartRow) {
				const isStartFirst = selectionStart.row <= selectionEnd.row;
				const startCol = isStartFirst ? selectionStart.col : selectionEnd.col;
				octx.fillRect(startCol * m.cellWidth, y, (row.count - startCol) * m.cellWidth, m.cellHeight);
			} else if (absRi === absEndRow) {
				const isStartFirst = selectionStart.row <= selectionEnd.row;
				const endCol = isStartFirst ? selectionEnd.col : selectionStart.col;
				octx.fillRect(0, y, (endCol + 1) * m.cellWidth, m.cellHeight);
			} else {
				octx.fillRect(0, y, row.count * m.cellWidth, m.cellHeight);
			}
		}
	}

	function selectionSpansOffscreen(): boolean {
		if (!selectionStart || !selectionEnd) return false;
		const absStartRow = Math.min(selectionStart.row, selectionEnd.row);
		const absEndRow = Math.max(selectionStart.row, selectionEnd.row);
		for (let absRi = absStartRow; absRi <= absEndRow; absRi++) {
			if (absRowToViewport(absRi) === null) return true;
		}
		return false;
	}

	function getLocalSelectionText(): string {
		if (!selectionStart || !selectionEnd) return "";
		const absStartRow = Math.min(selectionStart.row, selectionEnd.row);
		const absEndRow = Math.max(selectionStart.row, selectionEnd.row);
		const lines: string[] = [];

		for (let absRi = absStartRow; absRi <= absEndRow; absRi++) {
			const vpRow = absRowToViewport(absRi);
			const row = vpRow !== null ? rowMap.get(vpRow) : null;
			if (!row) {
				lines.push("");
				continue;
			}

			let startCol = 0;
			let endCol = row.count - 1;
			if (absStartRow === absEndRow) {
				startCol = Math.min(selectionStart.col, selectionEnd.col);
				endCol = Math.max(selectionStart.col, selectionEnd.col);
			} else if (absRi === absStartRow) {
				const isStartFirst = selectionStart.row <= selectionEnd.row;
				startCol = isStartFirst ? selectionStart.col : selectionEnd.col;
			} else if (absRi === absEndRow) {
				const isStartFirst = selectionStart.row <= selectionEnd.row;
				endCol = isStartFirst ? selectionEnd.col : selectionStart.col;
			}

			let rowText = "";
			for (let c = startCol; c <= endCol; c++) {
				const cp = row.codepoints[c];
				rowText += cp === 0 ? " " : String.fromCodePoint(cp);
			}
			lines.push(rowText.replace(/\s+$/, ""));
		}

		// Remove trailing blank lines (empty rows at end of selection)
		while (lines.length > 0 && lines[lines.length - 1] === "") {
			lines.pop();
		}

		// DEFERRED (2026-05-16) — JS fallback doesn't unwrap soft-wrapped lines (no WRAPLINE flag
		// available client-side). Primary path uses Rust terminal_get_selection_text which handles it.
		return lines.join("\n");
	}

	function paintCursor(frame: DecodedFrame, m: CellMetrics) {
		if (frame.displayOffset > 0) return;
		if (!frame.cursorVisible) return;
		if (!focused()) return;
		if (!cursorBlinkOn) return;

		const settingShape: CursorShape =
			settingsStore.state.cursorStyle === "block"
				? "block"
				: settingsStore.state.cursorStyle === "underline"
					? "underline"
					: "beam";
		const shape: CursorShape = frame.cursorShape !== "block" ? frame.cursorShape : settingShape;
		const rect = computeCursorRect(shape, frame.cursorRow, frame.cursorCol, m);

		octx.fillStyle = cachedFgDefault;
		octx.fillRect(rect.x, rect.y, rect.w, rect.h);

		if (shape === "block") {
			const row = rowMap.get(frame.cursorRow);
			const col = frame.cursorCol;
			if (row && col < row.count) {
				const cp = row.codepoints[col];
				if (cp !== 0 && cp !== 0x20) {
					const fontFamily = settingsStore.getFontFamily();
					octx.font = gridRenderer.buildFontStyle(row.attrs[col], m.fontSize, fontFamily);
					octx.fillStyle = cachedBgDefault;
					octx.fillText(String.fromCodePoint(cp), rect.x, frame.cursorRow * m.cellHeight + m.baseline);
				}
			}
		}

		syncImePosition(frame.cursorRow, frame.cursorCol, m);
	}

	function syncImePosition(row: number, col: number, m: CellMetrics) {
		const x = GUTTER_PX + col * m.cellWidth;
		const y = row * m.cellHeight;
		keyInputRef.style.left = `${x}px`;
		keyInputRef.style.top = `${y}px`;
		keyInputRef.style.height = `${m.cellHeight}px`;
		keyInputRef.style.fontSize = `${m.fontSize}px`;
	}

	// --- Scrollbar ---

	function updateScrollbar(frame: DecodedFrame) {
		if (!scrollbarRef || !scrollThumbRef) return;
		const total = frame.historySize + (frame.screenRows || lastResizeRows || 24);
		const m = metrics();
		const visible = m ? lastResizeRows || Math.floor(canvasRef.clientHeight / m.cellHeight) : lastResizeRows || 24;

		if (frame.historySize === 0) {
			scrollbarRef.style.display = "none";
			return;
		}
		scrollbarRef.style.display = "block";

		const thumbRatio = Math.min(1, visible / total);
		const thumbHeight = Math.max(20, scrollbarRef.clientHeight * thumbRatio);
		const scrollRange = scrollbarRef.clientHeight - thumbHeight;
		const scrollPos = frame.historySize > 0 ? (1 - frame.displayOffset / frame.historySize) * scrollRange : scrollRange;

		scrollThumbRef.style.height = `${thumbHeight}px`;
		scrollThumbRef.style.transform = `translateY(${scrollPos}px)`;

		paintScrollbarMarks(total);
	}

	let scrollbarMarksContainer: HTMLDivElement | null = null;
	let lastScrollbarMarksKey = "";

	function paintScrollbarMarks(totalRows: number) {
		if (!scrollbarRef || !settingsStore.state.showScrollbarMarks) return;
		if (!scrollbarMarksContainer) {
			scrollbarMarksContainer = document.createElement("div");
			scrollbarMarksContainer.style.cssText =
				"position:absolute;top:0;left:0;width:100%;height:100%;pointer-events:none";
			scrollbarRef.appendChild(scrollbarMarksContainer);
		}
		const term = terminalsStore.get(props.terminalId);
		if (!term) return;
		const blocks = term.commandBlocks;
		const searchCount = searchMatches.length;
		const showBlocks = blockTimestampsVisible;
		const key = `${showBlocks ? blocks.length : 0}:${totalRows}:${showBlocks ? (blocks[blocks.length - 1]?.exitCode ?? "") : ""}:s${searchCount}:${searchCount > 0 ? searchMatches[0].row : ""}`;
		if (key === lastScrollbarMarksKey) return;
		lastScrollbarMarksKey = key;

		const trackH = scrollbarRef.clientHeight;
		let html = "";
		if (showBlocks) {
			for (const block of blocks) {
				const ratio = block.promptLine / totalRows;
				const color = block.exitCode !== null && block.exitCode !== 0 ? "#f85149" : "rgba(88,166,255,0.5)";
				html += `<div style="position:absolute;right:0;width:100%;height:2px;top:${ratio * trackH}px;background:${color}"></div>`;
			}
		}
		if (searchCount > 0) {
			const seen = new Set<number>();
			for (const match of searchMatches) {
				const rounded = Math.round((match.row / totalRows) * trackH);
				if (seen.has(rounded)) continue;
				seen.add(rounded);
				html += `<div style="position:absolute;right:0;width:100%;height:2px;top:${rounded}px;background:#e8984c"></div>`;
			}
		}
		scrollbarMarksContainer.innerHTML = html;
	}

	// --- Suggest / Intent overlay ---

	function rowToText(row: DecodedFrame["rows"][0]): string {
		let text = "";
		for (let ci = 0; ci < row.count; ci++) {
			const cp = row.codepoints[ci];
			text += cp === 0 ? " " : String.fromCodePoint(cp);
		}
		return text;
	}

	function makeOverlayDiv(top: number, height: number, background: string): HTMLDivElement {
		const div = document.createElement("div");
		div.style.cssText = `position:absolute;left:0;right:0;top:${top}px;height:${height}px;background:${background}`;
		return div;
	}

	// Cached suggest/intent overlay state to avoid full DOM rebuild
	let lastSuggestOverlayKey = "";

	function updateSuggestOverlay(_frame: DecodedFrame, m: CellMetrics, dirtyIndices?: Set<number>) {
		if (!overlayRef) return;

		// Skip full rescan if no dirty rows touch suggest/intent patterns
		if (dirtyIndices && !fullRepaintNeeded) {
			let hasSuggestContent = false;
			for (const idx of dirtyIndices) {
				const row = rowMap.get(idx);
				if (!row) continue;
				const text = rowToText(row);
				if (SUGGEST_ANCHOR_RE.test(text) || INTENT_RE.test(text)) {
					hasSuggestContent = true;
					break;
				}
			}
			if (!hasSuggestContent) {
				if (lastSuggestOverlayKey === "") return;
				// Stale overlay — fall through to rebuild/clear it
			}
		}

		const bg = cachedBgDefault;
		const numRows = lastResizeRows || 24;

		const getRowSnapshot = (i: number) => {
			const row = rowMap.get(i);
			if (!row) return null;
			return { text: rowToText(row), isWrapped: false };
		};

		// Build new overlay key to detect changes
		const parts: string[] = [];
		const newChildren: HTMLDivElement[] = [];
		for (let row = 0; row < numRows; row++) {
			const snapshot = getRowSnapshot(row);
			if (!snapshot) continue;
			const text = snapshot.text;

			if (SUGGEST_ANCHOR_RE.test(text) && isSuggestBlock(row, numRows, getRowSnapshot)) {
				newChildren.push(makeOverlayDiv(row * m.cellHeight, m.cellHeight, bg));
				parts.push(`s${row}`);
				const hiddenRows = continuationRowsAfterSuggest(row, numRows, getRowSnapshot);
				for (const contRow of hiddenRows) {
					newChildren.push(makeOverlayDiv(contRow * m.cellHeight, m.cellHeight, bg));
					parts.push(`c${contRow}`);
				}
				if (hiddenRows.length > 0) row = hiddenRows[hiddenRows.length - 1];
			} else if (INTENT_RE.test(text)) {
				newChildren.push(makeOverlayDiv(row * m.cellHeight, m.cellHeight, "rgba(181,147,90,0.12)"));
				parts.push(`i${row}`);
			}
		}

		const newKey = parts.join(",");
		if (newKey === lastSuggestOverlayKey) return;
		lastSuggestOverlayKey = newKey;

		overlayRef.textContent = "";
		for (const child of newChildren) {
			overlayRef.appendChild(child);
		}
	}

	function startBlink() {
		if (blinkInterval != null) return;
		cursorBlinkOn = true;
		blinkResetAt = performance.now();
		blinkInterval = setInterval(() => {
			const elapsed = performance.now() - blinkResetAt;
			const phase = Math.floor(elapsed / 700) % 2 === 0;
			if (cursorBlinkOn !== phase) {
				cursorBlinkOn = phase;
				if (rafId === undefined) {
					rafId = requestAnimationFrame(() => {
						rafId = undefined;
						if (!alive || hidden) return;
						const m = metrics();
						if (currentFrame && m) repaintCursorOnly(currentFrame, m);
					});
				}
			}
		}, 700);
	}

	function stopBlink() {
		if (blinkInterval != null) {
			clearInterval(blinkInterval);
			blinkInterval = undefined;
		}
	}

	function resetBlink() {
		cursorBlinkOn = true;
		blinkResetAt = performance.now();
		if (blinkInterval == null) startBlink();
	}

	function repaintCursorOnly(frame: DecodedFrame, m: CellMetrics) {
		repaintOverlay(frame, m);
	}

	function repaintCursorIfNeeded() {
		const m = metrics();
		if (currentFrame && m) repaintCursorOnly(currentFrame, m);
	}

	function onFrame(data: ArrayBuffer | number[]) {
		// Freeze-investigation: main-thread frame work (ack+decode) runs here even
		// in worker mode; a frame storm starving the rAF loop will breadcrumb here.
		markPerf("term.onFrame");
		const buffer = data instanceof ArrayBuffer ? data : new Uint8Array(data).buffer;

		// Frame receipt ordering (Option A — main decodes + overlays, worker paints):
		// ack FIRST (the ticker must not starve), then ALWAYS decode on the MAIN thread
		// — decode is cheap and keeps rowMap + currentFrame alive so the overlay
		// (cursor/selection/links/search/scrollbar) and currentFrame-driven input work
		// in worker mode too — then, in worker mode only, transfer the buffer to the
		// worker for the expensive base-grid paint. Decode runs BEFORE the transfer
		// neuters the buffer. See receiveFrame for the sacrosanct ordering.
		const timing = isFrameTimingEnabled();
		const frame = receiveFrame(buffer, {
			ack: () => invokeRef?.("ack_terminal_frame", { sessionId: props.sessionId }).catch(ipcErr("ack_terminal_frame")),
			decode: (buf) => {
				const decodeT0 = timing ? performance.now() : 0;
				const f = decodeBinaryFrame(buf);
				if (timing) recordFrameTiming(props.sessionId, "decode", performance.now() - decodeT0);
				return f;
			},
			transferToWorker:
				rendererMode === "worker" && workerRenderer ? (buf) => workerRenderer?.postFrame(buf) : undefined,
		});
		if (!frame) return;

		if (frame.bell) props.onBell?.();

		// Grid decision (geom/scroll/full-replace/scroll-wait) is shared with the
		// worker's applyFrameToGrid via decideFrameGrid, so the main overlay's rowMap
		// and the worker's paint rowMap never diverge on what to clear/merge.
		const decision = decideFrameGrid(
			{ lastScreenRows, lastScreenCols, lastDisplayOffset, lastHistorySize },
			frame,
			lastResizeRows,
		);
		const { geomChanged, scrollChanged } = decision;

		// When geometry changes, viewport is entirely different — must clear and repaint
		if (geomChanged) {
			selectionStart = null;
			selectionEnd = null;
			cachedSelectionText = "";
			rowMap.clear();
			detectedLinks.clear();
			fullRepaintNeeded = true;
		}

		if (scrollChanged || geomChanged) {
			lastDisplayOffset = frame.displayOffset;
			lastHistorySize = frame.historySize;
			lastScreenRows = frame.screenRows;
			lastScreenCols = frame.screenCols;
			if (hoveredLink) {
				hoveredLink = null;
				canvasRef.style.cursor = "text";
			}
		}

		// When backend sends all screen rows, replace rowMap to discard stale entries
		if (decision.fullReplace) {
			rowMap.clear();
			detectedLinks.clear();

			fullRepaintNeeded = true;
		} else if (decision.scrollWait) {
			// Scroll changed but only partial rows arrived. Old rowMap entries are keyed
			// to the previous viewportTop — rendering them with the new displayOffset maps
			// them to wrong screen positions, producing ghost content.
			// Clear immediately (brief blank < ~5ms) and request a full frame. The worker
			// (if any) already got this buffer and clears+waits in lock-step.
			rowMap.clear();
			detectedLinks.clear();
			fullRepaintNeeded = true;
			invokeRef?.("terminal_request_frame", { sessionId: props.sessionId }).catch(ipcErr("terminal_request_frame"));
			currentFrame = frame;
			return;
		}
		for (const row of frame.rows) {
			rowMap.set(row.index, row);
			pendingDirtyRows.add(row.index);
			scanRowForLinks(row.index);
		}

		currentFrame = frame;

		// Only compare content when the selection is fully on-screen — off-screen rows return empty
		// strings from getLocalSelectionText() causing spurious mismatches that clear the selection.
		if (selectionStart && cachedSelectionText && decision.fullReplace && !selectionSpansOffscreen()) {
			const nowText = getLocalSelectionText();
			if (nowText !== cachedSelectionText) {
				selectionStart = null;
				selectionEnd = null;
				cachedSelectionText = "";
			}
		}

		if (hidden) return;
		scheduleRepaint();
		scheduleFileLinkVerification();
	}

	// --- Link detection ---

	const WEB_URL_RE = /https?:\/\/[^\s<>"{}|\\^`[\]]+/g;
	const FILE_PATH_RE = filePathRegex();
	const FILE_URL_RE = fileUrlRegex();

	// Per-session cache: row text → verified file link spans (null = checked, none exist)
	const fileLinkCache = new Map<string, { spans: { colStart: number; colEnd: number }[] | null; ts: number }>();
	const FILE_LINK_RECHECK_MS = 3_000;
	const FILE_LINK_CACHE_MAX = 500;
	let fileLinkVerifyTimeout: ReturnType<typeof setTimeout> | undefined;

	function scanRowForLinks(rowIndex: number) {
		const row = rowMap.get(rowIndex);
		if (!row) {
			detectedLinks.delete(rowIndex);
			return;
		}
		const text = rowToText(row);
		const spans: { colStart: number; colEnd: number }[] = [];
		let match: RegExpExecArray | null;

		WEB_URL_RE.lastIndex = 0;
		while ((match = WEB_URL_RE.exec(text)) !== null) {
			spans.push({ colStart: match.index, colEnd: match.index + match[0].length });
		}

		// File paths: only underline if previously verified to exist
		const cached = fileLinkCache.get(text);
		if (cached?.spans) {
			spans.push(...cached.spans);
		}

		if (spans.length > 0) detectedLinks.set(rowIndex, spans);
		else detectedLinks.delete(rowIndex);
	}

	function scheduleFileLinkVerification() {
		if (fileLinkVerifyTimeout !== undefined) return;
		fileLinkVerifyTimeout = setTimeout(() => {
			fileLinkVerifyTimeout = undefined;
			verifyVisibleFileLinks();
		}, 150);
	}

	async function verifyVisibleFileLinks() {
		const ref = invokeRef;
		if (!ref || !alive) return;
		const maxRow = currentFrame?.screenRows || lastResizeRows;
		const cols = lastScreenCols > 0 ? lastScreenCols : currentFrame?.screenCols || 80;
		const now = Date.now();
		const toCheck: { text: string; candidates: { colStart: number; colEnd: number; raw: string }[] }[] = [];

		for (let i = 0; i < maxRow; i++) {
			const row = rowMap.get(i);
			if (!row) continue;
			const text = rowToText(row);

			const cached = fileLinkCache.get(text);
			if (cached) {
				if (cached.spans !== null) continue;
				if (now - cached.ts < FILE_LINK_RECHECK_MS) continue;
			}

			const candidates: { colStart: number; colEnd: number; raw: string }[] = [];
			FILE_PATH_RE.lastIndex = 0;
			let m: RegExpExecArray | null;
			while ((m = FILE_PATH_RE.exec(text)) !== null) {
				const idx = text.indexOf(m[1], m.index);
				candidates.push({ colStart: idx, colEnd: idx + m[1].length, raw: m[1] });
			}
			FILE_URL_RE.lastIndex = 0;
			while ((m = FILE_URL_RE.exec(text)) !== null) {
				candidates.push({ colStart: m.index, colEnd: m.index + m[0].length, raw: m[1] });
			}
			if (candidates.length > 0) toCheck.push({ text, candidates });
		}

		const termId = terminalsStore.getTerminalForSession(props.sessionId);
		const termData = termId ? terminalsStore.get(termId) : undefined;
		const cwd = termData?.cwd || "";
		let anyFound = false;

		// Single-row verification
		for (const item of toCheck) {
			if (!alive) return;
			const verified: { colStart: number; colEnd: number }[] = [];
			for (const c of item.candidates) {
				try {
					const r = (await ref("resolve_terminal_path", { cwd, candidate: c.raw })) as {
						absolute_path: string;
						is_directory: boolean;
					} | null;
					if (r) verified.push({ colStart: c.colStart, colEnd: c.colEnd });
				} catch (e) {
					appLogger.debug("terminal", "resolve_terminal_path failed", { candidate: c.raw, error: e });
				}
			}
			if (fileLinkCache.size >= FILE_LINK_CACHE_MAX) {
				const oldest = fileLinkCache.keys().next().value;
				if (oldest !== undefined) fileLinkCache.delete(oldest);
			}
			fileLinkCache.set(item.text, { spans: verified.length > 0 ? verified : null, ts: Date.now() });
			if (verified.length > 0) anyFound = true;
		}

		// Multi-row pass: detect file:// URLs spanning soft-wrapped rows.
		// Check each row that is full-width (likely wrapped) for partial file:// prefix.
		const checkedLogicalStarts = new Set<number>();
		for (let i = 0; i < maxRow; i++) {
			if (!alive) return;
			const row = rowMap.get(i);
			if (!row) continue;
			const text = rowToText(row);
			if (text.length < cols) continue; // not full-width, not wrapped
			if (!text.includes("file://")) continue;
			if (checkedLogicalStarts.has(i)) continue;
			try {
				const [startRow, logicalText] = (await ref("terminal_get_logical_line", {
					sessionId: props.sessionId,
					row: i,
				})) as [number, string];
				if (!alive) return;
				if (startRow === i && logicalText === text) continue; // single row
				checkedLogicalStarts.add(startRow);
				FILE_URL_RE.lastIndex = 0;
				let m: RegExpExecArray | null;
				while ((m = FILE_URL_RE.exec(logicalText)) !== null) {
					const matchEnd = m.index + m[0].length;
					// Only process if this match spans multiple rows
					if (Math.floor(m.index / cols) === Math.floor((matchEnd - 1) / cols)) continue;
					try {
						const r = (await ref("resolve_terminal_path", { cwd, candidate: m[1] })) as {
							absolute_path: string;
							is_directory: boolean;
						} | null;
						if (!r) continue;
						// Add spans to detectedLinks for each row
						for (let offset = m.index; offset < matchEnd; ) {
							const spanRow = startRow + Math.floor(offset / cols);
							const spanColStart = offset % cols;
							const remaining = matchEnd - offset;
							const spanColEnd = Math.min(spanColStart + remaining, cols);
							const existing = detectedLinks.get(spanRow) || [];
							existing.push({ colStart: spanColStart, colEnd: spanColEnd });
							detectedLinks.set(spanRow, existing);
							offset += spanColEnd - spanColStart;
						}
						anyFound = true;
					} catch {
						/* resolve failed */
					}
				}
			} catch {
				/* terminal_get_logical_line not available */
				break;
			}
		}

		if (anyFound) {
			for (let i = 0; i < maxRow; i++) scanRowForLinks(i);
			scheduleRepaint();
		}
	}

	let linkThrottle: ReturnType<typeof setTimeout> | undefined;

	async function checkLinksAtRow(row: number, col: number) {
		const ref = invokeRef;
		if (!ref || !alive) return;
		const gen = ++linkCheckGeneration;

		// OSC 8 hyperlinks take priority — the program explicitly tagged this cell
		try {
			const span = (await ref("terminal_hyperlink_span", {
				sessionId: props.sessionId,
				row,
				col,
			})) as [number, number, string] | null;
			if (span) {
				const [colStart, colEnd, uri] = span;
				let resolvedPath = uri;
				if (!uri.startsWith("http://") && !uri.startsWith("https://")) {
					const raw = uri.startsWith("file://") ? uri.slice(7) : uri;
					const termId = terminalsStore.getTerminalForSession(props.sessionId);
					const termData = termId ? terminalsStore.get(termId) : undefined;
					const cwd = termData?.cwd || "";
					try {
						const r = (await ref("resolve_terminal_path", { cwd, candidate: raw })) as {
							absolute_path: string;
							is_directory: boolean;
						} | null;
						if (r) resolvedPath = r.absolute_path;
					} catch {
						/* resolve failed — use raw URI */
					}
				}
				if (!alive || gen !== linkCheckGeneration) return;
				hoveredLink = { row, colStart, colEnd, path: resolvedPath };
				canvasRef.style.cursor = "pointer";
				if (currentFrame) {
					const m = metrics();
					if (m) repaintOverlay(currentFrame, m);
				}
				return;
			}
		} catch {
			/* ignore — command may not exist on older backend */
		}
		if (!alive || gen !== linkCheckGeneration) return;

		let rowText: string;
		try {
			rowText = (await ref("terminal_get_row_text", {
				sessionId: props.sessionId,
				row,
			})) as string;
		} catch {
			return;
		}
		if (!alive || gen !== linkCheckGeneration) return;

		const cacheKey = `${row}:${rowText}`;
		let links = linkCache.get(cacheKey);
		if (links === undefined) {
			const fpRe = FILE_PATH_RE;
			const fuRe = FILE_URL_RE;
			const webUrlRe = WEB_URL_RE;
			const fileMatches: { text: string; candidate: string; index: number }[] = [];
			const urlMatches: { text: string; path: string; index: number }[] = [];
			let match: RegExpExecArray | null;

			// Web URLs (no resolution needed)
			webUrlRe.lastIndex = 0;
			while ((match = webUrlRe.exec(rowText)) !== null) {
				urlMatches.push({ text: match[0], path: match[0], index: match.index });
			}

			// File paths
			fpRe.lastIndex = 0;
			while ((match = fpRe.exec(rowText)) !== null) {
				const idx = rowText.indexOf(match[1], match.index);
				fileMatches.push({ text: match[1], candidate: match[1], index: idx });
			}
			fuRe.lastIndex = 0;
			while ((match = fuRe.exec(rowText)) !== null) {
				fileMatches.push({ text: match[0], candidate: match[1], index: match.index });
			}

			if (fileMatches.length === 0 && urlMatches.length === 0) {
				linkCache.set(cacheKey, null);
				if (linkCache.size > 200) {
					const oldest = linkCache.keys().next().value;
					if (oldest !== undefined) linkCache.delete(oldest);
				}
				links = null;
			} else {
				// Resolve file paths
				const termId = terminalsStore.getTerminalForSession(props.sessionId);
				const termData = termId ? terminalsStore.get(termId) : undefined;
				const cwd = termData?.cwd || "";
				const resolvedFiles = await Promise.all(
					fileMatches.map(async (m) => {
						try {
							const r = (await ref("resolve_terminal_path", { cwd, candidate: m.candidate })) as {
								absolute_path: string;
								is_directory: boolean;
							} | null;
							if (!r) return null;
							let line: number | undefined;
							let col: number | undefined;
							const lc = m.candidate.match(/:(\d+)(?::(\d+))?$/);
							if (lc) {
								line = parseInt(lc[1], 10);
								if (lc[2]) col = parseInt(lc[2], 10);
							}
							return { text: m.text, path: r.absolute_path, line, col, index: m.index };
						} catch (e) {
							appLogger.debug("terminal", "resolve_terminal_path failed", { candidate: m.candidate, error: e });
							return null;
						}
					}),
				);
				const validFiles = resolvedFiles.filter(Boolean) as {
					text: string;
					path: string;
					line?: number;
					col?: number;
					index: number;
				}[];
				const allLinks = [...validFiles, ...urlMatches];
				links = allLinks.length > 0 ? allLinks : null;
				if (linkCache.size > 200) {
					const oldest = linkCache.keys().next().value;
					if (oldest !== undefined) linkCache.delete(oldest);
				}
				linkCache.set(cacheKey, links);
			}
		}

		hoveredLink = null;
		if (links) {
			for (const link of links) {
				const start = link.index ?? 0;
				const end = start + link.text.length;
				if (col >= start && col < end) {
					hoveredLink = { row, colStart: start, colEnd: end, path: link.path, line: link.line, col: link.col };
					break;
				}
			}
		}

		// If no single-row link found, try logical line (joins soft-wrapped rows)
		if (!hoveredLink && ref) {
			try {
				const [startRow, logicalText] = (await ref("terminal_get_logical_line", {
					sessionId: props.sessionId,
					row,
				})) as [number, string];
				if (!alive || gen !== linkCheckGeneration) return;
				if (startRow !== row || logicalText !== rowText) {
					const cols = lastScreenCols > 0 ? lastScreenCols : currentFrame?.screenCols || 80;
					const colOffset = (row - startRow) * cols;
					const logicalCol = colOffset + col;
					const fuRe = FILE_URL_RE;
					const fpRe = FILE_PATH_RE;
					const webRe = WEB_URL_RE;
					const logicalMatches: { text: string; candidate: string; index: number; isUrl: boolean }[] = [];

					fuRe.lastIndex = 0;
					let m: RegExpExecArray | null;
					while ((m = fuRe.exec(logicalText)) !== null) {
						logicalMatches.push({ text: m[0], candidate: m[1], index: m.index, isUrl: false });
					}
					fpRe.lastIndex = 0;
					while ((m = fpRe.exec(logicalText)) !== null) {
						const idx = logicalText.indexOf(m[1], m.index);
						logicalMatches.push({ text: m[1], candidate: m[1], index: idx, isUrl: false });
					}
					webRe.lastIndex = 0;
					while ((m = webRe.exec(logicalText)) !== null) {
						logicalMatches.push({ text: m[0], candidate: m[0], index: m.index, isUrl: true });
					}

					for (const lm of logicalMatches) {
						const matchEnd = lm.index + lm.text.length;
						if (logicalCol >= lm.index && logicalCol < matchEnd) {
							let resolvedPath = lm.candidate;
							if (!lm.isUrl) {
								const termId = terminalsStore.getTerminalForSession(props.sessionId);
								const termData = termId ? terminalsStore.get(termId) : undefined;
								const cwd = termData?.cwd || "";
								const r = (await ref("resolve_terminal_path", { cwd, candidate: lm.candidate })) as {
									absolute_path: string;
									is_directory: boolean;
								} | null;
								if (!alive || gen !== linkCheckGeneration) return;
								if (!r) break;
								resolvedPath = r.absolute_path;
							}
							// Build multi-row spans
							const spans: { row: number; colStart: number; colEnd: number }[] = [];
							for (let offset = lm.index; offset < matchEnd; ) {
								const spanRow = startRow + Math.floor(offset / cols);
								const spanColStart = offset % cols;
								const remaining = matchEnd - offset;
								const spanColEnd = Math.min(spanColStart + remaining, cols);
								spans.push({ row: spanRow, colStart: spanColStart, colEnd: spanColEnd });
								offset += spanColEnd - spanColStart;
							}
							const firstSpan = spans[0];
							hoveredLink = {
								row: firstSpan.row,
								colStart: firstSpan.colStart,
								colEnd: firstSpan.colEnd,
								path: resolvedPath,
								spans,
							};
							break;
						}
					}
				}
			} catch {
				/* terminal_get_logical_line not available */
			}
		}

		canvasRef.style.cursor = hoveredLink ? "pointer" : "text";
		if (currentFrame) {
			const m = metrics();
			if (m) repaintOverlay(currentFrame, m);
		}
	}

	let scrollGestureEndTimer: ReturnType<typeof setTimeout> | undefined;

	onMount(async () => {
		const overlayCtx = overlayCanvasRef.getContext("2d");
		if (!overlayCtx) {
			appLogger.error("terminal", "Failed to acquire overlay 2D context");
			return;
		}
		octx = overlayCtx;

		// Worker mode is off by default (settings.ts). Measured trade-off (2026-06-07,
		// frameTiming sched + live keyboard, see mdkb worker-firstchar-lag-load-dependent):
		// under heavy CPU load the worker adds a perceptible first-character lag — the
		// cursor (main-thread overlay) advances instantly while the glyph trails through
		// the extra decode→postMessage→worker-paint hop. The main-thread renderer keeps up
		// even during real builds (paint ~1ms/frame; backend in_flight backpressure already
		// caps frame load), so the worker's benefit is marginal. Kept as experimental opt-in.
		rendererMode = chooseRenderer(
			settingsStore.state.offscreenRenderer,
			"transferControlToOffscreen" in HTMLCanvasElement.prototype,
		);

		if (rendererMode === "worker") {
			// Off-main-thread: transfer the base canvas to the worker (irreversible,
			// once-per-node) and feed it fonts. Decode + base-grid paint run there.
			renderWorker = new Worker(new URL("./renderer.worker.ts", import.meta.url), { type: "module" });
			workerRenderer = new WorkerRenderer(renderWorker);
			// Worker returns drained frame buffers → recycle; or timing samples (only
			// when instrumentation is on) → record into this session's frameTiming rings
			// so __terminalFrameTiming.stats() reports worker paint/sched latency too.
			renderWorker.onmessage = (e: MessageEvent) => {
				if (e.data instanceof ArrayBuffer) {
					workerRenderer?.recycle(e.data);
				} else if (e.data?.type === "frameTiming") {
					recordFrameTiming(props.sessionId, e.data.kind, e.data.ms);
				}
			};
			workerRenderer.init(canvasRef);
			// Mirror the timing toggle into the worker (its own module instance), now
			// and on every future change, until this terminal is torn down.
			if (isFrameTimingEnabled()) workerRenderer.postTiming(true);
			unregisterTimingListener = onFrameTimingEnabledChange((on) => workerRenderer?.postTiming(on));
			void (async () => {
				try {
					const faces = resolveWorkerFontFaces(settingsStore.state.font);
					const payloads = await fetchFontPayloads(faces);
					workerRenderer?.postFonts(payloads);
				} catch (err) {
					appLogger.warn("terminal", "worker font prefetch failed", { error: err });
				}
			})();
		} else {
			const baseCtx = canvasRef.getContext("2d", { alpha: false });
			if (!baseCtx) {
				appLogger.error("terminal", "Failed to acquire canvas 2D context");
				return;
			}
			ctx = baseCtx;
			gridRenderer = createGridRenderer(ctx, {
				fontWeight: () => settingsStore.state.fontWeight,
				getFontFamily: () => settingsStore.getFontFamily(),
			});
		}
		installFrameTimingDebugHook();
		acquireCache();
		const fontFamily = settingsStore.getFontFamily();
		const fontSize = settingsStore.state.defaultFontSize;
		const fontWeight = settingsStore.state.fontWeight;
		await Promise.all([
			document.fonts.load(`${fontWeight} ${fontSize}px ${fontFamily}`, "M"),
			document.fonts.load(`400 ${fontSize}px "Symbols Nerd Font Mono"`, ""),
		]).catch(() => document.fonts.ready);
		remeasure();

		resizeObserver = new ResizeObserver(() => {
			clearTimeout(resizeDebounce);
			resizeDebounce = setTimeout(() => remeasure(), 100);
		});
		resizeObserver.observe(containerRef);

		// Flow control: stop acking frames when hidden, request full frame on show
		visibilityObserver = new IntersectionObserver(
			(entries) => {
				const isVisible = entries[0]?.isIntersecting ?? false;
				if (isVisible && hidden) {
					hidden = false;
					fullRepaintNeeded = true;
					lastDisplayOffset = -1;
					// Freeze-investigation: hidden→visible is the repo-switch show path.
					// Breadcrumb + burst note expose the un-staggered thundering herd.
					markPerf("term.show", { sessionId: props.sessionId });
					// Don't clear rowMap/currentFrame here — keep showing the
					// last painted content until the fresh frame arrives.
					// onFrame() replaces rowMap when a full frame arrives
					// (rows.length >= screenRowCount), so stale data is
					// naturally discarded without a blank flash.
					remeasure();
					if (focused()) startBlink();
					// If remeasure saw 0x0 (layout not yet computed after
					// display:none → display:block), retry after a frame.
					const rect = containerRef.getBoundingClientRect();
					if (rect.width <= 0 || rect.height <= 0) {
						requestAnimationFrame(() => {
							remeasure();
							noteFrameRequest();
							invokeRef?.("terminal_request_frame", { sessionId: props.sessionId }).catch(
								ipcErr("terminal_request_frame"),
							);
						});
					} else {
						noteFrameRequest();
						invokeRef?.("terminal_request_frame", { sessionId: props.sessionId }).catch(
							ipcErr("terminal_request_frame"),
						);
					}
				} else if (!isVisible && !hidden) {
					hidden = true;
					stopBlink();
					// Shrink to free the backing store while hidden. In worker mode the
					// base canvas was transferred via transferControlToOffscreen(), so
					// touching canvasRef.width/height from this thread throws
					// InvalidStateError — only the worker may resize it (see remeasure).
					// DEFERRED (2026-06-05) — worker-mode offscreen isn't shrunk while
					// hidden; would need a postResize the worker repaints on. Skipped:
					// flow control stops frames when hidden, so it just sits idle.
					if (rendererMode === "main") {
						canvasRef.width = 1;
						canvasRef.height = 1;
					}
					overlayCanvasRef.width = 1;
					overlayCanvasRef.height = 1;
					rowMap.clear();
					fileLinkCache.clear();
				}
			},
			{ threshold: 0 },
		);
		visibilityObserver.observe(containerRef);

		// DPR change: browser zoom or external monitor switch
		dprChangeHandler = () => {
			dprMediaQuery?.removeEventListener("change", dprChangeHandler!);
			dprMediaQuery = window.matchMedia(`(resolution: ${window.devicePixelRatio}dppx)`);
			dprMediaQuery.addEventListener("change", dprChangeHandler!);
			remeasure();
		};
		dprMediaQuery = window.matchMedia(`(resolution: ${window.devicePixelRatio}dppx)`);
		dprMediaQuery.addEventListener("change", dprChangeHandler);

		// Keyboard input is routed through keyInputRef (a hidden <input>) so that
		// macOS dead-key composition and IME work correctly. Canvas elements in
		// WKWebView don't fully participate in the macOS text input system, so dead
		// keys (quotes, accents, etc.) fail when keydown listeners live on the canvas.
		// When the canvas gains focus, we redirect to keyInputRef.
		canvasRef.addEventListener("focus", () => {
			keyInputRef.focus({ preventScroll: true });
		});
		keyInputRef.addEventListener("focus", () => {
			setFocused(true);
			startBlink();
			props.onFocus?.();
			if (currentFrame?.focusReporting) writePtyNoScroll("\x1b[I");
		});
		keyInputRef.addEventListener("blur", () => {
			setFocused(false);
			stopBlink();
			repaintCursorIfNeeded();
			if (currentFrame?.focusReporting) writePtyNoScroll("\x1b[O");
		});

		// Clear stray text outside of composition. During composition the input
		// must hold the in-progress text or compositionend will never resolve.
		keyInputRef.addEventListener("input", (e) => {
			if ((e as InputEvent).isComposing) return;
			keyInputRef.value = "";
		});

		// --- Keyboard ---
		const composition = createCompositionState();
		keyInputRef.addEventListener("compositionstart", () => {
			const m = metrics();
			if (currentFrame && m) syncImePosition(currentFrame.cursorRow, currentFrame.cursorCol, m);
		});
		keyInputRef.addEventListener("compositionend", (e) => {
			const data = composition.onCompositionEnd(e.data);
			if (data) writePty(data);
			queueMicrotask(() => {
				keyInputRef.value = "";
			});
		});

		let leftOptionHeld = false;

		keyInputRef.addEventListener("keydown", (e: KeyboardEvent) => {
			if (composition.shouldSuppressKeydown(e.isComposing, e.key)) {
				e.preventDefault();
				return;
			}
			resetBlink();

			if (e.ctrlKey && e.metaKey && !blockTimestampsVisible) {
				blockTimestampsVisible = true;
				fullRepaintNeeded = true;
				if (currentFrame && metrics()) paintFrame(currentFrame, metrics()!);
			}

			// Arrow Down with no modifiers: snap to bottom when scrolled up
			if (
				e.key === "ArrowDown" &&
				!e.shiftKey &&
				!e.ctrlKey &&
				!e.metaKey &&
				!e.altKey &&
				currentFrame &&
				currentFrame.displayOffset > 0
			) {
				e.preventDefault();
				invokeRef?.("terminal_scroll", { sessionId: props.sessionId, delta: -currentFrame.displayOffset }).catch(
					ipcErr("terminal_scroll"),
				);
				return;
			}

			// Cmd+Up/Down (macOS) or Ctrl+Up/Down (Win/Linux): navigate between command blocks (OSC 133)
			if ((e.metaKey || e.ctrlKey) && !e.altKey && !e.shiftKey && (e.key === "ArrowUp" || e.key === "ArrowDown")) {
				const term = terminalsStore.get(props.terminalId);
				if (term) {
					const blocks = term.commandBlocks;
					const active = term.activeBlock;
					const allPromptLines = blocks.map((b) => b.promptLine).concat(active ? [active.promptLine] : []);
					if (allPromptLines.length > 0 && currentFrame) {
						const currentViewLine = currentFrame.historySize - currentFrame.displayOffset;
						let targetLine: number | undefined;
						if (e.key === "ArrowUp") {
							for (let i = allPromptLines.length - 1; i >= 0; i--) {
								if (allPromptLines[i] < currentViewLine) {
									targetLine = allPromptLines[i];
									break;
								}
							}
						} else {
							for (let i = 0; i < allPromptLines.length; i++) {
								if (allPromptLines[i] > currentViewLine) {
									targetLine = allPromptLines[i];
									break;
								}
							}
						}
						if (targetLine !== undefined) {
							invokeRef?.("terminal_scroll_to", { sessionId: props.sessionId, line: targetLine }).catch(
								ipcErr("terminal_scroll_to"),
							);
						}
						e.preventDefault();
						return;
					}
				}
			}

			// Cmd+Shift+. (macOS) or Ctrl+Shift+. (Win/Linux): toggle fold on current block
			if (
				(e.metaKey || e.ctrlKey) &&
				e.shiftKey &&
				e.key === "." &&
				!e.altKey &&
				settingsStore.state.blockFoldingEnabled
			) {
				const term = terminalsStore.get(props.terminalId);
				if (term && currentFrame) {
					const viewTop = currentFrame.historySize - currentFrame.displayOffset;
					const blocks = [...term.commandBlocks, term.activeBlock].filter(
						Boolean,
					) as import("../../stores/terminals").CommandBlock[];
					const current = blocks.find(
						(b) => b.promptLine <= viewTop + (lastResizeRows >> 1) && (b.endLine ?? Infinity) >= viewTop,
					);
					if (current) {
						terminalsStore.toggleBlockFold(props.terminalId, current.promptLine);
						fullRepaintNeeded = true;
						if (metrics()) paintFrame(currentFrame, metrics()!);
					}
				}
				e.preventDefault();
				return;
			}

			// Force re-render: clear accumulated buffer and request fresh frame from Rust
			if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key.toLowerCase() === "l" && !e.altKey) {
				e.preventDefault();

				rowMap.clear();
				detectedLinks.clear();
				fullRepaintNeeded = true;
				currentFrame = null;
				lastDisplayOffset = -1;
				remeasure();
				invokeRef?.("terminal_request_frame", { sessionId: props.sessionId }).catch(ipcErr("terminal_request_frame"));
				return;
			}

			if ((e.metaKey || e.ctrlKey) && e.key === "f" && !e.altKey && !e.shiftKey) {
				e.preventDefault();
				props.onSearchOpen?.();
				return;
			}

			// Escape closes search when visible
			if (e.key === "Escape" && props.searchVisible) {
				e.preventDefault();
				props.onSearchClose?.();
				return;
			}

			// Resume banner: Space/Enter accept, other keys dismiss
			if (props.hasPendingResume) {
				if (e.key === " " || e.key === "Enter") {
					e.preventDefault();
					props.onResume?.();
				} else if (e.key.length === 1) {
					props.onResumeDismiss?.();
					// Let the keystroke pass through to PTY
					// macOS Right Option: send composed char directly, skip ESC prefix
					if (isMacOS() && e.altKey && !leftOptionHeld) {
						writePty(e.key);
					} else {
						const seq = keyToSequence(e);
						if (seq !== null) writePty(seq);
					}
				} else if (e.key === "Escape" || e.key === "Backspace" || e.key === "Delete" || e.key === "Tab") {
					e.preventDefault();
					props.onResumeDismiss?.();
				}
				return;
			}

			// Cmd+Enter: don't send \r to PTY — let document-level keybinding handle
			if (e.metaKey && e.key === "Enter") {
				return;
			}

			// Copy selection with the platform copy modifier (Cmd on macOS, Ctrl on Win/Linux).
			// On macOS Ctrl+C is the interrupt key — distinct from Cmd+C — so it must NOT be
			// hijacked into copy here; it falls through to the Emacs Ctrl+letter path → \x03.
			// Also fires when coords were cleared by mouseup (auto-copy) but cache is still warm.
			const copyModifier = isMacOS() ? e.metaKey : e.ctrlKey;
			if (copyModifier && e.key.toLowerCase() === "c" && ((selectionStart && selectionEnd) || cachedSelectionText)) {
				e.preventDefault();
				e.stopPropagation();
				copySelection();
				return;
			}

			// Windows Ctrl+V paste
			if (isWindows() && e.ctrlKey && !e.altKey && !e.shiftKey && !e.metaKey && (e.key === "v" || e.key === "V")) {
				e.preventDefault();
				navigator.clipboard
					.readText()
					.then((text) => {
						if (text) {
							if (currentFrame?.bracketedPaste) {
								writePty(`\x1b[200~${text}\x1b[201~`);
							} else {
								writePty(text);
							}
						}
					})
					.catch(ipcErr("clipboard_read"));
				return;
			}

			// Any keypress clears selection — full repaint to remove ghost highlights.
			// Skip modifier keys and Cmd+C/V so the chord completes before selection is dropped.
			// Include cachedSelectionText so a stale warm cache (coords already null) gets cleared
			// too — otherwise it sticks until a resize and keeps swallowing Ctrl+C into copy.
			if (
				(selectionStart || cachedSelectionText) &&
				e.key !== "Meta" &&
				e.key !== "Control" &&
				e.key !== "Alt" &&
				e.key !== "Shift" &&
				!((e.ctrlKey || e.metaKey) && (e.key.toLowerCase() === "c" || e.key.toLowerCase() === "v"))
			) {
				selectionStart = null;
				selectionEnd = null;
				cachedSelectionText = "";
				fullRepaintNeeded = true;
				scheduleRepaint();
			}

			// Shift+Enter → ESC CR (multi-line for Claude Code, Ink, etc.)
			// Must run BEFORE Kitty block — CC expects \x1b\r, not CSI 13;2 u
			if (e.key === "Enter" && e.shiftKey && !e.ctrlKey && !e.metaKey && !e.altKey) {
				e.preventDefault();
				writePty("\x1b\r");
				return;
			}

			// Shift+Tab: send CSI Z but prevent browser focus navigation
			if (e.key === "Tab" && e.shiftKey && !e.ctrlKey && !e.metaKey && !e.altKey) {
				e.preventDefault();
				writePty("\x1b[Z");
				return;
			}

			// macOS WebKit Emacs keybindings: Ctrl+A/D/E/K etc. intercepted by native
			// text system before our handler. Use e.code for reliable mapping.
			if (isMacOS() && e.ctrlKey && !e.metaKey && !e.altKey && !e.shiftKey) {
				const cm = e.code.match(/^Key([A-Z])$/);
				if (cm) {
					const ctrl = String.fromCharCode(cm[1].charCodeAt(0) - 0x40);
					e.preventDefault();
					writePty(ctrl);
					return;
				}
			}

			// Kitty keyboard protocol: encode special keys when flag 1 (disambiguate) is active
			const kbFlags = currentFrame?.keyboardFlags ?? 0;
			if (kbFlags & 1) {
				const seq = kittySequenceForKey(e.key, e.shiftKey, e.altKey, e.ctrlKey, e.metaKey);
				if (seq !== null) {
					e.preventDefault();
					writePty(seq);
					return;
				}
			}

			// macOS Alt/Option key handling
			// Left Option → ESC sequences (word-jump, backward-kill-word, etc.)
			// Right Option → compose characters (~ @ # [ ] { } on international keyboards)
			if (isMacOS() && e.altKey && !e.metaKey && !e.ctrlKey) {
				if (e.code === "AltLeft") {
					leftOptionHeld = true;
					return;
				}
				if (leftOptionHeld) {
					const altSeq = altSequenceFromCode(e);
					if (altSeq) {
						e.preventDefault();
						writePty(altSeq);
						return;
					}
				}
				// Right Option: send the composed character directly (e.g. ~ @ # [ ])
				if (!leftOptionHeld && e.key.length === 1) {
					e.preventDefault();
					writePty(e.key);
					return;
				}
			}
			if (!e.altKey) leftOptionHeld = false;

			// Default: legacy VT100 encoding
			const seq = keyToSequence(e);
			if (seq !== null) {
				e.preventDefault();
				e.stopPropagation();
				writePty(seq);
			}
		});

		// Track Alt key release for macOS left-option state
		keyInputRef.addEventListener("keyup", (e: KeyboardEvent) => {
			if (e.code === "AltLeft") leftOptionHeld = false;
			if (blockTimestampsVisible && (!e.ctrlKey || !e.metaKey)) {
				blockTimestampsVisible = false;
				fullRepaintNeeded = true;
				if (currentFrame && metrics()) paintFrame(currentFrame, metrics()!);
			}
		});

		keyInputRef.addEventListener("paste", (e: ClipboardEvent) => {
			if (e.clipboardData) {
				const items = e.clipboardData.items;
				for (let i = 0; i < items.length; i++) {
					if (items[i].type.startsWith("image/")) {
						e.preventDefault();
						writePty("\x16");
						return;
					}
				}
			}
			const text = e.clipboardData?.getData("text");
			if (text) {
				if (currentFrame?.bracketedPaste) {
					writePty(`\x1b[200~${text}\x1b[201~`);
				} else {
					writePty(text);
				}
			}
			e.preventDefault();
		});

		// --- Mouse selection ---
		let clickCount = 0;
		let lastClickTime = 0;

		canvasRef.addEventListener("mousedown", (e: MouseEvent) => {
			keyInputRef.focus({ preventScroll: true });
			if (currentFrame && currentFrame.mouseMode > 0 && !e.shiftKey) {
				const pos = canvasToGrid(e);
				// Right-click on a detected link → let the contextmenu handler fire (Open /
				// Copy link), even while an app has mouse reporting on. In WKWebView,
				// preventDefault on a right-button mousedown suppresses the contextmenu event,
				// so over a link we neither forward nor preventDefault: the app loses this one
				// right-click, but the link menu works — UI-first (see #57).
				if (e.button === 2 && detectedLinks.get(pos.row)?.some((sp) => pos.col >= sp.colStart && pos.col < sp.colEnd)) {
					return;
				}
				if (currentFrame.sgrMouse) {
					writePtyNoScroll(sgrMouseSequence(e.button, pos.col, pos.row, true, e));
				}
				e.preventDefault();
				return;
			}
			if (e.button !== 0) return;
			const pos = canvasToGrid(e);
			const absRow = viewportRowToAbs(pos.row);
			if (absRow === null) return;

			// Gutter click: select entire block output
			{
				const rect = canvasRef.getBoundingClientRect();
				const rawX = e.clientX - rect.left;
				if (rawX < GUTTER_PX) {
					const term = terminalsStore.get(props.terminalId);
					if (term) {
						const allBlocks = [...term.commandBlocks, term.activeBlock].filter(
							Boolean,
						) as import("../../stores/terminals").CommandBlock[];
						const block = allBlocks.find((b) => b.promptLine <= absRow && (b.endLine ?? Infinity) >= absRow);
						if (block) {
							const startRow = (block.executionLine ?? block.promptLine) + 1;
							const endRow = (block.endLine ?? absRow) - 1;
							if (endRow >= startRow) {
								selectionStart = { row: startRow, col: 0 };
								selectionEnd = { row: endRow, col: lastResizeCols - 1 };
								selecting = false;
								fullRepaintNeeded = true;
								scheduleRepaint();
								e.preventDefault();
								return;
							}
						}
					}
				}
			}

			const absPos = { col: pos.col, row: absRow };

			// Shift+click: extend selection from existing anchor
			if (e.shiftKey && selectionStart) {
				selectionEnd = absPos;
				selecting = true;
				fullRepaintNeeded = true;
				scheduleRepaint();
				return;
			}

			const now = Date.now();

			if (now - lastClickTime < 400) {
				clickCount++;
			} else {
				clickCount = 1;
			}
			lastClickTime = now;

			if (clickCount === 2) {
				const vpRow = absRowToViewport(absRow);
				const row = vpRow !== null ? rowMap.get(vpRow) : null;
				if (row) {
					const isWordChar = (col: number) => {
						if (col < 0 || col >= row.count) return false;
						const cp = row.codepoints[col];
						if (cp === 0 || cp === 32) return false;
						const ch = String.fromCodePoint(cp);
						return !/[\s\t\x00-\x1f\x7f "'`(){}[\]<>|;:,.!?@#$%^&*~=+/\\]/.test(ch);
					};
					let left = pos.col;
					let right = pos.col;
					while (left > 0 && isWordChar(left - 1)) left--;
					while (right < row.count - 1 && isWordChar(right + 1)) right++;
					if (isWordChar(pos.col)) {
						selectionStart = { col: left, row: absRow };
						selectionEnd = { col: right, row: absRow };
					} else {
						selectionStart = absPos;
						selectionEnd = absPos;
					}
				} else {
					selectionStart = absPos;
					selectionEnd = absPos;
				}
			} else if (clickCount >= 3) {
				const m = metrics();
				const maxCol = m ? Math.floor(canvasRef.getBoundingClientRect().width / m.cellWidth) - 1 : 79;
				selectionStart = { col: 0, row: absRow };
				selectionEnd = { col: maxCol, row: absRow };
				clickCount = 3;
			} else {
				selectionStart = absPos;
				selectionEnd = null;
			}
			selecting = true;
			fullRepaintNeeded = true;
			scheduleRepaint();
		});

		const onMouseMove = (e: MouseEvent) => {
			if (currentFrame && currentFrame.mouseMode > 0 && !e.shiftKey) {
				if (currentFrame.mouseMode >= 3) {
					const pos = canvasToGrid(e);
					writePtyNoScroll(sgrMouseSequence(35, pos.col, pos.row, true, e));
				} else if (currentFrame.mouseMode >= 2 && e.buttons > 0) {
					const pos = canvasToGrid(e);
					const btn = e.buttons & 1 ? 0 : e.buttons & 4 ? 1 : 2;
					writePtyNoScroll(sgrMouseSequence(32 + btn, pos.col, pos.row, true, e));
				}
				return;
			}

			if (selecting && selectionStart) {
				const rect = canvasRef.getBoundingClientRect();
				const m = metrics();
				if (m) {
					const yAbove = rect.top - e.clientY;
					const yBelow = e.clientY - rect.bottom;
					if (yAbove > 0) {
						const rows = Math.ceil(yAbove / m.cellHeight);
						startSelectionScroll(rows);
					} else if (yBelow > 0) {
						const rows = Math.ceil(yBelow / m.cellHeight);
						startSelectionScroll(-rows);
					} else {
						stopSelectionScroll();
					}
				}
				const pos = canvasToGrid(e);
				const absRow = viewportRowToAbs(pos.row);
				if (absRow === null) return;
				selectionEnd = { col: pos.col, row: absRow };
				const mRepaint = metrics();
				if (currentFrame && mRepaint) paintFrame(currentFrame, mRepaint);
			}

			// Link detection (throttled)
			if (!selecting) {
				clearTimeout(linkThrottle);
				linkThrottle = setTimeout(() => {
					const pos = canvasToGrid(e);
					checkLinksAtRow(pos.row, pos.col);
				}, 100);
			}
		};

		const onMouseUp = (e: MouseEvent) => {
			if (currentFrame && currentFrame.mouseMode > 0 && !e.shiftKey) {
				const pos = canvasToGrid(e);
				if (currentFrame.sgrMouse) {
					writePtyNoScroll(sgrMouseSequence(e.button, pos.col, pos.row, false, e));
				}
				return;
			}
			stopSelectionScroll();
			if (selecting && selectionStart && selectionEnd) {
				if (selectionStart.row !== selectionEnd.row || selectionStart.col !== selectionEnd.col) {
					copySelection();
				} else {
					selectionStart = null;
					selectionEnd = null;
					fullRepaintNeeded = true;
					scheduleRepaint();
				}
			}
			selecting = false;
		};

		document.addEventListener("mousemove", onMouseMove);
		document.addEventListener("mouseup", onMouseUp);

		// Link click — plain click opens, skip if user was selecting text
		canvasRef.addEventListener("click", () => {
			if (!hoveredLink) return;
			const dragged =
				selectionStart &&
				selectionEnd &&
				(selectionStart.row !== selectionEnd.row || selectionStart.col !== selectionEnd.col);
			if (dragged) return;
			openLink(hoveredLink);
		});

		// Right-click on a detected link → context menu (Open / Copy link).
		// Only when the click lands on a link span; elsewhere the default is left alone.
		canvasRef.addEventListener("contextmenu", async (e: MouseEvent) => {
			const pos = canvasToGrid(e);
			const onLink = detectedLinks.get(pos.row)?.some((sp) => pos.col >= sp.colStart && pos.col < sp.colEnd);
			if (!onLink) return;
			e.preventDefault();
			// Stop the App-level terminal context menu (#terminal-panes onContextMenu)
			// from also opening and covering our Open/Copy-link menu.
			e.stopPropagation();
			await checkLinksAtRow(pos.row, pos.col);
			if (!hoveredLink) return;
			setLinkMenuTarget({ path: hoveredLink.path, line: hoveredLink.line, col: hoveredLink.col });
			linkMenu.openAt(e.clientX, e.clientY);
		});

		// --- Scroll ---

		function resetScrollGesture() {
			scrollAccumPx = 0;
			scrollGestureDistPx = 0;
		}

		function handleWheel(e: WheelEvent) {
			e.preventDefault();
			e.stopPropagation();
			if (currentFrame && currentFrame.mouseMode > 0) {
				const pos = canvasToGrid(e as unknown as MouseEvent);
				const btn = e.deltaY < 0 ? 64 : 65;
				writePtyNoScroll(sgrMouseSequence(btn, pos.col, pos.row, true, e as unknown as MouseEvent));
				return;
			}
			const m = metrics();
			const ch = m?.cellHeight ?? 20;
			const screenPx = ch * (lastResizeRows || 24);

			const dy = e.deltaY;
			const atBottom = currentFrame && currentFrame.displayOffset === 0;
			const atTop = currentFrame && currentFrame.displayOffset >= currentFrame.historySize;
			if ((atBottom && dy > 0) || (atTop && dy < 0)) return;

			scrollGestureDistPx += Math.abs(dy);
			const excess = Math.max(0, scrollGestureDistPx - screenPx);
			const factor = 0.5 + 0.5 * (excess / screenPx);
			scrollAccumPx += dy * factor;

			const lines = Math.trunc(scrollAccumPx / ch);
			if (lines !== 0) {
				scrollAccumPx -= lines * ch;
				invokeRef?.("terminal_scroll", { sessionId: props.sessionId, delta: -lines }).catch(ipcErr("terminal_scroll"));
			}

			clearTimeout(scrollGestureEndTimer);
			scrollGestureEndTimer = setTimeout(resetScrollGesture, 200);
		}
		canvasRef.addEventListener("wheel", handleWheel, { passive: false });
		scrollbarRef.addEventListener("wheel", handleWheel, { passive: false });

		// Scrollbar drag
		let scrollDragging = false;
		let scrollDragStartY = 0;
		let scrollDragStartOffset = 0;

		// Scrollbar track click: jump to position
		scrollbarRef.addEventListener("mousedown", (e: MouseEvent) => {
			if (e.target === scrollThumbRef) return; // thumb has its own handler
			if (!currentFrame || currentFrame.historySize === 0) return;
			e.preventDefault();
			const rect = scrollbarRef.getBoundingClientRect();
			const clickRatio = (e.clientY - rect.top) / rect.height;
			const targetOffset = Math.round((1 - clickRatio) * currentFrame.historySize);
			const delta = targetOffset - currentFrame.displayOffset;
			if (delta !== 0) {
				invokeRef?.("terminal_scroll", { sessionId: props.sessionId, delta }).catch(ipcErr("terminal_scroll"));
			}
		});

		scrollThumbRef.addEventListener("mousedown", (e: MouseEvent) => {
			e.preventDefault();
			e.stopPropagation();
			scrollDragging = true;
			scrollDragStartY = e.clientY;
			scrollDragStartOffset = currentFrame?.displayOffset ?? 0;
		});

		const onScrollDragMove = (e: MouseEvent) => {
			if (!scrollDragging || !currentFrame) return;
			const historySize = currentFrame.historySize;
			if (historySize === 0) return;
			const trackHeight = scrollbarRef.clientHeight;
			const thumbHeight = parseFloat(scrollThumbRef.style.height) || 20;
			const scrollRange = trackHeight - thumbHeight;
			if (scrollRange <= 0) return;

			const dy = e.clientY - scrollDragStartY;
			const offsetDelta = Math.round((dy / scrollRange) * historySize);
			const newOffset = Math.max(0, Math.min(historySize, scrollDragStartOffset - offsetDelta));
			const delta = newOffset - currentFrame.displayOffset;
			if (delta !== 0) {
				invokeRef?.("terminal_scroll", { sessionId: props.sessionId, delta }).catch(ipcErr("terminal_scroll"));
			}
		};

		const onScrollDragUp = () => {
			scrollDragging = false;
		};

		document.addEventListener("mousemove", onScrollDragMove);
		document.addEventListener("mouseup", onScrollDragUp);

		// Touch input (mobile/tablet)
		cleanupTouch = installTouchHandlers(canvasRef, touchTextareaRef, {
			onScrollPixels: (dy) => {
				const m = metrics();
				const ch = m?.cellHeight ?? 20;
				const screenPx = ch * (lastResizeRows || 24);
				scrollGestureDistPx += Math.abs(dy);
				const excess = Math.max(0, scrollGestureDistPx - screenPx);
				const factor = 0.5 + 0.5 * (excess / screenPx);
				scrollAccumPx += dy * factor;
				const lines = Math.trunc(scrollAccumPx / ch);
				if (lines !== 0) {
					scrollAccumPx -= lines * ch;
					invokeRef?.("terminal_scroll", { sessionId: props.sessionId, delta: -lines }).catch(
						ipcErr("terminal_scroll"),
					);
				}
			},
			onScrollEnd: resetScrollGesture,
			onInput: (data) => writePty(data),
			onFocus: () => {
				setFocused(true);
				startBlink();
				props.onFocus?.();
			},
			onFontSizeChange: (delta) => {
				const cur = settingsStore.state.defaultFontSize;
				const next = Math.round(cur + delta);
				if (next !== cur) settingsStore.setDefaultFontSize(next);
			},
			onSelectionMode: () => {
				/* future: enter selection UI */
			},
		});

		// Subscribe to grid channel via transport abstraction
		try {
			transport = createTransport(props.sessionId);
			invokeRef = (cmd, args) => transport!.invoke(cmd, args);
			await transport.subscribe((data) => onFrame(data));
			unsubscribe = () => {
				document.removeEventListener("mousemove", onMouseMove);
				document.removeEventListener("mouseup", onMouseUp);
				document.removeEventListener("mousemove", onScrollDragMove);
				document.removeEventListener("mouseup", onScrollDragUp);
				stopSelectionScroll();
				transport?.unsubscribe();
			};
		} catch (e) {
			appLogger.error("terminal", "Failed to subscribe to terminal grid channel", {
				sessionId: props.sessionId,
				error: e,
			});
			unsubscribe = () => {
				document.removeEventListener("mousemove", onMouseMove);
				document.removeEventListener("mouseup", onMouseUp);
				document.removeEventListener("mousemove", onScrollDragMove);
				document.removeEventListener("mouseup", onScrollDragUp);
				stopSelectionScroll();
			};
		}

		// Listen for session events via transport
		if (transport) {
			await transport.onEvent("cwd", (payload) => {
				const cwd = (payload as { cwd: string }).cwd ?? (payload as string);
				terminalsStore.update(props.terminalId, { cwd });
				props.onCwdChange?.(props.terminalId, cwd);
			});
			await transport.onEvent("osc133", (payload) => {
				const { marker, line, exit_code } = payload as { marker: string; line: number; exit_code: number | null };
				terminalsStore.handleOsc133(props.terminalId, marker, line, exit_code ?? undefined);
			});
			await transport.onEvent("output", (payload) => {
				const { data } = payload as { data: string };
				pluginRegistry.processRawOutput(data, props.sessionId);
			});
		}

		props.onRef?.({
			focus: () => keyInputRef.focus({ preventScroll: true }),
			getSelectionText: () => cachedSelectionText,
			refresh: () => {
				rowMap.clear();
				detectedLinks.clear();
				fullRepaintNeeded = true;
				currentFrame = null;
				lastDisplayOffset = -1;
				lastResizeCols = 0;
				lastResizeRows = 0;
				remeasure();
				invokeRef?.("terminal_request_frame", { sessionId: props.sessionId }).catch(ipcErr("terminal_request_frame"));
			},
			resubscribe: async () => {
				await transport?.resubscribe();
			},
			searchFind: async (query: string, blockScope?: boolean) => {
				if (!query || !invokeRef) {
					searchMatches = [];
					activeSearchIndex = -1;
					const m = metrics();
					if (currentFrame && m) paintFrame(currentFrame, m);
					return { index: -1, count: 0 };
				}
				let matches = (await invokeRef("terminal_search", {
					sessionId: props.sessionId,
					query,
				})) as { row: number; col_start: number; col_end: number }[];
				if (blockScope && currentFrame) {
					const term = terminalsStore.get(props.terminalId);
					if (term) {
						const allBlocks = term.activeBlock ? [...term.commandBlocks, term.activeBlock] : term.commandBlocks;
						const viewTop = currentFrame.historySize - currentFrame.displayOffset;
						const viewCenter = viewTop + Math.floor(currentFrame.screenRows / 2);
						matches = filterMatchesToBlock(matches, allBlocks, viewCenter);
					}
				}
				searchMatches = matches;
				if (matches.length > 0) {
					activeSearchIndex = findNearestVisibleMatch(matches);
					scrollToMatch(matches[activeSearchIndex]);
				} else {
					activeSearchIndex = -1;
				}
				const m = metrics();
				if (currentFrame && m) paintFrame(currentFrame, m);
				return { index: activeSearchIndex, count: matches.length };
			},
			searchNext: () => {
				if (searchMatches.length === 0) return { index: -1, count: 0 };
				activeSearchIndex = (activeSearchIndex + 1) % searchMatches.length;
				scrollToMatch(searchMatches[activeSearchIndex]);
				const m = metrics();
				if (currentFrame && m) paintFrame(currentFrame, m);
				return { index: activeSearchIndex, count: searchMatches.length };
			},
			searchPrev: () => {
				if (searchMatches.length === 0) return { index: -1, count: 0 };
				activeSearchIndex = (activeSearchIndex - 1 + searchMatches.length) % searchMatches.length;
				scrollToMatch(searchMatches[activeSearchIndex]);
				const m = metrics();
				if (currentFrame && m) paintFrame(currentFrame, m);
				return { index: activeSearchIndex, count: searchMatches.length };
			},
			searchClear: () => {
				searchMatches = [];
				activeSearchIndex = -1;
				const m = metrics();
				if (currentFrame && m) paintFrame(currentFrame, m);
			},
			paste: (text: string) => {
				if (currentFrame?.bracketedPaste) {
					writePty(`\x1b[200~${text}\x1b[201~`);
				} else {
					writePty(text);
				}
			},
		});
	});

	createEffect(() => {
		terminalsStore.state.terminals[props.terminalId]?.fontSize;
		settingsStore.state.defaultFontSize;
		settingsStore.state.font;
		settingsStore.state.fontWeight;
		if (!alive) return;
		settingsStore.state.theme;
		invalidateGlyphCache();
		// Worker mode: caches live in the worker's gridRenderer; remeasure()'s
		// postResize re-pushes theme/font and the worker invalidates on resize.
		gridRenderer?.invalidateCaches();
		fullRepaintNeeded = true;
		remeasure();
	});

	async function copySelection() {
		const setStatus = (window as unknown as Record<string, unknown>).__tuic_setStatusInfo as
			| ((msg: string) => void)
			| undefined;
		try {
			let text: string;
			if (selectionSpansOffscreen() && invokeRef && selectionStart && selectionEnd) {
				text = (await invokeRef("terminal_get_selection_text", {
					sessionId: props.sessionId,
					startRow: selectionStart.row,
					startCol: selectionStart.col,
					endRow: selectionEnd.row,
					endCol: selectionEnd.col,
				})) as string;
			} else {
				text = getLocalSelectionText();
			}
			if (text) {
				cachedSelectionText = text;
				await navigator.clipboard.writeText(text);
				setStatus?.("Copied to clipboard");
			}
		} catch (e) {
			appLogger.warn("terminal", "Clipboard write failed", { error: e });
			setStatus?.("Copy failed — clipboard unavailable");
		}
	}

	onCleanup(() => {
		alive = false;
		stopBlink();
		if (rafId !== undefined) {
			cancelAnimationFrame(rafId);
			rafId = undefined;
		}
		clearTimeout(resizeDebounce);
		resizeObserver?.disconnect();
		visibilityObserver?.disconnect();
		if (dprChangeHandler) dprMediaQuery?.removeEventListener("change", dprChangeHandler);
		unsubscribe?.();
		cleanupTouch?.();
		clearTimeout(linkThrottle);
		clearTimeout(fileLinkVerifyTimeout);
		clearTimeout(scrollGestureEndTimer);
		linkCache.clear();
		fileLinkCache.clear();
		resetFrameTiming(props.sessionId);
		rowMap.clear();
		detectedLinks.clear();
		gridRenderer?.invalidateCaches();
		// Tear down the render worker (no leak): terminate + null refs. The base
		// canvas was transferred to it, so this node's worker is single-use.
		unregisterTimingListener?.();
		unregisterTimingListener = null;
		renderWorker?.terminate();
		renderWorker = null;
		workerRenderer = null;
		releaseCache();
	});

	return (
		<div
			ref={containerRef!}
			data-terminal-container
			style={{
				position: "relative",
				width: "100%",
				height: "100%",
				overflow: "hidden",
			}}
			onDragOver={(e) => {
				if (e.dataTransfer?.types?.includes("application/x-tuic-path")) {
					e.preventDefault();
					e.dataTransfer.dropEffect = "copy";
				}
			}}
			onDrop={(e) => {
				const path = e.dataTransfer?.getData("application/x-tuic-path");
				if (!path) return;
				e.preventDefault();
				const quoted = `'${path.replace(/'/g, "'\\''")}' `;
				writePty(quoted);
				keyInputRef.focus({ preventScroll: true });
			}}
		>
			{/* Offscreen textarea for mobile virtual keyboard input */}
			<textarea
				ref={touchTextareaRef!}
				style={{
					position: "fixed",
					top: "-9999px",
					left: "-9999px",
					width: "1px",
					height: "1px",
					opacity: "0",
					"pointer-events": "none",
				}}
				autocomplete="off"
				autocorrect="off"
				autocapitalize="off"
				spellcheck={false}
				tabIndex={-1}
			/>
			{/* Hidden input that receives all keyboard events including dead-key composition.
			    Canvas elements in WKWebView don't participate in the macOS text input system,
			    so dead keys (quotes, accents, etc.) are lost when listeners live on the canvas.
			    Using a real <input> fixes composition on macOS without affecting rendering. */}
			<input
				ref={keyInputRef!}
				type="text"
				aria-hidden="true"
				style={{
					position: "absolute",
					top: "0",
					left: "0",
					width: "1px",
					height: "1em",
					opacity: "0",
					border: "none",
					outline: "none",
					padding: "0",
					margin: "0",
					overflow: "hidden",
					"pointer-events": "none",
					"font-size": "1px",
					"z-index": "-1",
				}}
				tabIndex={-1}
				autocomplete="off"
				autocorrect="off"
				autocapitalize="off"
				spellcheck={false}
			/>
			<canvas
				ref={canvasRef!}
				style={{
					display: "block",
					outline: "none",
					cursor: "text",
				}}
				tabIndex={0}
			/>
			{/* Overlay canvas: cursor, selection, search highlights — redrawn every frame without touching base canvas */}
			<canvas
				ref={overlayCanvasRef!}
				style={{
					position: "absolute",
					top: "0",
					left: "0",
					"pointer-events": "none",
				}}
			/>
			{/* Suggest/intent overlay */}
			<div
				ref={overlayRef!}
				style={{
					position: "absolute",
					top: "0",
					left: "0",
					right: "0",
					bottom: "0",
					"pointer-events": "none",
					"z-index": "10",
					overflow: "hidden",
				}}
			/>
			{/* Scrollbar */}
			<div
				ref={scrollbarRef!}
				style={{
					position: "absolute",
					top: "0",
					right: "0",
					width: "14px",
					height: "100%",
					display: "none",
					"z-index": "20",
				}}
			>
				<div
					ref={scrollThumbRef!}
					style={{
						width: "10px",
						"margin-left": "2px",
						"border-radius": "5px",
						background: "var(--fg-primary, rgba(255,255,255,0.3))",
						opacity: "var(--scrollbar-opacity, 0.3)",
						"min-height": "20px",
						position: "absolute",
						top: "0",
						cursor: "grab",
					}}
				/>
			</div>
			<ContextMenu
				items={[
					{
						label: "Open",
						action: () => {
							const t = linkMenuTarget();
							if (t) openLink(t);
						},
					},
					{
						label: "Copy link",
						action: () => {
							const t = linkMenuTarget();
							if (t) copyLink(t);
						},
					},
				]}
				x={linkMenu.position().x}
				y={linkMenu.position().y}
				visible={linkMenu.visible()}
				onClose={() => linkMenu.close()}
			/>
		</div>
	);
};

export default CanvasTerminal;
