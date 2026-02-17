import { Component, Show, createSignal, createEffect, onCleanup } from "solid-js";

export interface RunCommandDialogProps {
  visible: boolean;
  savedCommand: string;
  onClose: () => void;
  onSaveAndRun: (command: string) => void;
}

export const RunCommandDialog: Component<RunCommandDialogProps> = (props) => {
  const [command, setCommand] = createSignal("");
  let inputRef: HTMLInputElement | undefined;

  // Reset state and focus input when dialog opens
  createEffect(() => {
    if (props.visible) {
      setCommand(props.savedCommand);
      setTimeout(() => {
        if (inputRef) {
          inputRef.focus();
          inputRef.select();
        }
      }, 0);
    }
  });

  // Handle keyboard events
  createEffect(() => {
    if (!props.visible) return;

    const handleKeydown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        props.onClose();
      } else if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        handleSaveAndRun();
      }
    };

    document.addEventListener("keydown", handleKeydown, true);
    onCleanup(() => document.removeEventListener("keydown", handleKeydown, true));
  });

  const handleSaveAndRun = () => {
    const trimmed = command().trim();
    if (!trimmed) return;
    props.onSaveAndRun(trimmed);
  };

  return (
    <Show when={props.visible}>
      <div class="branch-popover-overlay" onClick={props.onClose}>
        <div class="branch-popover run-command-dialog" onClick={(e) => e.stopPropagation()}>
          <div class="branch-popover-header">
            <span class="branch-icon">▶</span>
            <h4>Run Command</h4>
          </div>
          <div class="branch-popover-content">
            <p class="run-command-description">
              Enter a command to run in this worktree. It will be saved to repository settings.
            </p>
            <input
              ref={inputRef}
              type="text"
              class="run-command-input"
              value={command()}
              onInput={(e) => setCommand((e.target as HTMLInputElement).value)}
              placeholder="npm run dev, cargo watch, make dev..."
            />
          </div>
          <div class="branch-popover-actions">
            <button
              class="branch-popover-cancel"
              onClick={props.onClose}
            >
              Cancel
            </button>
            <button
              class="branch-popover-rename"
              onClick={handleSaveAndRun}
              disabled={!command().trim()}
            >
              Save & Run
            </button>
          </div>
        </div>
      </div>
    </Show>
  );
};

export default RunCommandDialog;
