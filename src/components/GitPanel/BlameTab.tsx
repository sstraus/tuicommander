import { Component, createEffect, createMemo, createSignal, For, Show } from "solid-js";
import { invoke } from "../../invoke";
import { repositoriesStore } from "../../stores/repositories";
import { formatRelativeTime } from "../../utils/time";
import { cx } from "../../utils";
import s from "./BlameTab.module.css";

/** Mirrors the Rust BlameLine struct from git.rs */
interface BlameLine {
  hash: string;
  author: string;
  /** Unix timestamp in seconds */
  author_time: number;
  line_number: number;
  content: string;
}

export interface BlameTabProps {
  repoPath: string | null;
  filePath: string | null;
}

/** Compute a heatmap background color based on age fraction (0 = newest, 1 = oldest) */
function heatmapBackground(ageFraction: number): string {
  // Interpolate opacity: recent = 0.15, old = 0.02
  const opacity = 0.15 - ageFraction * 0.13;
  return `rgba(100, 200, 100, ${opacity.toFixed(3)})`;
}

export const BlameTab: Component<BlameTabProps> = (props) => {
  const [lines, setLines] = createSignal<BlameLine[]>([]);
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [highlightedHash, setHighlightedHash] = createSignal<string | null>(null);

  // Fetch blame data when file or repo changes
  createEffect(() => {
    const repoPath = props.repoPath;
    const filePath = props.filePath;

    if (!repoPath || !filePath) {
      setLines([]);
      setError(null);
      return;
    }

    // Subscribe to revision for reactivity
    void repositoriesStore.getRevision(repoPath);

    setLoading(true);
    setError(null);
    setHighlightedHash(null);

    invoke<BlameLine[]>("get_file_blame", { path: repoPath, file: filePath })
      .then((result) => {
        setLines(result);
      })
      .catch((err) => {
        setError(String(err));
        setLines([]);
      })
      .finally(() => {
        setLoading(false);
      });
  });

  // Compute age range for heatmap normalization
  const ageRange = createMemo(() => {
    const data = lines();
    if (data.length === 0) return { oldest: 0, newest: 0 };
    let oldest = data[0].author_time;
    let newest = data[0].author_time;
    for (const line of data) {
      if (line.author_time < oldest) oldest = line.author_time;
      if (line.author_time > newest) newest = line.author_time;
    }
    return { oldest, newest };
  });

  /** Calculate age fraction for a given timestamp (0 = newest, 1 = oldest) */
  function ageFraction(authorTime: number): number {
    const { oldest, newest } = ageRange();
    if (newest === oldest) return 0; // all same age
    return (newest - authorTime) / (newest - oldest);
  }

  /** Check if this line starts a new commit group (different hash from previous line) */
  function isGroupStart(index: number): boolean {
    if (index === 0) return true;
    const data = lines();
    return data[index].hash !== data[index - 1].hash;
  }

  function handleGutterClick(hash: string) {
    setHighlightedHash((prev) => (prev === hash ? null : hash));
  }

  return (
    <div class={s.container}>
      {/* File header */}
      <Show when={props.filePath}>
        <div class={s.fileHeader}>
          <span class={s.fileLabel}>File:</span>
          <span class={s.filePath}>{props.filePath}</span>
        </div>
      </Show>

      {/* Empty / loading / error states */}
      <Show when={!props.repoPath}>
        <div class={s.empty}>No repository selected</div>
      </Show>

      <Show when={props.repoPath && !props.filePath}>
        <div class={s.empty}>Select a file to view blame</div>
      </Show>

      <Show when={loading()}>
        <div class={s.loading}>
          <span class={s.loadingDot}>Loading blame</span>
        </div>
      </Show>

      <Show when={error()}>
        <div class={s.empty}>{error()}</div>
      </Show>

      {/* Blame content */}
      <Show when={!loading() && !error() && lines().length > 0}>
        <div class={s.blameScroll}>
          <div class={s.blameTable}>
            <For each={lines()}>
              {(line, index) => {
                const groupStart = () => isGroupStart(index());
                const highlighted = () => highlightedHash() === line.hash;
                const fraction = () => ageFraction(line.author_time);

                return (
                  <div
                    class={cx(
                      s.blameLine,
                      groupStart() && s.blameLineGroupStart,
                      highlighted() && s.blameLineHighlighted,
                    )}
                    style={{ background: highlighted() ? undefined : heatmapBackground(fraction()) }}
                  >
                    <span
                      class={cx(s.gutter, !groupStart() && s.gutterContinuation)}
                      onClick={() => handleGutterClick(line.hash)}
                    >
                      <Show when={groupStart()}>
                        <span class={s.gutterHash}>{line.hash.slice(0, 7)}</span>
                        <span class={s.gutterAuthor}>{line.author}</span>
                        <span class={s.gutterAge}>
                          {formatRelativeTime(line.author_time * 1000)}
                        </span>
                      </Show>
                    </span>
                    <span class={s.lineNumber}>{line.line_number}</span>
                    <span class={s.code}>{line.content}</span>
                  </div>
                );
              }}
            </For>
          </div>
        </div>

        {/* Heatmap legend */}
        <div class={s.heatmapLegend}>
          <span>Heatmap:</span>
          <div class={s.heatmapBar}>
            <div class={s.heatmapRecent} />
            <div class={s.heatmapOld} />
          </div>
          <span>recent</span>
          <span>old</span>
        </div>
      </Show>
    </div>
  );
};

export default BlameTab;
