import { Component } from "solid-js";

export interface ZoomIndicatorProps {
  fontSize: number;
  defaultFontSize: number;
}

export const ZoomIndicator: Component<ZoomIndicatorProps> = (props) => {
  const percentage = () => Math.round((props.fontSize / props.defaultFontSize) * 100);

  return <span id="zoom-indicator">{percentage()}%</span>;
};
