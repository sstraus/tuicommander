import { Component, For, Show, createSignal, createEffect, onCleanup } from "solid-js";
import { promptStore } from "../../stores/prompt";
import { usePty } from "../../hooks/usePty";
import { t } from "../../i18n";
import { cx } from "../../utils";
import s from "./PromptOverlay.module.css";

export interface PromptOverlayProps {
  onDismiss?: () => void;
}

export const PromptOverlay: Component<PromptOverlayProps> = (props) => {
  const [selectedIndex, setSelectedIndex] = createSignal(0);
  const pty = usePty();

  const prompt = () => promptStore.state.activePrompt;
  const isVisible = () => prompt() !== null;

  // Reset selection when prompt changes
  createEffect(() => {
    if (prompt()) {
      setSelectedIndex(0);
    }
  });

  // Handle keyboard events
  createEffect(() => {
    if (!isVisible()) return;

    const handleKeydown = (e: KeyboardEvent) => {
      const currentPrompt = prompt();
      if (!currentPrompt) return;

      // Number keys 1-9
      if (e.key >= "1" && e.key <= "9") {
        e.preventDefault();
        const index = parseInt(e.key) - 1;
        if (index < currentPrompt.options.length) {
          selectAndConfirm(index);
        }
        return;
      }

      switch (e.key) {
        case "ArrowUp":
          e.preventDefault();
          setSelectedIndex((i) => Math.max(0, i - 1));
          break;
        case "ArrowDown":
          e.preventDefault();
          setSelectedIndex((i) =>
            Math.min(currentPrompt.options.length - 1, i + 1)
          );
          break;
        case "Enter":
          e.preventDefault();
          confirm();
          break;
        case "Escape":
          e.preventDefault();
          dismiss();
          break;
      }
    };

    document.addEventListener("keydown", handleKeydown);
    onCleanup(() => document.removeEventListener("keydown", handleKeydown));
  });

  const selectAndConfirm = async (index: number) => {
    setSelectedIndex(index);
    await confirm();
  };

  const confirm = async () => {
    const currentPrompt = prompt();
    if (!currentPrompt) return;

    const selection = String(selectedIndex() + 1);
    const sessionId = currentPrompt.sessionId;

    // Hide prompt first
    promptStore.hidePrompt();

    // Send selection to PTY
    try {
      await pty.write(sessionId, selection + "\n");
    } catch (err) {
      console.error("Failed to send selection:", err);
    }
  };

  const dismiss = () => {
    promptStore.hidePrompt();
    props.onDismiss?.();
  };

  return (
    <div class={cx(s.overlay, !isVisible() && s.hidden)}>
      <Show when={prompt()}>
        <div class={s.dialog}>
          <div class={s.question}>
            {prompt()?.question || t("promptOverlay.defaultQuestion", "Select an option:")}
          </div>
          <div class={s.options}>
            <For each={prompt()?.options ?? []}>
              {(option, index) => (
                <div
                  class={cx(s.option, index() === selectedIndex() && s.selected)}
                  onClick={() => selectAndConfirm(index())}
                >
                  <span class={s.optionKey}>{index() + 1}</span>
                  <span class={s.optionText}>{option}</span>
                </div>
              )}
            </For>
          </div>
          <div class={s.hint}>
            {t("promptOverlay.hint", "Press 1-{count} or Enter to select, Escape to cancel", { count: String(prompt()?.options.length ?? 0) })}
          </div>
        </div>
      </Show>
    </div>
  );
};

export default PromptOverlay;
