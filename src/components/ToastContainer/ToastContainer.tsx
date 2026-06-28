import { type Component, For, Show } from "solid-js";
import { toastsStore } from "../../stores/toasts";
import { onClickKeyDown } from "../../utils/a11y";
import styles from "./ToastContainer.module.css";

export const ToastContainer: Component = () => {
	return (
		<div class={styles.container}>
			<For each={toastsStore.toasts}>
				{(toast) => (
					<div class={styles.toast} data-level={toast.level} role="button" tabIndex={0} onClick={() => toastsStore.remove(toast.id)} onKeyDown={onClickKeyDown(() => toastsStore.remove(toast.id))}>
						<span class={styles.level} data-level={toast.level} />
						<span class={styles.title}>{toast.title}</span>
						{toast.message && <span class={styles.message}>{toast.message}</span>}
						<Show when={toast.action}>
							<button
								class={styles.action}
								onClick={(e) => {
									e.stopPropagation();
									toast.action!.onClick();
									toastsStore.remove(toast.id);
								}}
							>
								{toast.action!.label}
							</button>
						</Show>
					</div>
				)}
			</For>
		</div>
	);
};
