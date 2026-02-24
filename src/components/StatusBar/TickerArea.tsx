import { Component, Show, For, createSignal, onMount, onCleanup } from "solid-js";
import { statusBarTicker, URGENT_PRIORITY } from "../../stores/statusBarTicker";
import { terminalsStore } from "../../stores/terminals";
import { cx } from "../../utils/cx";
import s from "./StatusBar.module.css";

/**
 * Shared ticker area in the status bar.
 * Displays one message at a time from all plugins, with source label,
 * counter badge, click-to-cycle, and right-click popover.
 */
export const TickerArea: Component = () => {
  const [popoverOpen, setPopoverOpen] = createSignal(false);
  let tickerRef: HTMLSpanElement | undefined;

  // Close popover on Escape
  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape" && popoverOpen()) {
      setPopoverOpen(false);
    }
  };

  onMount(() => document.addEventListener("keydown", handleKeyDown));
  onCleanup(() => document.removeEventListener("keydown", handleKeyDown));

  // Rotation state (respects priority tiers)
  const rotation = () => {
    const state = statusBarTicker.getRotationState();
    if (!state.message) return null;

    // Hide claude-usage ticker when it's already shown in the agent badge
    const activeAgent = terminalsStore.getActive()?.agentType;
    if (activeAgent === "claude" && state.message.pluginId === "claude-usage") {
      // Recalculate without this message? No — the store handles the rotation.
      // If the only message is claude-usage and it's absorbed, show nothing.
      return null;
    }
    return state;
  };

  const allMessages = () => statusBarTicker.getActiveMessages();

  const handleBadgeClick = (e: MouseEvent) => {
    e.stopPropagation();
    statusBarTicker.advanceManually();
  };

  const handleTextClick = () => {
    const state = rotation();
    state?.message?.onClick?.();
  };

  const handleContextMenu = (e: MouseEvent) => {
    e.preventDefault();
    setPopoverOpen((prev) => !prev);
  };

  const handlePopoverRowClick = (onClick?: () => void) => {
    onClick?.();
    setPopoverOpen(false);
  };

  return (
    <>
      <Show when={rotation()}>
        {(state) => (
          <span
            ref={tickerRef}
            class={cx(
              s.tickerMessage,
              state().message!.priority >= URGENT_PRIORITY && s.tickerWarning,
              state().message!.priority >= 80 && state().message!.priority < URGENT_PRIORITY && s.tickerAttention,
              state().message!.onClick && s.tickerClickable,
            )}
            title={state().message!.text}
            onContextMenu={handleContextMenu}
          >
            <Show when={state().message!.icon}>
              <span class={s.tickerIcon} innerHTML={state().message!.icon!} />
            </Show>
            <Show when={state().message!.label}>
              <span class={s.tickerLabel}>{state().message!.label!}</span>
              <span class={s.tickerSep}>{" \u00B7 "}</span>
            </Show>
            <span class={s.tickerText} onClick={handleTextClick}>
              {state().message!.text}
            </span>
            <Show when={state().total > 1}>
              <span
                class={s.tickerBadge}
                onClick={handleBadgeClick}
                title="Click to cycle tickers"
              >
                {state().current}/{state().total} {"\u25B8"}
              </span>
            </Show>
          </span>
        )}
      </Show>

      {/* Popover listing all active tickers */}
      <Show when={popoverOpen()}>
        <div class={s.tickerOverlay} onClick={() => setPopoverOpen(false)} />
        <div class={s.tickerPopover}>
          <div class={s.tickerPopoverHeader}>
            Active Tickers
            <button class={s.tickerPopoverClose} onClick={() => setPopoverOpen(false)}>&times;</button>
          </div>
          <div class={s.tickerPopoverList}>
            <For each={allMessages()} fallback={
              <div class={s.tickerPopoverEmpty}>No active tickers</div>
            }>
              {(msg) => (
                <div
                  class={cx(
                    s.tickerPopoverRow,
                    msg.onClick && s.tickerPopoverRowClickable,
                    msg.priority >= URGENT_PRIORITY && s.tickerPopoverRowUrgent,
                  )}
                  onClick={() => handlePopoverRowClick(msg.onClick)}
                >
                  <Show when={msg.icon}>
                    <span class={s.tickerIcon} innerHTML={msg.icon!} />
                  </Show>
                  <Show when={msg.label}>
                    <span class={s.tickerPopoverLabel}>{msg.label}</span>
                  </Show>
                  <span class={s.tickerPopoverText}>{msg.text}</span>
                  <Show when={msg.priority >= URGENT_PRIORITY}>
                    <span class={s.tickerPopoverUrgentBadge}>URGENT</span>
                  </Show>
                </div>
              )}
            </For>
          </div>
        </div>
      </Show>
    </>
  );
};
