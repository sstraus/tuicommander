import { Component, Show } from "solid-js";
import { isTauri } from "../../transport";
import { detachPanel, reattachPanel, closePanel } from "../../panelRouter";

export const IconDetach = () => (
  <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.3">
    <path d="M8 2h4v4M8 6l4-4M6 3H3a1 1 0 00-1 1v7a1 1 0 001 1h7a1 1 0 001-1V8" stroke-linecap="round" stroke-linejoin="round" />
  </svg>
);

export const IconReattach = () => (
  <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.3">
    <path d="M6 12H3a1 1 0 01-1-1V4a1 1 0 011-1h7a1 1 0 011 1v3M10 8l-4 4M10 12V8H6" stroke-linecap="round" stroke-linejoin="round" />
  </svg>
);

interface PanelWindowControlsProps {
  panelId: string;
  mode: "inline" | "detached";
  onInlineClose?: () => void;
}

export const PanelWindowControls: Component<PanelWindowControlsProps> = (props) => {
  return (
    <>
      <Show when={props.mode === "inline" && isTauri()}>
        <button onClick={() => detachPanel(props.panelId)} title="Open in separate window">
          <IconDetach />
        </button>
      </Show>
      <Show when={props.mode === "detached"}>
        <button onClick={() => reattachPanel(props.panelId)} title="Bring back to main window">
          <IconReattach />
        </button>
      </Show>
      <button
        onClick={() => props.mode === "detached" ? closePanel(props.panelId) : props.onInlineClose?.()}
        title="Close"
      >
        &times;
      </button>
    </>
  );
};
