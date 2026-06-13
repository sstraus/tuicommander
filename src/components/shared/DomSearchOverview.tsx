import { type Accessor, type Component, createEffect, onCleanup } from "solid-js";

export interface DomSearchOverviewProps {
	/** The overflow scroll container the matched content lives in. */
	scrollEl: Accessor<HTMLElement | undefined>;
	/** Match-center fractions (0..1) of the scroll content height, from
	 * DomSearchEngine.matchFractions(). */
	fractions: Accessor<number[]>;
}

/**
 * Scrollbar overview ticks for DOM-based search (diff, markdown viewers). Mirrors
 * the terminal and CodeMirror editor: one orange tick (`--attention`) per match
 * on a thin strip down the right edge. Rendered as a child of the scroll container
 * and kept pinned to the visible viewport via a transform updated on scroll, so it
 * behaves like a minimap rather than scrolling away with the content.
 */
export const DomSearchOverview: Component<DomSearchOverviewProps> = (props) => {
	let strip: HTMLDivElement | undefined;

	const reposition = () => {
		const el = props.scrollEl();
		if (!el || !strip) return;
		strip.style.height = `${el.clientHeight}px`;
		strip.style.transform = `translateY(${el.scrollTop}px)`;
	};

	// Keep the strip pinned to the viewport as the container scrolls or resizes.
	createEffect(() => {
		const el = props.scrollEl();
		if (!el) return;
		reposition();
		el.addEventListener("scroll", reposition, { passive: true });
		const ro = new ResizeObserver(reposition);
		ro.observe(el);
		onCleanup(() => {
			el.removeEventListener("scroll", reposition);
			ro.disconnect();
		});
	});

	// Paint one tick per match fraction.
	createEffect(() => {
		const fr = props.fractions();
		if (!strip) return;
		strip.textContent = "";
		if (fr.length > 0) {
			const frag = document.createDocumentFragment();
			for (const f of fr) {
				const tick = document.createElement("div");
				// Full-width marks spanning the strip — matches the terminal scrollbar.
				tick.style.cssText =
					"position:absolute;right:0;width:100%;height:2px;background:var(--attention,#e8984c)";
				tick.style.top = `${f * 100}%`;
				frag.appendChild(tick);
			}
			strip.appendChild(frag);
		}
		reposition();
	});

	return (
		<div
			ref={strip}
			style={{
				position: "absolute",
				top: "0",
				right: "0",
				width: "14px",
				"pointer-events": "none",
				"z-index": "5",
			}}
		/>
	);
};
