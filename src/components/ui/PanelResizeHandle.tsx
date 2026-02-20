import { Component } from "solid-js";

export interface PanelResizeHandleProps {
  /** CSS selector of the panel element to resize */
  panelId: string;
  /** Minimum width in px (default 200) */
  minWidth?: number;
  /** Maximum width in px (default 800) */
  maxWidth?: number;
}

/**
 * Drag handle for right-side panels. Placed inside the panel,
 * positioned on the left edge. Dragging left = wider, right = narrower.
 */
export const PanelResizeHandle: Component<PanelResizeHandleProps> = (props) => {
  const minW = () => props.minWidth ?? 200;
  const maxW = () => props.maxWidth ?? 800;

  const handleMouseDown = (e: MouseEvent) => {
    e.preventDefault();
    const panel = document.getElementById(props.panelId);
    if (!panel) return;

    const startX = e.clientX;
    const startWidth = panel.getBoundingClientRect().width;

    // Disable transitions for smooth resize
    panel.style.transition = "none";

    const onMove = (ev: MouseEvent) => {
      // Panel is on the right: moving mouse left (negative delta) = wider
      const delta = ev.clientX - startX;
      const newWidth = Math.min(maxW(), Math.max(minW(), startWidth - delta));
      panel.style.width = `${newWidth}px`;
    };

    const onUp = () => {
      document.removeEventListener("mousemove", onMove);
      document.removeEventListener("mouseup", onUp);
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
      panel.style.transition = "";
    };

    document.addEventListener("mousemove", onMove);
    document.addEventListener("mouseup", onUp);
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";
  };

  return <div class="panel-resize-handle" onMouseDown={handleMouseDown} />;
};
