import type { JSX } from "solid-js";

// `handler` may accept the triggering KeyboardEvent (e.g. to call
// `stopPropagation()`); callers that don't care can still pass `() => void`.
export function onClickKeyDown(handler: (e: KeyboardEvent) => void): JSX.EventHandlerUnion<HTMLElement, KeyboardEvent> {
	return (e: KeyboardEvent) => {
		if (e.key === "Enter" || e.key === " ") {
			e.preventDefault();
			handler(e);
		}
	};
}
