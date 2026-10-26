import { Show } from "solid-js";
import { Button } from "../../components/ui";
import styles from "./MobileJumpBottom.module.css";

/** Mount inside the positioned output area, never inside the controls. */
export function MobileJumpBottom(props: { visible: boolean; onScrollToBottom: () => void }) {
	return (
		<Show when={props.visible}>
			<Button
				shape="icon"
				type="button"
				class={styles.jumpBottom}
				aria-label="Scroll to bottom"
				onMouseDown={(event) => event.preventDefault()}
				onClick={() => props.onScrollToBottom()}
			>
				<svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
					<path d="M11 4h2v12.17l4.59-4.58L19 13l-7 7-7-7 1.41-1.41L11 16.17z" />
				</svg>
			</Button>
		</Show>
	);
}
