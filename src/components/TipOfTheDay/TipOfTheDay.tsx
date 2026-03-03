import { createSignal } from "solid-js";
import { TIPS } from "../../data/tips";
import styles from "./TipOfTheDay.module.css";

export default function TipOfTheDay() {
  const [index, setIndex] = createSignal(Math.floor(Math.random() * TIPS.length));

  const prev = () => setIndex((i) => (i - 1 + TIPS.length) % TIPS.length);
  const next = () => setIndex((i) => (i + 1) % TIPS.length);

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
        {TIPS.map((_, i) => (
          <span
            class={styles.dot}
            classList={{ [styles.dotActive]: i === index() }}
            onClick={() => setIndex(i)}
          />
        ))}
      </div>
    </div>
  );
}
