import { Component, createEffect, createSignal, onCleanup } from "solid-js";
import type { SearchAddon, ISearchOptions, ISearchResultChangeEvent } from "@xterm/addon-search";
import type { SearchOptions } from "../shared/DomSearchEngine";
import { SearchBar } from "../shared/SearchBar";

/** Decoration colors for search highlights (dark theme) */
const DECORATIONS = {
  matchBackground: "#ffff0040",
  matchBorder: "transparent",
  matchOverviewRuler: "#ffff00",
  activeMatchBackground: "#ff8c00b0",
  activeMatchBorder: "#ff8c00",
  activeMatchColorOverviewRuler: "#ff8c00",
} as const;

export interface TerminalSearchProps {
  visible: boolean;
  searchAddon: SearchAddon | undefined;
  onClose: () => void;
}

export const TerminalSearch: Component<TerminalSearchProps> = (props) => {
  const [resultIndex, setResultIndex] = createSignal(-1);
  const [resultCount, setResultCount] = createSignal(0);

  let lastTerm = "";
  let lastOpts: ISearchOptions = { incremental: true, decorations: DECORATIONS };
  let resultsListener: { dispose: () => void } | undefined;

  // Subscribe to result count changes when addon is available
  createEffect(() => {
    resultsListener?.dispose();
    const addon = props.searchAddon;
    if (!addon) return;

    resultsListener = addon.onDidChangeResults((e: ISearchResultChangeEvent) => {
      setResultIndex(e.resultIndex);
      setResultCount(e.resultCount);
    });

    onCleanup(() => {
      resultsListener?.dispose();
      resultsListener = undefined;
    });
  });

  // Clear decorations when closing
  createEffect(() => {
    if (!props.visible) {
      props.searchAddon?.clearDecorations();
      setResultIndex(-1);
      setResultCount(0);
    }
  });

  const buildXtermOpts = (opts: SearchOptions): ISearchOptions => ({
    caseSensitive: opts.caseSensitive,
    regex: opts.regex,
    wholeWord: opts.wholeWord,
    incremental: true,
    decorations: DECORATIONS,
  });

  const handleSearch = (term: string, opts: SearchOptions) => {
    lastTerm = term;
    lastOpts = buildXtermOpts(opts);
    if (term && props.searchAddon) {
      props.searchAddon.findNext(term, lastOpts);
    } else if (!term) {
      props.searchAddon?.clearDecorations();
      setResultIndex(-1);
      setResultCount(0);
    }
  };

  const handleNext = () => {
    if (lastTerm && props.searchAddon) {
      props.searchAddon.findNext(lastTerm, lastOpts);
    }
  };

  const handlePrev = () => {
    if (lastTerm && props.searchAddon) {
      props.searchAddon.findPrevious(lastTerm, lastOpts);
    }
  };

  return (
    <SearchBar
      visible={props.visible}
      onSearch={handleSearch}
      onNext={handleNext}
      onPrev={handlePrev}
      onClose={props.onClose}
      matchIndex={resultIndex()}
      matchCount={resultCount()}
    />
  );
};
