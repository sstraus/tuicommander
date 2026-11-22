import { type Component, createEffect, createSignal, type JSX, onCleanup, Show } from "solid-js";

interface RetainedPanelProps {
	visible: boolean;
	/** Work that would be lost on eviction (for example, an unsaved draft). */
	retain?: boolean;
	children: JSX.Element;
}

/** Keep a recently closed panel warm without keeping its data alive indefinitely. */
export const RetainedPanel: Component<RetainedPanelProps> = (props) => {
	const [mounted, setMounted] = createSignal(props.visible);
	createEffect(() => {
		if (props.visible) {
			setMounted(true);
			return;
		}
		if (!mounted() || props.retain) return;
		const timer = setTimeout(() => setMounted(false), 60_000);
		onCleanup(() => clearTimeout(timer));
	});
	return <Show when={props.visible || mounted()}>{props.children}</Show>;
};
