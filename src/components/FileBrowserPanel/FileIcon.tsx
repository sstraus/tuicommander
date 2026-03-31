import { Component, createMemo } from "solid-js";
import { fileIconRegistry } from "../../plugins/fileIconRegistry";

/** Default monochrome folder icon */
const DEFAULT_FOLDER = `<svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor"><path d="M1 3.5A1.5 1.5 0 0 1 2.5 2h3.172a1.5 1.5 0 0 1 1.06.44l.658.658A.5.5 0 0 0 7.744 3.25H13.5A1.5 1.5 0 0 1 15 4.75v7.75a1.5 1.5 0 0 1-1.5 1.5h-11A1.5 1.5 0 0 1 1 12.5V3.5z"/></svg>`;

/** Default monochrome file icon */
const DEFAULT_FILE = `<svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor"><path d="M3.5 1A1.5 1.5 0 0 0 2 2.5v11A1.5 1.5 0 0 0 3.5 15h9a1.5 1.5 0 0 0 1.5-1.5V5.621a1.5 1.5 0 0 0-.44-1.06l-3.12-3.122A1.5 1.5 0 0 0 9.378 1H3.5zM10 4.5V2l3.5 3.5H11a1 1 0 0 1-1-1z"/></svg>`;

export interface FileIconProps {
  name: string;
  isDir: boolean;
  class?: string;
}

/**
 * Renders a file/folder icon from the active FileIconProvider plugin,
 * or falls back to default monochrome SVG icons.
 *
 * Renders as a <span> with innerHTML set to the SVG string.
 * Pass a CSS class for sizing/alignment (e.g. the entryIcon module class).
 */
export const FileIcon: Component<FileIconProps> = (props) => {
  const icon = createMemo(() => {
    // Read version to track provider changes reactively
    fileIconRegistry.getVersion();
    return fileIconRegistry.resolve(props.name, props.isDir);
  });

  const svg = () => icon() ?? (props.isDir ? DEFAULT_FOLDER : DEFAULT_FILE);

  return <span class={props.class} innerHTML={svg()} />;
};
