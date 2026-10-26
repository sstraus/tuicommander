import { Button } from "../ui/Button";
import { Input } from "../ui/Input";
/**
 * The conversation, drawn from the projection `acpTranscript` builds.
 *
 * Nothing here reaches for the store or the client: an entry arrives already
 * folded — chunks joined, tool-call updates merged into the card that opened
 * them, one plan rather than every intermediate copy of it — so this file is
 * only the shape each kind takes on screen.
 */

import {
	type Component,
	createEffect,
	createMemo,
	createSignal,
	For,
	type JSX,
	Match,
	onCleanup,
	Show,
	Switch,
} from "solid-js";
import type { AcpNoticeAction, AcpTranscriptEntry } from "../../stores/acpTranscript";
import { appLogger } from "../../stores/appLogger";
import { transcriptPreferences } from "../../stores/transcriptPreferences";
import type { AcpToolCall, AcpToolCallContent } from "../../types/acp";
import { cx } from "../../utils";
import { writeClipboard } from "../../utils/clipboard";
import { handleOpenUrl } from "../../utils/openUrl";
import { stripAnswerMarkers } from "../../utils/stripAnswerMarkers";
import { TerminalUserText, terminalAnswerRows, terminalToolTitle } from "../Terminal/chatPresentation";
import { filePathRegex, matchWebUrls } from "../Terminal/linkProvider";
import { ANSWER_MARKER_RE, answerBlockRanges, type ChatBlock } from "../Terminal/suggestOverlay";
import { ContentRenderer } from "../ui/ContentRenderer";
import s from "./AIChatPanel.module.css";
import { projectChatProtocolText } from "./protocolText";

/** Why a turn ended, for the turns that ended without an answer. */
const SETTLEMENTS: Record<string, string> = {
	empty: "Turn ended without a reply.",
	cancelled: "Turn cancelled.",
	refusal: "The agent refused this turn.",
	max_tokens: "The turn stopped: the answer ran out of room.",
	max_turn_requests: "The turn stopped: it reached its request limit.",
};

const NOTICE_LABELS: Record<AcpNoticeAction["kind"], { title: string; button: string }> = {
	open_result: { title: "Result ready", button: "Open result" },
	answer: { title: "Question", button: "Answer" },
	approve: { title: "Approval needed", button: "Approve" },
};

function settlement(stopReason: string): string {
	return SETTLEMENTS[stopReason] ?? `Turn ended: ${stopReason}.`;
}

const STATUS_CLASS: Record<string, string> = {
	pending: s.toolCallPending,
	in_progress: s.toolCallPending,
	completed: s.toolCallSuccess,
	failed: s.toolCallFailure,
};

/** The one line a tool-call body is worth: what it touched, or what it said. */
function toolCallDetail(call: AcpToolCall): string {
	const locations = call.locations?.map((location) => location.path) ?? [];
	if (locations.length > 0) return locations.join(", ");
	return (call.content ?? []).map(contentLine).filter(Boolean).join("\n");
}

function contentLine(content: AcpToolCallContent): string {
	if (content.type === "diff") return content.path;
	if (content.type === "terminal") return `terminal ${content.terminalId}`;
	return content.content.type === "text" ? content.content.text : "";
}

function toolName(title: string): string {
	return title.split(/\s+-lc\s+|\s+-c\s+/, 1)[0];
}

const ToolActivity: Component<{
	calls: () => AcpToolCall[];
	observeDuration?: boolean;
	terminalAgent?: string;
	intent?: { title?: string | null; text: string };
}> = (props) => {
	const startedAt = performance.now();
	let finishedAt: number | undefined;
	let observedCount = 0;
	const status = () => {
		const calls = props.calls();
		if (calls.some((call) => call.status === "failed")) return "Failed";
		if (calls.some((call) => call.status === "pending" || call.status === "in_progress" || !call.status))
			return "Running";
		return "Completed";
	};
	const duration = () => {
		const calls = props.calls();
		if (calls.length !== observedCount) {
			observedCount = calls.length;
			finishedAt = undefined;
		}
		if (calls.every((call) => call.status === "completed" || call.status === "failed"))
			finishedAt ??= performance.now();
		const seconds = ((finishedAt ?? performance.now()) - startedAt) / 1000;
		return `${seconds.toFixed(1)}s`;
	};
	return (
		<details class={s.toolActivity}>
			<summary class={s.toolActivitySummary}>
				<span
					class={cx(
						s.toolCallStatusDot,
						status() === "Failed" ? s.toolCallFailure : status() === "Running" ? s.toolCallPending : s.toolCallSuccess,
					)}
				/>
				<Show when={props.intent}>
					<span class={s.stepTitle}>{props.intent?.title ?? "Status"}</span>
					<span class={s.stepDescription}>{props.intent?.text}</span>
				</Show>
				<span class={s.toolCallCount}>
					{props.calls().length}{" "}
					{props.calls().every((call) => call.kind === "read")
						? props.calls().length === 1
							? "read"
							: "reads"
						: props.calls().length === 1
							? "tool call"
							: "tool calls"}{" "}
					›
				</span>
				<span class={s.toolActivityTitles} hidden={!!props.intent}>
					{props
						.calls()
						.slice(0, 2)
						.map((call) => (props.terminalAgent ? terminalToolTitle(call, props.terminalAgent) : toolName(call.title)))
						.join(" · ")}
					{props.calls().length > 2 ? " · …" : ""}
				</span>
				<span class={s.toolCallDuration}>
					<Show when={props.observeDuration !== false}>{duration()} observed · </Show>
					{status()}
				</span>
			</summary>
			<div class={s.toolActivityCalls}>
				<For each={props.calls()}>
					{(call) => (
						<details class={s.toolActivityCall}>
							<summary class={s.toolActivityCallSummary}>
								{/* Keyed on status so a settled call gets a fresh element: WebKit does not
								    restyle a closed <details>, so swapping the class there left the
								    pending pulse running on a completed dot (#1153-a8b8). */}
								<Show when={STATUS_CLASS[call.status ?? "pending"]} keyed>
									{(statusClass) => <span class={cx(s.toolCallStatusDot, statusClass)} />}
								</Show>
								<span class={s.toolCallName}>{call.title}</span>
								<span class={s.toolCallDuration}>
									{call.kind ?? "other"} ·{" "}
									{call.status === "failed" ? "Failed" : call.status === "completed" ? "Completed" : "Running"}
								</span>
							</summary>
							<For each={call.content}>
								{(content) => (
									<Show when={content.type === "diff" && content}>
										{(diff) => (
											<figure class={s.editDiff} aria-label={`Edit diff: ${diff().path}`}>
												<figcaption>{diff().path}</figcaption>
												<div
													class={s.editDiffScroll}
													role="region"
													aria-label={`Scrollable edit: ${diff().path}`}
													tabindex="0"
												>
													<div class={s.editDiffVersions}>
														<Show when={diff().oldText != null}>
															<div class={s.editDiffOld}>
																<span>Old</span>
																<pre aria-label="Old text">{diff().oldText}</pre>
															</div>
														</Show>
														<div class={s.editDiffNew}>
															<span>New</span>
															<pre aria-label="New text">{diff().newText}</pre>
														</div>
													</div>
												</div>
											</figure>
										)}
									</Show>
								)}
							</For>
							<Show when={toolCallDetail(call)}>
								<div class={s.toolCallBody}>
									{toolCallDetail(call)}
									<CopyButton label="Copy tool output" text={toolCallDetail(call)} />
								</div>
							</Show>
						</details>
					)}
				</For>
			</div>
		</details>
	);
};

/** A 💬 line anywhere in the message: an answer is never intermediate text. */
const ANSWER_LINE_RE = new RegExp(ANSWER_MARKER_RE.source, "m");

/** Keep the first call as the stable row anchor; later calls belong to it. */
function activityRows(
	entries: AcpTranscriptEntry[],
	showSuggestions: boolean,
): {
	visible: AcpTranscriptEntry[];
	calls: Map<string, AcpToolCall[]>;
	refusals: Map<string, string>;
	thoughts: Map<string, string>;
	intermediate: Set<string>;
	intents: Map<string, { title?: string | null; text: string }>;
	mergedIntents: Set<string>;
} {
	const visible: AcpTranscriptEntry[] = [];
	const calls = new Map<string, AcpToolCall[]>();
	const refusals = new Map<string, string>();
	const thoughts = new Map<string, string>();
	const intermediate = new Set<string>();
	const intents = new Map<string, { title?: string | null; text: string }>();
	const mergedIntents = new Set<string>();
	let pendingIntent: { id: string; title?: string | null; text: string } | undefined;
	// Assistant text of the open turn that no tool call has followed yet.
	let pendingText: string[] = [];
	let current: AcpToolCall[] | undefined;
	for (const entry of entries) {
		if (entry.kind === "agent") {
			const projected = projectChatProtocolText(entry.text);
			if (projected.intent) pendingIntent = { id: entry.id, ...projected.intent };
			if (!projected.body.trim() && !projected.intent && !(showSuggestions && projected.suggestions.length)) continue;
		}
		if (entry.kind === "thought") {
			if (!entry.text.trim()) continue;
			const previous = visible.at(-1);
			if (previous?.kind === "thought" && previous.inherited === entry.inherited) {
				thoughts.set(previous.id, `${thoughts.get(previous.id)}\n\n${entry.text}`);
				continue;
			}
			thoughts.set(entry.id, entry.text);
		}
		if (
			entry.kind === "user" ||
			entry.kind === "settled" ||
			entry.kind === "failed" ||
			visible.at(-1)?.inherited !== entry.inherited
		)
			current = undefined;
		if (entry.kind === "settled" && entry.stopReason === "refusal") {
			const reply: string[] = [];
			for (let i = visible.length - 1; i >= 0; i--) {
				const previous = visible[i];
				if (previous.kind === "user" || previous.kind === "settled" || previous.kind === "failed") break;
				if (previous.kind === "agent") {
					reply.unshift(previous.text);
					visible.splice(i, 1);
				}
			}
			refusals.set(entry.id, reply.join(" ").replace(/\s+/g, " ").trim());
		}
		if (entry.kind === "user" || entry.kind === "settled" || entry.kind === "failed") {
			pendingText = [];
			pendingIntent = undefined;
		}
		if (entry.kind === "agent" && !ANSWER_LINE_RE.test(projectChatProtocolText(entry.text).body))
			pendingText.push(entry.id);
		if (entry.kind === "tool") {
			for (const id of pendingText) intermediate.add(id);
			pendingText = [];
			if (!current) {
				current = [];
				calls.set(entry.id, current);
				if (pendingIntent) {
					intents.set(entry.id, pendingIntent);
					mergedIntents.add(pendingIntent.id);
					pendingIntent = undefined;
				}
				visible.push(entry);
			}
			current.push(entry.call);
		} else {
			visible.push(entry);
		}
	}
	return { visible, calls, refusals, thoughts, intermediate, intents, mergedIntents };
}

export interface TranscriptSearchRef {
	open: () => void;
	/** Close Find and report whether it was open. */
	close: () => boolean;
}

export interface TranscriptProps {
	/** Let a host expose the existing follow state and jump action outside the transcript. */
	onAtBottomChange?: (atBottom: boolean) => void;
	onRegisterScrollToBottom?: (jump: (() => void) | undefined) => void;
	/** Terminal-only reading layout; the agent scopes Claude Bash descriptions. */
	terminalAgent?: string;
	/** Lets a containing terminal route its Find action to this transcript. */
	onSearchRef?: (ref: TranscriptSearchRef | undefined) => void;
	/** Constrain intrinsic content to the phone viewport. */
	mobile?: boolean;
	entries: () => AcpTranscriptEntry[];
	/** Shown while a turn is running and nothing has streamed back yet. */
	busy: () => boolean;
	/** History has no execution timestamps: never time it from the view's mount. */
	observeToolDuration?: boolean;
	/** ego's provider-retry line ("… retrying in 2s (attempt 2/6)"), shown instead of the pulse. */
	retry?: () => string | null;
	emptyMessage: string;
	onOpenFile?: (href: string) => void | Promise<boolean | undefined> | Promise<void>;
	onOpenTable?: (source: string) => void;
	onClear?: () => void;
	canForkAtMessage?: () => boolean;
	onFork?: (messageId: string) => void;
	/** `open_result` opens its file. `answer` and `approve` have nothing to open: the open interaction is drawn at the end, and the transcript scrolls there. */
	onNoticeAction?: (action: AcpNoticeAction) => void;
	/** Draw assistant text that a tool call follows in the same turn as a muted status line. */
	muteIntermediate?: boolean;
	/** Absent for a read-only transcript: suggested replies are then not drawn. */
	onSuggestion?: (text: string) => void;
	/** Open questions, drawn at the end of the conversation they belong to. */
	children?: JSX.Element;
}

const CopyButton: Component<{ label: string; text: string; answer?: boolean }> = (props) => (
	<Button
		type="button"
		variant="ghost"
		class={s.copyAction}
		aria-label={props.label}
		onClick={() =>
			void writeClipboard(props.answer ? stripAnswerMarkers(props.text) : props.text).catch((error) =>
				appLogger.error("ai-chat", "Copy failed", error),
			)
		}
	>
		Copy
	</Button>
);

const LinkedPlainText: Component<{ text: string; onOpenFile?: (href: string) => void }> = (props) => {
	const parts = createMemo(() => {
		const source = props.text;
		const links = matchWebUrls(source).map((url) => ({ start: url.index, text: url.text, web: true }));
		for (const match of source.matchAll(filePathRegex())) {
			links.push({ start: match.index + match[0].indexOf(match[1]), text: match[1], web: false });
		}
		links.sort((a, b) => a.start - b.start);
		const segments: { text: string; web?: boolean; file?: boolean }[] = [];
		let end = 0;
		for (const link of links) {
			if (link.start < end) continue;
			segments.push({ text: source.slice(end, link.start) });
			segments.push({ text: link.text, web: link.web, file: !link.web });
			end = link.start + link.text.length;
		}
		segments.push({ text: source.slice(end) });
		return segments;
	});
	return (
		<For each={parts()}>
			{(part) =>
				part.web ? (
					<a
						href={part.text}
						data-tuic-href={part.text}
						onClick={(event) => {
							event.preventDefault();
							handleOpenUrl(part.text);
						}}
					>
						{part.text}
					</a>
				) : part.file ? (
					<a
						href={part.text}
						onClick={(event) => {
							event.preventDefault();
							props.onOpenFile?.(part.text);
						}}
					>
						{part.text}
					</a>
				) : (
					part.text
				)
			}
		</For>
	);
};

/** These exact harness turns are not words typed by the person reading the chat. */
function isHarnessNotice(text: string): boolean {
	return (
		/^\[TUIC\] message available — read it with: agent action=inbox\s*$/.test(text.trim()) ||
		text.trim() === "[Request interrupted by user for tool use]" ||
		text.trim() === "[Request interrupted by user]"
	);
}

const UserText: Component<{ text: string; onOpenFile?: (href: string) => void }> = (props) => {
	const parts = createMemo(() => {
		const parts: { text: string; image?: string }[] = [];
		let end = 0;
		for (const match of props.text.matchAll(/\[Image #(\d+)\](?:\s*\[image\])?|\[image\]/g)) {
			parts.push({ text: props.text.slice(end, match.index) });
			parts.push({
				text: match[1] ? `Image ${match[1]}` : "Image",
				image: match[1] ? `Image attachment ${match[1]}` : "Image attachment",
			});
			end = match.index + match[0].length;
		}
		parts.push({ text: props.text.slice(end) });
		return parts;
	});
	return (
		<For each={parts()}>
			{(part) =>
				part.image ? (
					<span class={s.attachmentChip} role="img" aria-label={part.image}>
						{part.text}
					</span>
				) : (
					<LinkedPlainText text={part.text} onOpenFile={props.onOpenFile} />
				)
			}
		</For>
	);
};

/** Reuse the terminal's answer marker ranges after Markdown has rendered.
 * An answer spans the blocks the grid highlights (`answerBlockRanges`); the 💬 glyph is
 * dropped from each marked paragraph, which keeps `data-tuic-answer-start` so a later pass
 * still finds it. Code examples and quotations keep their literal marker; stored/copy text stays intact. */
function highlightAnswers(container: HTMLDivElement | undefined): void {
	const elements = Array.from(container?.querySelectorAll<HTMLElement>(":scope > div > *") ?? []);
	const blocks: ChatBlock[] = elements.map((element) => {
		const text = element.textContent ?? "";
		const paragraph = element.tagName === "P";
		return {
			kind: paragraph ? "paragraph" : element.tagName === "PRE" ? "code" : "other",
			text,
			marker: paragraph && (element.hasAttribute("data-tuic-answer-start") || ANSWER_MARKER_RE.test(text)),
		};
	});
	for (const element of elements) {
		element.removeAttribute("data-tuic-answer");
		element.removeAttribute("data-tuic-answer-region-start");
	}
	for (const [first, last] of answerBlockRanges(blocks)) {
		elements[first].setAttribute("data-tuic-answer-region-start", "");
		for (let index = first; index <= last; index++)
			elements[index].setAttribute("data-tuic-answer", index === last ? "end" : "");
	}
	for (const [index, block] of blocks.entries()) {
		if (block.marker) stripAnswerMarker(elements[index]);
	}
	// A list answer belongs to the item, not its entire UL/OL (which may also
	// contain unmarked siblings). A paragraph answer above a list still owns it.
	for (const item of container?.querySelectorAll<HTMLElement>("li") ?? []) {
		if (item.closest("blockquote, pre")) continue;
		let text = "";
		for (const node of item.childNodes) {
			if (node instanceof HTMLElement && /^(UL|OL|PRE|BLOCKQUOTE)$/.test(node.tagName)) break;
			text += node.textContent ?? "";
		}
		if (!item.hasAttribute("data-tuic-answer-start") && !ANSWER_MARKER_RE.test(text)) continue;
		if (!item.parentElement?.closest("[data-tuic-answer]")) {
			item.setAttribute("data-tuic-answer-region-start", "");
			item.setAttribute("data-tuic-answer", "end");
		}
		stripAnswerMarker(item);
	}
}

/** Strip only the opening marker, including when Markdown splits it across text nodes. */
function stripAnswerMarker(head: HTMLElement): void {
	if (head.hasAttribute("data-tuic-answer-start")) return;
	head.setAttribute("data-tuic-answer-start", "");
	let remaining = /^[\s●⏺•]*💬[\t ]*/.exec(head.textContent ?? "")?.[0].length ?? 0;
	const walker = document.createTreeWalker(head, NodeFilter.SHOW_TEXT);
	while (remaining > 0 && walker.nextNode()) {
		const node = walker.currentNode;
		const text = node.textContent ?? "";
		node.textContent = text.slice(remaining);
		remaining -= Math.min(remaining, text.length);
	}
}

export const Transcript: Component<TranscriptProps> = (props) => {
	const activity = createMemo(() => {
		const activity = activityRows(props.entries(), !!props.onSuggestion);
		if (props.terminalAgent) {
			activity.visible = terminalAnswerRows(activity.visible, activity.intermediate);
			activity.mergedIntents.clear();
		}
		return activity;
	});
	const latestAnswerId = createMemo(
		() =>
			props.entries().findLast((entry) => {
				if (entry.kind !== "agent") return false;
				const projected = projectChatProtocolText(entry.text);
				return !!projected.body.trim() || projected.suggestions.length > 0;
			})?.id,
	);
	const firstLocalId = createMemo(() =>
		activity().visible.some((entry) => entry.inherited)
			? activity().visible.find((entry) => !entry.inherited)?.id
			: undefined,
	);
	const [finding, setFinding] = createSignal(false);
	const [query, setQuery] = createSignal("");
	let container: HTMLDivElement | undefined;
	let searchInput: HTMLInputElement | undefined;
	let matchIndex = -1;
	let stickToBottom = true;
	let touchActive = false;
	let lastScrollTop = 0;
	const [newTextBelow, setNewTextBelow] = createSignal(false);
	let previousContent = new Map<string, number>();
	createEffect(() => {
		props.busy();
		props.retry?.();
		const content = new Map(
			props
				.entries()
				.map((entry): [string, number] => [
					entry.id,
					"text" in entry ? entry.text.length : entry.kind === "tool" ? toolCallDetail(entry.call).length : 0,
				]),
		);
		const sameConversation = [...previousContent.keys()].some((id) => content.has(id));
		const grew = [...content].some(
			([id, length]) => !previousContent.has(id) || length > (previousContent.get(id) ?? 0),
		);
		if (!sameConversation || !transcriptPreferences.enabled()) setNewTextBelow(false);
		else if (grew && !stickToBottom) setNewTextBelow(true);
		previousContent = content;
		queueMicrotask(() => {
			if (container && stickToBottom && !touchActive) {
				container.scrollTop = container.scrollHeight;
				lastScrollTop = container.scrollTop;
				props.onAtBottomChange?.(true);
			}
		});
	});
	const scrollToBottom = () => {
		stickToBottom = true;
		setNewTextBelow(false);
		if (container) {
			container.scrollTop = container.scrollHeight;
			lastScrollTop = container.scrollTop;
			props.onAtBottomChange?.(true);
		}
	};
	props.onRegisterScrollToBottom?.(scrollToBottom);
	onCleanup(() => props.onRegisterScrollToBottom?.(undefined));
	const onTouchEnd = () => {
		touchActive = false;
		if (stickToBottom) scrollToBottom();
	};
	const onScroll = () => {
		if (!container) return;
		const atBottom = container.scrollHeight - container.clientHeight - container.scrollTop <= 1;
		props.onAtBottomChange?.(atBottom);
		if (atBottom) stickToBottom = true;
		else if (container.scrollTop < lastScrollTop) stickToBottom = false;
		lastScrollTop = container.scrollTop;
		if (stickToBottom) setNewTextBelow(false);
	};

	const runNoticeAction = (action: AcpNoticeAction) => {
		if (action.kind !== "open_result" && container) {
			scrollToBottom();
		}
		props.onNoticeAction?.(action);
	};
	const clearSearchSelection = () => {
		const selection = window.getSelection();
		if (container && selection?.rangeCount && container.contains(selection.getRangeAt(0).commonAncestorContainer)) {
			selection.removeAllRanges();
		}
	};
	const openSearch = () => {
		setFinding(true);
		queueMicrotask(() => {
			if (!finding() || !searchInput?.isConnected) return;
			searchInput.focus();
			searchInput.select();
		});
	};
	const closeSearch = () => {
		const wasOpen = finding();
		if (!wasOpen) return false;
		clearSearchSelection();
		setFinding(false);
		matchIndex = -1;
		return wasOpen;
	};
	props.onSearchRef?.({ open: openSearch, close: closeSearch });
	onCleanup(() => {
		clearSearchSelection();
		props.onSearchRef?.(undefined);
	});
	const findNext = (direction = 1) => {
		if (!container || !query()) return;
		const pattern = new RegExp(query().replace(/[.*+?^${}()|[\]\\]/g, "\\$&"), "giu");
		const matches: Range[] = [];
		const blocks: {
			element: Element | null;
			text: string;
			offsets: { node: Node; start: number; end: number; endNode?: Node }[];
		}[] = [];
		let current: (typeof blocks)[number] | undefined;
		const walker = document.createTreeWalker(container, NodeFilter.SHOW_TEXT);
		while (walker.nextNode()) {
			const node = walker.currentNode;
			const parent = node.parentElement;
			let hidden = false;
			for (
				let disclosure = parent?.closest("details:not([open])");
				disclosure;
				disclosure = disclosure.parentElement?.closest("details:not([open])")
			) {
				if (!disclosure.querySelector(":scope > summary")?.contains(node)) hidden = true;
			}
			if (hidden || parent?.closest("button, input, ." + s.findBar.split(" ").join("."))) {
				current = undefined;
				continue;
			}
			// Inline Markdown belongs to one searchable block, but separate messages,
			// paragraphs and table cells must never produce a synthetic phrase.
			const element = parent?.closest("p, h1, h2, h3, h4, h5, h6, pre, li, td, th, div, summary, blockquote") ?? null;
			if (!current || current.element !== element) {
				current = { element, text: "", offsets: [] };
				blocks.push(current);
			}
			let preformatted = !!parent?.closest("pre");
			for (let ancestor = parent; ancestor && !preformatted; ancestor = ancestor.parentElement) {
				const whitespace = getComputedStyle(ancestor).whiteSpace;
				preformatted = whitespace.startsWith("pre") || whitespace === "break-spaces";
			}
			const text = node.textContent ?? "";
			for (let offset = 0; offset < text.length; offset++) {
				// CSS collapses ASCII whitespace across inline nodes in normal prose.
				const character = !preformatted && /[ \t\r\n\f]/.test(text[offset]) ? " " : text[offset];
				if (!preformatted && character === " " && current.text.endsWith(" ")) {
					const previous = current.offsets.at(-1)!;
					previous.end = offset + 1;
					// A collapsed run can span nodes; retain its final DOM endpoint.
					previous.endNode = node;
					continue;
				}
				current.text += character;
				current.offsets.push({ node, start: offset, end: offset + 1 });
			}
		}
		for (const block of blocks) {
			for (const match of block.text.matchAll(pattern)) {
				const first = block.offsets[match.index];
				const last = block.offsets[match.index + match[0].length - 1];
				if (!first || !last) continue;
				const range = document.createRange();
				range.setStart(first.node, first.start);
				range.setEnd(last.endNode ?? last.node, last.end);
				matches.push(range);
			}
		}
		clearSearchSelection();
		if (!matches.length) return;
		matchIndex =
			matchIndex < 0
				? direction < 0
					? matches.length - 1
					: 0
				: (matchIndex + direction + matches.length) % matches.length;
		stickToBottom = false;
		setNewTextBelow(false);
		const selection = window.getSelection();
		selection?.removeAllRanges();
		selection?.addRange(matches[matchIndex]);
		matches[matchIndex].startContainer.parentElement?.scrollIntoView?.({ block: "center" });
	};
	const onKeyDown = (event: KeyboardEvent) => {
		if (event.key === "Escape" && finding()) {
			event.preventDefault();
			event.stopPropagation();
			closeSearch();
			return;
		}
		if (!(event.metaKey || event.ctrlKey) || event.shiftKey || event.altKey) return;
		if (event.target === searchInput) return;
		const key = event.key.toLowerCase();
		if (key === "a" && container) {
			event.preventDefault();
			event.stopPropagation();
			const range = document.createRange();
			range.selectNodeContents(container);
			window.getSelection()?.removeAllRanges();
			window.getSelection()?.addRange(range);
		} else if (key === "c" && container) {
			const selection = window.getSelection();
			if (!selection?.rangeCount || !container.contains(selection.getRangeAt(0).commonAncestorContainer)) return;
			const selected = selection.toString();
			if (!selected) return;
			event.preventDefault();
			event.stopPropagation();
			void writeClipboard(selected).catch((error) => appLogger.error("ai-chat", "Copy failed", error));
		} else if (key === "f") {
			event.preventDefault();
			event.stopPropagation();
			openSearch();
		} else if (key === "k") {
			event.preventDefault();
			event.stopPropagation();
			props.onClear?.();
		}
	};
	return (
		<div class={s.transcriptViewport}>
			<div
				class={cx(s.messageList, props.mobile && s.mobileTranscript)}
				ref={container}
				aria-label="Chat transcript"
				data-terminal-chat={props.terminalAgent ? "" : undefined}
				tabIndex={0}
				onKeyDown={onKeyDown}
				onScroll={onScroll}
				onTouchStart={() => {
					touchActive = true;
				}}
				onTouchEnd={onTouchEnd}
				onTouchCancel={onTouchEnd}
			>
				<Show when={finding()}>
					<div class={s.findBar}>
						<Input
							ref={searchInput}
							aria-label="Find in chat"
							value={query()}
							onInput={(event) => {
								clearSearchSelection();
								setQuery(event.currentTarget.value);
								matchIndex = -1;
							}}
							onKeyDown={(event) => {
								if (event.key === "Enter") {
									event.preventDefault();
									event.stopPropagation();
									findNext(event.shiftKey ? -1 : 1);
								}
								if (event.key === "Escape") {
									event.preventDefault();
									event.stopPropagation();
									closeSearch();
								}
							}}
						/>
						<Button type="button" onClick={() => findNext(-1)}>
							Previous
						</Button>
						<Button type="button" onClick={() => findNext()}>
							Next
						</Button>
						<Button type="button" shape="icon" aria-label="Close chat search" onClick={closeSearch}>
							<svg width="12" height="12" viewBox="0 0 12 12" fill="currentColor" aria-hidden="true">
								<path d="M2.8 2l3.2 3.2L9.2 2l.8.8L6.8 6l3.2 3.2-.8.8L6 6.8 2.8 10l-.8-.8L5.2 6 2 2.8z" />
							</svg>
						</Button>
					</div>
				</Show>
				<Show when={props.entries().length > 0} fallback={<div class={s.emptyState}>{props.emptyMessage}</div>}>
					<Show when={props.entries()[0]?.inherited}>
						<div class={s.historyBoundary}>Inherited history</div>
					</Show>
					<For each={activity().visible}>
						{(entry) => (
							<>
								<Show when={entry.id === firstLocalId()}>
									<div class={s.historyBoundary} role="separator" aria-label="Inherited history ends">
										This conversation
									</div>
								</Show>
								<Switch>
									<Match when={entry.kind === "user" && entry}>
										{(user) => (
											<Show
												when={!isHarnessNotice(user().text)}
												fallback={
													<Show when={!props.terminalAgent || !user().text.trim().startsWith("[TUIC]")}>
														<div class={s.systemNotice} role="note" title={user().text}>
															{user().text}
														</div>
													</Show>
												}
											>
												<div class={s.userMsg}>
													<Show
														when={props.terminalAgent}
														fallback={<UserText text={user().text} onOpenFile={props.onOpenFile} />}
													>
														<TerminalUserText
															text={user().text}
															prose={(text) => <UserText text={text} onOpenFile={props.onOpenFile} />}
														/>
													</Show>
													<CopyButton label="Copy user message" text={user().text} />
												</div>
											</Show>
										)}
									</Match>
									<Match when={entry.kind === "agent" && entry}>
										{(agent) => {
											const projected = createMemo(() => projectChatProtocolText(agent().text));
											const intermediate = () =>
												props.muteIntermediate !== false && activity().intermediate.has(agent().id);
											let content: HTMLDivElement | undefined;
											createEffect(() => {
												projected().body;
												queueMicrotask(() => highlightAnswers(content));
											});
											return (
												<Show
													when={
														projected().body.trim() ||
														(projected().intent && !activity().mergedIntents.has(agent().id)) ||
														(props.onSuggestion &&
															latestAnswerId() === agent().id &&
															projected().suggestions.length > 0)
													}
												>
													<div
														class={cx(s.assistantMsg, intermediate() && s.intermediateMsg)}
														data-intermediate={intermediate() ? "" : undefined}
														data-answer-card={
															props.terminalAgent && !activity().intermediate.has(agent().id) && projected().body.trim()
																? ""
																: undefined
														}
													>
														<Show
															when={
																projected().intent && !activity().mergedIntents.has(agent().id)
																	? projected().intent
																	: undefined
															}
														>
															{(intent) => (
																<div class={s.agentIntent} aria-label="Agent intent">
																	<span>{intent().title ?? "Status"}</span>
																	{intent().text}
																</div>
															)}
														</Show>
														<Show when={projected().body}>
															<ContentRenderer
																contentRef={(element) => {
																	content = element;
																}}
																content={projected().body}
																incremental={true}
																tableActions={
																	!props.terminalAgent && props.onOpenTable
																		? {
																				copy: (source) =>
																					void writeClipboard(source).catch((error) =>
																						appLogger.error("ai-chat", "Copy failed", error),
																					),
																				open: props.onOpenTable,
																			}
																		: undefined
																}
																onLinkClick={props.onOpenFile}
																autoLinkFiles={true}
																onCodeCopy={(text) =>
																	void writeClipboard(text).catch((error) =>
																		appLogger.error("ai-chat", "Copy failed", error),
																	)
																}
															/>
														</Show>
														<div class={s.replyActions}>
															<CopyButton label="Copy assistant message" text={agent().text} answer />
															<Show when={props.canForkAtMessage?.() && agent().messageId}>
																<Button
																	type="button"
																	variant="ghost"
																	class={s.copyAction}
																	aria-label="Fork from here"
																	disabled={props.busy()}
																	onClick={() => {
																		const id = agent().messageId;
																		if (id) props.onFork?.(id);
																	}}
																>
																	Fork from here
																</Button>
															</Show>
														</div>
														<Show
															when={
																props.onSuggestion &&
																latestAnswerId() === agent().id &&
																projected().suggestions.length > 0
															}
														>
															<div class={s.suggestedReplies} aria-label="Suggested replies">
																<For each={projected().suggestions}>
																	{(item) => (
																		<Button type="button" onClick={() => props.onSuggestion?.(item)}>
																			{item}
																		</Button>
																	)}
																</For>
															</div>
														</Show>
													</div>
												</Show>
											);
										}}
									</Match>
									<Match when={entry.kind === "notice" && entry}>
										{(notice) => (
											<div class={s.noticeCard} role="group" aria-label="Notice">
												<div class={s.noticeTitle}>
													{(notice().action && NOTICE_LABELS[notice().action!.kind].title) || "Notice"}
												</div>
												<div class={s.noticeBody}>{notice().text}</div>
												<Show when={notice().action}>
													{(action) => (
														<Button type="button" class={s.noticeAction} onClick={() => runNoticeAction(action())}>
															{NOTICE_LABELS[action().kind].button}
														</Button>
													)}
												</Show>
											</div>
										)}
									</Match>
									<Match when={entry.kind === "thought" && entry}>
										{(thought) => (
											<details class={s.reasoningDisclosure}>
												<summary class={s.reasoningSummary}>Thinking</summary>
												<div class={s.reasoningBody}>{activity().thoughts.get(thought().id)}</div>
											</details>
										)}
									</Match>
									<Match when={entry.kind === "tool" && entry}>
										{(tool) => (
											<ToolActivity
												calls={() => activity().calls.get(tool().id) ?? []}
												intent={props.terminalAgent ? undefined : activity().intents.get(tool().id)}
												observeDuration={props.observeToolDuration}
												terminalAgent={props.terminalAgent}
											/>
										)}
									</Match>
									<Match when={entry.kind === "plan" && entry}>
										{(plan) => (
											<div class={s.toolCallCard}>
												<div class={s.toolCallHeader}>
													<span class={s.toolCallName}>Plan</span>
												</div>
												<ul class={s.planList}>
													<For each={plan().entries}>
														{(step) => (
															<li
																class={cx(
																	s.planItem,
																	step.status === "completed" && s.planItemDone,
																	step.status === "in_progress" && s.planItemActive,
																)}
															>
																<span>{step.status === "completed" ? "✓" : "•"}</span>
																<span>{step.content}</span>
															</li>
														)}
													</For>
												</ul>
											</div>
										)}
									</Match>
									<Match when={entry.kind === "settled" && entry}>
										{(ended) => (
											<Show
												when={ended().stopReason === "refusal"}
												fallback={<div class={s.settledNote}>{settlement(ended().stopReason)}</div>}
											>
												<div class={s.noticeCard} role="group" aria-label="Agent refusal">
													{activity().refusals.get(ended().id) || settlement("refusal")}
												</div>
											</Show>
										)}
									</Match>
									<Match when={entry.kind === "failed" && entry}>
										{(failed) => <div class={s.settledNote}>{failed().message}</div>}
									</Match>
								</Switch>
							</>
						)}
					</For>
				</Show>
				<Show
					when={props.retry?.()}
					fallback={
						<Show when={props.busy()}>
							<div class={cx(s.assistantMsg, s.thinkingPulse)}>…</div>
						</Show>
					}
				>
					{(retry) => (
						<div class={s.providerRetry} role="status">
							{retry()}
						</div>
					)}
				</Show>
				{props.children}
			</div>
			<Show when={newTextBelow() && transcriptPreferences.enabled()}>
				<Button type="button" class={s.newTextBadge} onClick={scrollToBottom}>
					<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
						<path d="M7 2h2v8.2l3.6-3.6L14 8l-6 6-6-6 1.4-1.4L7 10.2z" />
					</svg>
					New text below
				</Button>
			</Show>
		</div>
	);
};
