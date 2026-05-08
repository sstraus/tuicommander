import { For } from "solid-js";
import { appLogger } from "../../stores/appLogger";
import { rpc } from "../../transport";
import type { SlashMenuItem } from "../useSessions";
import styles from "./SlashMenuOverlay.module.css";

interface SlashMenuOverlayProps {
	items: SlashMenuItem[];
	sessionId: string;
	onSelect: (command: string) => void;
}

/** Compact dropup that renders above the input area. Items come pre-filtered
 *  from the backend (Claude Code's own slash menu filtering). */
export function SlashMenuOverlay(props: SlashMenuOverlayProps) {
	function navigate(direction: "up" | "down") {
		const key = direction === "up" ? "\x1b[A" : "\x1b[B";
		rpc("write_pty", { sessionId: props.sessionId, data: key }).catch((err: unknown) => {
			appLogger.warn("network", "SlashMenu navigate failed", { error: err });
		});
	}

	return (
		<div class={styles.dropup} onTouchMove={(e) => e.stopPropagation()}>
			<For each={props.items}>
				{(item) => (
					<button
						class={styles.item}
						classList={{ [styles.itemHighlighted]: item.highlighted }}
						onClick={() => props.onSelect(item.command)}
					>
						<span class={styles.command}>{item.command}</span>
						<span class={styles.description}>{item.description}</span>
					</button>
				)}
			</For>
			<div class={styles.nav}>
				<button class={styles.navBtn} onClick={() => navigate("up")} aria-label="Previous">
					<svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor">
						<path d="M7.41 15.41L12 10.83l4.59 4.58L18 14l-6-6-6 6z" />
					</svg>
				</button>
				<button class={styles.navBtn} onClick={() => navigate("down")} aria-label="Next">
					<svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor">
						<path d="M7.41 8.59L12 13.17l4.59-4.58L18 10l-6 6-6-6z" />
					</svg>
				</button>
			</div>
		</div>
	);
}
