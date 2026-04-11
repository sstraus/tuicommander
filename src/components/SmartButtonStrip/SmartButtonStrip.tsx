import { Component, For, Show, createMemo, createSignal, onCleanup } from "solid-js";
import { promptLibraryStore, type SavedPrompt, type SmartPlacement } from "../../stores/promptLibrary";
import { useSmartPrompts } from "../../hooks/useSmartPrompts";
import { appLogger } from "../../stores/appLogger";
import { cx } from "../../utils";
import s from "./SmartButtonStrip.module.css";

const SparkleIcon = () => (
  <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
    <path d="M8 0l1.5 4.5L14 6l-4.5 1.5L8 12 6.5 7.5 2 6l4.5-1.5L8 0zm4 9l.75 2.25L15 12l-2.25.75L12 15l-.75-2.25L9 12l2.25-.75L12 9z" />
  </svg>
);

const ChevronIcon = () => (
  <svg width="10" height="10" viewBox="0 0 16 16" fill="currentColor">
    <path d="M4 6l4 4 4-4" stroke="currentColor" stroke-width="2" fill="none" />
  </svg>
);

const CheckIcon = () => (
  <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor" class={s.checkMark}>
    <path d="M13.78 4.22a.75.75 0 0 1 0 1.06l-7.25 7.25a.75.75 0 0 1-1.06 0L2.22 9.28a.75.75 0 0 1 1.06-1.06L6 10.94l6.72-6.72a.75.75 0 0 1 1.06 0z" />
  </svg>
);

export interface SmartButtonStripProps {
  placement: SmartPlacement;
  repoPath: string;
  /** Default prompt ID to show in the main button (before user picks one) */
  defaultPromptId?: string;
  extraFilter?: (p: SavedPrompt) => boolean;
  /** Called with error message when a prompt fails */
  onError?: (msg: string) => void;
  /** Called when busy state changes (for parent spinner/state) */
  onBusyChange?: (busy: boolean) => void;
  /** Extra context variables to pass as manualVariables to executeSmartPrompt */
  contextVariables?: () => Record<string, string>;
}

/** Translate internal error codes into user-friendly messages */
function friendlyError(result: { reason?: string; output?: string }, promptName: string): string {
  if (result.reason === "unresolved_variables" && result.output) {
    try {
      const vars = JSON.parse(result.output) as string[];
      if (vars.includes("staged_diff")) return "Stage some files first — no staged changes to analyze";
      return `Missing context: ${vars.join(", ")}`;
    } catch { /* fall through */ }
  }
  return result.reason ?? `"${promptName}" failed`;
}

export const SmartButtonStrip: Component<SmartButtonStripProps> = (props) => {
  const { canExecute, executeSmartPrompt } = useSmartPrompts();
  const [menuOpen, setMenuOpen] = createSignal(false);
  const [busy, setBusy] = createSignal(false);
  let rootRef: HTMLDivElement | undefined;

  // Close dropdown on click outside
  const handleClickOutside = (e: MouseEvent) => {
    if (menuOpen() && rootRef && !rootRef.contains(e.target as Node)) {
      setMenuOpen(false);
    }
  };
  document.addEventListener("mousedown", handleClickOutside);
  onCleanup(() => document.removeEventListener("mousedown", handleClickOutside));

  const prompts = createMemo(() => {
    const all = promptLibraryStore.getSmartByPlacement(props.placement);
    return props.extraFilter ? all.filter(props.extraFilter) : all;
  });

  /** Most recently used prompt for this placement, or the configured default */
  const activePrompt = createMemo((): SavedPrompt | undefined => {
    const all = prompts();
    if (all.length === 0) return undefined;
    const recent = promptLibraryStore.state.recentIds;
    const allIds = new Set(all.map((p) => p.id));
    for (const id of recent) {
      if (allIds.has(id)) return all.find((p) => p.id === id);
    }
    if (props.defaultPromptId) {
      const def = all.find((p) => p.id === props.defaultPromptId);
      if (def) return def;
    }
    return all[0];
  });

  async function runPrompt(prompt: SavedPrompt) {
    setMenuOpen(false);
    promptLibraryStore.markAsUsed(prompt.id);
    const check = canExecute(prompt);
    if (!check.ok) {
      props.onError?.(friendlyError(check, prompt.name));
      return;
    }
    props.onError?.(""); // clear previous error
    setBusy(true);
    props.onBusyChange?.(true);
    try {
      const extraVars = props.contextVariables?.();
      const result = await executeSmartPrompt(prompt, extraVars);
      if (!result.ok) props.onError?.(friendlyError(result, prompt.name));
    } catch (err) {
      props.onError?.(String(err));
      appLogger.error("prompts", `SmartButtonStrip "${prompt.name}" failed`, err);
    } finally {
      setBusy(false);
      props.onBusyChange?.(false);
    }
  }

  return (
    <Show when={activePrompt()}>
      <div class={s.split} ref={rootRef}>
        <button
          class={cx(s.splitMain, busy() && s.splitBusy, !canExecute(activePrompt()!).ok && s.splitDisabled)}
          disabled={busy() || !canExecute(activePrompt()!).ok}
          onClick={() => runPrompt(activePrompt()!)}
          title={activePrompt()!.description}
        >
          <Show when={busy()} fallback={<SparkleIcon />}>
            <span class={s.spinner} />
          </Show>
          {activePrompt()!.name}
        </button>
        <Show when={prompts().length > 1}>
          <button
            class={cx(s.splitArrow, menuOpen() && s.splitArrowOpen)}
            onClick={() => setMenuOpen(!menuOpen())}
            title="More actions"
          >
            <ChevronIcon />
          </button>
          <Show when={menuOpen()}>
            <div class={s.menu}>
              <For each={prompts()}>
                {(prompt) => {
                  const check = () => canExecute(prompt);
                  const isActive = () => prompt.id === activePrompt()?.id;
                  return (
                    <button
                      class={cx(
                        s.menuItem,
                        isActive() && s.menuItemActive,
                        !check().ok && s.menuItemDisabled,
                      )}
                      disabled={!check().ok}
                      title={!check().ok ? check().reason : prompt.description}
                      onClick={(e) => { e.stopPropagation(); runPrompt(prompt); }}
                    >
                      {prompt.name}
                      <Show when={isActive()}>
                        <CheckIcon />
                      </Show>
                    </button>
                  );
                }}
              </For>
            </div>
          </Show>
        </Show>
      </div>
    </Show>
  );
};
