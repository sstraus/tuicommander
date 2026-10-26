import { createEffect, createMemo, createSignal, For, type JSX, onCleanup, onMount, Show } from "solid-js";
import {
	ANSWER_RULE_SHADOW,
	INTENT_HIGHLIGHT_RE,
	intentTitleEnd,
	planSuggestOverlay,
} from "../../components/Terminal/suggestOverlay";
import { appLogger } from "../../stores/appLogger";
import { type PtySubscription, subscribePty } from "../../transport";
import { mobileFontSizes } from "../mobileFontSize";
import { recordMobileSocketState } from "../resumeTrail";
import {
	groupLineBlocks,
	type LogLine,
	lineMatchesNeedle,
	normalizeLogLine,
	reflowDisplayLines,
	sameLine,
	spanStyle,
} from "../utils/logLine";
import { detectOutputLinks } from "../utils/outputLinks";
import { createVisibilityGate } from "../utils/pageVisibility";
import styles from "./OutputView.module.css";

const MAX_LINES = 500;
/** Lines fetched on initial HTTP load (tail). Older lines loaded on scroll-up. */
const INITIAL_FETCH_LIMIT = 100;
/** Lines fetched per scroll-up chunk. */
const SCROLL_CHUNK_SIZE = 100;
/** Scroll-up trigger threshold in pixels from top. */
const SCROLL_UP_THRESHOLD = 300;

interface OutputViewProps {
	sessionExists?: boolean;
	onConnectionError?: (error: string | null) => void;
	onRegisterRetry?: (retry: (() => void) | undefined) => void;
	onAtBottomChange?: (atBottom: boolean) => void;
	onRegisterScrollToBottom?: (jump: (() => void) | undefined) => void;
	sessionId: string;
	/** Hidden Chat output keeps its stream, and follows bottom on CLI re-entry. */
	visible?: boolean;
	/** Real-time session state pushed via WebSocket (bypasses 3s polling). */
	onStateChange?: (state: Record<string, unknown>) => void;
	/** Receive current PTY input line text from the prompt row. */
	onInputLine?: (text: string | null) => void;
	/** When set, only lines matching this query (case-insensitive) are shown. */
	searchQuery?: string;
	/** Open a verified terminal path in the session's file viewer. */
	onOpenFileLink?: (candidate: string, line?: number) => void;
}

export function OutputView(props: OutputViewProps) {
	const [logLines, setLogLines] = createSignal<LogLine[]>([]);
	const [screenRows, setScreenRows] = createSignal<LogLine[]>([]);
	const [subscribeError, setSubscribeError] = createSignal<string | null>(null);
	const [loadingOlder, setLoadingOlder] = createSignal(false);
	let containerEl: HTMLDivElement | undefined;
	let textEl: HTMLPreElement | undefined;
	let unsubscribe: PtySubscription | null = null;
	let streamGeneration = 0;
	let disposed = false;
	let retryTimer: ReturnType<typeof setTimeout> | undefined;
	let retryDelay = 3_000;
	let connectionGrace: ReturnType<typeof setTimeout> | undefined;
	const connected = () => {
		clearTimeout(connectionGrace);
		connectionGrace = undefined;
		props.onConnectionError?.(null);
	};
	const connecting = () => {
		if (connectionGrace !== undefined) return;
		connectionGrace = setTimeout(() => props.onConnectionError?.("Terminal stream is unavailable."), 5_000);
	};
	props.onRegisterRetry?.(() => void startStream());
	const scheduleRetry = () => {
		clearTimeout(retryTimer);
		retryTimer = setTimeout(() => {
			if (!disposed && props.sessionExists !== false && document.visibilityState !== "hidden") void startStream();
		}, retryDelay);
		retryDelay = Math.min(retryDelay * 2, 30_000);
	};
	createEffect(() => {
		if (props.sessionExists === false) {
			clearTimeout(retryTimer);
			connected();
		}
	});
	// When the user scrolls up manually, stop auto-scrolling until they
	// return to the bottom.
	let userScrolledUp = false;
	// Tracks the oldest log offset we've loaded — 0 means full history is loaded.
	// DEFERRED (2026-10-12) — live-tail eviction needs source cursor metadata to
	// advance this boundary correctly; chrome filtering makes row counts unsafe.
	let oldestLoadedOffset = 0;

	// Capture the visible row before changing metrics. CSS rewraps the log locally;
	// resizing the shared PTY would also change the desktop terminal's columns.
	createEffect(() => {
		const size = mobileFontSizes.terminal();
		if (!containerEl || !textEl) return;
		const viewport = containerEl;
		const top = viewport.getBoundingClientRect().top;
		const anchor = [...textEl.querySelectorAll<HTMLElement>(`.${styles.line}`)].find(
			(line) => line.getBoundingClientRect().bottom > top,
		);
		const offset = anchor ? anchor.getBoundingClientRect().top - top : 0;
		const follow = !userScrolledUp;
		textEl.style.fontSize = `${size}px`;
		const frame = requestAnimationFrame(() => {
			if (follow) scrollToBottom();
			else if (anchor?.isConnected) {
				viewport.scrollTop += anchor.getBoundingClientRect().top - top - offset;
				lastScrollTop = viewport.scrollTop;
			}
		});
		onCleanup(() => cancelAnimationFrame(frame));
	});

	/** Fetch initial log lines + screen rows via HTTP; returns the total_lines offset for WS catch-up. */
	async function fetchInitialOutput(): Promise<number> {
		const controller = new AbortController();
		const timeout = setTimeout(() => controller.abort(), 5_000);
		try {
			const resp = await fetch(`/sessions/${props.sessionId}/output?format=log&limit=${INITIAL_FETCH_LIMIT}`, {
				signal: controller.signal,
			});
			if (!resp.ok) {
				appLogger.warn("terminal", "fetchInitialOutput non-ok response, will rely on WS", { status: resp.status });
				return 0;
			}
			const json = (await resp.json()) as {
				lines: unknown[];
				total_lines: number;
				offset?: number;
				screen?: string[];
			};
			if (json.lines && json.lines.length > 0) {
				setLogLines(json.lines.map(normalizeLogLine));
			}
			if (json.screen && json.screen.length > 0) {
				reconcileScreenRows(json.screen as unknown[]);
			}
			// Sync initial input_line from HTTP response
			if (props.onInputLine) {
				const il = (json as Record<string, unknown>).input_line;
				props.onInputLine(typeof il === "string" ? il : null);
			}
			const total = json.total_lines ?? 0;
			// The backend reports where the returned window actually starts. Deriving it
			// from lines.length would drift: chrome lines (agent prompt box, footer)
			// occupy offsets without being returned, so the subtraction lands inside the
			// window we already hold and replays those lines on scroll-up.
			oldestLoadedOffset = json.offset ?? Math.max(0, total - (json.lines?.length ?? 0));
			scrollToBottom(true);
			return total;
		} catch (err) {
			appLogger.warn("terminal", "fetchInitialOutput failed, will rely on WS catch-up", { error: err });
		} finally {
			clearTimeout(timeout);
		}
		return 0;
	}

	/** Fetch older lines when user scrolls near the top. Prepends and anchors viewport. */
	async function fetchOlderLines() {
		if (loadingOlder() || oldestLoadedOffset <= 0) return;
		setLoadingOlder(true);
		try {
			const fetchOffset = Math.max(0, oldestLoadedOffset - SCROLL_CHUNK_SIZE);
			const fetchLimit = oldestLoadedOffset - fetchOffset;
			const resp = await fetch(
				`/sessions/${props.sessionId}/output?format=log&offset=${fetchOffset}&limit=${fetchLimit}`,
			);
			if (!resp.ok) {
				appLogger.warn("terminal", "fetchOlderLines HTTP error", { status: resp.status });
				oldestLoadedOffset = 0; // prevent retry storm
				return;
			}
			const json = (await resp.json()) as { lines: unknown[] };
			if (!json.lines || json.lines.length === 0) {
				oldestLoadedOffset = 0;
				return;
			}
			const older = json.lines.map(normalizeLogLine);
			// Exclude the transient loading row from the prepend height delta.
			setLoadingOlder(false);
			const prevHeight = containerEl?.scrollHeight ?? 0;
			const prevTop = containerEl?.scrollTop ?? 0;
			// History is explicitly requested by a detached reader. Applying the live
			// tail cap here discards that page immediately once the tail is full.
			setLogLines((prev) => [...older, ...prev]);
			const addedHeight = (containerEl?.scrollHeight ?? 0) - prevHeight;
			oldestLoadedOffset = fetchOffset;
			requestAnimationFrame(() => {
				if (containerEl) {
					// A later live append must not count as prepended history.
					containerEl.scrollTop = prevTop + addedHeight;
				}
			});
		} catch (err) {
			appLogger.warn("terminal", "fetchOlderLines failed", { error: err });
		} finally {
			setLoadingOlder(false);
		}
	}

	/**
	 * The backend re-sends the whole screen on every frame, but almost every row
	 * renders exactly as it did before. Each row arrives freshly deserialized, so
	 * identity has to be re-established by value: keeping the previous frame's
	 * LogLine for an unchanged row keeps the block wrapper `<For>` is keyed on,
	 * and with it the row's DOM nodes.
	 */
	function reconcileScreenRows(rows: unknown[]) {
		setScreenRows((prev) =>
			rows.map((raw, i) => {
				const line = normalizeLogLine(raw);
				const before = prev[i];
				return before && sameLine(before, line) ? before : line;
			}),
		);
	}

	// Touch inertia guard: while the user is actively touching, don't auto-scroll
	let touchActive = false;
	let lastScrollTop = 0;
	const handleTouchStart = () => {
		touchActive = true;
	};
	const handleTouchEnd = () => {
		touchActive = false;
	};

	function scrollToBottom(force = false) {
		if (props.visible === false || (!force && (userScrolledUp || touchActive))) return;
		requestAnimationFrame(() => {
			// Output can queue this frame before the user starts scrolling.
			if (containerEl && props.visible !== false && (force || (!userScrolledUp && !touchActive))) {
				containerEl.scrollTop = containerEl.scrollHeight;
				lastScrollTop = containerEl.scrollTop;
				props.onAtBottomChange?.(true);
			}
		});
	}

	function handleScroll() {
		if (!containerEl) return;
		// scrollTop can be fractional while scrollHeight/clientHeight are rounded.
		// Only tolerate rounding, not an upward gesture inside a near-bottom zone.
		const atBottom = containerEl.scrollHeight - containerEl.scrollTop - containerEl.clientHeight <= 1;
		props.onAtBottomChange?.(atBottom);
		// Only an upward move detaches: the scroll event of our own follow scroll can be
		// measured after a new chunk grew the content, and must not read as a detach.
		if (atBottom) userScrolledUp = false;
		else if (containerEl.scrollTop < lastScrollTop) userScrolledUp = true;
		lastScrollTop = containerEl.scrollTop;
		// Lazy-load older lines when scrolling near the top
		if (containerEl.scrollTop < SCROLL_UP_THRESHOLD && oldestLoadedOffset > 0) {
			fetchOlderLines();
		}
	}

	props.onRegisterScrollToBottom?.(() => {
		userScrolledUp = false;
		touchActive = false;
		scrollToBottom(true);
	});
	onCleanup(() => props.onRegisterScrollToBottom?.(undefined));

	createEffect(() => {
		if (props.visible !== false) {
			userScrolledUp = false;
			touchActive = false;
			scrollToBottom(true);
		}
	});

	// A backgrounded PWA renders nothing, so every frame a busy agent pushes is
	// pure battery and radio cost. Registered in the component body, not in the
	// async mount, so the listener has an owner to be released by.
	createVisibilityGate(
		() => {
			unsubscribe?.pause();
			clearTimeout(connectionGrace);
			connectionGrace = undefined;
			if (unsubscribe) recordMobileSocketState(props.sessionId, "paused");
		},
		() => {
			if (unsubscribe) {
				recordMobileSocketState(props.sessionId, "connecting");
				unsubscribe.resume();
			} else void startStream();
		},
	);

	onMount(() => {
		// Keyboard and rotation resize the clip without delivering new PTY output.
		// Reuse the live-output follow path so detached readers and touches stay put.
		const resizeObserver = new ResizeObserver(() => scrollToBottom());
		if (containerEl) resizeObserver.observe(containerEl);
		onCleanup(() => resizeObserver.disconnect());
		containerEl?.addEventListener("scroll", handleScroll, { passive: true });
		containerEl?.addEventListener("touchstart", handleTouchStart, { passive: true });
		containerEl?.addEventListener("touchend", handleTouchEnd, { passive: true });
		containerEl?.addEventListener("touchcancel", handleTouchEnd, { passive: true });
		void startStream();
	});

	async function startStream() {
		clearTimeout(retryTimer);
		const generation = ++streamGeneration;
		unsubscribe?.();
		unsubscribe = null;
		setSubscribeError(null);
		connecting();
		const offset = await fetchInitialOutput();
		if (disposed || generation !== streamGeneration) return;
		try {
			recordMobileSocketState(props.sessionId, "connecting");
			const subscription =
				(await subscribePty(
					props.sessionId,
					() => {}, // unused — onLogLines handles log delivery
					() => {
						recordMobileSocketState(props.sessionId, "closed");
						// Transport retry exhaustion also reports exit. The session poll,
						// not a failed socket, decides whether the terminal still exists.
						if (props.sessionExists === false) connected();
						else {
							connecting();
							scheduleRetry();
						}
						setLogLines((prev) => [...prev, { spans: [{ text: "--- session exited ---" }] }]);
						reconcileScreenRows([]);
					},
					{
						format: "log",
						logOffset: offset,
						onLogLines(rawLines) {
							setLogLines((prev) => {
								const incoming = rawLines.map(normalizeLogLine);
								const next = [...prev, ...incoming];
								// Keep the reader's rows mounted while detached (or touching).
								// Resume the bounded tail when following resumes.
								return userScrolledUp || touchActive ? next : next.slice(-MAX_LINES);
							});
							scrollToBottom();
						},
						onScreenRows(rows) {
							reconcileScreenRows(rows);
							scrollToBottom();
						},
						onStateChange: props.onStateChange,
						onInputLine: props.onInputLine,
						onReconnecting: () => {
							connecting();
							recordMobileSocketState(props.sessionId, "reconnecting");
						},
						onReconnected: () => {
							connected();
							recordMobileSocketState(props.sessionId, "open");
						},
					},
				)) ?? null;
			if (disposed || generation !== streamGeneration) {
				subscription?.();
				return;
			}
			unsubscribe = subscription;
			retryDelay = 3_000;
			connected();
			// The subscription is installed after an awaited fetch. A page hidden
			// during that window produced no visibilitychange to catch, so without
			// this the stream would run until the next one — which for a tab
			// restored in the background may never come.
			if (document.visibilityState === "hidden") {
				unsubscribe?.pause();
				recordMobileSocketState(props.sessionId, "paused");
			} else recordMobileSocketState(props.sessionId, "open");
		} catch (err) {
			if (disposed || generation !== streamGeneration) return;
			if (props.sessionExists === false) {
				connected();
				return;
			}
			recordMobileSocketState(props.sessionId, "failed");
			const msg = err instanceof Error ? err.message : String(err);
			appLogger.error("terminal", "Failed to subscribe to PTY output", { error: msg });
			setSubscribeError(msg);
			clearTimeout(connectionGrace);
			props.onConnectionError?.("Terminal stream is unavailable.");
			scheduleRetry();
		}
	}

	onCleanup(() => {
		disposed = true;
		streamGeneration++;
		clearTimeout(retryTimer);
		clearTimeout(connectionGrace);
		connectionGrace = undefined;
		props.onConnectionError?.(null);
		props.onRegisterRetry?.(undefined);
		unsubscribe?.();
		if (unsubscribe) recordMobileSocketState(props.sessionId, "closed");
		containerEl?.removeEventListener("scroll", handleScroll);
		containerEl?.removeEventListener("touchstart", handleTouchStart);
		containerEl?.removeEventListener("touchend", handleTouchEnd);
		containerEl?.removeEventListener("touchcancel", handleTouchEnd);
	});

	// Reflow before filtering: it joins rows by their adjacency on the source
	// terminal, which a search filter no longer preserves.
	const allLines = createMemo(() => reflowDisplayLines([...logLines(), ...screenRows()]));

	// Classify the full reflowed source before search removes boundary rows.
	// Reflow has already joined soft wraps, so these are logical row starts.
	const answers = createMemo(() => {
		const lines = allLines();
		const rows = lines.map((line) => ({
			text: line.spans
				.map((span) => span.text)
				.join("")
				.replaceAll("\uFE0E", ""),
			isWrapped: false,
		}));
		const plan = planSuggestOverlay(rows.length, (index) => rows[index] ?? null);
		return {
			lines: new Set(plan.blocks.filter((block) => block.kind === "answer").map((block) => lines[block.row])),
			markers: new Set([...plan.answerMarkers].map((index) => lines[index])),
		};
	});

	const displayedLines = createMemo(() => {
		const q = props.searchQuery;
		if (!q) return allLines();
		const needle = q.toLowerCase();
		return allLines().filter((line) => lineMatchesNeedle(line, needle));
	});

	const lineBlocks = createMemo(() => groupLineBlocks(displayedLines()));

	function styledRange(line: LogLine, start: number, end: number): JSX.Element[] {
		const result: JSX.Element[] = [];
		let offset = 0;
		const text = line.spans.map((span) => span.text).join("");
		const marker = answers().markers.has(line) ? text.indexOf("💬") : -1;
		const markerEnd = marker < 0 ? -1 : marker + (text.slice(marker).match(/^💬[\uFE0E\uFE0F]? ?/)?.[0].length ?? 2);
		for (const span of line.spans) {
			const from = Math.max(start, offset);
			const to = Math.min(end, offset + span.text.length);
			const ranges =
				marker < 0
					? [[from, to]]
					: [
							[from, Math.min(to, marker)],
							[Math.max(from, markerEnd), to],
						];
			for (const [partFrom, partTo] of ranges) {
				if (partTo <= partFrom) continue;
				const part = span.text.slice(partFrom - offset, partTo - offset);
				const style = spanStyle(span);
				// Claude replaces its leading dot with one space while it pulses.
				// Font fallback can give even the text-form dot a different advance.
				const slotLength =
					partFrom === 0 ? (part.startsWith("⏺") ? (part[1] === "\uFE0E" ? 2 : 1) : part.startsWith(" ") ? 1 : 0) : 0;
				const content = slotLength ? (
					<>
						<span class={styles.dotSlot}>{part.slice(0, slotLength)}</span>
						{part.slice(slotLength)}
					</>
				) : (
					part
				);
				result.push(style ? <span style={style}>{content}</span> : content);
			}
			offset += span.text.length;
		}
		return result;
	}

	function renderLine(line: LogLine) {
		const text = line.spans.map((span) => span.text).join("");
		let indent = 0;
		// Count the dot's cell as indentation in both ON and blank OFF frames,
		// so their continuations use the same width and start after the dot slot.
		const indentText = text.replace(/^⏺\uFE0E?(?=[ \t])/, " ");
		for (const char of indentText) {
			if (char === " ") indent++;
			else if (char === "\t") indent += 8 - (indent % 8);
			else break;
		}
		// VS15 is display-only but still occupies a UTF-16 offset in styledRange.
		const anchor = INTENT_HIGHLIGHT_RE.exec(text.replaceAll("\uFE0E", " "));
		const boundary = anchor ? intentTitleEnd(text.slice(anchor[0].length)) : undefined;
		const end = boundary !== undefined ? anchor![0].length + boundary : text.length;
		const partial = !!anchor && text.slice(end).trim() !== "";
		const isIntent = !!anchor;
		const isAnswer = () => answers().lines.has(line);
		const marker = () => (isIntent && !partial ? "intent" : isAnswer() ? "answer" : undefined);
		const lineClass = isIntent && !partial ? `${styles.line} ${styles.intent}` : styles.line;
		const wrapStyle = () => ({
			"--wrap-indent": indent ? `${indent}ch` : undefined,
		});
		const links = detectOutputLinks(text).filter((link) => link.kind === "web" || props.onOpenFileLink);
		function content(start: number, end: number): JSX.Element[] {
			const parts: JSX.Element[] = [];
			let offset = start;
			for (const link of links) {
				if (link.end <= start || link.start >= end) continue;
				const from = Math.max(start, link.start);
				const to = Math.min(end, link.end);
				parts.push(...styledRange(line, offset, from));
				const label = styledRange(line, from, to);
				parts.push(
					link.kind === "web" ? (
						<a class={styles.webLink} href={link.text} target="_blank" rel="noopener noreferrer external">
							{label}
						</a>
					) : (
						<span
							role="button"
							tabIndex={0}
							class={styles.fileLink}
							onClick={() => props.onOpenFileLink?.(link.candidate!, link.line)}
							onKeyDown={(event) => {
								if (event.key === "Enter" || event.key === " ") {
									event.preventDefault();
									event.currentTarget.click();
								}
							}}
						>
							{label}
						</span>
					),
				);
				offset = to;
			}
			parts.push(...styledRange(line, offset, end));
			return parts;
		}
		return (
			<div class={lineClass} style={wrapStyle()} data-terminal-marker={marker()}>
				<Show when={isAnswer()}>
					<span aria-hidden="true" class={styles.answerRule} style={{ "box-shadow": ANSWER_RULE_SHADOW }} />
				</Show>
				{partial ? (
					<>
						<span class={styles.intent} data-terminal-marker="intent">
							{content(0, end)}
						</span>
						{content(end, text.length)}
					</>
				) : (
					content(0, text.length)
				)}
			</div>
		);
	}

	return (
		<div ref={containerEl} class={styles.output} style={{ display: props.visible === false ? "none" : undefined }}>
			<Show when={subscribeError()}>
				{(errMsg) => <div class={styles.error}>Failed to connect to terminal: {errMsg()}</div>}
			</Show>
			<Show when={loadingOlder()}>
				<div class={styles.loadingOlder}>Loading older output...</div>
			</Show>
			<pre ref={textEl} class={styles.text}>
				<For each={lineBlocks()}>
					{(block) => {
						return block.type === "table" ? (
							<div class={styles.tableBlock}>
								<For each={block.lines}>{renderLine}</For>
							</div>
						) : (
							renderLine(block.line)
						);
					}}
				</For>
			</pre>
		</div>
	);
}
