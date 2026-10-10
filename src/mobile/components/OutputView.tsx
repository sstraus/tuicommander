import { createMemo, createSignal, For, type JSX, onCleanup, onMount, Show } from "solid-js";
import { appLogger } from "../../stores/appLogger";
import { type PtySubscription, subscribePty } from "../../transport";
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
	sessionId: string;
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
	let unsubscribe: PtySubscription | null = null;
	// When the user scrolls up manually, stop auto-scrolling until they
	// return near the bottom.
	let userScrolledUp = false;
	// Tracks the oldest log offset we've loaded — 0 means full history is loaded.
	let oldestLoadedOffset = 0;

	/** Fetch initial log lines + screen rows via HTTP; returns the total_lines offset for WS catch-up. */
	async function fetchInitialOutput(): Promise<number> {
		try {
			const resp = await fetch(`/sessions/${props.sessionId}/output?format=log&limit=${INITIAL_FETCH_LIMIT}`);
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
			// Anchor viewport: save height before prepend, offset scrollTop by delta after
			const prevHeight = containerEl?.scrollHeight ?? 0;
			setLogLines((prev) => [...older, ...prev].slice(-MAX_LINES));
			oldestLoadedOffset = fetchOffset;
			requestAnimationFrame(() => {
				if (containerEl) {
					containerEl.scrollTop += containerEl.scrollHeight - prevHeight;
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
	const handleTouchStart = () => {
		touchActive = true;
	};
	const handleTouchEnd = () => {
		touchActive = false;
	};

	function scrollToBottom(force = false) {
		if (!force && (userScrolledUp || touchActive)) return;
		requestAnimationFrame(() => {
			if (containerEl) {
				containerEl.scrollTop = containerEl.scrollHeight;
			}
		});
	}

	function handleScroll() {
		if (!containerEl) return;
		// Larger threshold for touch devices where inertia scroll is imprecise
		const threshold = "ontouchstart" in window ? 200 : 80;
		const atBottom = containerEl.scrollHeight - containerEl.scrollTop - containerEl.clientHeight < threshold;
		userScrolledUp = !atBottom;
		// Lazy-load older lines when scrolling near the top
		if (containerEl.scrollTop < SCROLL_UP_THRESHOLD && oldestLoadedOffset > 0) {
			fetchOlderLines();
		}
	}

	// A backgrounded PWA renders nothing, so every frame a busy agent pushes is
	// pure battery and radio cost. Registered in the component body, not in the
	// async mount, so the listener has an owner to be released by.
	createVisibilityGate(
		() => unsubscribe?.pause(),
		() => unsubscribe?.resume(),
	);

	onMount(async () => {
		containerEl?.addEventListener("scroll", handleScroll, { passive: true });
		containerEl?.addEventListener("touchstart", handleTouchStart, { passive: true });
		containerEl?.addEventListener("touchend", handleTouchEnd, { passive: true });
		const offset = await fetchInitialOutput();

		try {
			unsubscribe =
				(await subscribePty(
					props.sessionId,
					() => {}, // unused — onLogLines handles log delivery
					() => {
						setLogLines((prev) => [...prev, { spans: [{ text: "--- session exited ---" }] }]);
						reconcileScreenRows([]);
					},
					{
						format: "log",
						logOffset: offset,
						onLogLines(rawLines) {
							setLogLines((prev) => {
								const incoming = rawLines.map(normalizeLogLine);
								return [...prev, ...incoming].slice(-MAX_LINES);
							});
							scrollToBottom();
						},
						onScreenRows(rows) {
							reconcileScreenRows(rows);
							scrollToBottom();
						},
						onStateChange: props.onStateChange,
						onInputLine: props.onInputLine,
					},
				)) ?? null;
			// The subscription is installed after an awaited fetch. A page hidden
			// during that window produced no visibilitychange to catch, so without
			// this the stream would run until the next one — which for a tab
			// restored in the background may never come.
			if (document.visibilityState === "hidden") unsubscribe?.pause();
		} catch (err) {
			const msg = err instanceof Error ? err.message : String(err);
			appLogger.error("terminal", "Failed to subscribe to PTY output", { error: msg });
			setSubscribeError(msg);
		}
	});

	onCleanup(() => {
		unsubscribe?.();
		containerEl?.removeEventListener("scroll", handleScroll);
		containerEl?.removeEventListener("touchstart", handleTouchStart);
		containerEl?.removeEventListener("touchend", handleTouchEnd);
	});

	// Reflow before filtering: it joins rows by their adjacency on the source
	// terminal, which a search filter no longer preserves.
	const allLines = createMemo(() => reflowDisplayLines([...logLines(), ...screenRows()]));

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
		for (const span of line.spans) {
			const from = Math.max(start, offset);
			const to = Math.min(end, offset + span.text.length);
			if (to > from) {
				const part = span.text.slice(from - offset, to - offset);
				const style = spanStyle(span);
				// Claude replaces its leading dot with one space while it pulses.
				// Font fallback can give even the text-form dot a different advance.
				const slotLength =
					from === 0 ? (part.startsWith("⏺") ? (part[1] === "\uFE0E" ? 2 : 1) : part.startsWith(" ") ? 1 : 0) : 0;
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
		const wrapStyle = indent ? { "--wrap-indent": `${indent}ch` } : undefined;
		const links = detectOutputLinks(text).filter((link) => link.kind === "web" || props.onOpenFileLink);
		if (links.length === 0) {
			return (
				<div class={styles.line} style={wrapStyle}>
					{styledRange(line, 0, text.length)}
				</div>
			);
		}
		const parts: JSX.Element[] = [];
		let offset = 0;
		for (const link of links) {
			parts.push(...styledRange(line, offset, link.start));
			const content = styledRange(line, link.start, link.end);
			parts.push(
				link.kind === "web" ? (
					<a class={styles.webLink} href={link.text} target="_blank" rel="noopener noreferrer external">
						{content}
					</a>
				) : (
					<button
						type="button"
						class={styles.fileLink}
						onClick={() => props.onOpenFileLink?.(link.candidate!, link.line)}
					>
						{content}
					</button>
				),
			);
			offset = link.end;
		}
		parts.push(...styledRange(line, offset, text.length));
		return (
			<div class={styles.line} style={wrapStyle}>
				{parts}
			</div>
		);
	}

	return (
		<div ref={containerEl} class={styles.output}>
			<Show when={subscribeError()}>
				{(errMsg) => <div class={styles.error}>Failed to connect to terminal: {errMsg()}</div>}
			</Show>
			<Show when={loadingOlder()}>
				<div class={styles.loadingOlder}>Loading older output...</div>
			</Show>
			<pre class={styles.text}>
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
