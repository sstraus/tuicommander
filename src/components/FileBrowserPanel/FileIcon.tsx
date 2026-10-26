import { type Component, createMemo } from "solid-js";
import { fileIconRegistry } from "../../plugins/fileIconRegistry";

/** Default monochrome folder icon */
const DEFAULT_FOLDER = `<svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor"><path d="M1 3.5A1.5 1.5 0 0 1 2.5 2h3.172a1.5 1.5 0 0 1 1.06.44l.658.658A.5.5 0 0 0 7.744 3.25H13.5A1.5 1.5 0 0 1 15 4.75v7.75a1.5 1.5 0 0 1-1.5 1.5h-11A1.5 1.5 0 0 1 1 12.5V3.5z"/></svg>`;

/** Default monochrome file icon */
const DEFAULT_FILE = `<svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor"><path d="M3.5 1A1.5 1.5 0 0 0 2 2.5v11A1.5 1.5 0 0 0 3.5 15h9a1.5 1.5 0 0 0 1.5-1.5V5.621a1.5 1.5 0 0 0-.44-1.06l-3.12-3.122A1.5 1.5 0 0 0 9.378 1H3.5zM10 4.5V2l3.5 3.5H11a1 1 0 0 1-1-1z"/></svg>`;

export interface FileIconProps {
	name: string;
	isDir: boolean;
	isLink?: boolean;
	class?: string;
}

/**
 * Icon nodes parsed once, keyed by SVG source. A directory listing mounts one
 * row per entry, and setting innerHTML per row re-parsed ~1.2 KB of markup every
 * time. Cloning a cached node skips the parse. Bounded by the icon theme's
 * distinct icons (~1k), and every row gets its own clone.
 */
const parsedIcons = new Map<string, Element>();

function iconNode(svg: string): Node {
	let parsed = parsedIcons.get(svg);
	if (!parsed) {
		const template = document.createElement("template");
		template.innerHTML = svg;
		// A provider may hand us markup with no element in it — render nothing
		// rather than throwing, matching the old innerHTML behaviour.
		parsed = template.content.firstElementChild ?? document.createElement("span");
		parsedIcons.set(svg, parsed);
	}
	return parsed.cloneNode(true);
}

/**
 * Renders a file/folder icon from the active FileIconProvider plugin,
 * or falls back to default monochrome SVG icons.
 *
 * Renders as a <span> holding a clone of the parsed SVG.
 * Pass a CSS class for sizing/alignment (e.g. the entryIcon module class).
 */
export const FileIcon: Component<FileIconProps> = (props) => {
	const icon = createMemo(() => {
		// Read version to track provider changes reactively
		fileIconRegistry.getVersion();
		return fileIconRegistry.resolve(props.name, props.isDir);
	});

	const svg = () =>
		props.isLink
			? '<svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor"><path d="M6.5 3a3.5 3.5 0 0 1 5-0.1l1.6 1.6a3.5 3.5 0 0 1 0 5l-2 2-1-1 2-2a2 2 0 0 0 0-3L10.5 4a2 2 0 0 0-3 0l-2 2-1-1 2-2zm3 10a3.5 3.5 0 0 1-5 0.1l-1.6-1.6a3.5 3.5 0 0 1 0-5l2-2 1 1-2 2a2 2 0 0 0 0 3L5.5 12a2 2 0 0 0 3 0l2-2 1 1-2 2zM5 10l5-5 1 1-5 5z"/></svg>'
			: (icon() ?? (props.isDir ? DEFAULT_FOLDER : DEFAULT_FILE));

	return <span class={props.class}>{iconNode(svg())}</span>;
};
