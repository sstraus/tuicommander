import { createEffect, createMemo, createSignal, For, on, Show } from "solid-js";
import { Button } from "../../components/ui";
import { appLogger } from "../../stores/appLogger";
import { toastsStore } from "../../stores/toasts";
import { rpc } from "../../transport";
import { sendCommand } from "../../utils/sendCommand";
import { retryWrite } from "../utils/retryWrite";
import styles from "./QuickActions.module.css";

interface SuggestChipsProps {
	sessionId: string;
	items: string[];
	agentType?: string | null;
}

/** Strip leading "N) " or "N. " numbering from suggest items. */
const stripNumberPrefix = (text: string): string => text.replace(/^\d+[).]\s*/, "");

export function SuggestChips(props: SuggestChipsProps) {
	const listKey = createMemo(() => JSON.stringify([props.sessionId, props.items]));
	const [picked, setPicked] = createSignal(false);
	// Only removal/replacement of the backend list re-arms it. Unrelated parsed
	// activity can change last_activity_ms while retaining these same choices.
	createEffect(on(listKey, () => setPicked(false)));
	const send = async (text: string) => {
		// Match desktop: consume the whole list before the first asynchronous write.
		// The shared submitted-line lifecycle clears backend suggestions on Enter;
		// keep an unchanged poll from restoring them while that submission is pending.
		setPicked(true);
		const sessionId = props.sessionId;
		const agentType = props.agentType;
		try {
			await sendCommand(
				(data) => retryWrite(() => rpc("write_pty", { sessionId, data })),
				text,
				agentType,
				undefined,
				true,
				sessionId,
			);
		} catch (err) {
			const msg = err instanceof Error ? err.message : String(err);
			appLogger.warn("network", `Failed to send suggest action: ${msg}`);
			toastsStore.add("Send failed", `Could not send command: ${msg}`, "error", true);
		}
	};

	return (
		<Show when={!picked()}>
			<div class={styles.row}>
				<For each={props.items}>
					{(item) => (
						<Button class={styles.chip} onClick={() => send(item)}>
							{stripNumberPrefix(item)}
						</Button>
					)}
				</For>
			</div>
		</Show>
	);
}
