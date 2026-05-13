import type { Component } from "solid-js";
import s from "./ZoomIndicator.module.css";

export interface ZoomIndicatorProps {
	fontSize: number;
	defaultFontSize: number;
}

export const ZoomIndicator: Component<ZoomIndicatorProps> = (props) => {
	const percentage = () => Math.round((props.fontSize / props.defaultFontSize) * 100);

	return <span class={s.indicator} data-testid="zoom-indicator">{percentage()}%</span>;
};
