import { createMemo, createSignal, For } from "solid-js";
import { TIPS } from "../../data/tips";
import styles from "./TipOfTheDay.module.css";

/** Max visible dots in the sliding window */
const MAX_DOTS = 7;
const HALF = Math.floor(MAX_DOTS / 2);

export default function TipOfTheDay() {
  const [index, setIndex] = createSignal(Math.floor(Math.random() * TIPS.length));

  const prev = () => setIndex((i) => (i - 1 + TIPS.length) % TIPS.length);
  const next = () => setIndex((i) => (i + 1) % TIPS.length);

  const visibleDots = createMemo(() => {
    const n = TIPS.length;
    if (n <= MAX_DOTS) return TIPS.map((_, i) => i);
    const cur = index();
    let start = cur - HALF;
    if (start < 0) start = 0;
    if (start > n - MAX_DOTS) start = n - MAX_DOTS;
    const result: number[] = [];
    for (let i = start; i < start + MAX_DOTS; i++) result.push(i);
    return result;
  });

  const dotScale = (i: number) => {
    const dots = visibleDots();
    const pos = dots.indexOf(i);
    if (pos === -1) return 0;
    if (pos === 0 && dots[0] > 0) return 0.6;
    if (pos === dots.length - 1 && dots[dots.length - 1] < TIPS.length - 1) return 0.6;
    return 1;
  };

  return (
    <div class={styles.card}>
      <div class={styles.label}>TIP OF THE DAY</div>
      <div class={styles.body}>
        <button class={styles.arrow} onClick={prev} aria-label="Previous tip">
          <svg width="18" height="18" viewBox="0 0 16 16" fill="currentColor">
            <path d="M10 3L5 8l5 5V3z" />
          </svg>
        </button>
        <div class={styles.content}>
          <span class={styles.feature}>{TIPS[index()].feature}</span>
          <span class={styles.description}>{TIPS[index()].description}</span>
          {TIPS[index()].shortcut && (
            <kbd class={styles.shortcut}>{TIPS[index()].shortcut}</kbd>
          )}
        </div>
        <button class={styles.arrow} onClick={next} aria-label="Next tip">
          <svg width="18" height="18" viewBox="0 0 16 16" fill="currentColor">
            <path d="M6 3l5 5-5 5V3z" />
          </svg>
        </button>
      </div>
      <div class={styles.dots}>
        <For each={visibleDots()}>
          {(i) => (
            <span
              class={styles.dot}
              classList={{ [styles.dotActive]: i === index() }}
              style={{ transform: `scale(${dotScale(i)})` }}
              onClick={() => setIndex(i)}
            />
          )}
        </For>
      </div>
    </div>
  );
}
