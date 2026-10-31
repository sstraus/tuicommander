import { createEffect, createSignal, onCleanup } from "solid-js";
import { ContextualTip } from "./ContextualTip";

/** One timeout per busy cycle, cancelled when the run ends or the setting is enabled. */
export function LongRunTip(props: { eligible: boolean; busyDuration: number }) {
	const [longRun, setLongRun] = createSignal(false);
	createEffect(() => {
		setLongRun(false);
		if (!props.eligible) return;
		const remaining = Math.max(0, 60_000 - props.busyDuration);
		const timer = window.setTimeout(() => setLongRun(true), remaining);
		onCleanup(() => window.clearTimeout(timer));
	});
	return <ContextualTip id="preventSleep" when={props.eligible && longRun()} />;
}
