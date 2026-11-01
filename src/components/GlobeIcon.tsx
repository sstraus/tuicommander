import type { Component } from "solid-js";

/** Shared globe: circular rim, symmetric meridian and equator in the text colour. */
export const GlobeIcon: Component<{ size?: number }> = (props) => (
	<svg viewBox="0 0 16 16" width={props.size ?? 14} height={props.size ?? 14} fill="currentColor">
		<path fill-rule="evenodd" d="M8 0a8 8 0 1 0 0 16A8 8 0 0 0 8 0Zm0 1.5a6.5 6.5 0 1 1 0 13 6.5 6.5 0 0 1 0-13Z" />
		<path fill-rule="evenodd" d="M8 0a3.5 8 0 1 0 0 16A3.5 8 0 0 0 8 0Zm0 1.5a2 6.5 0 1 1 0 13 2 6.5 0 0 1 0-13Z" />
		<path d="M.75 7.25h14.5v1.5H.75Z" />
	</svg>
);
