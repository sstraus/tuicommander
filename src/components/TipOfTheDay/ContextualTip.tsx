import { createSignal, onCleanup, onMount, Show } from "solid-js";
import { CONTEXTUAL_TIPS } from "../../data/tips";
import { Button } from "../ui";
import styles from "./TipOfTheDay.module.css";

type TipId = keyof typeof CONTEXTUAL_TIPS;
const prefix = "tuic.dismissed-tip.";
const dismissEvent = "tuic-tip-dismissed";
const memory = new Set<TipId>();

function dismissed(id: TipId): boolean {
	try {
		return localStorage.getItem(prefix + id) === "true" || memory.has(id);
	} catch {
		return memory.has(id);
	}
}

/** Contextual entries share the tip catalogue and card, with per-client permanent dismissal. */
export function ContextualTip(props: { id: TipId; when: boolean }) {
	const [hidden, setHidden] = createSignal(dismissed(props.id));
	onMount(() => {
		const sync = () => setHidden(dismissed(props.id));
		window.addEventListener("storage", sync);
		window.addEventListener(dismissEvent, sync);
		onCleanup(() => {
			window.removeEventListener("storage", sync);
			window.removeEventListener(dismissEvent, sync);
		});
	});
	const dismiss = () => {
		try {
			localStorage.setItem(prefix + props.id, "true");
		} catch {
			// Storage can be unavailable; retain dismissal for this document's lifetime.
			memory.add(props.id);
		}
		setHidden(true);
		window.dispatchEvent(new Event(dismissEvent));
	};
	return (
		<Show when={props.when && !hidden()}>
			<aside class={styles.contextual} aria-label={CONTEXTUAL_TIPS[props.id].feature}>
				<div class={styles.content}>
					<strong class={styles.feature}>{CONTEXTUAL_TIPS[props.id].feature}</strong>
					<span class={styles.description}>{CONTEXTUAL_TIPS[props.id].description}</span>
				</div>
				<Button variant="ghost" onClick={dismiss} aria-label={`Dismiss ${CONTEXTUAL_TIPS[props.id].feature}`}>
					Dismiss
				</Button>
			</aside>
		</Show>
	);
}
