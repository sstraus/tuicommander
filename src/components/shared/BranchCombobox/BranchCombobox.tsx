import { Component, For, Show, createEffect, createSignal, onCleanup } from "solid-js";
import { cx } from "../../../utils";
import s from "./BranchCombobox.module.css";

export interface BranchComboboxProps {
  branches: string[];
  currentBranch: string | null;
  value: string;
  onChange: (branch: string) => void;
  placeholder?: string;
  loading?: boolean;
  disabled?: boolean;
}

const ChevronIcon = () => (
  <svg viewBox="0 0 16 16" fill="currentColor">
    <path d="M4.5 5.5l3.5 4 3.5-4H4.5z" />
  </svg>
);

export const BranchCombobox: Component<BranchComboboxProps> = (props) => {
  const [open, setOpen] = createSignal(false);
  const [filter, setFilter] = createSignal("");
  const [activeIndex, setActiveIndex] = createSignal(-1);

  let containerRef: HTMLDivElement | undefined;
  let inputRef: HTMLInputElement | undefined;

  const filtered = () => {
    const term = filter().toLowerCase();
    if (!term) return props.branches;
    return props.branches.filter((b) => b.toLowerCase().includes(term));
  };

  const selectBranch = (branch: string) => {
    props.onChange(branch);
    setFilter("");
    setOpen(false);
    inputRef?.blur();
  };

  const handleFocus = () => {
    if (!props.disabled) {
      setOpen(true);
      setActiveIndex(-1);
    }
  };

  const handleInput = (value: string) => {
    setFilter(value);
    setActiveIndex(-1);
    if (!open()) setOpen(true);
  };

  const handleKeyDown = (e: KeyboardEvent) => {
    const items = filtered();
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActiveIndex((i) => Math.min(i + 1, items.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActiveIndex((i) => Math.max(i - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      const idx = activeIndex();
      if (idx >= 0 && idx < items.length) {
        selectBranch(items[idx]);
      }
    } else if (e.key === "Escape") {
      e.preventDefault();
      setOpen(false);
      inputRef?.blur();
    }
  };

  // Close on outside click
  const handleClickOutside = (e: MouseEvent) => {
    if (containerRef && !containerRef.contains(e.target as Node)) {
      setOpen(false);
    }
  };

  createEffect(() => {
    if (open()) {
      document.addEventListener("mousedown", handleClickOutside);
    } else {
      document.removeEventListener("mousedown", handleClickOutside);
    }
  });

  onCleanup(() => {
    document.removeEventListener("mousedown", handleClickOutside);
  });

  // Scroll active item into view
  createEffect(() => {
    const idx = activeIndex();
    if (idx < 0) return;
    const dropdown = containerRef?.querySelector(`.${s.dropdown}`);
    const items = dropdown?.querySelectorAll(`.${s.item}`);
    if (items && items[idx]) {
      items[idx].scrollIntoView({ block: "nearest" });
    }
  });

  const displayValue = () => {
    if (open()) return filter();
    return props.value;
  };

  return (
    <div class={s.container} ref={containerRef}>
      <div class={s.inputWrap}>
        <input
          ref={inputRef}
          class={s.input}
          type="text"
          value={displayValue()}
          placeholder={props.placeholder || "Select a branch..."}
          disabled={props.disabled}
          onFocus={handleFocus}
          onInput={(e) => handleInput(e.currentTarget.value)}
          onKeyDown={handleKeyDown}
          spellcheck={false}
          autocomplete="off"
        />
        <span
          class={cx(s.chevron, open() && s.chevronOpen)}
          onClick={() => {
            if (!props.disabled) {
              setOpen(!open());
              if (!open()) inputRef?.focus();
            }
          }}
        >
          <ChevronIcon />
        </span>
      </div>

      <Show when={open() && !props.disabled}>
        <div class={s.dropdown}>
          <Show when={props.loading}>
            <div class={s.loading}>Loading...</div>
          </Show>
          <Show when={!props.loading && filtered().length === 0}>
            <div class={s.empty}>No matching branches</div>
          </Show>
          <Show when={!props.loading}>
            <For each={filtered()}>
              {(branch, i) => {
                const isCurrent = () => branch === props.currentBranch;
                return (
                  <div
                    class={cx(
                      s.item,
                      i() === activeIndex() && s.itemActive,
                      isCurrent() && s.itemCurrent,
                    )}
                    onMouseDown={(e) => {
                      e.preventDefault();
                      selectBranch(branch);
                    }}
                    onMouseEnter={() => setActiveIndex(i())}
                  >
                    {branch}
                    <Show when={isCurrent()}>
                      <span class={s.itemCurrentBadge}>current</span>
                    </Show>
                  </div>
                );
              }}
            </For>
          </Show>
        </div>
      </Show>
    </div>
  );
};
