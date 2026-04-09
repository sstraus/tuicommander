import { Component, For, Show, createMemo, createSignal, onCleanup } from "solid-js";
import { terminalsStore, type CommandBlock } from "../../stores/terminals";
import s from "./CommandOverview.module.css";

/** Extract command text from a block using the terminal's buffer lines */
function getCommandText(termId: string, block: CommandBlock): string {
  if (block.commandLine == null || block.executionLine == null) return "";
  const term = terminalsStore.get(termId);
  const ref = term?.ref;
  if (!ref) return "";
  const lines = ref.getBufferLines(block.commandLine, block.executionLine);
  return lines.join(" ").trim();
}

/** Format duration in human-readable form */
function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  const s = Math.floor(ms / 1000);
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  const rem = s % 60;
  return rem > 0 ? `${m}m ${rem}s` : `${m}m`;
}

/** Format relative time (e.g. "2m ago", "just now") */
function formatAgo(timestamp: number, now: number): string {
  const diff = now - timestamp;
  if (diff < 5000) return "just now";
  if (diff < 60_000) return `${Math.floor(diff / 1000)}s ago`;
  if (diff < 3600_000) return `${Math.floor(diff / 60_000)}m ago`;
  return `${Math.floor(diff / 3600_000)}h ago`;
}

interface TerminalOverviewEntry {
  termId: string;
  name: string;
  lastBlock: CommandBlock | null;
  activeBlock: CommandBlock | null;
  shellState: string | null;
}

export const CommandOverview: Component = () => {
  // Tick every 10s to refresh relative timestamps
  const [now, setNow] = createSignal(Date.now());
  const timer = setInterval(() => setNow(Date.now()), 10_000);
  onCleanup(() => clearInterval(timer));

  const entries = createMemo<TerminalOverviewEntry[]>(() => {
    const ids = terminalsStore.getIds();
    return ids.map((id) => {
      const term = terminalsStore.get(id);
      if (!term) return null;
      const blocks = term.commandBlocks;
      const lastBlock = blocks.length > 0 ? blocks[blocks.length - 1] : null;
      return {
        termId: id,
        name: term.name || id.slice(0, 6),
        lastBlock,
        activeBlock: term.activeBlock ?? null,
        shellState: term.shellState ?? null,
      };
    }).filter(Boolean) as TerminalOverviewEntry[];
  });

  function handleClick(termId: string) {
    terminalsStore.setActive(termId);
  }

  return (
    <div class={s.dashboard}>
      <div class={s.header}>Commands</div>
      <Show when={entries().length === 0}>
        <div class={s.empty}>No terminals open</div>
      </Show>
      <For each={entries()}>
        {(entry) => {
          const block = () => entry.lastBlock;
          const active = () => entry.activeBlock;
          const isRunning = () => entry.shellState === "busy" || (active() != null && active()!.endLine == null);
          const exitCode = () => block()?.exitCode ?? null;
          const commandText = () => {
            const b = block();
            if (!b) {
              const a = active();
              if (a) return getCommandText(entry.termId, a);
              return "";
            }
            return getCommandText(entry.termId, b);
          };
          const duration = () => {
            const b = block();
            if (!b?.startedAt || !b?.endedAt) return null;
            return b.endedAt - b.startedAt;
          };
          const ago = () => {
            const b = block();
            if (!b?.endedAt) return null;
            return formatAgo(b.endedAt, now());
          };

          return (
            <div class={s.row} onClick={() => handleClick(entry.termId)}>
              <div
                class={`${s.exitDot} ${isRunning() ? s.exitRunning : exitCode() === 0 ? s.exitOk : exitCode() != null ? s.exitErr : ""}`}
              />
              <div class={s.name}>{entry.name}</div>
              <div class={s.command}>{commandText() || (isRunning() ? "running..." : "idle")}</div>
              <div class={s.meta}>
                <Show when={duration() != null}>
                  <span class={s.duration}>{formatDuration(duration()!)}</span>
                </Show>
                <Show when={ago() != null}>
                  <span>{ago()}</span>
                </Show>
              </div>
            </div>
          );
        }}
      </For>
    </div>
  );
};
