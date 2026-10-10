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
import type { AcpToolCall, AcpToolCallContent } from "../../types/acp";
import { cx } from "../../utils";
import { writeClipboard } from "../../utils/clipboard";
import { handleOpenUrl } from "../../utils/openUrl";
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

const ToolActivity: Component<{ calls: () => AcpToolCall[]; observeDuration?: boolean }> = (props) => {
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
				<span class={s.toolCallCount}>
					{props.calls().length} tool {props.calls().length === 1 ? "call" : "calls"}
				</span>
				<span class={s.toolActivityTitles}>
					{props
						.calls()
						.slice(0, 2)
						.map((call) => toolName(call.title))
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

/** Keep the first call as the stable row anchor; later calls belong to it. */
function activityRows(
	entries: AcpTranscriptEntry[],
	showSuggestions: boolean,
): {
	visible: AcpTranscriptEntry[];
	calls: Map<string, AcpToolCall[]>;
	refusals: Map<string, string>;
	thoughts: Map<string, string>;
} {
	const visible: AcpTranscriptEntry[] = [];
	const calls = new Map<string, AcpToolCall[]>();
	const refusals = new Map<string, string>();
	const thoughts = new Map<string, string>();
	let current: AcpToolCall[] | undefined;
	for (const entry of entries) {
		if (entry.kind === "agent") {
			const projected = projectChatProtocolText(entry.text);
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
		if (entry.kind === "tool") {
			if (!current) {
				current = [];
				calls.set(entry.id, current);
				visible.push(entry);
			}
			current.push(entry.call);
		} else {
			visible.push(entry);
		}
	}
	return { visible, calls, refusals, thoughts };
}

export interface TranscriptSearchRef {
	open: () => void;
	close: () => void;
}

export interface TranscriptProps {
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
	onOpenFile?: (href: string) => void;
	onClear?: () => void;
	canForkAtMessage?: () => boolean;
	onFork?: (messageId: string) => void;
	/** `open_result` opens its file. `answer` and `approve` have nothing to open: the open interaction is drawn at the end, and the transcript scrolls there. */
	onNoticeAction?: (action: AcpNoticeAction) => void;
	/** Absent for a read-only transcript: suggested replies are then not drawn. */
	onSuggestion?: (text: string) => void;
	/** Open questions, drawn at the end of the conversation they belong to. */
	children?: JSX.Element;
}

const CopyButton: Component<{ label: string; text: string }> = (props) => (
	<button
		type="button"
		class={s.copyAction}
		aria-label={props.label}
		onClick={() => void writeClipboard(props.text).catch((error) => appLogger.error("ai-chat", "Copy failed", error))}
	>
		Copy
	</button>
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

/** Reuse the terminal's answer marker and tint, after Markdown has rendered.
 * An answer spans the blocks the grid highlights (`answerBlockRanges`); the 💬 glyph is
 * dropped from its first paragraph, which keeps `data-tuic-answer-start` so a later pass
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
	for (const element of elements) element.removeAttribute("data-tuic-answer");
	for (const [first, last] of answerBlockRanges(blocks)) {
		for (let index = first; index <= last; index++)
			elements[index].setAttribute("data-tuic-answer", index === last ? "end" : "");
		const head = elements[first];
		if (head.hasAttribute("data-tuic-answer-start")) continue;
		head.setAttribute("data-tuic-answer-start", "");
		let remaining = /^[\s●⏺]*💬[\t ]*/.exec(head.textContent ?? "")?.[0].length ?? 0;
		const walker = document.createTreeWalker(head, NodeFilter.SHOW_TEXT);
		while (remaining > 0 && walker.nextNode()) {
			const node = walker.currentNode;
			const text = node.textContent ?? "";
			node.textContent = text.slice(remaining);
			remaining -= Math.min(remaining, text.length);
		}
	}
}

export const Transcript: Component<TranscriptProps> = (props) => {
	const activity = createMemo(() => activityRows(props.entries(), !!props.onSuggestion));
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
	const onScroll = () => {
		if (!container) return;
		stickToBottom = container.scrollHeight - container.clientHeight - container.scrollTop <= 24;
	};
	createEffect(() => {
		props.entries();
		props.busy();
		props.retry?.();
		queueMicrotask(() => {
			if (container && stickToBottom) container.scrollTop = container.scrollHeight;
		});
	});
	const runNoticeAction = (action: AcpNoticeAction) => {
		if (action.kind !== "open_result" && container) {
			stickToBottom = true;
			container.scrollTop = container.scrollHeight;
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
		clearSearchSelection();
		setFinding(false);
		matchIndex = -1;
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
			if (hidden || parent?.closest("button, input, ." + s.findBar)) {
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
		<div
			class={cx(s.messageList, props.mobile && s.mobileTranscript)}
			ref={container}
			aria-label="Chat transcript"
			tabIndex={0}
			onKeyDown={onKeyDown}
			onScroll={onScroll}
		>
			<Show when={finding()}>
				<div class={s.findBar}>
					<input
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
					<button type="button" onClick={() => findNext(-1)}>
						Previous
					</button>
					<button type="button" onClick={() => findNext()}>
						Next
					</button>
					<button type="button" aria-label="Close chat search" onClick={closeSearch}>
						<svg width="12" height="12" viewBox="0 0 12 12" fill="currentColor" aria-hidden="true">
							<path d="M2.8 2l3.2 3.2L9.2 2l.8.8L6.8 6l3.2 3.2-.8.8L6 6.8 2.8 10l-.8-.8L5.2 6 2 2.8z" />
						</svg>
					</button>
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
												<div class={s.systemNotice} role="note">
													{user().text}
												</div>
											}
										>
											<div class={s.userMsg}>
												<UserText text={user().text} onOpenFile={props.onOpenFile} />
												<CopyButton label="Copy user message" text={user().text} />
											</div>
										</Show>
									)}
								</Match>
								<Match when={entry.kind === "agent" && entry}>
									{(agent) => {
										const projected = createMemo(() => projectChatProtocolText(agent().text));
										let content: HTMLDivElement | undefined;
										createEffect(() => {
											projected().body;
											queueMicrotask(() => highlightAnswers(content));
										});
										return (
											<Show
												when={
													projected().body.trim() ||
													projected().intent ||
													(props.onSuggestion && projected().suggestions.length > 0)
												}
											>
												<div class={s.assistantMsg}>
													<Show when={projected().intent}>
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
														<CopyButton label="Copy assistant message" text={agent().text} />
														<Show when={props.canForkAtMessage?.() && agent().messageId}>
															<button
																type="button"
																class={s.copyAction}
																aria-label="Fork from here"
																disabled={props.busy()}
																onClick={() => {
																	const id = agent().messageId;
																	if (id) props.onFork?.(id);
																}}
															>
																Fork from here
															</button>
														</Show>
													</div>
													<Show when={props.onSuggestion && projected().suggestions.length > 0}>
														<div class={s.suggestedReplies} aria-label="Suggested replies">
															<For each={projected().suggestions}>
																{(item) => (
																	<button type="button" onClick={() => props.onSuggestion?.(item)}>
																		{item}
																	</button>
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
											<div>{notice().text}</div>
											<Show when={notice().action}>
												{(action) => (
													<button type="button" class={s.noticeAction} onClick={() => runNoticeAction(action())}>
														{NOTICE_LABELS[action().kind].button}
													</button>
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
											observeDuration={props.observeToolDuration}
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
	);
};
