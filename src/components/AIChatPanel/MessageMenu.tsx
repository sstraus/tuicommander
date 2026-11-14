import { type Component, createSignal, For, Show } from "solid-js";
import type { MessageActionReaction } from "../../types/messageActions";
import { Button } from "../ui/Button";
import { Dropdown, type DropdownItem } from "../ui/Dropdown";
import { Textarea } from "../ui/Textarea";
import s from "./AIChatPanel.module.css";

/** A reply quote: every line of the excerpt prefixed, then a blank line for the user's text. */
export function quoteMessage(text: string): string {
	return `${text
		.trim()
		.split("\n")
		.map((line) => (line ? `> ${line}` : ">"))
		.join("\n")}\n\n`;
}

// Terminal Chat retains its explicit draft feedback; AI Chat uses conversational message actions.
export type Reaction = "approve" | "disapprove";

/**
 * Response feedback as ordinary user text: the quote, the verdict, and an explicit statement that it
 * is not permission to run anything. It is drafted for the user to send, never sent by the menu.
 */
export function feedbackMessage(text: string, reaction: Reaction): string {
	const verdict = reaction === "approve" ? "I approve of this response." : "I disapprove of this response.";
	return `Feedback on your earlier response:\n${quoteMessage(text).trimEnd()}\n\n${verdict}\nThis is feedback on the response, not approval to execute an action.\n\n`;
}

export const REACTIONS: { id: MessageActionReaction; label: string; path: string }[] = [
	{
		id: "thumbs_up",
		label: "Thumbs up",
		path: "M7 10H3v11h4V10Zm2 11h8a2 2 0 0 0 2-2l2-7a2 2 0 0 0-2-2h-5l1-5c0-2-2-3-3-1l-3 6v11Z",
	},
	{
		id: "thumbs_down",
		label: "Thumbs down",
		path: "M7 3H3v11h4V3Zm2 0h8a2 2 0 0 1 2 2l2 7a2 2 0 0 1-2 2h-5l1 5c0 2-2 3-3 1l-3-6V3Z",
	},
	{ id: "heart", label: "Heart", path: "M12 21 3 12C-3 5 6-2 12 5c6-7 15 0 9 7l-9 9Z" },
	{
		id: "celebrate",
		label: "Celebrate",
		path: "m3 21 4-14 10 10L3 21Zm11-18h2v5h-2V3Zm5 5 3-3 1 2-3 3-1-2ZM9 1h2v4H9V1Zm10 12h5v2h-5v-2Z",
	},
];

/** Room the open menu needs above its trigger (5 rows plus padding), in px. */
const MENU_HEIGHT = 220;

/** Quiet per-message actions: one trigger, a keyboard-complete menu. */
export const MessageMenu: Component<{
	/** "user" or "assistant": names the trigger. */
	role: "user" | "assistant";
	onCopy: () => void;
	/** Absent when the host has no composer to quote into. */
	onReply?: () => void;
	/** Absent unless the host has a composer and the message is a finished answer. */
	onReact?: (reaction: Reaction) => void;
	/** Absent for messages with no Markdown source to reveal. */
	actions?: {
		reactions: string[];
		disabled?: boolean;
		onReact: (reaction: MessageActionReaction, remove: boolean) => Promise<void>;
		onComment: (comment: string) => Promise<void>;
	};
	raw?: { shown: boolean; onToggle: () => void };
}> = (props) => {
	const [open, setOpen] = createSignal(false);
	// The list is ~5 rows tall: near the top of the viewport it opens downward instead of being clipped.
	const [below, setBelow] = createSignal(false);
	const [commenting, setCommenting] = createSignal(false);
	const [comment, setComment] = createSignal("");
	const [pending, setPending] = createSignal(false);
	const [error, setError] = createSignal("");
	const perform = async (send: () => Promise<void>) => {
		if (pending()) return;
		setPending(true);
		setError("");
		try {
			await send();
			setCommenting(false);
			setComment("");
		} catch (failure) {
			// Match ACP describe(): desktop commands reject with a serialized message-bearing object.
			setError(String(failure && typeof failure === "object" && "message" in failure ? failure.message : failure));
		} finally {
			setPending(false);
		}
	};
	let commentInput: HTMLTextAreaElement | undefined;
	let trigger: HTMLButtonElement | undefined;
	let root: HTMLSpanElement | undefined;
	const close = () => {
		setOpen(false);
		// Return focus only from inside the menu (select, Escape); an outside click already placed it.
		const active = document.activeElement;
		if (
			!active ||
			active === document.body ||
			root?.contains(active) ||
			active.closest('[role="menu"][aria-label="Message actions"]')
		)
			trigger?.focus();
	};
	const items = (): DropdownItem[] => [
		{ id: "copy", label: "Copy message" },
		...(props.raw ? [{ id: "raw", label: props.raw.shown ? "Show formatted message" : "Show raw Markdown" }] : []),
		...(props.onReply ? [{ id: "reply", label: "Quote reply" }] : []),
		...(props.actions
			? [
					...REACTIONS.map((reaction) => ({
						id: reaction.id,
						label: `${props.actions?.reactions.includes(reaction.id) ? "Remove " : ""}${reaction.label}`,
						icon: (
							<svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="currentColor">
								<path d={reaction.path} />
							</svg>
						),
						disabled: pending() || props.actions?.disabled,
					})),
					{ id: "comment", label: "Comment on message", disabled: pending() || props.actions?.disabled },
				]
			: []),
		...(props.onReact
			? [
					{ id: "approve", label: "Approve response" },
					{ id: "disapprove", label: "Disapprove response" },
				]
			: []),
	];
	const select = (id: string) => {
		const reaction = REACTIONS.find((item) => item.id === id);
		if (reaction && props.actions) {
			const actions = props.actions;
			const remove = actions.reactions.includes(reaction.id);
			void perform(() => actions.onReact(reaction.id, remove));
		} else if (id === "comment") {
			setCommenting(true);
			queueMicrotask(() => commentInput?.focus());
		} else if (id === "copy") props.onCopy();
		else if (id === "raw") props.raw?.onToggle();
		else if (id === "reply") props.onReply?.();
		else if (id === "approve" || id === "disapprove") props.onReact?.(id);
		close();
	};
	return (
		<span ref={root} class={s.messageMenu} data-open={open() || undefined}>
			<Button
				ref={trigger}
				type="button"
				variant="ghost"
				size="compact"
				shape="icon"
				class={s.copyAction}
				aria-label={`${props.role === "user" ? "User" : "Assistant"} message actions`}
				aria-haspopup="menu"
				aria-expanded={open()}
				onClick={() => {
					if (!open()) setBelow((trigger?.getBoundingClientRect().top ?? Infinity) < MENU_HEIGHT);
					setOpen(!open());
				}}
			>
				<svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="currentColor">
					<circle cx="5" cy="12" r="2" />
					<circle cx="12" cy="12" r="2" />
					<circle cx="19" cy="12" r="2" />
				</svg>
			</Button>
			<Dropdown
				aria-label="Message actions"
				anchor={() => trigger?.getBoundingClientRect()}
				items={items()}
				visible={open()}
				class={s.messageMenuList}
				position={below() ? "bottom" : "top"}
				onSelect={select}
				onClose={close}
			/>
			<For each={REACTIONS.filter((reaction) => props.actions?.reactions.includes(reaction.id))}>
				{(reaction) => (
					<Button
						type="button"
						variant="ghost"
						size="compact"
						disabled={pending() || props.actions?.disabled}
						aria-pressed="true"
						aria-label={`Remove ${reaction.label} reaction`}
						title={`${reaction.label} — sent to conversation. Click to remove.`}
						onClick={() => {
							const actions = props.actions;
							if (actions) void perform(() => actions.onReact(reaction.id, true));
						}}
					>
						<svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="currentColor">
							<path d={reaction.path} />
						</svg>
					</Button>
				)}
			</For>
			<Show when={commenting()}>
				<div class={s.messageComment} role="region" aria-label="Comment on message">
					<Textarea
						ref={(node) => {
							commentInput = node;
						}}
						aria-label="Comment on this message"
						value={comment()}
						onInput={(event) => setComment(event.currentTarget.value)}
						autofocus
					/>
					<small>Sent to this conversation when the agent is ready. Not an evaluation or authorization.</small>
					<Button
						type="button"
						size="compact"
						disabled={pending()}
						onClick={() => {
							setCommenting(false);
							trigger?.focus();
						}}
					>
						Cancel
					</Button>
					<Button
						type="button"
						size="compact"
						disabled={pending() || props.actions?.disabled || !comment().trim()}
						onClick={() => {
							const actions = props.actions;
							if (actions) void perform(() => actions.onComment(comment()));
						}}
					>
						Send comment
					</Button>
				</div>
			</Show>
			<Show when={pending()}>
				<span role="status">Sending…</span>
			</Show>
			<Show when={error()}>
				<span role="alert">{error()}</span>
			</Show>
		</span>
	);
};
