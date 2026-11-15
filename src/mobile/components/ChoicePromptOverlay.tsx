import { For, Show } from "solid-js";
import { Button } from "../../components/ui";
import type { ChoicePrompt } from "../useSessions";
import styles from "./ChoicePromptOverlay.module.css";

interface ChoicePromptOverlayProps {
	prompt: ChoicePrompt;
	disabled?: boolean;
	embedded?: boolean;
	onSelect: (key: string) => void;
}

/** Overlay that surfaces an agent's choice dialog as tappable buttons. The
 *  caller follows the backend's selection mode when sending PTY keys. */
export function ChoicePromptOverlay(props: ChoicePromptOverlayProps) {
	return (
		<div class={styles.dropup} classList={{ [styles.embedded]: props.embedded }}>
			<Show when={props.prompt.header || props.prompt.question_count}>
				<div class={styles.context}>
					<Show when={props.prompt.header}>
						<span>{props.prompt.header}</span>
					</Show>
					<Show when={props.prompt.question_count}>
						<span class={styles.position}>
							{props.prompt.question_index}/{props.prompt.question_count}
						</span>
					</Show>
				</div>
			</Show>
			<div class={styles.title}>{props.prompt.title}</div>
			<For each={props.prompt.options}>
				{(option) => (
					<button
						type="button"
						data-control-layout="row"
						class={styles.item}
						disabled={props.disabled}
						classList={{
							[styles.itemHighlighted]: option.highlighted,
							[styles.itemDestructive]: option.destructive,
						}}
						onClick={() => props.onSelect(option.key)}
					>
						<span class={styles.key}>{option.key}</span>
						<span class={styles.label}>{option.label}</span>
						<Show when={option.hint}>
							<Show when={option.hint?.includes("\n")} fallback={<span class={styles.hint}>{option.hint}</span>}>
								<pre class={styles.preview}>{option.hint}</pre>
							</Show>
						</Show>
					</button>
				)}
			</For>
			<Show when={props.prompt.selection_mode === "toggle-enter"}>
				<Button variant="ghost" class={styles.submit} disabled={props.disabled} onClick={() => props.onSelect("\r")}>
					Submit answers
				</Button>
			</Show>
		</div>
	);
}
