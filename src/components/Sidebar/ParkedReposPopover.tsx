import { Component, For, Show, onMount, onCleanup } from "solid-js";
import { repositoriesStore } from "../../stores/repositories";
import type { RepositoryState } from "../../stores/repositories";
import s from "./Sidebar.module.css";

export interface ParkedReposPopoverProps {
  onClose: () => void;
  onUnpark: (repoPath: string) => void;
}

export const ParkedReposPopover: Component<ParkedReposPopoverProps> = (props) => {
  let popoverRef: HTMLDivElement | undefined;

  const parkedRepos = () => repositoriesStore.getParkedRepos();

  // Close on click outside or Escape
  onMount(() => {
    const handleClick = (e: MouseEvent) => {
      if (popoverRef && !popoverRef.contains(e.target as Node)) {
        props.onClose();
      }
    };
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") props.onClose();
    };
    // Delay listener to avoid closing from the click that opened us
    requestAnimationFrame(() => {
      document.addEventListener("mousedown", handleClick);
      document.addEventListener("keydown", handleKey);
    });
    onCleanup(() => {
      document.removeEventListener("mousedown", handleClick);
      document.removeEventListener("keydown", handleKey);
    });
  });

  return (
    <div ref={popoverRef} class={s.parkedPopover}>
      <div class={s.parkedPopoverHeader}>Parked Repositories</div>
      <Show when={parkedRepos().length === 0}>
        <div class={s.parkedPopoverEmpty}>No parked repositories</div>
      </Show>
      <For each={parkedRepos()}>
        {(repo: RepositoryState) => (
          <div class={s.parkedPopoverItem}>
            <button
              class={s.parkedPopoverName}
              onClick={() => props.onUnpark(repo.path)}
              title={`Unpark and switch to ${repo.displayName}`}
            >
              {repo.displayName}
            </button>
            <button
              class={s.parkedPopoverUnpark}
              onClick={() => {
                repositoriesStore.setPark(repo.path, false);
              }}
              title="Unpark"
            >
              <svg width="12" height="12" viewBox="0 0 16 16" fill="none">
                <path d="M8 14V5M4 8l4-4 4 4" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"/>
              </svg>
            </button>
          </div>
        )}
      </For>
    </div>
  );
};
