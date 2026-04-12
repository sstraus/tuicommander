import { Component, Show, createEffect, createSignal } from "solid-js";
import type { SearchOptions } from "./DomSearchEngine";
import { cx } from "../../utils";
import s from "./SearchBar.module.css";

export interface SearchBarProps {
  visible: boolean;
  /** Called when search term or options change */
  onSearch: (term: string, opts: SearchOptions) => void;
  onNext: () => void;
  onPrev: () => void;
  onClose: () => void;
  /** 0-based active match index */
  matchIndex: number;
  /** Total number of matches */
  matchCount: number;
  /** Optional label override for match count (e.g. "500+" for truncated results) */
  matchLabel?: string;
}

/** SVG icons for toggle buttons and navigation (VS Code style) */
const CaseSensitiveIcon = () => (
  <svg viewBox="0 0 16 16" fill="currentColor">
    <path d="M8.854 11.702h-1l-.816-2.159H3.772l-.768 2.16H2L5.09 4h.76l3.004 7.702zm-2.27-3.074L5.452 5.549a1.635 1.635 0 01-.066-.252h-.02a1.674 1.674 0 01-.07.256L4.17 8.628h2.415zM13.995 11.7v-.73c-.37.47-.955.792-1.705.792-1.2 0-2.088-.797-2.088-1.836 0-1.092.855-1.792 2.156-1.867l1.636-.09v-.362c0-.788-.49-1.257-1.328-1.257-.678 0-1.174.31-1.399.778h-.91c.153-.95 1.085-1.635 2.333-1.635 1.39 0 2.227.79 2.227 2.04V11.7h-.922z"/>
  </svg>
);

const WholeWordIcon = () => (
  <svg viewBox="0 0 16 16" fill="currentColor">
    <path d="M2 6h1v7H2V6zm5.38 4.534h-.022c-.248.371-.7.596-1.205.596C5.344 11.13 4.7 10.48 4.7 9.6c0-.925.604-1.46 1.703-1.522l1-.052V7.72c0-.547-.336-.87-.918-.87-.514 0-.836.22-1.002.563H4.59c.156-.734.808-1.253 1.866-1.253 1.117 0 1.825.6 1.825 1.548v3.023h-.9v-.197zM5.604 9.6c0 .37.283.64.674.64.548 0 .96-.373.96-.836v-.45l-.864.046c-.57.034-.77.256-.77.6zM10.552 6.26c.467 0 .824.186 1.078.54V4h.904v8.73h-.9v-.65c-.258.456-.66.72-1.158.72C9.546 12.8 8.8 11.88 8.8 10.52c0-1.37.74-2.26 1.752-2.26zm.18.816c-.647 0-1.038.56-1.038 1.44s.39 1.46 1.048 1.46c.647 0 1.043-.57 1.043-1.45s-.396-1.45-1.053-1.45z"/>
    <path d="M1 13h14v1H1z"/>
  </svg>
);

const RegexIcon = () => (
  <svg viewBox="0 0 16 16" fill="currentColor">
    <path d="M10.012 2h.976v3.113l2.56-1.557.486.885L11.47 6l2.564 1.559-.486.885-2.56-1.557V10h-.976V6.887l-2.56 1.557-.486-.885L9.53 6 6.966 4.441l.486-.885 2.56 1.557V2zM2 10h4v4H2v-4z"/>
  </svg>
);

const PrevIcon = () => (
  <svg viewBox="0 0 16 16" fill="currentColor">
    <path d="M8 3.5l-5 5h3V14h4V8.5h3l-5-5z"/>
  </svg>
);

const NextIcon = () => (
  <svg viewBox="0 0 16 16" fill="currentColor">
    <path d="M8 12.5l5-5h-3V2H6v5.5H3l5 5z"/>
  </svg>
);

const CloseIcon = () => (
  <svg viewBox="0 0 16 16" fill="currentColor">
    <path d="M8 8.707l3.646 3.647.708-.707L8.707 8l3.647-3.646-.707-.708L8 7.293 4.354 3.646l-.708.708L7.293 8l-3.647 3.646.708.708L8 8.707z"/>
  </svg>
);

export const SearchBar: Component<SearchBarProps> = (props) => {
  const [searchTerm, setSearchTerm] = createSignal("");
  const [caseSensitive, setCaseSensitive] = createSignal(false);
  const [useRegex, setUseRegex] = createSignal(false);
  const [wholeWord, setWholeWord] = createSignal(false);

  let inputRef: HTMLInputElement | undefined;

  const getOptions = (): SearchOptions => ({
    caseSensitive: caseSensitive(),
    regex: useRegex(),
    wholeWord: wholeWord(),
  });

  // Auto-focus and select when becoming visible
  createEffect(() => {
    if (props.visible) {
      requestAnimationFrame(() => {
        inputRef?.focus();
        inputRef?.select();
      });
    }
  });

  const fireSearch = () => {
    props.onSearch(searchTerm(), getOptions());
  };

  const handleInput = (value: string) => {
    setSearchTerm(value);
    props.onSearch(value, getOptions());
  };

  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      props.onClose();
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (e.shiftKey) {
        props.onPrev();
      } else {
        props.onNext();
      }
    } else if (e.key === "g" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      if (e.shiftKey) {
        props.onPrev();
      } else {
        props.onNext();
      }
    }
  };

  const toggleCaseSensitive = () => {
    setCaseSensitive((v) => !v);
    requestAnimationFrame(fireSearch);
  };
  const toggleRegex = () => {
    setUseRegex((v) => !v);
    requestAnimationFrame(fireSearch);
  };
  const toggleWholeWord = () => {
    setWholeWord((v) => !v);
    requestAnimationFrame(fireSearch);
  };

  const counterText = () => {
    if (!searchTerm()) return "";
    if (props.matchCount === 0) return "No results";
    const countLabel = props.matchLabel ?? String(props.matchCount);
    if (props.matchIndex < 0) return `${countLabel} found`;
    return `${props.matchIndex + 1} of ${countLabel}`;
  };

  return (
    <Show when={props.visible}>
      <div class={s.overlay}>
        <input
          ref={inputRef}
          class={s.input}
          type="text"
          placeholder="Find…"
          value={searchTerm()}
          onInput={(e) => handleInput(e.currentTarget.value)}
          onKeyDown={handleKeyDown}
          spellcheck={false}
        />

        <span class={s.counter}>{counterText()}</span>

        <div class={s.separator} />

        <button
          class={cx(s.toggleBtn, caseSensitive() && s.toggleActive)}
          onClick={toggleCaseSensitive}
          title="Match Case"
        >
          <CaseSensitiveIcon />
        </button>

        <button
          class={cx(s.toggleBtn, wholeWord() && s.toggleActive)}
          onClick={toggleWholeWord}
          title="Match Whole Word"
        >
          <WholeWordIcon />
        </button>

        <button
          class={cx(s.toggleBtn, useRegex() && s.toggleActive)}
          onClick={toggleRegex}
          title="Use Regular Expression"
        >
          <RegexIcon />
        </button>

        <div class={s.separator} />

        <button class={s.btn} onClick={() => props.onPrev()} title="Previous Match (Shift+Enter)">
          <PrevIcon />
        </button>

        <button class={s.btn} onClick={() => props.onNext()} title="Next Match (Enter)">
          <NextIcon />
        </button>

        <div class={s.separator} />

        <button class={s.btn} onClick={() => props.onClose()} title="Close (Escape)">
          <CloseIcon />
        </button>
      </div>
    </Show>
  );
};
