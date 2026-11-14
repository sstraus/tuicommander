import { type Component, Show } from "solid-js";
import { Button } from "../ui/Button";

const MARKER_RE = /… \[truncated, (\d+) chars total\]/;

/** The backend cut this message at its cap and said so; offers to read all of it. */
export const TruncatedTextAction: Component<{
	text: string;
	messageId?: string;
	onShow?: (messageId: string) => void;
}> = (props) => {
	const total = () => MARKER_RE.exec(props.text)?.[1];
	return (
		<Show when={props.onShow && props.messageId && total()}>
			<Button
				type="button"
				variant="ghost"
				size="compact"
				aria-label="Show full text"
				onClick={() => props.onShow?.(props.messageId as string)}
			>
				Show full text ({total()} chars)
			</Button>
		</Show>
	);
};
