import { Component, createEffect, createSignal, onCleanup } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import type { SearchOptions } from "../shared/DomSearchEngine";
import { SearchBar } from "../shared/SearchBar";
import type { ScrollbackMatch } from "./ScrollbackOverlay";
import { nextMatchIdx, prevMatchIdx } from "./scrollbackSearchUtils";
import { appLogger } from "../../stores/appLogger";

/**
 * Scrollback-aware find bar. Mirrors `TerminalSearch` (which targets the
 * xterm.js alt-screen buffer via the search addon) but searches the full
 * `VtLogBuffer` history over a dedicated Tauri IPC channel.
 *
 * - Debounces input by 120ms so fast typing doesn't spam `search_vt_log`.
 * - Regex + whole-word are currently *not* supported by the Rust side;
 *   they're silently ignored here (the toggle buttons still render for
 *   consistency with `TerminalSearch`, but future work could pipe them
 *   through to the backend). Only `caseSensitive` is routed end-to-end.
 * - `onActiveMatchChange` drives the overlay scroll/highlight — parent
 *   holds the match array, we just emit the selected one so the parent
 *   can render the `activeMatch` prop on `ScrollbackOverlay`.
 */
export interface VtLogSearchProps {
  visible: boolean;
  /** Rust session id to scope the VtLogBuffer query to. */
  sessionId: string | null;
  /** Cap on returned matches — matches `search_vt_log` semantics. */
  maxMatches?: number;
  /** Fired every time the active (navigated-to) match changes. */
  onActiveMatchChange: (match: ScrollbackMatch | null) => void;
  /** Parent closes the bar + hands focus back to its container. */
  onClose: () => void;
}

/** Backend response shape — keep in sync with `SearchResults` in `vt_log_search.rs`. */
interface SearchResults {
  matches: ScrollbackMatch[];
  truncated: boolean;
}

const DEFAULT_MAX_MATCHES = 500;
const DEBOUNCE_MS = 120;

export const VtLogSearch: Component<VtLogSearchProps> = (props) => {
  const [matches, setMatches] = createSignal<ScrollbackMatch[]>([]);
  const [activeIdx, setActiveIdx] = createSignal(-1);
  const [truncated, setTruncated] = createSignal(false);

  // Latest term/opts — retained so `onNext`/`onPrev` can act without
  // re-reading the input; `runSearch` stores them here on every fire.
  let lastTerm = "";
  let lastCaseSensitive = false;
  let debounceTimer: ReturnType<typeof setTimeout> | undefined;
  // Monotonic token to drop responses from stale invokes — the user may
  // type faster than the backend responds, and without this we'd flash
  // older match sets over the newest one.
  let requestToken = 0;

  // Reset state + emit null when the bar is hidden so the parent clears
  // the active highlight and the overlay removes its match marker.
  createEffect(() => {
    if (!props.visible) {
      setMatches([]);
      setActiveIdx(-1);
      setTruncated(false);
      lastTerm = "";
      props.onActiveMatchChange(null);
    }
  });

  // Emit the currently active match whenever the index moves. Derived from
  // `matches()` + `activeIdx()` rather than stored separately to stay single
  // source of truth.
  createEffect(() => {
    const list = matches();
    const idx = activeIdx();
    if (idx < 0 || idx >= list.length) {
      props.onActiveMatchChange(null);
      return;
    }
    props.onActiveMatchChange(list[idx]);
  });

  onCleanup(() => {
    if (debounceTimer) clearTimeout(debounceTimer);
  });

  async function runSearch(term: string, caseSensitive: boolean) {
    if (!props.sessionId) {
      setMatches([]);
      setActiveIdx(-1);
      return;
    }
    if (!term) {
      setMatches([]);
      setActiveIdx(-1);
      return;
    }
    const token = ++requestToken;
    try {
      const result = await invoke<SearchResults>("search_vt_log", {
        sessionId: props.sessionId,
        query: term,
        caseSensitive,
        maxMatches: props.maxMatches ?? DEFAULT_MAX_MATCHES,
      });
      // Drop stale responses — a newer request has already started.
      if (token !== requestToken) return;
      setMatches(result.matches);
      setTruncated(result.truncated);
      // Select the first match automatically so the user immediately
      // sees the navigation / scroll behavior.
      setActiveIdx(result.matches.length > 0 ? 0 : -1);
    } catch (err) {
      if (token !== requestToken) return;
      appLogger.warn("terminal", "search_vt_log failed", { error: String(err) });
      setMatches([]);
      setActiveIdx(-1);
    }
  }

  const handleSearch = (term: string, opts: SearchOptions) => {
    lastTerm = term;
    lastCaseSensitive = opts.caseSensitive;
    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => {
      void runSearch(lastTerm, lastCaseSensitive);
    }, DEBOUNCE_MS);
  };

  const handleNext = () => {
    const total = matches().length;
    if (total === 0) return;
    setActiveIdx((cur) => nextMatchIdx(cur, total));
  };

  const handlePrev = () => {
    const total = matches().length;
    if (total === 0) return;
    setActiveIdx((cur) => prevMatchIdx(cur, total));
  };

  return (
    <SearchBar
      visible={props.visible}
      onSearch={handleSearch}
      onNext={handleNext}
      onPrev={handlePrev}
      onClose={props.onClose}
      matchIndex={activeIdx()}
      matchCount={matches().length}
      matchLabel={truncated() ? `${matches().length}+` : undefined}
    />
  );
};
