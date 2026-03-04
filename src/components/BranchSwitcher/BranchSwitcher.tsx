import { Component, For, Show, createEffect, createMemo, createSignal, onCleanup } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { branchSwitcherStore } from "../../stores/branchSwitcher";
import s from "./BranchSwitcher.module.css";

interface BranchInfo {
  name: string;
  is_current: boolean;
  is_remote: boolean;
  is_main: boolean;
}

export interface BranchSwitcherProps {
  activeRepoPath: string | undefined;
  onSelect: (repoPath: string, branchName: string) => void;
  onCheckoutRemote: (repoPath: string, branchName: string) => void;
}

export const BranchSwitcher: Component<BranchSwitcherProps> = (props) => {
  const [selectedIndex, setSelectedIndex] = createSignal(0);
  const [branches, setBranches] = createSignal<BranchInfo[]>([]);
  const [loading, setLoading] = createSignal(false);
  let inputRef: HTMLInputElement | undefined;
  let listRef: HTMLDivElement | undefined;

  const isOpen = () => branchSwitcherStore.state.isOpen;

  // Fetch branches when dialog opens
  createEffect(() => {
    if (!isOpen()) return;
    const repoPath = props.activeRepoPath;
    if (!repoPath) {
      setBranches([]);
      return;
    }

    setLoading(true);
    invoke<BranchInfo[]>("get_git_branches", { path: repoPath })
      .then((result) => setBranches(result))
      .catch(() => setBranches([]))
      .finally(() => setLoading(false));
  });

  // Filter and sort branches
  const filteredBranches = createMemo(() => {
    const query = branchSwitcherStore.state.query.toLowerCase();
    let items = branches();

    if (query) {
      items = items.filter((b) => b.name.toLowerCase().includes(query));
    }

    // Sort: current first, then main, then locals alphabetical, then remotes alphabetical
    return [...items].sort((a, b) => {
      if (a.is_current && !b.is_current) return -1;
      if (!a.is_current && b.is_current) return 1;
      if (a.is_main && !b.is_main) return -1;
      if (!a.is_main && b.is_main) return 1;
      if (!a.is_remote && b.is_remote) return -1;
      if (a.is_remote && !b.is_remote) return 1;
      return a.name.localeCompare(b.name);
    });
  });

  // Reset selection when query changes
  createEffect(() => {
    branchSwitcherStore.state.query;
    setSelectedIndex(0);
  });

  // Focus input when opened
  createEffect(() => {
    if (isOpen()) {
      requestAnimationFrame(() => inputRef?.focus());
    }
  });

  // Scroll selected item into view
  createEffect(() => {
    const idx = selectedIndex();
    if (!listRef) return;
    const item = listRef.children[idx] as HTMLElement | undefined;
    item?.scrollIntoView({ block: "nearest" });
  });

  // Keyboard navigation
  createEffect(() => {
    if (!isOpen()) return;

    const handleKeydown = (e: KeyboardEvent) => {
      const items = filteredBranches();

      switch (e.key) {
        case "ArrowDown":
          e.preventDefault();
          e.stopPropagation();
          setSelectedIndex((i) => Math.min(i + 1, items.length - 1));
          break;
        case "ArrowUp":
          e.preventDefault();
          e.stopPropagation();
          setSelectedIndex((i) => Math.max(i - 1, 0));
          break;
        case "Enter":
          e.preventDefault();
          e.stopPropagation();
          if (items[selectedIndex()]) {
            selectBranch(items[selectedIndex()]);
          }
          break;
        case "Escape":
          e.preventDefault();
          e.stopPropagation();
          branchSwitcherStore.close();
          break;
      }
    };

    document.addEventListener("keydown", handleKeydown, true);
    onCleanup(() => document.removeEventListener("keydown", handleKeydown, true));
  });

  const selectBranch = (branch: BranchInfo) => {
    const repoPath = props.activeRepoPath;
    if (!repoPath) return;

    branchSwitcherStore.close();

    // Current branch — nothing to do
    if (branch.is_current) return;

    if (branch.is_remote) {
      // Strip "origin/" prefix for checkout
      const localName = branch.name.replace(/^origin\//, "");
      props.onCheckoutRemote(repoPath, localName);
    } else {
      props.onSelect(repoPath, branch.name);
    }
  };

  return (
    <Show when={isOpen()}>
      <div class={s.overlay} onClick={() => branchSwitcherStore.close()}>
        <div class={s.palette} onClick={(e) => e.stopPropagation()}>
          <div class={s.search}>
            <input
              ref={inputRef}
              type="text"
              placeholder="Switch to branch..."
              value={branchSwitcherStore.state.query}
              onInput={(e) => branchSwitcherStore.setQuery(e.currentTarget.value)}
            />
          </div>

          <div class={s.list} ref={listRef}>
            <Show when={!props.activeRepoPath}>
              <div class={s.empty}>No repository selected</div>
            </Show>

            <Show when={props.activeRepoPath && loading()}>
              <div class={s.loading}>Loading branches...</div>
            </Show>

            <Show when={props.activeRepoPath && !loading() && filteredBranches().length === 0}>
              <div class={s.empty}>No branches match</div>
            </Show>

            <For each={filteredBranches()}>
              {(branch, idx) => (
                <div
                  data-testid="branch-item"
                  class={`${s.item} ${idx() === selectedIndex() ? s.selected : ""}`}
                  onClick={() => selectBranch(branch)}
                  onMouseEnter={() => setSelectedIndex(idx())}
                >
                  <span class={s.branchName}>{branch.name}</span>
                  <Show when={branch.is_current}>
                    <span data-testid="badge-current" class={`${s.badge} ${s.badgeCurrent}`}>current</span>
                  </Show>
                  <Show when={branch.is_remote}>
                    <span data-testid="badge-remote" class={`${s.badge} ${s.badgeRemote}`}>remote</span>
                  </Show>
                  <Show when={branch.is_main && !branch.is_remote}>
                    <span data-testid="badge-main" class={`${s.badge} ${s.badgeMain}`}>main</span>
                  </Show>
                </div>
              )}
            </For>
          </div>

          <div class={s.footer}>
            <span class={s.footerHint}><kbd>↑↓</kbd> navigate</span>
            <span class={s.footerHint}><kbd>↵</kbd> switch</span>
            <span class={s.footerHint}><kbd>esc</kbd> close</span>
          </div>
        </div>
      </div>
    </Show>
  );
};
