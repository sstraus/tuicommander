import { Component, For, Show, createSignal, createEffect, onCleanup } from "solid-js";
import { PromptOption } from "../ui";
import { promptStore } from "../../stores/prompt";
import { usePty } from "../../hooks/usePty";

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
    <div id="prompt-overlay" class={isVisible() ? "" : "hidden"}>
      <Show when={prompt()}>
        <div class="prompt-dialog">
          <div id="prompt-question">
            {prompt()?.question || "Select an option:"}
          </div>
          <div id="prompt-options">
            <For each={prompt()?.options ?? []}>
              {(option, index) => (
                <PromptOption
                  index={index()}
                  label={option}
                  selected={index() === selectedIndex()}
                  onClick={() => selectAndConfirm(index())}
                />
              )}
            </For>
          </div>
          <div class="prompt-hint">
            Press 1-{prompt()?.options.length} or Enter to select, Escape to cancel
          </div>
        </div>
      </Show>
    </div>
  );
};

export default PromptOverlay;
