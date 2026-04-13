import { Component, For } from "solid-js";
import { toastsStore } from "../../stores/toasts";
import styles from "./ToastContainer.module.css";

export const ToastContainer: Component = () => {
  return (
    <div class={styles.container}>
      <For each={toastsStore.toasts}>
        {(toast) => (
          <div
            class={styles.toast}
            data-level={toast.level}
            onClick={() => toastsStore.remove(toast.id)}
          >
            <span class={styles.level} data-level={toast.level} />
            <span class={styles.title}>{toast.title}</span>
            {toast.message && <span class={styles.message}>{toast.message}</span>}
          </div>
        )}
      </For>
    </div>
  );
};
