import type { JSX } from "solid-js";

export function onClickKeyDown(handler: () => void): JSX.EventHandlerUnion<HTMLElement, KeyboardEvent> {
	return (e: KeyboardEvent) => {
		if (e.key === "Enter" || e.key === " ") {
			e.preventDefault();
			handler();
		}
	};
}
