import { type Accessor, createResource, createSignal, Show } from "solid-js";
import type { QueuedCommand } from "../../hooks/usePty";
import { appLogger } from "../../stores/appLogger";
import { Button } from "../ui/Button";
import s from "./Terminal.module.css";

interface ComposeToggleProps {
	shortcut: string;
	queuedCount: Accessor<number>;
	onOpen: () => void;
	onLoadQueue: () => Promise<QueuedCommand[]>;
}

export function ComposeToggle(props: ComposeToggleProps) {
	const [hovered, setHovered] = createSignal(false);
	const [focused, setFocused] = createSignal(false);
	const [queue] = createResource(
		() => (hovered() || focused()) && props.queuedCount() > 0 && props.queuedCount(),
		async () => {
			try {
				return await props.onLoadQueue();
			} catch (err) {
				appLogger.debug("terminal", "Failed to load compose queue details", err);
				return undefined;
			}
		},
	);
	const title = () => {
		const base = `Open compose editor (${props.shortcut})`;
		const count = props.queuedCount();
		if (!count) return base;
		const items = !queue.loading && (hovered() || focused()) ? queue() : undefined;
		if (!items) return `${base} — ${count} queued`;
		const commands = items.filter((item) => item.kind === "user_command").length;
		const messages = items.length - commands;
		const labels = [];
		if (commands) labels.push(`${commands} command${commands === 1 ? "" : "s"}`);
		if (messages) labels.push(`${messages} message${messages === 1 ? "" : "s"}`);
		return `${base} — ${labels.join(", ") || `${count} queued`}`;
	};
	return (
		<Button
			variant="hint"
			size="compact"
			class={s.composeHint}
			onClick={props.onOpen}
			onMouseEnter={() => setHovered(true)}
			onMouseLeave={() => setHovered(false)}
			onFocus={() => setFocused(true)}
			onBlur={() => setFocused(false)}
			title={title()}
		>
			<svg width="10" height="10" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
				<path d="M12.146.854a.5.5 0 0 1 .708 0l2.292 2.292a.5.5 0 0 1 0 .708L5.854 13.146a.5.5 0 0 1-.233.131l-3.5 1a.5.5 0 0 1-.617-.617l1-3.5a.5.5 0 0 1 .131-.233L12.146.854z" />
			</svg>
			Compose {props.shortcut}
			<Show when={props.queuedCount() > 0}>
				<span class={s.composeQueueCount}>{props.queuedCount()}</span>
			</Show>
		</Button>
	);
}
