import { Component } from "solid-js";
import { cx } from "../../utils";
import s from "../PromptOverlay/PromptOverlay.module.css";

export interface PromptOptionProps {
  index: number;
  label: string;
  selected: boolean;
  onClick: () => void;
}

export const PromptOption: Component<PromptOptionProps> = (props) => {
  return (
    <div
      class={cx(s.option, props.selected && s.selected)}
      onClick={props.onClick}
    >
      <span class={s.optionKey}>{props.index + 1}</span>
      <span class={s.optionText}>{props.label}</span>
    </div>
  );
};
