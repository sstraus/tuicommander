import { Component } from "solid-js";

export interface PromptOptionProps {
  index: number;
  label: string;
  selected: boolean;
  onClick: () => void;
}

export const PromptOption: Component<PromptOptionProps> = (props) => {
  return (
    <div
      class={`prompt-option ${props.selected ? "selected" : ""}`}
      onClick={props.onClick}
    >
      <span class="prompt-option-key">{props.index + 1}</span>
      <span class="prompt-option-text">{props.label}</span>
    </div>
  );
};
