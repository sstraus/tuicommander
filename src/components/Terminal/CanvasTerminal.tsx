import { type Component, createEffect, createSignal, onCleanup, onMount, Show, untrack } from "solid-js";
import { lastMenuActionTime } from "../../menuDedup";
import { isMacOS, isWindows } from "../../platform";
import { pluginRegistry } from "../../plugins/pluginRegistry";
import { appLogger } from "../../stores/appLogger";
import { settingsStore } from "../../stores/settings";
import { reclaimParkedTerminal } from "../../stores/terminalOwnership";
import { terminalsStore } from "../../stores/terminals";
import { toastsStore } from "../../stores/toasts";
import { getSessionConnection } from "../../transportRuntime";
import { filterMatchesToBlock } from "../../utils/blockSearchFilter";
import { writeClipboard } from "../../utils/clipboard";
import { formatRelativeTime } from "../../utils/formatRelativeTime";
import { ensureKeyboardViewportTracking, keyboardOcclusion } from "../../utils/keyboardViewport";
import { handleOpenUrl } from "../../utils/openUrl";
import { isImagePaste } from "../../utils/pastedImage";
import { isPerfDebug } from "../../utils/perfDebug";
import { markPerf, noteFrameRequest } from "../../utils/perfTrace";
import { applyPinchFontDelta } from "../../utils/terminalZoom";
import { ContextMenu, createContextMenu } from "../ContextMenu/ContextMenu";
import { AnswersPanel } from "./AnswersPanel";
import { type AnswersTurn, newTurnCache, readAnswersHistory, sameAnswersHistory } from "./answersTurn";
import { createCanvasTerminalBindings } from "./canvasTerminalBindings";
import { canvasToGrid, sgrMouseSequence } from "./canvasTerminalInputGeometry";
import {
	createCanvasLinkController,
	createLinkPressTracker,
	isOverSpan,
	linkClaimsPress,
	linkCovers,
	spanAt,
} from "./canvasTerminalLinks";
import { paintCursor, paintSearchHighlights, paintSelection, syncImePosition, thumbFor } from "./canvasTerminalPaint";
import { createCanvasScrollController, ROW_CACHE_CHUNK } from "./canvasTerminalScroll";
import {
	commitSelectionCopy,
	createCanvasSearchController,
	createCanvasSelectionController,
	selectionRowToGridRow,
	selectionRowToViewport,
	shouldValidateSelectionSnapshot,
	viewportRowToSelectionRow,
} from "./canvasTerminalSelection";
import {
	getLocalSelectionText,
	type HoveredLink,
	hoverSignature,
	logicalCellToStringOffset,
	logicalStringSpanToCells,
	rowStringSpanToCells,
	underlinedText,
	underlinedTextAt,
} from "./canvasTerminalText";
import { installTouchHandlers } from "./canvasTerminalTouch";
import { createTransport, type TerminalTransport, toBinaryPayload } from "./canvasTerminalTransport";
import {
	type CellMetrics,
	type CursorShape,
	cellText,
	createHiddenAckThrottle,
	createLeadingThrottle,
	type DecodedFrame,
	type DecodedRow,
	decideFrameGrid,
	decodeBinaryFrame,
	decodeStyledRange,
	GUTTER_PX,
	gridDimsForBox,
	HIDDEN_ACK_INTERVAL_MS,
	installFrameRows,
	reconcileDelay,
	rowText,
	type StyledRange,
	shouldFireReconcile,
	snapLineHeight,
	textSpanToCellRanges,
} from "./canvasTerminalUtils";
import { installFrameTimingDebugHook, isFrameTimingEnabled, recordFrameTiming, resetFrameTiming } from "./frameTiming";
import { acquireCache, getSharedMetrics, invalidateGlyphCache, releaseCache } from "./glyphCache";
import { createGridRenderer, type GridRenderer } from "./gridRenderer";
import { kittySequenceForKey } from "./kittyKeyboard";
import { filePathRegex, fileUrlRegex, matchWebUrls } from "./linkProvider";
import { buildScrollbarMarksHtml } from "./scrollbarMarks";
import {
	ANSWER_MARKER_RE,
	INTENT_HIGHLIGHT_RE,
	paintOverlayBlocks,
	planSuggestOverlay,
	SUGGEST_ANCHOR_RE,
} from "./suggestOverlay";
import {
	altSequenceFromCode,
	createCompositionState,
	isPointerInsideRect,
	keyToSequence,
	shouldReportMouseUp,
} from "./terminalInput";
import { cssColorToRgb, publishTerminalPalette } from "./terminalPalette";
import { openTerminalPathLink } from "./terminalPathLink";
import { latestIntersectionVisibility, retryUntilMeasured, SIZE_RETRY_MAX_FRAMES } from "./visibilityLifecycle";

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

/** Shortest gap between two runs of the open search.
 *  A redrawing TUI emits frames far faster than a regex sweep over the whole
 *  scrollback is worth running, so the sweep is bounded to one per window —
 *  bounded, not postponed: waiting for the frames to stop meant never running. */
const SEARCH_REFRESH_THROTTLE_MS = 150;

const CanvasTerminal: Component<CanvasTerminalProps> = (props) => {
	let canvasRef!: HTMLCanvasElement;
	let overlayCanvasRef!: HTMLCanvasElement;
	let keyInputRef!: HTMLInputElement;
	let scrollbarRef!: HTMLDivElement;
	let scrollThumbRef!: HTMLDivElement;
	let overlayRef!: HTMLDivElement;
	let containerRef!: HTMLDivElement;
	// Smooth-scroll stage: wraps base + overlay canvases and gets a transient
	// translateY during a scroll gesture (snaps back to 0 on a line boundary).
	let stageRef!: HTMLDivElement;
	// Wraps the stage; gets a translateY to slide ONLY this terminal up so the
	// cursor stays visible above the on-screen keyboard on touch devices. Clipped
	// by containerRef's overflow:hidden; independent of the stage's scroll transform.
	let kbLiftRef!: HTMLDivElement;
	// Behind the base canvas: paints only the one row above and one below the
	// viewport, revealed as the stage slides. Never used for hit-testing.
	let overscanCanvasRef!: HTMLCanvasElement;
	let ctx!: CanvasRenderingContext2D;
	let octx!: CanvasRenderingContext2D;
	let octxOverscan: CanvasRenderingContext2D | null = null;
	let overscanRenderer: GridRenderer | null = null;
	// Client-side styled-row cache for smooth scroll, keyed by the backend's
	// eviction-stable absolute row index (`historyBase + grid-relative`, where
	// historyBase counts lines already dropped from the history top). A physical line
	// keeps its key for life — even once the scrollback cap rotates — so a cached row
	// can never alias onto a different line and ghost/duplicate during a scroll.
	// `requestedChunks` dedupes background range prefetches.
	const scroll = createCanvasScrollController();
	const rowCache = scroll.rowCache;
	const requestedChunks = scroll.requestedChunks;
	// Base-grid renderer (the canvas2d paint implementation). Created in onMount
	// once ctx exists.
	let gridRenderer!: GridRenderer;

	const [metrics, setMetrics] = createSignal<CellMetrics | null>(null);
	const [focused, setFocused] = createSignal(false);
	const isTouchDevice = navigator.maxTouchPoints > 0 || "ontouchstart" in window;
	let currentFrame: DecodedFrame | null = null;
	let lastDisplayOffset = -1;
	let lastAltScreen = false;
	let screenGeneration = 0;
	let lastScreenRows = -1;
	let lastScreenCols = -1;
	const search = createCanvasSearchController();
	// Live search query, kept so incoming frames can re-run it. Search matches are
	// anchored to ABSOLUTE rows, but a TUI (ink agents, vim, lazygit) rewrites the
	// live screen rows in place: the text under a highlight changes while the
	// highlight stays pinned, painting an orange box over cells that no longer
	// match. Rewritten rows drop their matches immediately; this re-search restores
	// the ones that still hit.
	let searchQuery = "";
	let searchBlockScope = false;
	/** Leading-edge, so a continuously redrawing TUI still refreshes the search.
	 *  A trailing debounce reset its timer on every frame and so never fired. */
	const searchRefresh = createLeadingThrottle(() => {
		// Nobody awaits this one, so it needs its own catch: the backend read
		// can reject (a closed session over HTTP, a failed blocking-pool task),
		// and an unhandled rejection from a timer is invisible until it isn't.
		// The matches simply stay as they were until the next frame retriggers.
		runSearchQuery(false).catch((e) => {
			appLogger.warn("terminal", "search refresh failed", { error: String(e) });
		});
	}, SEARCH_REFRESH_THROTTLE_MS);
	let cursorBlinkOn = true;
	let blinkInterval: ReturnType<typeof setInterval> | undefined;
	let blinkResetAt = 0;
	let unsubscribe: (() => void) | undefined;
	let resizeObserver: ResizeObserver | undefined;
	let visibilityObserver: IntersectionObserver | undefined;
	let lastResizeCols = 0;
	let lastResizeRows = 0;
	// Scrollbar track height (= canvas logical height), cached at remeasure so the
	// per-frame scroll path never reads scrollbarRef.clientHeight (a layout-forcing
	// read — same class as the documented getBoundingClientRect-per-frame P1).
	let scrollbarTrackHeight = 0;
	let transport: TerminalTransport | undefined;
	let invokeRef: ((cmd: string, args: Record<string, unknown>) => Promise<unknown>) | undefined;
	let rafId: number | undefined;
	// Render-scheduling stamp: when a repaint is first requested, the gap to the rAF
	// callback is the "sched" metric — scheduling latency under CPU load (0 = no
	// pending request). Gated by isFrameTimingEnabled().
	let mainDirtySince = 0;
	let resizeDebounce: ReturnType<typeof setTimeout> | undefined;
	// Disposer for the frame loop waiting on an unsized pane (see remeasure).
	let cancelSizeRetry: (() => void) | undefined;
	let dprMediaQuery: MediaQueryList | undefined;
	let dprChangeHandler: (() => void) | undefined;
	let cleanupTouch: (() => void) | undefined;
	let alive = true;
	const bindings = createCanvasTerminalBindings();
	const ipcErr = (cmd: string) => (e: unknown) =>
		appLogger.debug("terminal", `${cmd} failed`, { sessionId: props.sessionId, error: e });

	// Selection rows use eviction-stable indices (historyBase + grid-relative row)
	// so output cannot alias a selected line onto its replacement at the scrollback cap.
	const selection = createCanvasSelectionController();
	let selectionScrollTimer: ReturnType<typeof setInterval> | null = null;
	let selectionScrollDelta = 0;
	// Selection-drag coalescing (#9b13): cache the canvas rect once per drag and
	// collapse the repaint burst into one paint per frame via rAF — mirrors the
	// scroll/resize coalescing so raw mousemoves don't force a gBCR + full paint each.
	let selectionDragRect: DOMRect | null = null;
	let selectionRafId: number | undefined;
	let lastSelectionEvent: MouseEvent | null = null;

	// Link detection
	const linkController = createCanvasLinkController();
	const linkCache = linkController.rowCache;
	const STALE = Symbol("stale link lookup");
	let hoveredLink: HoveredLink | null = null;
	/** What the hovered link underlined when the probe resolved it; see pressSpanAt. */
	let hoveredSignature = "";
	const detectedLinks = linkController.detectedSpans;
	// Spans of links that span soft-wrapped rows (web + file://), keyed by row.
	// scanRowForLinks() merges these each time it rebuilds a row's dashed-underline
	// spans, so multi-row underlines survive the per-frame rebuild at line ~1367.
	// Recomputed by verifyVisibleFileLinks(); cleared with detectedLinks so it
	// can't paint stale rows after a scroll/resize.
	const wrappedLinkSpans = linkController.wrappedSpans;
	function clearDetectedLinks() {
		linkController.clearDetected();
	}

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
			if (!invokeRef) return;
			const termId = terminalsStore.getTerminalForSession(props.sessionId);
			const cwd = (termId ? terminalsStore.get(termId)?.cwd : undefined) || "";
			void openTerminalPathLink(path, cwd, invokeRef, props.onOpenFilePath, link.line, link.col);
		}
	};

	// The press claimed a span; what it opens is resolved now, at the pointer, by its
	// own lookup — neither `hoveredLink` nor the hover probe's staleness check.
	const linkPress = createLinkPressTracker();
	async function openLinkAt(row: number, col: number) {
		const link = await resolveLinkAt(row, col, () => !alive);
		if (link === STALE) {
			appLogger.debug("terminal", "link click: lookup went stale", { row, col });
		} else if (!link) {
			appLogger.debug("terminal", "link click: nothing resolved under the pointer", { row, col });
		} else if (!linkCovers(link, row, col)) {
			appLogger.debug("terminal", "link click: resolved link does not cover the pointer", { row, col, link });
		} else {
			appLogger.debug("terminal", "link click: opening", { row, col, path: link.path });
			openLink(link);
		}
	}
	/**
	 * The span a press at the cell belongs to: the dashed underline, or the link the hover
	 * probe resolved under the pointer (OSC 8 text, a path wrapped over rows) that the
	 * dashed underline does not cover. The hover is what showed the pointer cursor.
	 */
	function pressSpanAt(row: number, col: number): { colStart: number; colEnd: number } | undefined {
		const underlined = spanAt(detectedLinks.get(row), col);
		if (underlined || !hoveredLink || !linkCovers(hoveredLink, row, col)) return underlined;
		// An in-place redraw leaves the hover standing; it only counts while it underlines what it did.
		if (hoverSignature(rowMap, hoveredLink) !== hoveredSignature) return undefined;
		return (hoveredLink.spans ?? [hoveredLink]).find((sp) => sp.row === row && col >= sp.colStart && col < sp.colEnd);
	}
	const copyLink = (link: LinkTarget) => {
		const text = link.path.startsWith("file://") ? link.path.slice(7) : link.path;
		writeClipboard(text).catch(() => {});
	};

	// Cached CSS custom properties (re-read on remeasure, not every frame)
	let cachedBgDefault = "#1e1e1e";
	let cachedFgDefault = "#d4d4d4";

	// Tracks cumulative gesture distance (px) to ramp the scroll acceleration factor.
	// Row index → row data lookup (persistent, updated incrementally)
	const rowMap = new Map<number, DecodedFrame["rows"][0]>();
	// Rows that arrived in the latest onFrame batch (drives incremental repaint)
	const pendingDirtyRows = new Set<number>();
	// When true, next paint must redraw everything (scroll, resize, clear)
	let fullRepaintNeeded = true;
	let hidden = false;
	let lastHistorySize = -1;
	let lastHistoryBase = -1;
	let awaitingFullFrame = false;
	// How many grid frames this transport has delivered. The ack echoes it so Rust
	// can tell "the frontend caught up" from "a late ack for a frame the ticker
	// already gave up on" — without an id, the second one reopened the gate and
	// sent a burst at exactly the moment the frontend was behind. Reset with the
	// channel: (re)subscribing gives Rust a fresh gate counting from zero.
	let framesReceived = 0;

	function ackFrame() {
		transport?.ackFrame(framesReceived);
	}

	// The gate must reopen on this ack, not on the ticker deciding the frontend is
	// stuck. The margin that buys is in HIDDEN_ACK_INTERVAL_MS — do not inline it.
	const hiddenAck = createHiddenAckThrottle(ackFrame, HIDDEN_ACK_INTERVAL_MS);

	function writePtyNoScroll(data: string) {
		invokeRef?.("write_pty", { sessionId: props.sessionId, data }).catch((e) => {
			appLogger.warn("terminal", "PTY write failed", { sessionId: props.sessionId, error: e });
		});
	}

	function writePty(data: string) {
		// Typing jumps to the bottom — abandon any in-flight smooth scroll gesture so
		// its transient transform/cache render doesn't fight the programmatic jump.
		resetSmoothScroll();
		if (currentFrame && currentFrame.displayOffset > 0) {
			invokeRef?.("terminal_scroll", { sessionId: props.sessionId, delta: -currentFrame.displayOffset }).catch(
				ipcErr("terminal_scroll"),
			);
		}
		writePtyNoScroll(data);
	}

	function scheduleRepaint() {
		// A smooth-scroll gesture owns the base canvas (rendered locally from cache);
		// don't let backend-frame repaints fight it until the gesture settles.
		if (scroll.position != null) {
			return;
		}
		if (rafId !== undefined) return;
		if (hidden) {
			return;
		}
		if (!alive) return;
		// Stamp the first repaint request of this cycle ("sched" — see decl).
		if (mainDirtySince === 0 && isFrameTimingEnabled()) {
			mainDirtySince = performance.now();
		}
		rafId = requestAnimationFrame(() => {
			rafId = undefined;
			if (!alive || hidden) return;
			const m = metrics();
			if (currentFrame && m) {
				const dirty = pendingDirtyRows.size > 0 ? new Set(pendingDirtyRows) : undefined;
				pendingDirtyRows.clear();
				const timing = isFrameTimingEnabled();
				// "sched": request->rAF-callback delay — scheduling latency / vsync rAF priority.
				if (timing && mainDirtySince) {
					recordFrameTiming(props.sessionId, "sched", performance.now() - mainDirtySince);
				}
				// "paint": the base grid paint cost.
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
			if (!selection.selecting || !selection.start || !currentFrame || !invokeRef) {
				stopSelectionScroll();
				return;
			}
			const scrollDir = delta > 0 ? 1 : -1;
			invokeRef("terminal_scroll", { sessionId: props.sessionId, delta: scrollDir }).catch(ipcErr("terminal_scroll"));
			const edgeRow = scrollDir > 0 ? 0 : (currentFrame.screenRows || lastResizeRows) - 1;
			const absRow = viewportRowToAbs(edgeRow);
			if (absRow !== null) {
				selection.end = { col: scrollDir > 0 ? 0 : 9999, row: absRow + scrollDir };
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

	function viewportRowToAbs(viewportRow: number): number | null {
		if (!currentFrame) return null;
		return viewportRowToSelectionRow(
			{ ...currentFrame, screenRows: currentFrame.screenRows || lastResizeRows },
			viewportRow,
		);
	}

	/**
	 * Measure the pane and apply its geometry to the metrics, both canvases and
	 * the PTY. Returns false when there was no usable box to measure — the pane
	 * has not been laid out yet, or is still too small to hold one cell — in
	 * which case nothing here has run and the measurement is still owed.
	 */
	function measureNow(): boolean {
		if (!ctx) return false;
		const rect = containerRef.getBoundingClientRect();
		if (rect.width <= 0 || rect.height <= 0) return false;

		const dpr = window.devicePixelRatio || 1;
		const perTerminalSize = terminalsStore.state.terminals[props.terminalId]?.fontSize;
		const fontSize = perTerminalSize ?? settingsStore.state.defaultFontSize;
		const fontFamily = settingsStore.getFontFamily();
		const fontWeight = settingsStore.state.fontWeight;
		const m = getSharedMetrics(fontSize, fontFamily, dpr, snapLineHeight(fontSize), fontWeight);
		setMetrics(m);

		cachedBgDefault = getComputedStyle(canvasRef).getPropertyValue("--bg-secondary").trim() || "#1e1e1e";
		cachedFgDefault = getComputedStyle(canvasRef).getPropertyValue("--fg-primary").trim() || "#d4d4d4";
		gridRenderer.setTheme(cachedBgDefault, cachedFgDefault);

		// The emulator answers OSC 10/11/12 from these. It has no other way to
		// know the theme, and staying silent makes a probing agent retry forever
		// (see terminalPalette.ts). Cursor reuses the foreground: that is what the
		// overlay actually paints it with (octx.strokeStyle = cachedFgDefault).
		const bgRgb = cssColorToRgb(cachedBgDefault);
		const fgRgb = cssColorToRgb(cachedFgDefault);
		if (bgRgb && fgRgb) publishTerminalPalette(fgRgb, bgRgb, fgRgb);

		const { rows, cols } = gridDimsForBox(rect.width, rect.height, m.cellWidth, m.cellHeight);
		if (cols <= 0 || rows <= 0) return false;
		// A resize invalidates the smooth-scroll geometry (cell metrics, overscan,
		// row cache). Cancel any in-flight gesture so the new geometry takes over
		// cleanly. Cheap no-op when no gesture is active (scrollPosF already null).
		resetSmoothScroll();
		const logicalW = cols * m.cellWidth + GUTTER_PX;
		const logicalH = rows * m.cellHeight;
		// Cache the scrollbar track height here (resize time) so the per-frame path
		// uses this instead of reading scrollbarRef.clientHeight every frame.
		scrollbarTrackHeight = logicalH;
		canvasRef.width = logicalW * dpr;
		canvasRef.height = logicalH * dpr;
		ctx.scale(dpr, dpr);
		ctx.translate(GUTTER_PX, 0);
		canvasRef.style.width = `${logicalW}px`;
		canvasRef.style.height = `${logicalH}px`;
		overlayCanvasRef.width = logicalW * dpr;
		overlayCanvasRef.height = logicalH * dpr;
		overlayCanvasRef.style.width = `${logicalW}px`;
		overlayCanvasRef.style.height = `${logicalH}px`;
		octx.scale(dpr, dpr);
		octx.translate(GUTTER_PX, 0);

		// Overscan canvas (smooth scroll): one extra row above and below the viewport.
		// Positioned -cellHeight so its drawing y=0 maps to the row just above the
		// viewport; the row below is drawn at (rows+1)*cellHeight.
		if (overscanCanvasRef) {
			const overscanH = logicalH + 2 * m.cellHeight;
			overscanCanvasRef.width = logicalW * dpr;
			overscanCanvasRef.height = overscanH * dpr;
			overscanCanvasRef.style.width = `${logicalW}px`;
			overscanCanvasRef.style.height = `${overscanH}px`;
			overscanCanvasRef.style.top = `${-m.cellHeight}px`;
			if (!octxOverscan) {
				octxOverscan = overscanCanvasRef.getContext("2d", { alpha: true });
				if (octxOverscan) {
					overscanRenderer = createGridRenderer(octxOverscan, {
						fontWeight: () => settingsStore.state.fontWeight,
						getFontFamily: () => settingsStore.getFontFamily(),
					});
				}
			}
			if (octxOverscan && overscanRenderer) {
				octxOverscan.setTransform(1, 0, 0, 1, 0, 0);
				octxOverscan.scale(dpr, dpr);
				octxOverscan.translate(GUTTER_PX, 0);
				overscanRenderer.setTheme(cachedBgDefault, cachedFgDefault);
			}
			scroll.clearCache();
		}
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

			// Do NOT clear rowMap/detectedLinks here. resize_pty is async — the new
			// geometry's frame is still in flight — and painting from an emptied
			// rowMap below flashes a blank grid for one frame. The canvas bitmap was
			// already reset above, so leaving rowMap alone repaints the STALE old
			// content into the new size instead, which is what actually stays on
			// screen until the new frame's geomChanged handling clears and
			// repopulates rowMap for real (see the decideFrameGrid handling below).
			fullRepaintNeeded = true;
			lastDisplayOffset = -1;
			invokeRef("resize_pty", { sessionId: props.sessionId, rows, cols }).catch(ipcErr("resize_pty"));
		}

		if (currentFrame) {
			fullRepaintNeeded = true;
			paintFrame(currentFrame, m);
		}
		return true;
	}

	/**
	 * Measure the pane now, or as soon as it has a box to measure.
	 *
	 * A full page reload mounts every terminal before layout runs, so the first
	 * measurement of a mount routinely lands on a 0x0 pane. Dropping it left the
	 * canvas at its mount-time geometry — a small box in the corner of the pane,
	 * the rest black — until a window resize ran the measurement again (#716-031e).
	 */
	function remeasure() {
		cancelSizeRetry?.();
		cancelSizeRetry = undefined;
		if (measureNow()) return;
		cancelSizeRetry = retryUntilMeasured(measureNow, () =>
			appLogger.warn(
				"terminal",
				`Pane stayed unsized for ${SIZE_RETRY_MAX_FRAMES} frames — canvas geometry not applied for ${props.terminalId}`,
			),
		);
	}

	function paintFrame(frame: DecodedFrame, m: CellMetrics, dirtyIndices?: Set<number>) {
		gridRenderer.paintGrid(rowMap, m, { fullRepaint: fullRepaintNeeded, dirtyIndices });
		fullRepaintNeeded = false;

		// Overlay (cursor/selection/search/links/scrollbar/suggest) always stays on main.
		repaintOverlay(frame, m);

		updateScrollbar(frame);
		updateSuggestOverlay(frame, m, dirtyIndices);
		if (answersView() !== null) scheduleAnswersRefresh();
	}

	function repaintOverlay(frame: DecodedFrame, m: CellMetrics) {
		octx.clearRect(-GUTTER_PX, 0, overlayCanvasRef.width / m.dpr, overlayCanvasRef.height / m.dpr);
		paintSelection(selection, selectionAbsRowToViewport, overlayScrollOffset, rowCache, rowMap, octx, m);
		paintSearchHighlights(search, absRowToViewport, octx, m);
		paintLinkUnderline(frame, m);
		paintGutterMarkers(m);
		paintBlockTimestamps(m);
		paintFoldedBlocks(m);
		paintCursor(
			focused,
			cursorBlinkOn,
			octx,
			cachedFgDefault,
			rowMap,
			gridRenderer,
			cachedBgDefault,
			keyInputRef,
			frame,
			m,
		);
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

	/** Run `searchQuery` against the backend grid and adopt the result.
	 *
	 *  Shared by the `searchFind` ref method and by the frame-driven refresh, so a
	 *  live TUI redraw re-derives matches through exactly the same path as a fresh
	 *  find — no second copy of the block-scope / viewport-anchoring rules. */
	async function runSearchQuery(scrollToActive: boolean): Promise<{ index: number; count: number }> {
		if (!searchQuery || !invokeRef) {
			search.clear();
			return { index: -1, count: 0 };
		}
		const generation = screenGeneration;
		const query = searchQuery;
		let matches = (await invokeRef("terminal_search", {
			sessionId: props.sessionId,
			query,
		})) as { row: number; col_start: number; col_end: number }[];
		// The query can be cleared or replaced while the sweep is in flight, and
		// neither bumps `screenGeneration`. Cancelling the refresh throttle stops
		// the timer, not a request already out — so without this the stale answer
		// resurrects highlights the user just cleared, or overwrites the newer
		// query's matches with the older query's.
		if (generation !== screenGeneration || query !== searchQuery) return { index: -1, count: 0 };
		if (searchBlockScope && currentFrame) {
			const term = terminalsStore.get(props.terminalId);
			if (term) {
				const allBlocks = term.activeBlock ? [...term.commandBlocks, term.activeBlock] : term.commandBlocks;
				const viewTop = currentFrame.historySize - currentFrame.displayOffset;
				const viewCenter = viewTop + Math.floor(currentFrame.screenRows / 2);
				matches = filterMatchesToBlock(matches, allBlocks, viewCenter, currentFrame.historyBase);
			}
		}
		const activeMatch = search.replace(
			matches,
			currentFrame
				? {
						historySize: currentFrame.historySize,
						displayOffset: currentFrame.displayOffset,
						screenRows: currentFrame.screenRows || lastResizeRows,
					}
				: undefined,
		);
		// Only a user-driven find/next/prev may move the viewport. A refresh triggered
		// by the agent repainting must never yank the screen out from under the user.
		if (scrollToActive && activeMatch) scrollToMatch(activeMatch);
		const m = metrics();
		if (currentFrame && m) paintFrame(currentFrame, m);
		return { index: search.activeIndex, count: matches.length };
	}

	/** Coalesce the re-search: a redrawing TUI emits frames far faster than a
	 *  regex sweep over the whole scrollback is worth running. */
	function scheduleSearchRefresh(): void {
		if (!searchQuery) return;
		searchRefresh.trigger();
	}

	function clearSearchState(): void {
		searchQuery = "";
		searchRefresh.cancel();
		search.clear();
	}

	function absRowToViewport(absRow: number): number | null {
		if (!currentFrame) return null;
		// During a smooth-scroll gesture the base is cache-rendered at overlayScrollOffset
		// (the backend frame lags), so map overlay rows against that same offset.
		const offset = overlayScrollOffset ?? currentFrame.displayOffset;
		const viewportTop = currentFrame.historySize - offset;
		const viewportRow = absRow - viewportTop;
		if (viewportRow < 0 || viewportRow >= (currentFrame.screenRows || lastResizeRows)) return null;
		return viewportRow;
	}

	function selectionAbsRowToViewport(absRow: number): number | null {
		if (!currentFrame) return null;
		const offset = overlayScrollOffset ?? currentFrame.displayOffset;
		return selectionRowToViewport(
			{ ...currentFrame, displayOffset: offset, screenRows: currentFrame.screenRows || lastResizeRows },
			absRow,
		);
	}

	function clearSelectionIfEvicted(frame: DecodedFrame): void {
		if (
			(selection.start && selectionRowToGridRow(frame, selection.start.row) === null) ||
			(selection.end && selectionRowToGridRow(frame, selection.end.row) === null)
		) {
			selection.clear();
			stopSelectionScroll();
		}
	}

	function paintGutterMarkers(m: CellMetrics) {
		const term = terminalsStore.get(props.terminalId);
		if (!term) return;
		const blocks = term.commandBlocks;
		if (blocks.length === 0) return;
		for (const block of blocks) {
			if (block.exitCode === null || block.exitCode === 0) continue;
			const vpRow = absRowToViewport(block.promptLine - (currentFrame?.historyBase ?? 0));
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
			const startVp = absRowToViewport(foldStart - (currentFrame?.historyBase ?? 0));
			if (startVp === null) continue;
			const endVp = absRowToViewport(foldEnd - 1 - (currentFrame?.historyBase ?? 0));
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
			const gutterVp = absRowToViewport(block.promptLine - (currentFrame?.historyBase ?? 0));
			if (gutterVp !== null) {
				octx.fillRect(-GUTTER_PX, gutterVp * m.cellHeight, 3, m.cellHeight);
			}
		}
	}

	let blockTimestampsVisible = false;

	function setBlockTimestampsVisible(visible: boolean) {
		if (blockTimestampsVisible === visible) return;
		blockTimestampsVisible = visible;
		fullRepaintNeeded = true;
		const m = metrics();
		if (currentFrame && m) paintFrame(currentFrame, m);
	}

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
			const vpRow = absRowToViewport(block.promptLine - (currentFrame?.historyBase ?? 0));
			if (vpRow === null) continue;
			const y = vpRow * m.cellHeight;
			if (y < lastLabelBottom) continue;
			const label = formatRelativeTime(Date.now() - block.startedAt);
			const tw = octx.measureText(label).width;
			octx.fillText(label, canvasW - tw - 8, y + m.cellHeight * 0.75);
			lastLabelBottom = y + m.cellHeight;
		}
	}

	// --- Scrollbar ---

	function updateScrollbar(frame: DecodedFrame) {
		if (!scrollbarRef || !scrollThumbRef) return;
		const total = frame.historySize + (frame.screenRows || lastResizeRows || 24);

		if (frame.historySize === 0) {
			scrollbarRef.style.display = "none";
			scrollbarMarksContainer?.remove();
			scrollbarMarksContainer = null;
			lastScrollbarMarksKey = "";
			return;
		}
		scrollbarRef.style.display = "block";

		const thumb = thumbFor(scrollbarTrackHeight, lastResizeRows, frame);
		scrollThumbRef.style.height = `${thumb.height}px`;
		scrollThumbRef.style.transform = `translateY(${thumb.top}px)`;

		paintScrollbarMarks(total);
	}

	let scrollbarMarksContainer: HTMLDivElement | null = null;
	let lastScrollbarMarksKey = "";

	function paintScrollbarMarks(totalRows: number) {
		if (!scrollbarRef) return;
		const term = terminalsStore.get(props.terminalId);
		if (!term) return;
		const blocks = term.commandBlocks;
		const promptLines = term.userPromptLines;
		const historyBase = currentFrame?.historyBase ?? 0;
		const searchCount = search.matches.length;
		// `showScrollbarMarks` gates the HISTORY markers only — block boundaries and
		// user-prompt ticks — not the search hits below. Command history is a display
		// preference; a search hit is the live result of something the user just did,
		// and a Cmd+F that silently draws nothing because of a terminal display
		// setting is not a preference being honoured, it is a broken search.
		//
		// It is folded into `showBlocks` rather than returned on early, which also
		// fixes turning the setting OFF: an early return above the key computation
		// left the last-painted marks on screen forever, because the repaint that
		// would clear them never ran.
		const showBlocks = settingsStore.state.showScrollbarMarks;
		const key = `${showBlocks ? blocks.length : 0}:${showBlocks ? promptLines.length : 0}:${totalRows}:${historyBase}:${showBlocks ? (blocks.at(-1)?.exitCode ?? "") : ""}:s${searchCount}:${searchCount > 0 ? search.matches[0].row : ""}`;
		if (key === lastScrollbarMarksKey) return;
		lastScrollbarMarksKey = key;

		const html = buildScrollbarMarksHtml({
			blocks,
			promptLines,
			historyBase,
			matchRows: search.matches.map((m) => m.row),
			totalRows,
			trackH: scrollbarTrackHeight,
			showBlocks,
		});
		if (!html) {
			scrollbarMarksContainer?.remove();
			scrollbarMarksContainer = null;
			return;
		}
		if (!scrollbarMarksContainer) {
			scrollbarMarksContainer = document.createElement("div");
			scrollbarMarksContainer.style.cssText =
				"position:absolute;top:0;left:0;width:100%;height:100%;pointer-events:none";
			scrollbarRef.appendChild(scrollbarMarksContainer);
		}
		scrollbarMarksContainer.innerHTML = html;
	}

	// --- Suggest / Intent overlay ---

	const rowToText = rowText;

	// Cached suggest/intent overlay state to avoid full DOM rebuild
	let lastSuggestOverlayKey = "";

	function updateSuggestOverlay(
		_frame: DecodedFrame,
		m: CellMetrics,
		dirtyIndices?: Set<number>,
		snapshotOverride?: (i: number) => { text: string; isWrapped: boolean } | null,
	) {
		if (!overlayRef) return;

		// Skip full rescan if no dirty rows touch suggest/intent/answer patterns
		// (skipped entirely when rendering from the cache during a scroll gesture).
		if (!snapshotOverride && dirtyIndices && !fullRepaintNeeded) {
			let hasSuggestContent = false;
			for (const idx of dirtyIndices) {
				const row = rowMap.get(idx);
				if (!row) continue;
				const text = rowToText(row);
				if (SUGGEST_ANCHOR_RE.test(text) || INTENT_HIGHLIGHT_RE.test(text) || ANSWER_MARKER_RE.test(text)) {
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

		const getRowSnapshot =
			snapshotOverride ??
			((i: number) => {
				const row = rowMap.get(i);
				if (!row) return null;
				// `row.wrapped` says the row continues onto the NEXT one; the overlay
				// wants "this row continues the previous one".
				return { text: rowToText(row), isWrapped: rowMap.get(i - 1)?.wrapped ?? false };
			});

		// Decide what to mask before building anything: most repaints leave the
		// plan untouched, and the elements were being created and dropped just to
		// discover that.
		const { key, blocks } = planSuggestOverlay(numRows, getRowSnapshot);
		if (key === lastSuggestOverlayKey) return;
		lastSuggestOverlayKey = key;

		paintOverlayBlocks(overlayRef, blocks, m.cellHeight, bg);
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

	// Coalesced scroll: handlers compute the next absolute display offset
	// (latest-wins) and a single rAF flush sends it to the backend, with at most
	// one IPC in flight. Decouples input rate from IPC and avoids delta desync.
	let scrollRafId = 0;

	function scheduleScrollFlush() {
		if (!scrollRafId) scrollRafId = requestAnimationFrame(flushScroll);
	}
	function flushScroll() {
		scrollRafId = 0;
		if (scroll.pendingOffset == null) return;
		if (scroll.inFlight) {
			scheduleScrollFlush(); // retry next frame, keep pending
			return;
		}
		const target = scroll.pendingOffset;
		scroll.pendingOffset = null;
		scroll.inFlight = true;
		invokeRef?.("terminal_scroll_to_offset", { sessionId: props.sessionId, offset: target })
			.catch(ipcErr("terminal_scroll_to_offset"))
			.finally(() => {
				scroll.inFlight = false;
			});
	}

	// --- Smooth (sub-line) scroll, main renderer only ---
	// `scrollPosF` is the desired fractional display offset (in lines). The
	// integer part is committed to the backend (above); the fractional remainder
	// is shown as a transient translateY of the stage, with the adjacent overscan
	// row sliding into view. On gesture end it animates to the nearest line.
	// At rest (scrollPosF === null) the transform is identity → geometry unchanged.
	let smoothRafId = 0;
	let overlaysHiddenForScroll = false;
	// When non-null, the overlay (selection/cursor/search) is painted against this
	// integer display offset + the row cache instead of the live backend frame, so
	// it stays aligned with the cache-rendered base during a smooth-scroll gesture.
	let overlayScrollOffset: number | null = null;
	// True only while a gesture is actively producing deltas; false at rest (incl.
	// a fractional rest where scrollPosF stays non-integer — we never snap to a line).
	// We only ever hand off to normal rendering at the bottom (offset 0). Until the
	// backend frame reaches `settlePending` we keep cache-rendering it (no jump).
	let settleTimer = 0;

	// Full-frame reconciliation: partial frames (only the rows alacritty marked
	// dirty) merge into rowMap by index, so if the grid shifts content the canvas
	// can strand stale rows (duplicate/triplicate or vanished blocks) while the grid
	// itself stays correct. After an output burst settles, request one full frame so
	// the next onFrame does a fullReplace and rebuilds rowMap from the grid — a
	// self-heal that can't drift. Gated to at-rest, following-output (offset 0).
	let reconcileTimer: ReturnType<typeof setTimeout> | undefined;
	// First schedule of the current reschedule burst — bounds the total deferral
	// (reconcileDelay caps the trailing debounce at RECONCILE_MAX_WAIT_MS, so
	// sustained partial-frame output can't starve the self-heal forever).
	let reconcileBurstStart: number | null = null;
	// [dup] desync detector (perfDebug only): set when scheduleReconcile fires a
	// forced full-frame request, consumed by the next full frame in onFrame to diff
	// the partial-accumulated rowMap against the authoritative grid. See onFrame.
	let reconcileHealPending = false;
	function scheduleReconcile() {
		const now = Date.now();
		if (reconcileBurstStart == null) reconcileBurstStart = now;
		if (reconcileTimer) clearTimeout(reconcileTimer);
		reconcileTimer = setTimeout(
			() => {
				reconcileTimer = undefined;
				reconcileBurstStart = null;
				const off = currentFrame?.displayOffset ?? -1;
				const fire = shouldFireReconcile({
					alive,
					hidden,
					isScrolling: scroll.scrolling,
					scrollPosF: scroll.position,
					displayOffset: off,
				});
				if (!fire) {
					return;
				}
				// [dup] arm the desync detector: the frame this request pulls back is a
				// full grid snapshot at rest, so the next full frame's diff against the
				// partial-accumulated rowMap reveals any stranded stale rows.
				if (isPerfDebug()) reconcileHealPending = true;
				invokeRef?.("terminal_request_frame", { sessionId: props.sessionId }).catch(ipcErr("terminal_request_frame"));
			},
			reconcileDelay(now, reconcileBurstStart),
		);
	}

	function finishSettle() {
		settleTimer = 0;
		// A settle timer can fire after unmount; the row cache is released by then,
		// so endSmoothScroll must not run.
		if (!alive || scroll.settleTarget == null) return;
		scroll.settleTarget = null;
		scroll.position = null;
		endSmoothScroll();
	}
	function clearSettlePending() {
		if (settleTimer) {
			clearTimeout(settleTimer);
			settleTimer = 0;
		}
		scroll.settleTarget = null;
	}

	function scheduleSmoothRender() {
		if (!smoothRafId)
			smoothRafId = requestAnimationFrame(() => {
				smoothRafId = 0;
				renderSmooth();
			});
	}

	// Repaint the base canvas + the partial rows above/below locally from the row
	// cache for integer display offset `intOffset` — no backend round-trip, so it
	// keeps up at 60fps regardless of scroll speed.
	function renderCachedBase(intOffset: number, m: CellMetrics, rows: number, hist: number) {
		const cacheRow = (abs: number): DecodedRow | null => rowCache.get(abs) ?? null;
		const tempMap = new Map<number, DecodedRow>();
		for (let r = 0; r < rows; r++) {
			const cached = cacheRow(hist - intOffset + r);
			if (cached) tempMap.set(r, cached.index === r ? cached : { ...cached, index: r });
		}
		gridRenderer.paintGrid(tempMap, m, { fullRepaint: true });
		if (octxOverscan && overscanRenderer) {
			const ch = m.cellHeight;
			const w = overscanCanvasRef.width / m.dpr;
			octxOverscan.clearRect(-GUTTER_PX, 0, w, overscanCanvasRef.height / m.dpr);
			const above = cacheRow(hist - intOffset - 1);
			const below = cacheRow(hist - intOffset + rows);
			if (above) {
				octxOverscan.fillStyle = cachedBgDefault;
				octxOverscan.fillRect(-GUTTER_PX, 0, w, ch);
				overscanRenderer.paintRow(above, 0, m);
			}
			if (below) {
				const y = (rows + 1) * ch;
				octxOverscan.fillStyle = cachedBgDefault;
				octxOverscan.fillRect(-GUTTER_PX, y, w, ch);
				overscanRenderer.paintRow(below, y, m);
			}
		}
		ensureCacheBand(intOffset, rows, hist);
		// Rebuild suggest/intent masks from the cache at this offset so they track the
		// scrolling content instead of the lagging backend frame (no flicker, and the
		// raw suggest line stays masked).
		if (currentFrame) {
			updateSuggestOverlay(currentFrame, m, undefined, (i) => {
				const cached = cacheRow(hist - intOffset + i);
				if (!cached) return null;
				return { text: rowToText(cached), isWrapped: cacheRow(hist - intOffset + i - 1)?.wrapped ?? false };
			});
		}
	}

	function renderSmooth() {
		// A queued smooth-render RAF can fire after unmount; the row cache it reads
		// is released by then, so bail before touching it.
		if (!alive || scroll.position == null || !currentFrame) return;
		const m = metrics();
		if (!m) return;
		const ch = m.cellHeight;
		const rows = lastResizeRows || 24;
		// All-time top-of-history index: cache keys live in this eviction-stable space.
		const hist = currentFrame.historyBase + currentFrame.historySize;
		const intOffset = Math.floor(scroll.position);
		const frac = (scroll.position - intOffset) * ch; // [0, ch): how far past the line
		renderCachedBase(intOffset, m, rows, hist);
		// Repaint the selection/cursor/search overlay aligned to the cached offset so the
		// highlight tracks the content while scrolling (the overlay canvas is inside the
		// stage, so the fractional translate below keeps it pixel-aligned with the base).
		overlayScrollOffset = intOffset;
		repaintOverlay(currentFrame, m);
		overlayScrollOffset = null;
		stageRef.style.transform = `translate3d(0, ${frac}px, 0)`;
		// Track the scrollbar thumb live against the fractional position (paintFrame,
		// which normally drives it, is suppressed during the gesture).
		updateScrollbar({ ...currentFrame, displayOffset: scroll.position });
	}

	// Background-fetch any missing 64-row chunks in a one-screen band around the
	// viewport so fast scrolling always has cached rows ready to paint.
	function ensureCacheBand(intOffset: number, rows: number, hist: number) {
		if (!invokeRef) return;
		const lo = Math.max(0, hist - intOffset - rows);
		const hi = hist - intOffset + 2 * rows;
		const firstChunk = Math.floor(lo / ROW_CACHE_CHUNK);
		const lastChunk = Math.floor(hi / ROW_CACHE_CHUNK);
		for (let chunk = firstChunk; chunk <= lastChunk; chunk++) {
			if (chunk < 0 || requestedChunks.has(chunk)) continue;
			requestedChunks.add(chunk);
			void fetchChunk(chunk);
		}
	}

	async function fetchChunk(chunk: number) {
		if (!invokeRef) return;
		const start = chunk * ROW_CACHE_CHUNK;
		const cacheGeneration = scroll.cacheGeneration;
		const liveEpoch = scroll.liveEpoch;
		// First all-time row that was still a live screen row when the fetch began.
		const liveFromAbs = currentFrame ? currentFrame.historyBase + currentFrame.historySize : 0;
		try {
			const res = await invokeRef("terminal_styled_rows", {
				sessionId: props.sessionId,
				start,
				count: ROW_CACHE_CHUNK,
			});
			// Unmounted during the await: the row cache is released, so don't
			// repopulate it or schedule a render against it.
			if (!alive || !scroll.isCacheGenerationCurrent(cacheGeneration)) return;
			// Both transports deliver raw bytes; the shape still gets checked so a
			// non-binary answer is dropped rather than decoded as an empty chunk.
			const buffer = toBinaryPayload(res);
			if (!buffer) return;
			const decoded = decodeStyledRange(buffer);
			if (!decoded) {
				// Leave the range uninstalled and release its request key so the next
				// cache-band pass can fetch it again. A live full frame is the normal
				// synchronization barrier and also guarantees another render pass.
				requestedChunks.delete(chunk);
				invokeRef?.("terminal_request_frame", { sessionId: props.sessionId }).catch(ipcErr("terminal_request_frame"));
				return;
			}
			if (!scroll.cacheFetchedRows(decoded.rows, liveEpoch, liveFromAbs)) requestedChunks.delete(chunk);
			if (scroll.position != null) scheduleSmoothRender();
		} catch (e) {
			if (!scroll.isCacheGenerationCurrent(cacheGeneration)) return;
			requestedChunks.delete(chunk);
			ipcErr("terminal_styled_rows")(e);
		}
	}

	// --- Answers-only view ---
	// A panel above the canvas listing every user prompt of the session, each with the
	// 💬 answers of its turn, read from the backend grid (scrollback included) through the same
	// `terminal_styled_rows` reader as the scroll row cache. The PTY and the grid
	// are untouched; closing the panel returns to the live terminal as it was.
	const [answersView, setAnswersView] = createSignal<AnswersTurn[] | null>(null, {
		equals: sameAnswersHistory,
	});
	const answersCache = newTurnCache();
	let answersGeneration = 0;
	let answersTimer: ReturnType<typeof setTimeout> | undefined;
	const answersOnly = () => terminalsStore.get(props.terminalId)?.answersOnly === true;

	async function fetchStyledRange(start: number, count: number): Promise<StyledRange | null> {
		if (!invokeRef) return null;
		try {
			const buffer = toBinaryPayload(
				await invokeRef("terminal_styled_rows", { sessionId: props.sessionId, start, count }),
			);
			return buffer ? decodeStyledRange(buffer) : null;
		} catch (e) {
			ipcErr("terminal_styled_rows")(e);
			return null;
		}
	}

	async function refreshAnswers() {
		const frame = currentFrame;
		if (!alive || !frame) return;
		const generation = ++answersGeneration;
		const endAbs = frame.historyBase + frame.historySize + frame.screenRows;
		const turns = await readAnswersHistory(
			fetchStyledRange,
			terminalsStore.get(props.terminalId)?.userPromptLines ?? [],
			frame.historyBase,
			endAbs,
			answersCache,
		);
		// A newer refresh, the toggle going off, or unmount during the await wins.
		if (!turns || generation !== answersGeneration || !alive || !answersOnly()) return;
		setAnswersView(turns);
	}

	/** New output while the panel is open: re-read the turn, at most every 400 ms. */
	function scheduleAnswersRefresh() {
		if (answersTimer) return;
		answersTimer = setTimeout(() => {
			answersTimer = undefined;
			void refreshAnswers();
		}, 400);
	}

	createEffect(() => {
		if (answersOnly()) {
			untrack(() => void refreshAnswers());
		} else {
			answersGeneration++;
			setAnswersView(null);
		}
	});

	// During a gesture the cursor/selection canvas is hidden (those are anchored to
	// the backend frame and we're not selecting while scrolling). The suggest/intent
	// masks (overlayRef) stay visible — they're rebuilt from the cache and scroll
	// with the content, so they neither flicker nor uncover the raw suggest text.
	function setScrollOverlaysHidden(hidden: boolean) {
		if (overlaysHiddenForScroll === hidden) return;
		overlaysHiddenForScroll = hidden;
		if (overlayCanvasRef) overlayCanvasRef.style.visibility = hidden ? "hidden" : "";
	}

	// Wipe the overscan canvas. The above/below rows are only meaningful mid-gesture
	// while the stage slides; at rest the (opaque) base canvas covers the viewport but
	// the overscan's below-row strip peeks out beneath it. Leaving the last gesture's
	// row there shows it as a ghost line below the viewport, so clear on every return
	// to rest.
	function clearOverscan() {
		if (!octxOverscan || !overscanCanvasRef) return;
		const dpr = metrics()?.dpr ?? window.devicePixelRatio ?? 1;
		octxOverscan.clearRect(-GUTTER_PX, 0, overscanCanvasRef.width / dpr, overscanCanvasRef.height / dpr);
	}

	// Leave smooth-scroll mode: restore the overlays and repaint the base from the
	// real backend frame at its committed offset.
	function endSmoothScroll() {
		setScrollOverlaysHidden(false);
		if (stageRef) stageRef.style.transform = "";
		clearOverscan();
		const m = metrics();
		if (currentFrame && m) {
			fullRepaintNeeded = true;
			paintFrame(currentFrame, m);
		}
	}

	// Cancel an in-flight smooth gesture and restore the resting state. Self-contained:
	// also cancels the wheel gesture-end timer so a late resetScrollGesture can't fire
	// after we've handed control to another scroll path (scrollbar, programmatic jump).
	function resetSmoothScroll(repaint = true) {
		clearTimeout(scrollGestureEndTimer);
		if (scrollRafId) {
			cancelAnimationFrame(scrollRafId);
			scrollRafId = 0;
		}
		if (smoothRafId) {
			cancelAnimationFrame(smoothRafId);
			smoothRafId = 0;
		}
		const hadSmoothPosition = scroll.position != null;
		clearSettlePending();
		scroll.cancel();
		if (hadSmoothPosition && repaint) {
			endSmoothScroll();
		} else if (!repaint) {
			setScrollOverlaysHidden(false);
			if (stageRef) stageRef.style.transform = "";
			clearOverscan();
		}
	}

	// Seed the cache with the current viewport's rows so the first frame of a gesture
	// has content to paint immediately (the band prefetch fills the rest).
	function seedCacheFromCurrentFrame() {
		if (!currentFrame) return;
		const base = currentFrame.historyBase + currentFrame.historySize - currentFrame.displayOffset;
		scroll.cacheRows(Array.from(rowMap, ([r, row]) => ({ abs: base + r, row })));
	}

	function applySmoothScroll(deltaLines: number) {
		if (scroll.position == null) {
			// Entering a gesture: rebuild the cache from the current era (drops rows
			// staled by scrollback eviction). The overlay is NOT hidden — renderSmooth
			// repaints it from the cache so the selection highlight survives the scroll.
			scroll.clearCache();
			seedCacheFromCurrentFrame();
		}
		clearSettlePending();
		const hist = currentFrame?.historySize ?? 0;
		const position = scroll.applyDelta(deltaLines, currentFrame?.displayOffset ?? 0, hist);
		// Commit the integer floor so the backend display tracks the cache base.
		scheduleScrollFlush();
		// Reached the bottom — hand off to normal rendering (resume following output)
		// once the backend frame arrives at offset 0. No motion: 0 has no fractional part.
		if (position === 0) {
			scroll.settleTarget = 0;
			if (settleTimer) clearTimeout(settleTimer);
			settleTimer = window.setTimeout(finishSettle, 400);
		}
		scheduleSmoothRender();
	}

	// Gesture ended. Snap to the nearest line and hand off to normal (backend-frame)
	// rendering so the resting state always has scrollPosF === null. The old no-snap
	// behavior left scrollPosF at a fractional rest indefinitely; scheduleRepaint()
	// bails while scrollPosF != null, so the base canvas would never repaint again →
	// BLACK on the next resize / repo-switch / split-move (only on terminals that had
	// been scrolled, hence the history-size correlation). Snapping settles scrollPosF
	// back to null so repaints resume by construction.
	function resetScrollGesture() {
		const target = scroll.snap();
		if (target == null) return;
		scheduleScrollFlush();
		if (currentFrame && currentFrame.displayOffset === target) {
			// Backend already sits on the snapped line — no new frame will arrive to
			// trigger the onFrame handoff, so commit to normal rendering right now.
			clearSettlePending();
			scroll.position = null;
			endSmoothScroll();
			return;
		}
		// Otherwise keep cache-rendering the snapped line until the backend frame
		// reaches `target` (onFrame settle handler), with a timer as the safety net.
		scroll.settleTarget = target;
		if (settleTimer) clearTimeout(settleTimer);
		settleTimer = window.setTimeout(finishSettle, 400);
		scheduleSmoothRender();
	}

	// Apply one wheel/touch delta (raw pixels) with gesture acceleration → smooth
	// sub-line scroll.
	function handleScrollDelta(dy: number) {
		const m = metrics();
		const ch = m?.cellHeight ?? 20;
		const screenPx = ch * (lastResizeRows || 24);
		scroll.gestureDistancePx += Math.abs(dy);
		const excess = Math.max(0, scroll.gestureDistancePx - screenPx);
		const factor = 0.5 + 0.5 * (excess / screenPx);
		applySmoothScroll((dy * factor) / ch);
	}

	// The transport normalizes every payload shape (see toBinaryPayload) and drops
	// the ones that are not binary, so a frame arrives here as bytes or not at all.
	function onFrame(buffer: ArrayBuffer) {
		// Freeze-investigation: a frame storm starving the rAF loop breadcrumbs here.
		markPerf("term.onFrame");

		// Frame receipt ordering: ack FIRST (the ack only reopens the delivery gate;
		// the ticker sends the next frame on its own schedule, so ack must never wait
		// on decode/paint), then decode (cheap) which keeps rowMap + currentFrame alive
		// for the overlay (cursor/selection/links/search/scrollbar) and input semantics.
		//
		// A hidden terminal acks late instead of per frame. That is the whole of the
		// flow control this observer promises: a background tab is display:none and
		// never unmounted, so Rust keeps a producer running for it, and acking at
		// once reopens the gate at full rate for a frame nobody can see. The trailing
		// ack drops it to ~2 frames/s while still clearing the gate itself — staying
		// silent would leave that to the ticker's stuck-frontend path, which is a
		// warning per output burst for a tab that is merely in the background.
		framesReceived++;
		if (hidden) hiddenAck.schedule();
		else ackFrame();
		const timing = isFrameTimingEnabled();
		const decodeT0 = timing ? performance.now() : 0;
		// rowMap is the merge base for partial rows (see ROW_PARTIAL_FLAG): they
		// carry only their damaged columns and take the rest of the line from what
		// is already on screen.
		const frame = decodeBinaryFrame(buffer, rowMap);
		if (frame && terminalsStore.get(props.terminalId)?.historyBase !== frame.historyBase) {
			terminalsStore.update(props.terminalId, { historyBase: frame.historyBase });
		}
		if (timing) recordFrameTiming(props.sessionId, "decode", performance.now() - decodeT0);
		if (!frame) {
			// The only way to land here is a buffer shorter than the 26-byte header:
			// declared row/trailer truncation returns an empty frame with
			// needsFullFrame instead. The backend never sends an undersized header, so
			// this is a wire-format bug and must not be silent.
			// DEFERRED (2026-08-18) — no resync request on this path. The rows of a
			// dropped frame are gone (damage was consumed backend-side), but asking
			// for a full frame on a stream that is producing malformed buffers turns
			// one bad frame into a request loop at IPC rate. Revisit if this log is
			// ever seen in the wild — then we know the real failure mode.
			appLogger.error("terminal", "grid frame too short to decode", {
				sessionId: props.sessionId,
				byteLength: buffer.byteLength,
			});
			return;
		}

		// Decoded even while hidden: the bell rides in the frame header and a
		// background tab is exactly where it needs to reach the user.
		if (frame.bell) props.onBell?.();
		// Everything below paints, scans links or fills the scroll cache for a
		// viewport that is not on screen.
		if (hidden) return;

		// Grid decision: geom/scroll/full-replace/scroll-wait for the rowMap.
		const decision = decideFrameGrid(
			{
				lastScreenRows,
				lastScreenCols,
				lastDisplayOffset,
				lastHistorySize,
				lastHistoryBase,
				lastAltScreen,
				awaitingFullFrame,
			},
			frame,
			lastResizeRows,
		);
		const { geomChanged, scrollChanged, screenChanged } = decision;

		// A primary/alternate swap replaces the entire absolute-row universe. Reset
		// every stateful consumer as one transaction. A partial first frame cannot
		// inherit either rows or interaction state from the previous screen era.
		if (screenChanged) {
			screenGeneration++;
			resetSmoothScroll(false);
			scroll.clearCache();
			selection.clear();
			stopSelectionScroll();
			clearSearchState();
			rowMap.clear();
			pendingDirtyRows.clear();
			clearDetectedLinks();
			linkMenu.close();
			hoveredLink = null;
			canvasRef.style.cursor = "text";
			if (reconcileTimer) clearTimeout(reconcileTimer);
			reconcileTimer = undefined;
			reconcileBurstStart = null;
			reconcileHealPending = false;
			fullRepaintNeeded = true;
		}

		// When geometry changes, viewport is entirely different — must clear and repaint
		if (geomChanged) {
			selection.clear();
			stopSelectionScroll();
			rowMap.clear();
			clearDetectedLinks();
			fullRepaintNeeded = true;
		}

		if (decision.holdPreviousFrame) {
			// Same-generation viewport shifts retain the last coherent frame until its
			// replacement arrives. Screen-era and geometry changes are hard boundaries:
			// keep currentFrame null so pointer/copy paths cannot act on stale coordinates.
			if (screenChanged || geomChanged) {
				currentFrame = null;
				if (screenChanged) lastAltScreen = frame.altScreen;
				if (geomChanged) {
					lastScreenRows = frame.screenRows;
					lastScreenCols = frame.screenCols;
				}
			}
			awaitingFullFrame = true;
			if (decision.requestFullFrame) {
				const request = invokeRef?.("terminal_request_frame", { sessionId: props.sessionId });
				if (!request) {
					awaitingFullFrame = false;
				} else {
					request.catch((error) => {
						// Keep the coherent frame, but let the next delta retry the pull.
						awaitingFullFrame = false;
						ipcErr("terminal_request_frame")(error);
					});
				}
			}
			return;
		}
		awaitingFullFrame = false;

		if (scrollChanged || geomChanged || screenChanged) {
			if (hoveredLink) {
				hoveredLink = null;
				canvasRef.style.cursor = "text";
			}
		}
		// Lines that scrolled into history since the last accepted frame, in the
		// eviction-stable all-time space. Geometry and screen changes start a new era.
		const historyGrowth =
			screenChanged || geomChanged ? 0 : frame.historyBase + frame.historySize - (lastHistoryBase + lastHistorySize);
		// The rows that just left the live screen carry their final content only now.
		if (historyGrowth > 0) {
			const liveTop = lastHistoryBase + lastHistorySize;
			scroll.commitLiveRows(liveTop, liveTop + historyGrowth);
		}
		// These describe the merge origin for the next frame even when history and
		// display offset advanced together and left the visible all-time top stable.
		lastDisplayOffset = frame.displayOffset;
		lastHistorySize = frame.historySize;
		lastHistoryBase = frame.historyBase;
		lastScreenRows = frame.screenRows;
		lastScreenCols = frame.screenCols;
		lastAltScreen = frame.altScreen;

		// [dup] desync detector (perfDebug): if this full frame is the one pulled by a
		// reconcile at rest (no scroll/geom change), snapshot the partial-accumulated
		// rowMap BEFORE it's replaced. Any viewport row that differs from the grid below
		// was stale on the canvas — a stranded row the partial frames never carried
		// (alacritty damage under-report on in-place TUI redraws). Logged after the merge.
		let dupHealSnapshot: Map<number, string> | null = null;
		if (reconcileHealPending && decision.fullReplace && !decision.geomChanged && !decision.scrollChanged) {
			dupHealSnapshot = new Map();
			for (const [idx, row] of rowMap) dupHealSnapshot.set(idx, rowToText(row));
		}

		// When backend sends all screen rows, replace rowMap to discard stale entries
		if (decision.fullReplace) {
			reconcileHealPending = false;
			clearDetectedLinks();

			fullRepaintNeeded = true;
		}
		// Damage can include unchanged text (cursor, colours, or a full redraw).
		// Compare before installing the new rows: clearing those matches while the
		// asynchronous search refresh is pending makes every highlight blink.
		const rewrittenSearchRows = new Set<number>();
		if (searchQuery) {
			const viewportTop = frame.historySize - frame.displayOffset;
			for (const row of frame.rows) {
				const previous = rowMap.get(row.index);
				if (scrollChanged || !previous || rowToText(previous) !== rowToText(row)) {
					rewrittenSearchRows.add(viewportTop + row.index);
				}
			}
		}
		installFrameRows(rowMap, frame, decision);
		for (const row of frame.rows) {
			pendingDirtyRows.add(row.index);
			scanRowForLinks(row.index);
		}

		// A partial row arrived for a line we do not hold — it was dropped rather
		// than painted with holes, so pull the whole screen back. Self-limiting:
		// terminal_request_frame forces full damage, and a full frame repopulates
		// every row, so the next frame cannot land here again for the same reason.
		if (frame.needsFullFrame) {
			fullRepaintNeeded = true;
			invokeRef?.("terminal_request_frame", { sessionId: props.sessionId }).catch(ipcErr("terminal_request_frame"));
		}

		// Invalidate only changed text; unchanged rows keep their decorations until
		// the backend refresh adopts the next result. Changed rows still clear now
		// so a highlight cannot linger over text that no longer matches.
		if (searchQuery && frame.rows.length > 0) {
			search.dropRows(rewrittenSearchRows);
			scheduleSearchRefresh();
		}

		// [dup] desync detector: diff the pre-heal snapshot against the now-authoritative
		// rowMap. Diverging rows were stale on the canvas until this reconcile healed them.
		if (dupHealSnapshot) {
			const diverged: number[] = [];
			for (const [idx, oldText] of dupHealSnapshot) {
				const nr = rowMap.get(idx);
				const newText = nr ? rowToText(nr) : "";
				if (oldText !== newText) diverged.push(idx);
			}
			if (diverged.length > 0) {
				const sample = diverged.slice(0, 5).map((i) => {
					const nr = rowMap.get(i);
					return `#${i}: "${(dupHealSnapshot?.get(i) ?? "").slice(0, 40)}" => "${(nr ? rowToText(nr) : "").slice(0, 40)}"`;
				});
				appLogger.warn("terminal", `[dup] canvas desync healed: ${diverged.length} stale viewport row(s)`, {
					sessionId: props.sessionId,
					divergedRows: diverged,
					sample,
				});
			}
		}

		currentFrame = frame;
		clearSelectionIfEvicted(frame);

		// Re-anchor the soft-keyboard lift to the new cursor row (touch + keyboard
		// open only; a cheap no-op otherwise). currentFrame is a plain ref, so the
		// keyboard effect can't track cursor moves — this is the cursor-move trigger.
		updateKeyboardLift();

		// Partial frames merge by index and can strand stale rows (grid stays correct,
		// canvas drifts → duplicate/vanished blocks). A full frame already rebuilt the
		// rowMap, so only reconcile after partial frames; the debounce coalesces bursts.
		if (!decision.fullReplace) scheduleReconcile();

		// Smooth scroll: seed the client-side row cache from each frame's rows, keyed
		// by the eviction-stable absolute index `historyBase + historySize -
		// displayOffset + index`. `historyBase` (lines evicted from the history top)
		// climbs by exactly what the grid-relative coordinate loses on eviction, so a
		// physical line keeps its key for life — no stale row aliases onto a new one
		// after the scrollback cap rotates. Also pump a live render if a gesture is active.
		//
		// Seed on every frame, also while the backend trails the gesture: a row is keyed
		// by its eviction-stable absolute index, so a lagging frame still says what
		// each of its rows holds NOW. Skipping it left the rows an agent rewrites in
		// place (spinner, status, live region) painting their old text for as long
		// as the gesture ran (#1264-89c8).
		// The gesture offset is relative to the history bottom: move it with the
		// output first, so the cache render and the seed below stay on the same lines.
		if (scroll.followHistory(historyGrowth, frame.historySize, frame.displayOffset)) scheduleScrollFlush();
		const base = frame.historyBase + frame.historySize - frame.displayOffset;
		scroll.cacheRows(frame.rows.map((row) => ({ abs: base + row.index, row })));
		if (scroll.acceptSettledFrame(frame.displayOffset)) {
			// Backend reached the snapped line — hand off to normal rendering
			// seamlessly (the cache render already shows this exact frame).
			clearSettlePending();
			endSmoothScroll();
		} else if (scroll.scrolling) {
			// Active gesture: re-render against the freshly seeded cache.
			scheduleSmoothRender();
		} else if (scroll.position == null && stageRef?.style.transform) {
			// At rest on a line: clear any stray transform. (Gesture end always
			// snaps to a line and settles scrollPosF → null, so there is no
			// lingering fractional rest to keep a transform alive.)
			stageRef.style.transform = "";
			clearOverscan();
		}

		// Only compare content when the selection is fully on-screen — off-screen rows return empty
		// strings from getLocalSelectionText() causing spurious mismatches that clear the selection.
		if (shouldValidateSelectionSnapshot(selection, decision.fullReplace, selectionAbsRowToViewport)) {
			const nowText = getLocalSelectionText(selection, selectionAbsRowToViewport, rowMap);
			if (nowText !== selection.localSnapshot) selection.clear();
		}

		scheduleRepaint();
		scheduleFileLinkVerification();
	}

	// --- Link detection ---

	const FILE_PATH_RE = filePathRegex();
	const FILE_URL_RE = fileUrlRegex();

	// Per-session cache: row text → verified file link spans (null = checked, none exist)
	const fileLinkCache = linkController.fileCache;
	const FILE_LINK_RECHECK_MS = 3_000;
	const FILE_LINK_CACHE_MAX = 500;

	function scanRowForLinks(rowIndex: number) {
		const row = rowMap.get(rowIndex);
		if (!row) {
			detectedLinks.delete(rowIndex);
			return;
		}
		const text = rowToText(row);
		const spans: { colStart: number; colEnd: number }[] = [];

		for (const url of matchWebUrls(text)) {
			const cells = rowStringSpanToCells(rowMap, rowIndex, url.index, url.index + url.text.length);
			if (cells) spans.push({ colStart: cells[0], colEnd: cells[1] });
		}

		// File paths: only underline if previously verified to exist
		const cached = fileLinkCache.get(text);
		if (cached?.spans) {
			spans.push(...cached.spans);
		}

		// Spans from links that span soft-wrapped rows (web + file://) — invisible
		// from this single row's text, so merged from the wrapped-link map that
		// verifyVisibleFileLinks() builds via terminal_get_logical_line.
		const wrapped = wrappedLinkSpans.get(rowIndex);
		if (wrapped) spans.push(...wrapped);

		if (spans.length > 0) detectedLinks.set(rowIndex, spans);
		else detectedLinks.delete(rowIndex);
	}

	function scheduleFileLinkVerification() {
		linkController.scheduleVerification(verifyVisibleFileLinks);
	}

	async function verifyVisibleFileLinks() {
		const ref = invokeRef;
		if (!ref || !alive) return;
		const generation = screenGeneration;
		const maxRow = currentFrame?.screenRows || lastResizeRows;
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
				const cells = rowStringSpanToCells(rowMap, i, idx, idx + m[1].length);
				if (cells) candidates.push({ colStart: cells[0], colEnd: cells[1], raw: m[1] });
			}
			FILE_URL_RE.lastIndex = 0;
			while ((m = FILE_URL_RE.exec(text)) !== null) {
				const cells = rowStringSpanToCells(rowMap, i, m.index, m.index + m[0].length);
				if (cells) candidates.push({ colStart: cells[0], colEnd: cells[1], raw: m[1] });
			}
			if (candidates.length > 0) toCheck.push({ text, candidates });
		}

		const termId = terminalsStore.getTerminalForSession(props.sessionId);
		const termData = termId ? terminalsStore.get(termId) : undefined;
		const cwd = termData?.cwd || "";
		let anyFound = false;

		// Single-row verification
		// One call for the whole screen. This used to be one IPC per candidate,
		// awaited row by row, so a screen with links on twenty rows cost twenty
		// serial round trips — each one a filesystem-resolving hop for a single
		// string. The backend answers positionally, so the flat result is sliced
		// back onto the rows it came from.
		if (toCheck.length > 0) {
			const flat = toCheck.flatMap((item) => item.candidates.map((c) => c.raw));
			let resolved: (unknown | null)[];
			try {
				resolved = (await ref("resolve_terminal_paths", { cwd, candidates: flat })) as (unknown | null)[];
			} catch (e) {
				appLogger.debug("terminal", "resolve_terminal_paths failed", { count: flat.length, error: e });
				resolved = [];
			}
			if (!alive || generation !== screenGeneration) return;

			let at = 0;
			for (const item of toCheck) {
				const verified: { colStart: number; colEnd: number }[] = [];
				for (const c of item.candidates) {
					if (resolved[at]) verified.push({ colStart: c.colStart, colEnd: c.colEnd });
					at++;
				}
				if (fileLinkCache.size >= FILE_LINK_CACHE_MAX) {
					const oldest = fileLinkCache.keys().next().value;
					if (oldest !== undefined) fileLinkCache.delete(oldest);
				}
				fileLinkCache.set(item.text, { spans: verified.length > 0 ? verified : null, ts: Date.now() });
				if (verified.length > 0) anyFound = true;
			}
		}

		// Multi-row pass: detect web (http/https) + file:// URLs spanning
		// soft-wrapped rows. Each full-width row is a wrap candidate; the joined
		// logical line is scanned and any match that crosses a row boundary has
		// its per-row spans recorded in wrappedLinkSpans (merged by scanRowForLinks
		// so they survive the per-frame rebuild). Rebuilt fresh each pass.
		wrappedLinkSpans.clear();
		// Record a match's per-row spans into wrappedLinkSpans.
		const recordWrappedSpans = (startRow: number, text: string, matchIndex: number, matchEnd: number) => {
			for (const span of logicalStringSpanToCells(rowMap, startRow, text, matchIndex, matchEnd)) {
				const existing = wrappedLinkSpans.get(span.row) || [];
				existing.push({ colStart: span.colStart, colEnd: span.colEnd });
				wrappedLinkSpans.set(span.row, existing);
			}
		};
		const spansMultipleRows = (startRow: number, text: string, matchIndex: number, matchEnd: number) =>
			logicalStringSpanToCells(rowMap, startRow, text, matchIndex, matchEnd).length > 1;

		// Candidate rows: full-width rows that might be part of a soft-wrapped
		// web/file:// URL. Collected up front (no IPC) so every logical-line
		// lookup below can fire concurrently instead of one row at a time.
		const wrapCandidateRows: number[] = [];
		for (let i = 0; i < maxRow; i++) {
			const row = rowMap.get(i);
			if (!row) continue;
			const text = rowToText(row);
			if (!row.wrapped) continue;
			if (!text.includes("file://") && !/https?:\/\//.test(text)) continue;
			wrapCandidateRows.push(i);
		}

		if (wrapCandidateRows.length > 0) {
			// One terminal_get_logical_line per candidate row, all in flight at once —
			// this used to await them one row at a time, so a screen with N wrap
			// candidates cost N serial round trips before any of it resolved.
			const logicalLines = await Promise.all(
				wrapCandidateRows.map(async (i) => {
					try {
						const [startRow, logicalText] = (await ref("terminal_get_logical_line", {
							sessionId: props.sessionId,
							row: i,
						})) as [number, string];
						return { i, startRow, logicalText };
					} catch {
						return null; // terminal_get_logical_line not available on this backend
					}
				}),
			);
			if (!alive || generation !== screenGeneration) return;

			const checkedLogicalStarts = new Set<number>();
			// file:// candidates across every logical line, resolved in ONE call —
			// the same batching the single-row pass above already does.
			const fileCandidates: { startRow: number; logicalText: string; index: number; matchEnd: number; raw: string }[] =
				[];

			for (const result of logicalLines) {
				if (!result) continue;
				const { i, startRow, logicalText } = result;
				const row = rowMap.get(i);
				if (!row) continue;
				if (startRow === i && logicalText === rowToText(row)) continue; // single row
				if (checkedLogicalStarts.has(startRow)) continue;
				checkedLogicalStarts.add(startRow);

				// Web URLs — no path resolution needed.
				for (const url of matchWebUrls(logicalText)) {
					const matchEnd = url.index + url.text.length;
					if (!spansMultipleRows(startRow, logicalText, url.index, matchEnd)) continue;
					recordWrappedSpans(startRow, logicalText, url.index, matchEnd);
					anyFound = true;
				}

				// file:// URLs — underline only once resolved to a real path.
				FILE_URL_RE.lastIndex = 0;
				let m: RegExpExecArray | null;
				while ((m = FILE_URL_RE.exec(logicalText)) !== null) {
					const matchEnd = m.index + m[0].length;
					if (!spansMultipleRows(startRow, logicalText, m.index, matchEnd)) continue;
					fileCandidates.push({ startRow, logicalText, index: m.index, matchEnd, raw: m[1] });
				}
			}

			if (fileCandidates.length > 0) {
				let resolved: (unknown | null)[];
				try {
					resolved = (await ref("resolve_terminal_paths", {
						cwd,
						candidates: fileCandidates.map((c) => c.raw),
					})) as (unknown | null)[];
				} catch (e) {
					appLogger.debug("terminal", "resolve_terminal_paths failed", {
						count: fileCandidates.length,
						error: e,
					});
					resolved = [];
				}
				if (!alive || generation !== screenGeneration) return;
				fileCandidates.forEach((c, idx) => {
					if (resolved[idx]) {
						recordWrappedSpans(c.startRow, c.logicalText, c.index, c.matchEnd);
						anyFound = true;
					}
				});
			}
		}

		if (anyFound) {
			if (generation !== screenGeneration) return;
			for (let i = 0; i < maxRow; i++) scanRowForLinks(i);
			scheduleRepaint();
		}
	}

	let linkThrottle: ReturnType<typeof setTimeout> | undefined;

	/** Hover probe: the link under the cell becomes `hoveredLink`; a newer probe supersedes this one. */
	async function checkLinksAtRow(row: number, col: number) {
		if (!invokeRef || !alive) return;
		const gen = linkController.beginCheck();
		const found = await resolveLinkAt(row, col, () => !linkController.isCurrent(gen));
		if (found === STALE) return;
		hoveredLink = found;
		hoveredSignature = found ? hoverSignature(rowMap, found) : "";
		canvasRef.style.cursor = hoveredLink ? "pointer" : "text";
		if (currentFrame) {
			const m = metrics();
			if (m) repaintOverlay(currentFrame, m);
		}
	}

	/** The link under the cell, or `STALE` when `isStale` turned true while resolving. Touches no hover state. */
	async function resolveLinkAt(
		row: number,
		col: number,
		isStale: () => boolean,
	): Promise<HoveredLink | null | typeof STALE> {
		const ref = invokeRef;
		if (!ref || !alive) return STALE;

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
				if (!alive || isStale()) return STALE;
				return { row, colStart, colEnd, path: resolvedPath };
			}
		} catch {
			/* ignore — command may not exist on older backend */
		}
		if (!alive || isStale()) return STALE;

		let rowText: string;
		try {
			rowText = (await ref("terminal_get_row_text", {
				sessionId: props.sessionId,
				row,
			})) as string;
		} catch {
			return STALE;
		}
		if (!alive || isStale()) return STALE;

		const cacheKey = `${row}:${rowText}`;
		let links = linkCache.get(cacheKey);
		if (links === undefined) {
			const fpRe = FILE_PATH_RE;
			const fuRe = FILE_URL_RE;
			const fileMatches: { text: string; candidate: string; index: number }[] = [];
			const urlMatches: { text: string; path: string; index: number }[] = [];
			let match: RegExpExecArray | null;

			// Web URLs (no resolution needed). A URL that reaches the row's right
			// edge may be soft-wrapped onto the next row — defer it to the
			// logical-line pass below so the FULL url is captured, not the
			// truncated single-row prefix (e.g. http://127.0.0.1:8090 wrapping to
			// ".../8" + "090" must not open as http://127.0.0.1:8).
			for (const url of matchWebUrls(rowText)) {
				if (url.index + url.text.length >= rowText.length) continue;
				urlMatches.push({ text: url.text, path: url.text, index: url.index });
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
				// A file that did not resolve may be written a moment later (the tool
				// call line prints before the file exists). Caching that miss would keep
				// the name dead on click after the 3 s re-verification underlines it.
				if (validFiles.length === fileMatches.length) {
					if (linkCache.size > 200) {
						const oldest = linkCache.keys().next().value;
						if (oldest !== undefined) linkCache.delete(oldest);
					}
					linkCache.set(cacheKey, links);
				}
			}
		}

		let found: HoveredLink | null = null;
		const decodedRow = rowMap.get(row);
		if (links) {
			for (const link of links) {
				const start = link.index ?? 0;
				const end = start + link.text.length;
				const cells = decodedRow
					? textSpanToCellRanges([{ index: row, row: decodedRow }], rowText, start, end)?.[0]
					: null;
				if (cells && col >= cells.colStart && col < cells.colEnd) {
					found = {
						row,
						colStart: cells.colStart,
						colEnd: cells.colEnd,
						path: link.path,
						line: link.line,
						col: link.col,
					};
					break;
				}
			}
		}

		// A web URL that reaches the row's right edge was deferred above (it may be
		// soft-wrapped); force the logical-line pass so it's re-detected on the
		// joined line even when the row happens not to be wrapped.
		const rowHasEdgeUrl = matchWebUrls(rowText).some((url) => url.index + url.text.length >= rowText.length);

		// If no single-row link found, try logical line (joins soft-wrapped rows)
		if (!found && ref) {
			try {
				const [startRow, logicalText] = (await ref("terminal_get_logical_line", {
					sessionId: props.sessionId,
					row,
				})) as [number, string];
				if (!alive || isStale()) return STALE;
				if (startRow !== row || logicalText !== rowText || rowHasEdgeUrl) {
					const logicalOffset = logicalCellToStringOffset(rowMap, startRow, logicalText, row, col);
					const fuRe = FILE_URL_RE;
					const fpRe = FILE_PATH_RE;
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
					for (const url of matchWebUrls(logicalText)) {
						logicalMatches.push({ text: url.text, candidate: url.text, index: url.index, isUrl: true });
					}

					for (const lm of logicalMatches) {
						const matchEnd = lm.index + lm.text.length;
						if (logicalOffset !== null && logicalOffset >= lm.index && logicalOffset < matchEnd) {
							let resolvedPath = lm.candidate;
							if (!lm.isUrl) {
								const termId = terminalsStore.getTerminalForSession(props.sessionId);
								const termData = termId ? terminalsStore.get(termId) : undefined;
								const cwd = termData?.cwd || "";
								const r = (await ref("resolve_terminal_path", { cwd, candidate: lm.candidate })) as {
									absolute_path: string;
									is_directory: boolean;
								} | null;
								if (!alive || isStale()) return STALE;
								if (!r) break;
								resolvedPath = r.absolute_path;
							}
							const spans = logicalStringSpanToCells(rowMap, startRow, logicalText, lm.index, matchEnd);
							if (spans.length === 0) break;
							const firstSpan = spans[0];
							found = {
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

		return found;
	}

	let scrollGestureEndTimer: ReturnType<typeof setTimeout> | undefined;

	let streamToastId: number | undefined;

	function clearStreamNotice(): void {
		if (streamToastId !== undefined) toastsStore.remove(streamToastId);
		streamToastId = undefined;
	}

	function showStreamNotice(title: string, message: string, level: "warn" | "error"): void {
		if (!alive) return;
		clearStreamNotice();
		const name = terminalsStore.get(props.terminalId)?.name;
		streamToastId = toastsStore.add(
			name ? `${title} — ${name}` : title,
			message,
			level,
			false,
			undefined,
			0,
			undefined,
			props.sessionId,
			false,
		);
	}

	function showStreamError(error: unknown): void {
		showStreamNotice("Terminal stream failed", error instanceof Error ? error.message : String(error), "error");
	}

	onMount(async () => {
		const overlayCtx = overlayCanvasRef.getContext("2d");
		if (!overlayCtx) {
			appLogger.error("terminal", "Failed to acquire overlay 2D context");
			return;
		}
		octx = overlayCtx;

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
		installFrameTimingDebugHook();
		acquireCache();

		// Session events FIRST — before the font load below, not after the whole
		// setup. These are fire-and-forget pushes: an OSC 133 prompt marker or a
		// watcher line that lands while its listener is missing is gone, and the
		// shell prints its first prompt as soon as the PTY spawns, while
		// `document.fonts.load` may still have a cold-cache round trip to make.
		// The grid subscription stays below: frames replay on subscribe.
		// A session on a remote machine streams from that machine's WebSocket, not
		// from this origin — so the owner is resolved here rather than assumed local.
		transport = createTransport(props.sessionId, getSessionConnection(props.sessionId));
		invokeRef = (cmd, args) => transport!.invoke(cmd, args);
		// Teardown covering what exists right now: unmounting during the font load
		// below must not leave these listeners attached. Widened to the DOM
		// listeners further down, once those exist.
		unsubscribe = () => transport?.unsubscribe();
		try {
			// One shape on both transports: the Tauri event and the WS frame both
			// carry `{ cwd }`, so there is nothing to normalise here.
			transport.onStreamError?.((error) => {
				showStreamNotice(
					"Terminal stream reconnecting",
					error instanceof Error ? error.message : String(error),
					"warn",
				);
			});
			transport.onStreamReconnecting?.((attempt, maxAttempts) => {
				showStreamNotice("Terminal stream reconnecting", `Reconnecting ${attempt}/${maxAttempts}`, "warn");
			});
			transport.onStreamRecovered?.(clearStreamNotice);
			transport.onStreamExhausted?.((maxAttempts) => {
				showStreamError(new Error(`Reconnect failed after ${maxAttempts} attempts`));
			});
			await transport.onEvent("cwd", (payload) => {
				const { cwd } = payload as { cwd: string };
				terminalsStore.update(props.terminalId, { cwd });
				// A cd does NOT re-home an owned tab — it stays in the repo it was
				// opened in. This only settles a tab still parked with no owner.
				reclaimParkedTerminal(props.terminalId);
				props.onCwdChange?.(props.terminalId, cwd);
			});
			await transport.onEvent("osc133", (payload) => {
				const { marker, line, exit_code } = payload as { marker: string; line: number; exit_code: number | null };
				terminalsStore.handleOsc133(props.terminalId, marker, line, exit_code ?? undefined);
			});
			// Lines assembled by the Rust reader, carrying the ids it already
			// matched. No raw stream is scanned here any more.
			await transport.onEvent("watcher-lines", (payload) => {
				const { lines } = payload as { lines: Array<{ text: string; matched_ids: string[] }> };
				pluginRegistry.handleWatcherLines(props.sessionId, lines);
			});
		} catch (e) {
			appLogger.error("terminal", "Failed to subscribe to terminal session events", {
				sessionId: props.sessionId,
				error: e,
			});
		}
		if (!alive) {
			transport.unsubscribe();
			return;
		}

		const fontFamily = settingsStore.getFontFamily();
		const fontSize = settingsStore.state.defaultFontSize;
		const fontWeight = settingsStore.state.fontWeight;
		await Promise.all([
			document.fonts.load(`${fontWeight} ${fontSize}px ${fontFamily}`, "M"),
			// The 2nd arg is not decorative: fonts.load() only schedules the faces
			// covering the characters in it — "" matches nothing and downloads
			// nothing. Canvas 2D never pulls a webfont in by itself, so without a
			// real powerline glyph here U+E0B0 & co. render with a system fallback.
			document.fonts.load(`400 ${fontSize}px "Symbols Nerd Font Mono"`, "\ue0b0"),
		]).catch(() => document.fonts.ready);
		// Unmounted while the fonts were loading. onCleanup has already run — it
		// tore down the transport, the only thing installed at that point — and
		// everything below installs observers and document listeners, then
		// overwrites `unsubscribe` with a disposer nobody will call again. Without
		// this the disposed component is retained for the page lifetime and its
		// callbacks keep firing. Font loading is exactly the window a repo switch
		// or a fast tab close lands in.
		if (!alive) return;
		remeasure();

		resizeObserver = new ResizeObserver(() => {
			clearTimeout(resizeDebounce);
			resizeDebounce = setTimeout(() => remeasure(), 100);
		});
		resizeObserver.observe(containerRef);

		// Flow control: ack on a trailing timer when hidden, request full frame on show
		visibilityObserver = new IntersectionObserver(
			(entries) => {
				const isVisible = latestIntersectionVisibility(entries);
				if (isVisible && hidden) {
					hidden = false;
					awaitingFullFrame = false;
					// Back to full rate: clear the gate now instead of waiting out the
					// hidden interval, so the frame requested below is not queued behind it.
					hiddenAck.cancel();
					ackFrame();
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
					// Shrink to free the backing store while hidden.
					canvasRef.width = 1;
					canvasRef.height = 1;
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
		bindings.listen(canvasRef, "focus", () => {
			keyInputRef.focus({ preventScroll: true });
		});

		// iOS/iPadOS soft keyboards stop auto-repeating Backspace once the focused
		// field is empty, so holding Delete erased only a single character. Keep a
		// small space buffer in the hidden input on touch devices so
		// each key-repeat tick has something to delete and keeps firing
		// deleteContent* events. Desktop keeps the field empty so macOS dead-key
		// composition (which needs an empty input) is unaffected.
		const INPUT_BUFFER = "   ";
		const resetInputBuffer = () => {
			if (isTouchDevice) {
				keyInputRef.value = INPUT_BUFFER;
				try {
					keyInputRef.setSelectionRange(INPUT_BUFFER.length, INPUT_BUFFER.length);
				} catch {
					// setSelectionRange can throw on a hidden/detached input — harmless
				}
			} else {
				keyInputRef.value = "";
			}
		};

		bindings.listen(keyInputRef, "focus", () => {
			setFocused(true);
			startBlink();
			props.onFocus?.();
			resetInputBuffer();
			if (currentFrame?.focusReporting) writePtyNoScroll("\x1b[I");
		});
		bindings.listen(window, "blur", () => setBlockTimestampsVisible(false));
		bindings.listen(keyInputRef, "blur", () => {
			setBlockTimestampsVisible(false);
			setFocused(false);
			stopBlink();
			repaintCursorIfNeeded();
			if (currentFrame?.focusReporting) writePtyNoScroll("\x1b[O");
		});

		// Text from input methods that don't emit usable keydown events — iOS/
		// iPadOS soft keyboard, dictation, and predictive/autocorrect — arrives
		// only as `input` events (keydown fires with key "Unidentified", so
		// keyToSequence returns null and never preventDefaults). On desktop,
		// printable keys are handled in keydown with preventDefault(), so no
		// `input` event fires for them; anything that reaches here is mobile-style
		// text we must forward to the PTY ourselves. During composition the input
		// must hold the in-progress text or compositionend will never resolve, so
		// leave it untouched in that case.
		// DEFERRED (2026-06-27) — verify iOS dictation interim/replacement edge
		// cases on a real iPad: with autocorrect off the common path is
		// incremental insertText, but some iOS versions emit insertReplacementText
		// which could double-write. Needs device testing to confirm.
		bindings.listen(keyInputRef, "input", (e) => {
			const ie = e as InputEvent;
			if (ie.isComposing) return;
			switch (ie.inputType) {
				case "deleteContentBackward":
					writePty("\x7f");
					break;
				case "deleteContentForward":
					writePty("\x1b[3~");
					break;
				case "insertLineBreak":
				case "insertParagraph":
					writePty("\r");
					break;
				default:
					// insertText, insertReplacementText, insertFromDictation, …
					if (ie.data) writePty(ie.data);
			}
			resetInputBuffer();
		});

		// --- Keyboard ---
		const composition = createCompositionState();
		bindings.listen(keyInputRef, "compositionstart", () => {
			const m = metrics();
			if (currentFrame && m) syncImePosition(keyInputRef, currentFrame.cursorRow, currentFrame.cursorCol, m);
		});
		bindings.listen(keyInputRef, "compositionend", (e) => {
			const data = composition.onCompositionEnd(e.data);
			if (data) writePty(data);
			queueMicrotask(() => {
				resetInputBuffer();
			});
		});

		let leftOptionHeld = false;

		bindings.listen(keyInputRef, "keydown", (e: KeyboardEvent) => {
			if (composition.shouldSuppressKeydown(e.isComposing, e.key)) {
				e.preventDefault();
				return;
			}
			resetBlink();

			setBlockTimestampsVisible(e.ctrlKey && e.metaKey);

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
					const allPromptLines = blocks
						.map((b) => b.promptLine)
						.concat(active ? [active.promptLine] : [])
						.filter((line) => line >= (currentFrame?.historyBase ?? 0));
					if (allPromptLines.length > 0 && currentFrame) {
						const currentViewLine = currentFrame.historyBase + currentFrame.historySize - currentFrame.displayOffset;
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
							invokeRef?.("terminal_scroll_to", {
								sessionId: props.sessionId,
								line: targetLine - currentFrame.historyBase,
							}).catch(ipcErr("terminal_scroll_to"));
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
					const viewTop = currentFrame.historyBase + currentFrame.historySize - currentFrame.displayOffset;
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
				clearDetectedLinks();
				fullRepaintNeeded = true;
				currentFrame = null;
				awaitingFullFrame = false;
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
					// preventDefault stops the hidden key-input from also emitting an
					// `input` event for this printable key — without it the char is
					// written twice (keydown + input), e.g. "c" → "cc" (issue: resume
					// banner double-echo). Mirrors the normal printable path below.
					e.preventDefault();
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
			if (copyModifier && e.key.toLowerCase() === "c" && ((selection.start && selection.end) || selection.cachedText)) {
				e.preventDefault();
				e.stopPropagation();
				// Skip if the native Edit > Copy accelerator (menu.rs CmdOrCtrl+C) already fired for
				// this same keypress — otherwise we writeText() twice in <200ms, which macOS DeepL
				// reads as a double-Cmd+C and pops up its translation overlay. Same guard as
				// useKeyboardShortcuts.ts. The menu path (copyFromTerminal) handles the copy.
				if (Date.now() - lastMenuActionTime < 200) return;
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
				(selection.start || selection.cachedText) &&
				e.key !== "Meta" &&
				e.key !== "Control" &&
				e.key !== "Alt" &&
				e.key !== "Shift" &&
				!((e.ctrlKey || e.metaKey) && (e.key.toLowerCase() === "c" || e.key.toLowerCase() === "v"))
			) {
				selection.clear();
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
		bindings.listen(keyInputRef, "keyup", (e: KeyboardEvent) => {
			if (e.code === "AltLeft") leftOptionHeld = false;
			setBlockTimestampsVisible(e.ctrlKey && e.metaKey);
		});

		bindings.listen(keyInputRef, "paste", (e: ClipboardEvent) => {
			if (isImagePaste(e)) {
				e.preventDefault();
				writePty("\x16");
				return;
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
		/** Buttons this canvas reported down, so their release is owed to the app. */
		const reportedDown = new Set<number>();

		/** Bumped by every press and every context menu request; a menu lookup older than the counter is stale. */
		let pressSeq = 0;

		bindings.listen(canvasRef, "mousedown", (e: MouseEvent) => {
			// Keep the shared input focused: the default canvas focus runs AFTER this
			// handler and briefly blurs it, which can dismiss the soft keyboard. Cancel before
			// any grid/link early return, while preserving native context-menu presses.
			if (e.button === 0 && !(isMacOS() && e.ctrlKey)) e.preventDefault();
			pressSeq++;
			keyInputRef.focus({ preventScroll: true });
			{
				const at = canvasToGrid(metrics, canvasRef, e);
				const span = pressSpanAt(at.row, at.col);
				linkPress.begin(e.button, at.row, span, underlinedText(rowMap, at.row, span));
			}
			if (currentFrame && currentFrame.mouseMode > 0 && !e.shiftKey) {
				const pos = canvasToGrid(metrics, canvasRef, e);
				// A press on a detected link → let the link handlers fire (click opens,
				// contextmenu shows Open / Copy link), even while an app has mouse
				// reporting on. The underline promises the click opens, and Claude
				// Code's fullscreen mode reports the mouse, so forwarding the press left
				// every underlined path dead. In WKWebView, preventDefault on a
				// right-button mousedown suppresses the contextmenu event, so over a link
				// we neither forward nor preventDefault: the app loses this one press,
				// but the link works — UI-first (see #57). Shift bypasses reporting.
				if (linkClaimsPress(e.button, pressSpanAt(pos.row, pos.col) !== undefined)) {
					// A leftover Shift-drag selection would make the click bail as a drag.
					if (linkPress.isClaimed() && selection.hasRange()) {
						selection.clear();
						fullRepaintNeeded = true;
						scheduleRepaint();
					}
					return;
				}
				if (currentFrame.sgrMouse) {
					writePtyNoScroll(sgrMouseSequence(e.button, pos.col, pos.row, true, e));
					reportedDown.add(e.button);
				}
				e.preventDefault();
				return;
			}
			if (e.button !== 0) return;
			const pos = canvasToGrid(metrics, canvasRef, e);
			const absRow = viewportRowToAbs(pos.row);
			if (absRow === null) return;
			// cachedText belongs to the last completed/copied range. A new gesture
			// must not compare its evolving range against that previous snapshot.
			selection.invalidateSnapshot();

			// Gutter click: select entire block output
			{
				const rect = canvasRef.getBoundingClientRect();
				const rawX = e.clientX - rect.left;
				if (rawX < GUTTER_PX) {
					const term = terminalsStore.get(props.terminalId);
					const historyBase = currentFrame?.historyBase;
					const gridRow = currentFrame ? selectionRowToGridRow(currentFrame, absRow) : null;
					if (term && historyBase !== undefined && gridRow !== null) {
						const allBlocks = [...term.commandBlocks, term.activeBlock].filter(
							Boolean,
						) as import("../../stores/terminals").CommandBlock[];
						const block = allBlocks.find((b) => b.promptLine <= absRow && (b.endLine ?? Infinity) >= absRow);
						if (block) {
							const startRow = Math.max(historyBase, (block.executionLine ?? block.promptLine) + 1);
							const endRow = (block.endLine ?? absRow) - 1;
							if (endRow >= startRow) {
								selection.start = { row: startRow, col: 0 };
								selection.end = { row: endRow, col: lastResizeCols - 1 };
								selection.selecting = false;
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
			if (e.shiftKey && selection.start) {
				selection.end = absPos;
				selection.selecting = true;
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
				const vpRow = selectionAbsRowToViewport(absRow);
				const row = vpRow !== null ? rowMap.get(vpRow) : null;
				if (row) {
					const isWordChar = (col: number) => {
						if (col < 0 || col >= row.count) return false;
						const ch = cellText(row, col);
						if (ch === "" || /^\s+$/.test(ch)) return false;
						return !/[\s\t\x00-\x1f\x7f "'`(){}[\]<>|;:,.!?@#$%^&*~=+/\\]/.test(ch);
					};
					let left = pos.col;
					let right = pos.col;
					while (left > 0 && isWordChar(left - 1)) left--;
					while (right < row.count - 1 && isWordChar(right + 1)) right++;
					if (isWordChar(pos.col)) {
						selection.start = { col: left, row: absRow };
						selection.end = { col: right, row: absRow };
					} else {
						selection.start = absPos;
						selection.end = absPos;
					}
				} else {
					selection.start = absPos;
					selection.end = absPos;
				}
			} else if (clickCount >= 3) {
				const m = metrics();
				const maxCol = m ? Math.floor(canvasRef.getBoundingClientRect().width / m.cellWidth) - 1 : 79;
				selection.start = { col: 0, row: absRow };
				selection.end = { col: maxCol, row: absRow };
				clickCount = 3;
			} else {
				selection.start = absPos;
				selection.end = null;
			}
			selection.selecting = true;
			// Cache the canvas rect for the whole drag — its position doesn't move
			// while selecting (auto-scroll shifts content offset, not the element).
			selectionDragRect = canvasRef.getBoundingClientRect();
			fullRepaintNeeded = true;
			scheduleRepaint();
		});

		// Coalesced selection-drag processing: runs at most once per frame with the
		// latest mouse position (#9b13). Uses the cached drag rect (no per-move gBCR).
		const flushSelectionDrag = () => {
			selectionRafId = undefined;
			const e = lastSelectionEvent;
			if (!e || !selection.selecting || !selection.start) return;
			const rect = selectionDragRect ?? canvasRef.getBoundingClientRect();
			const m = metrics();
			if (m) {
				const yAbove = rect.top - e.clientY;
				const yBelow = e.clientY - rect.bottom;
				if (yAbove > 0) {
					startSelectionScroll(Math.ceil(yAbove / m.cellHeight));
				} else if (yBelow > 0) {
					startSelectionScroll(-Math.ceil(yBelow / m.cellHeight));
				} else {
					stopSelectionScroll();
				}
			}
			const pos = canvasToGrid(metrics, canvasRef, e, rect);
			const absRow = viewportRowToAbs(pos.row);
			if (absRow === null) return;
			selection.end = { col: pos.col, row: absRow };
			const mRepaint = metrics();
			if (currentFrame && mRepaint) paintFrame(currentFrame, mRepaint);
		};

		const scheduleLinkProbe = (e: MouseEvent) => {
			clearTimeout(linkThrottle);
			linkThrottle = setTimeout(() => {
				const pos = canvasToGrid(metrics, canvasRef, e);
				checkLinksAtRow(pos.row, pos.col);
			}, 100);
		};

		const onMouseMove = (e: MouseEvent) => {
			// The listener is on document, so this fires for every terminal on every
			// move. A hidden one has a 1x1 canvas and canvasToGrid clamps, so without
			// this it reported cell (0,0) into a PTY the pointer never touched.
			if (hidden) return;
			if (currentFrame && currentFrame.mouseMode > 0 && !e.shiftKey) {
				const rect = canvasRef.getBoundingClientRect();
				if (!isPointerInsideRect(e, rect)) return;
				// Keep probing links: the press over one is claimed from the app
				// (mousedown), and the click only opens what this probe found.
				if (!selection.selecting) scheduleLinkProbe(e);
				if (currentFrame.mouseMode >= 3) {
					const pos = canvasToGrid(metrics, canvasRef, e, rect);
					writePtyNoScroll(sgrMouseSequence(35, pos.col, pos.row, true, e));
				} else if (currentFrame.mouseMode >= 2 && e.buttons > 0 && !linkPress.isClaimed()) {
					const pos = canvasToGrid(metrics, canvasRef, e, rect);
					const btn = e.buttons & 1 ? 0 : e.buttons & 4 ? 1 : 2;
					writePtyNoScroll(sgrMouseSequence(32 + btn, pos.col, pos.row, true, e));
				}
				return;
			}

			// Selection drag: coalesce into one rAF/frame with the latest position.
			if (selection.selecting && selection.start) {
				lastSelectionEvent = e;
				if (selectionRafId === undefined) {
					selectionRafId = requestAnimationFrame(flushSelectionDrag);
				}
			}

			// Link detection (throttled). The listener is on document (see comment
			// above), so without a rect test every visible pane runs checkLinksAtRow
			// on every move — up to 3 IPC round trips each — for panes the pointer
			// never touched.
			if (!selection.selecting && isPointerInsideRect(e, canvasRef.getBoundingClientRect())) {
				scheduleLinkProbe(e);
			}
		};

		const onMouseUp = (e: MouseEvent) => {
			// A release we owe the application outlives the reasons to stay quiet:
			// hiding the terminal or dragging off it mid-gesture would otherwise
			// leave the button logically held with nothing left to retract it.
			const rect = canvasRef.getBoundingClientRect();
			// A claimed link press was never reported down, so its release is not owed either.
			const reportUp =
				!linkPress.isClaimed() && shouldReportMouseUp(reportedDown, e.button, isPointerInsideRect(e, rect));
			const owed = reportedDown.delete(e.button);
			if (hidden && !owed) return;
			if (currentFrame && currentFrame.mouseMode > 0 && !e.shiftKey) {
				if (!reportUp) return;
				// canvasToGrid clamps to the grid, so a release outside the canvas
				// reports the edge cell — what a terminal does for a drag-out.
				const pos = canvasToGrid(metrics, canvasRef, e, rect);
				if (currentFrame.sgrMouse) {
					writePtyNoScroll(sgrMouseSequence(e.button, pos.col, pos.row, false, e));
				}
				return;
			}
			stopSelectionScroll();
			if (selection.selecting && selection.start && selection.end) {
				if (selection.hasRange()) {
					copySelection();
				} else {
					selection.start = null;
					selection.end = null;
					fullRepaintNeeded = true;
					scheduleRepaint();
				}
			}
			selection.selecting = false;
			// End the coalesced drag: drop the cached rect + any pending frame.
			selectionDragRect = null;
			lastSelectionEvent = null;
			if (selectionRafId !== undefined) {
				cancelAnimationFrame(selectionRafId);
				selectionRafId = undefined;
			}
		};

		bindings.listen(document, "mousemove", onMouseMove);
		bindings.listen(document, "mouseup", onMouseUp);

		// Link click — plain click opens, skip if user was selecting text
		bindings.listen(canvasRef, "click", (e: MouseEvent) => {
			const at = canvasToGrid(metrics, canvasRef, e);
			const claimed = linkPress.release(at.row, at.col, underlinedTextAt(rowMap, pressSpanAt, at.row, at.col));
			// The first click of a double-click already opened it.
			if (!claimed || e.detail > 1 || selection.hasRange()) {
				// Only a click on something the user sees as a link is worth a line.
				if (claimed || isOverSpan(detectedLinks.get(at.row), at.col)) {
					appLogger.debug("terminal", "link click: not opened", {
						row: at.row,
						col: at.col,
						claimed: claimed !== null,
						detail: e.detail,
						hasRange: selection.hasRange(),
					});
				}
				return;
			}
			void openLinkAt(at.row, at.col);
		});

		// Right-click on a detected link → context menu (Open / Copy link).
		// Only when the click lands on a link span; elsewhere the default is left alone.
		bindings.listen(canvasRef, "contextmenu", async (e: MouseEvent) => {
			// Any menu request retires a pending lookup, also one off every link (Menu key, Shift+F10).
			const seq = ++pressSeq;
			const pos = canvasToGrid(metrics, canvasRef, e);
			if (!pressSpanAt(pos.row, pos.col)) return;
			e.preventDefault();
			// Stop the App-level terminal context menu (#terminal-panes onContextMenu)
			// from also opening and covering our Open/Copy-link menu.
			e.stopPropagation();
			// Suppression is synchronous; the target is resolved again here, so a link whose
			// hyperlink changed or vanished under the same text opens what is there now, or no menu.
			// The lookup's own answer, never the shared hover: a newer probe for another cell owns that.
			const link = await resolveLinkAt(pos.row, pos.col, () => !alive || pressSeq !== seq);
			if (!link || link === STALE || !linkCovers(link, pos.row, pos.col)) return;
			setLinkMenuTarget({ path: link.path, line: link.line, col: link.col });
			linkMenu.openAt(e.clientX, e.clientY);
		});

		// --- Scroll ---

		function handleWheel(e: WheelEvent) {
			e.preventDefault();
			e.stopPropagation();
			// While dragging the scrollbar thumb, ignore wheel input — otherwise it would
			// re-enter smooth-scroll (scrollPosF != null) and re-freeze repaints mid-drag.
			if (scrollDragging) return;
			// Forward the wheel to the app whenever it has mouse reporting enabled —
			// it owns the viewport and wants to drive its own scroll. This covers
			// both the alternate screen (vim, lazygit, htop) AND inline fullscreen
			// TUIs in the main buffer (e.g. `grok --no-alt-screen`, which scrolls its
			// own conversation on SGR wheel events — verified against grok 0.2.67).
			// We deliberately do NOT gate on historySize === 0: grok emits `\x1b[24S`
			// at startup, creating scrollback, so that proxy left grok's wheel dead
			// (TUIC scrolled its own history instead of forwarding to grok).
			// Shift+wheel always scrolls the TUIC scrollback, never the app — the
			// escape hatch matching the click/motion handlers' `!e.shiftKey` bypass.
			if (currentFrame && currentFrame.mouseMode > 0 && !e.shiftKey) {
				const pos = canvasToGrid(metrics, canvasRef, e as unknown as MouseEvent);
				const btn = e.deltaY < 0 ? 64 : 65;
				writePtyNoScroll(sgrMouseSequence(btn, pos.col, pos.row, true, e as unknown as MouseEvent));
				return;
			}
			const dy = e.deltaY;
			const atBottom =
				currentFrame && currentFrame.displayOffset === 0 && (scroll.position == null || scroll.position <= 0);
			const atTop =
				currentFrame &&
				currentFrame.displayOffset >= currentFrame.historySize &&
				(scroll.position == null || scroll.position >= currentFrame.historySize);
			if ((atBottom && dy > 0) || (atTop && dy < 0)) return;

			handleScrollDelta(dy);

			clearTimeout(scrollGestureEndTimer);
			scrollGestureEndTimer = setTimeout(resetScrollGesture, 200);
		}
		bindings.listen(canvasRef, "wheel", handleWheel, { passive: false });
		bindings.listen(scrollbarRef, "wheel", handleWheel, { passive: false });

		// Scrollbar drag
		let scrollDragging = false;
		let scrollDragStartY = 0;
		// Thumb travel already applied to the gesture, in lines. The drag feeds the
		// smooth-scroll gesture only the change since the last move, so output landing
		// mid-drag moves the gesture through followHistory instead of being dropped by
		// an offset anchored to the frame the drag started on (#1265-8b16).
		let scrollDragAppliedDelta = 0;

		// Scrollbar track click: jump to position
		bindings.listen(scrollbarRef, "mousedown", (e: MouseEvent) => {
			if (e.target === scrollThumbRef) return; // thumb has its own handler
			if (!currentFrame || currentFrame.historySize === 0) return;
			e.preventDefault();
			// Cancel any in-flight/settling smooth gesture (scrollPosF non-null
			// suppresses normal repaints, and the wheel gesture-end timer would
			// otherwise re-settle over this jump) so the terminal_scroll jump below
			// actually repaints the view.
			resetSmoothScroll();
			const rect = scrollbarRef.getBoundingClientRect();
			const clickRatio = (e.clientY - rect.top) / rect.height;
			const targetOffset = Math.round((1 - clickRatio) * currentFrame.historySize);
			// Coalesced absolute jump (latest-wins, back-pressured) — same path as wheel/touch.
			scroll.pendingOffset = Math.max(0, Math.min(currentFrame.historySize, targetOffset));
			scheduleScrollFlush();
		});

		bindings.listen(scrollThumbRef, "mousedown", (e: MouseEvent) => {
			e.preventDefault();
			e.stopPropagation();
			// Cancel any in-flight/settling smooth gesture first; otherwise scrollPosF
			// stays non-null and scheduleRepaint bails, freezing the view while we drag
			// the thumb. (resetSmoothScroll also cancels the wheel gesture-end timer.)
			resetSmoothScroll();
			scrollDragging = true;
			scrollDragStartY = e.clientY;
			scrollDragAppliedDelta = 0;
		});

		const onScrollDragMove = (e: MouseEvent) => {
			if (!scrollDragging || !currentFrame) return;
			const historySize = currentFrame.historySize;
			if (historySize === 0) return;
			const scrollRange = thumbFor(scrollbarTrackHeight, lastResizeRows, currentFrame).range;
			if (scrollRange <= 0) return;

			const dy = e.clientY - scrollDragStartY;
			const offsetDelta = Math.round((dy / scrollRange) * historySize);
			// Same gesture path as the wheel: position is distance from the history
			// bottom, rebased by followHistory when output lands, and clamped there.
			applySmoothScroll(offsetDelta - scrollDragAppliedDelta);
			scrollDragAppliedDelta = offsetDelta;
		};

		const onScrollDragUp = () => {
			if (!scrollDragging) return;
			scrollDragging = false;
			resetScrollGesture();
		};

		bindings.listen(document, "mousemove", onScrollDragMove);
		bindings.listen(document, "mouseup", onScrollDragUp);

		// Widen the cleanup NOW, before the transport.subscribe() await below. If the
		// component unmounts mid-await, onCleanup runs while `unsubscribe` would
		// otherwise still cover only the transport — leaking all four document
		// listeners for the page lifetime.
		const detachDomListeners = () => {
			bindings.dispose();
			if (scrollRafId) cancelAnimationFrame(scrollRafId);
			if (selectionRafId !== undefined) cancelAnimationFrame(selectionRafId);
			resetSmoothScroll();
			stopSelectionScroll();
		};
		unsubscribe = () => {
			detachDomListeners();
			transport?.unsubscribe();
		};

		// Touch input (mobile/tablet)
		cleanupTouch = installTouchHandlers(canvasRef, keyInputRef, {
			onScrollPixels: (dy) => {
				// Touch is direct manipulation: the content must follow the finger,
				// the OPPOSITE of the wheel convention handleScrollDelta expects
				// (positive dy = toward newer/bottom). Negate so swipe-up reveals
				// newer lines and swipe-down reveals older scrollback, matching
				// native iOS scrolling.
				handleScrollDelta(-dy);
			},
			onScrollEnd: resetScrollGesture,
			onFontSizeChange: (delta) => {
				// Per-terminal, like every keyboard/menu/palette zoom: the global
				// default is persisted config, and the renderer reads the terminal's
				// own size first, so writing the default changed everything except
				// the terminal being pinched.
				applyPinchFontDelta(props.terminalId, delta, settingsStore.state.defaultFontSize);
			},
			onSelectionMode: () => {
				/* future: enter selection UI */
			},
		});

		// Subscribe to the grid channel. The transport and its session-event
		// listeners already exist — installed before the font load above.
		try {
			// Rust installs a fresh delivery gate on subscribe, so the echoed receipt
			// count has to start from zero with it.
			hiddenAck.cancel();
			framesReceived = 0;
			awaitingFullFrame = false;
			await transport.subscribe((data) => onFrame(data));
			if (!alive) {
				// Unmounted while subscribe() was in flight. onCleanup already ran and
				// tore down what existed then — but the grid subscription only became
				// live on the line above and would leak. Tear it down here.
				transport.unsubscribe();
				return;
			}
			// Paint the current grid now. The browser-mode WS subscribe (unlike the
			// Tauri event channel) does not replay the current frame, so an idle
			// session with no pending output would render nothing and leave the
			// canvas black until the first interaction. Forcing a full frame here is
			// idempotent on desktop and fixes the black-on-load in browser mode.
			noteFrameRequest();
			invokeRef?.("terminal_request_frame", { sessionId: props.sessionId }).catch(ipcErr("terminal_request_frame"));
		} catch (e) {
			appLogger.error("terminal", "Failed to subscribe to terminal grid channel", {
				sessionId: props.sessionId,
				error: e,
			});
			showStreamError(e);
			// `unsubscribe` already covers this on unmount; drop the session-event
			// listeners now rather than keeping them alive on a terminal that will
			// never paint.
			transport?.unsubscribe();
		}

		props.onRef?.({
			focus: () => keyInputRef.focus({ preventScroll: true }),
			getSelectionText: () => selection.cachedText,
			refresh: () => {
				rowMap.clear();
				clearDetectedLinks();
				fullRepaintNeeded = true;
				currentFrame = null;
				awaitingFullFrame = false;
				lastDisplayOffset = -1;
				lastResizeCols = 0;
				lastResizeRows = 0;
				remeasure();
				invokeRef?.("terminal_request_frame", { sessionId: props.sessionId }).catch(ipcErr("terminal_request_frame"));
			},
			resubscribe: async () => {
				// A pending hidden ack would report the old channel's receipt count
				// against the fresh gate Rust installs on resubscribe.
				hiddenAck.cancel();
				framesReceived = 0;
				awaitingFullFrame = false;
				await transport?.resubscribe();
			},
			searchFind: async (query: string, blockScope?: boolean) => {
				if (!query || !invokeRef) {
					clearSearchState();
					const m = metrics();
					if (currentFrame && m) paintFrame(currentFrame, m);
					return { index: -1, count: 0 };
				}
				searchQuery = query;
				searchBlockScope = blockScope ?? false;
				return runSearchQuery(true);
			},
			searchNext: () => {
				const match = search.next();
				if (!match) return { index: -1, count: 0 };
				scrollToMatch(match);
				const m = metrics();
				if (currentFrame && m) paintFrame(currentFrame, m);
				return { index: search.activeIndex, count: search.matches.length };
			},
			searchPrev: () => {
				const match = search.previous();
				if (!match) return { index: -1, count: 0 };
				scrollToMatch(match);
				const m = metrics();
				if (currentFrame && m) paintFrame(currentFrame, m);
				return { index: search.activeIndex, count: search.matches.length };
			},
			searchClear: () => {
				clearSearchState();
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

	// On-screen keyboard handling (touch only): slide THIS terminal up just enough
	// to reveal the CURSOR ROW above the virtual keyboard, without resizing the app
	// layout or the PTY (no reflow/SIGWINCH). The lift is anchored to the cursor —
	// NOT the pane's bottom edge — so a freshly-started agent whose input sits near
	// the TOP of the grid isn't shifted off-screen (the mirror of the original
	// occlusion bug). It's a pure transform on kbLiftRef (wraps the stage), clipped
	// by containerRef's overflow:hidden. Only the focused terminal lifts.
	function updateKeyboardLift() {
		const occ = keyboardOcclusion();
		const isFocused = focused();
		const m = metrics();
		if (!isTouchDevice || !kbLiftRef) return;
		const frame = currentFrame;
		// Clear when unfocused, keyboard closed, no frame/metrics yet, or scrolled
		// back into history (frame.displayOffset > 0 → cursor isn't on screen).
		if (!isFocused || occ <= 0 || !frame || !m || frame.displayOffset > 0) {
			kbLiftRef.style.transform = "";
			return;
		}
		// containerRef doesn't move (only its child kbLiftRef is transformed), so its
		// top is the stable viewport origin for the cursor's untransformed Y. Lift by
		// how far the cursor row's bottom sits below the keyboard's top edge — zero
		// when the cursor is already above the keyboard, so a top-anchored input never
		// gets pushed up. Anchoring to the cursor bottom self-clamps: the cursor can't
		// overshoot above containerRef.top.
		const keyboardTop = window.innerHeight - occ;
		const cursorBottomY = containerRef.getBoundingClientRect().top + (frame.cursorRow + 1) * m.cellHeight;
		const lift = Math.max(0, Math.round(cursorBottomY - keyboardTop));
		kbLiftRef.style.transform = lift > 0 ? `translateY(${-lift}px)` : "";
	}
	if (isTouchDevice) {
		ensureKeyboardViewportTracking();
		// React to keyboard open/close, focus, and metrics (font) changes. Cursor
		// movement is driven separately from the frame-decode path, since currentFrame
		// is a plain ref, not a signal.
		createEffect(() => {
			updateKeyboardLift();
		});
	}

	createEffect(() => {
		terminalsStore.state.terminals[props.terminalId]?.fontSize;
		settingsStore.state.defaultFontSize;
		settingsStore.state.font;
		settingsStore.state.fontWeight;
		if (!alive) return;
		settingsStore.state.theme;
		invalidateGlyphCache();
		gridRenderer?.invalidateCaches();
		fullRepaintNeeded = true;
		remeasure();
	});

	async function copySelection() {
		const setStatus = (window as unknown as Record<string, unknown>).__tuic_setStatusInfo as
			| ((msg: string) => void)
			| undefined;
		try {
			const result = await commitSelectionCopy(
				async () => {
					// Always prefer the Rust path: it unwraps soft-wrapped logical lines via the
					// WRAPLINE flag. The snapshot base lets Rust rebase the grid-relative rows
					// under the same lock that reads them, so eviction cannot alias replacements.
					const startRow =
						currentFrame && selection.start ? selectionRowToGridRow(currentFrame, selection.start.row) : null;
					const endRow = currentFrame && selection.end ? selectionRowToGridRow(currentFrame, selection.end.row) : null;
					const historyBase = currentFrame?.historyBase;
					if (
						invokeRef &&
						selection.start &&
						selection.end &&
						startRow !== null &&
						endRow !== null &&
						historyBase !== undefined
					) {
						const text = (await invokeRef("terminal_get_selection_text", {
							sessionId: props.sessionId,
							startRow,
							startCol: selection.start.col,
							endRow,
							endCol: selection.end.col,
							historyBase,
						})) as string;
						// Legacy/empty response fallback; an eviction rejection throws and is
						// deliberately handled without a local or cached copy below.
						return text || getLocalSelectionText(selection, selectionAbsRowToViewport, rowMap);
					}
					return selection.cachedText || getLocalSelectionText(selection, selectionAbsRowToViewport, rowMap);
				},
				async (text) => {
					selection.cachedText = text;
					selection.localSnapshot = getLocalSelectionText(selection, selectionAbsRowToViewport, rowMap);
					await writeClipboard(text);
				},
			);
			if (result.kind === "copied") {
				setStatus?.("Copied to clipboard");
			} else if (result.kind === "expired") {
				selection.clear();
				stopSelectionScroll();
				const m = metrics();
				if (currentFrame && m) repaintOverlay(currentFrame, m);
				setStatus?.("Selection expired — text left scrollback");
			}
		} catch (e) {
			appLogger.warn("terminal", "Clipboard write failed", { error: e });
			setStatus?.("Copy failed — clipboard unavailable");
		}
	}

	onCleanup(() => {
		alive = false;
		clearStreamNotice();
		clearTimeout(answersTimer);
		stopBlink();
		if (rafId !== undefined) {
			cancelAnimationFrame(rafId);
			rafId = undefined;
		}
		// Smooth-scroll RAF + settle timer also outlive unmount and run against the
		// released row cache — cancel both.
		if (smoothRafId) {
			cancelAnimationFrame(smoothRafId);
			smoothRafId = 0;
		}
		clearSettlePending();
		hiddenAck.cancel();
		if (reconcileTimer) clearTimeout(reconcileTimer);
		clearTimeout(resizeDebounce);
		cancelSizeRetry?.();
		cancelSizeRetry = undefined;
		resizeObserver?.disconnect();
		visibilityObserver?.disconnect();
		if (dprChangeHandler) dprMediaQuery?.removeEventListener("change", dprChangeHandler);
		unsubscribe?.();
		cleanupTouch?.();
		clearTimeout(linkThrottle);
		clearTimeout(scrollGestureEndTimer);
		searchRefresh.cancel();
		linkController.dispose();
		resetFrameTiming(props.sessionId);
		rowMap.clear();
		gridRenderer?.invalidateCaches();
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
				const quoted = `'${path.replaceAll("'", "'\\''")}' `;
				writePty(quoted);
				keyInputRef.focus({ preventScroll: true });
			}}
		>
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
			{/* Keyboard-lift wrapper: slides the whole stage up on touch devices so the
			    cursor stays above the on-screen keyboard. Identity transform at rest;
			    clipped by containerRef's overflow:hidden. */}
			<div
				ref={kbLiftRef!}
				style={{
					position: "absolute",
					inset: "0",
					"will-change": "transform",
				}}
			>
				{/* Smooth-scroll stage: base + overlay translate together during a gesture.
				    At rest transform is identity → geometry/coordinates are unchanged. */}
				<div
					ref={stageRef!}
					style={{
						position: "absolute",
						top: "0",
						left: "0",
						"will-change": "transform",
					}}
				>
					{/* Overscan: the row above/below the viewport, revealed as the stage slides.
				    Sits behind the (opaque) base canvas; never hit-tested. */}
					<canvas
						ref={overscanCanvasRef!}
						style={{
							position: "absolute",
							left: "0",
							"pointer-events": "none",
						}}
					/>
					<canvas
						ref={canvasRef!}
						style={{
							position: "relative",
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
					{/* Suggest/intent overlay — inside the stage so it scrolls with the content
				    (rebuilt from the row cache during a smooth-scroll gesture). */}
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
				</div>
			</div>
			<Show when={answersView()}>
				{(view) => (
					<AnswersPanel
						view={view()}
						fontFamily={settingsStore.getFontFamily()}
						fontSize={terminalsStore.get(props.terminalId)?.fontSize ?? settingsStore.state.defaultFontSize}
					/>
				)}
			</Show>
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
					onMouseEnter={(e) => {
						// Darker, subtle hover like the old terminal scrollbar (#cccccc @0.3),
						// not the bright --fg-muted.
						e.currentTarget.style.background = "rgba(204, 204, 204, 0.3)";
					}}
					onMouseLeave={(e) => {
						e.currentTarget.style.background = "var(--bg-highlight)";
					}}
					style={{
						width: "10px",
						"margin-left": "2px",
						"border-radius": "5px",
						// Harmonized with the editor scrollbar: same --bg-highlight resting
						// color, --fg-muted on hover, and a hand pointer cursor.
						background: "var(--bg-highlight)",
						position: "absolute",
						top: "0",
						cursor: "pointer",
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
