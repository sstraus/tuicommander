import { createMemo, For, Show } from "solid-js";
import { Button } from "../../components/ui";
import { appLogger } from "../../stores/appLogger";
import { rpc } from "../../transport";
import type { SlashMenuItem } from "../useSessions";
import styles from "./SlashMenuOverlay.module.css";

interface SlashMenuOverlayProps {
	items: SlashMenuItem[];
	sessionId: string;
	onSelect: (command: string) => void;
	onClose?: () => void;
}

/** Compact dropup that renders above the input area. Items come pre-filtered
 *  from the backend (Claude Code's own slash menu filtering). */
export function SlashMenuOverlay(props: SlashMenuOverlayProps) {
	const availableItems = createMemo(() =>
		props.items.filter((item) => !/\(removed\)/i.test(`${item.command} ${item.description}`)),
	);
	function navigate(direction: "up" | "down") {
		const key = direction === "up" ? "\x1b[A" : "\x1b[B";
		rpc("write_pty", { sessionId: props.sessionId, data: key }).catch((err: unknown) => {
			appLogger.warn("network", "SlashMenu navigate failed", { error: err });
		});
	}

	return (
		<div class={styles.dropup} onTouchMove={(e) => e.stopPropagation()}>
			<div class={styles.header}>
				<span>Commands</span>
				<Button shape="icon" variant="ghost" aria-label="Close slash menu" onClick={() => props.onClose?.()}>
					<svg
						width="18"
						height="18"
						viewBox="0 0 24 24"
						fill="none"
						stroke="currentColor"
						stroke-width="2"
						aria-hidden="true"
					>
						<path d="M18 6 6 18M6 6l12 12" />
					</svg>
				</Button>
			</div>
			<For each={availableItems()}>
				{(item) => (
					<button
						data-control-layout="row"
						class={styles.item}
						classList={{ [styles.itemHighlighted]: item.highlighted }}
						onClick={() => props.onSelect(item.command)}
					>
						<span class={styles.command}>{item.command}</span>
						<Show when={item.description}>
							<span class={styles.description}>{item.description}</span>
						</Show>
					</button>
				)}
			</For>
			<div class={styles.nav}>
				<Button shape="icon" onClick={() => navigate("up")} aria-label="Previous">
					<svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor">
						<path d="M7.41 15.41L12 10.83l4.59 4.58L18 14l-6-6-6 6z" />
					</svg>
				</Button>
				<Button shape="icon" onClick={() => navigate("down")} aria-label="Next">
					<svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor">
						<path d="M7.41 8.59L12 13.17l4.59-4.58L18 10l-6 6-6-6z" />
					</svg>
				</Button>
			</div>
		</div>
	);
}
