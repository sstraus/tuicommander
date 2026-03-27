import { Component, For, Show, createMemo } from "solid-js";
import { promptLibraryStore, type SavedPrompt, type SmartPlacement } from "../../stores/promptLibrary";
import { useSmartPrompts } from "../../hooks/useSmartPrompts";
import { appLogger } from "../../stores/appLogger";
import { cx } from "../../utils";
import s from "./SmartButtonStrip.module.css";

export interface SmartButtonStripProps {
  placement: SmartPlacement;
  repoPath: string;
  extraFilter?: (p: SavedPrompt) => boolean;
}

export const SmartButtonStrip: Component<SmartButtonStripProps> = (props) => {
  const { canExecute, executeSmartPrompt } = useSmartPrompts();

  const prompts = createMemo(() => {
    const all = promptLibraryStore.getSmartByPlacement(props.placement);
    return props.extraFilter ? all.filter(props.extraFilter) : all;
  });

  function handleClick(prompt: SavedPrompt) {
    const check = canExecute(prompt);
    if (!check.ok) return;
    executeSmartPrompt(prompt).catch((err) =>
      appLogger.error("prompts", `Failed to execute "${prompt.name}"`, err),
    );
  }

  return (
    <Show when={prompts().length > 0}>
      <div class={s.strip}>
        <For each={prompts()}>
          {(prompt) => {
            const check = createMemo(() => canExecute(prompt));
            return (
              <button
                class={cx(s.btn, !check().ok && s.btnDisabled)}
                disabled={!check().ok}
                title={!check().ok ? check().reason : prompt.description}
                onClick={() => handleClick(prompt)}
              >
                <Show when={prompt.icon}>
                  <span class={s.icon}>{prompt.icon}</span>
                </Show>
                {prompt.name}
              </button>
            );
          }}
        </For>
      </div>
    </Show>
  );
};
