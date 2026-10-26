import { type Component, For, Show } from "solid-js";
import { type Toast, toastsStore } from "../../stores/toasts";
import { onClickKeyDown } from "../../utils/a11y";
// Mobile shares this list; the UI barrel eagerly loads desktop renderers and stores.
import { Button } from "../ui/Button";
import styles from "./ToastContainer.module.css";

/** "Go to repo": `onClick` returns false when it could not navigate, and the toast stays. */
export type RepoAction = { label: string; onClick: () => boolean };
type ToastAction = RepoAction | NonNullable<Toast["action"]>;

interface ToastListProps {
	onDismiss: (toast: Toast) => void;
	repoName: (toast: Toast) => string | null;
	repoAction?: (toast: Toast) => RepoAction | null;
	/** Px to keep clear on the right, e.g. for a docked panel. */
	rightInset?: number;
}

/** Shared toast presentation. Navigation stays in the shell-specific wrapper. */
export const ToastList: Component<ToastListProps> = (props) => {
	function toastActions(toast: Toast): ToastAction[] {
		const repoAction = props.repoAction?.(toast);
		return [...(repoAction ? [repoAction] : []), ...(toast.action ? [toast.action] : [])];
	}

	return (
		<Show when={toastsStore.toasts.length > 0}>
			<div
				class={styles.container}
				style={props.rightInset ? { "--toast-right-inset": `${props.rightInset}px` } : undefined}
			>
				<For each={toastsStore.toasts}>
					{(toast) => (
						<div
							class={styles.toast}
							data-level={toast.level}
							role="button"
							tabIndex={0}
							onClick={() => props.onDismiss(toast)}
							onKeyDown={onClickKeyDown(() => props.onDismiss(toast))}
						>
							<span class={styles.level} data-level={toast.level} />
							<span class={styles.body}>
								<span class={styles.titleRow}>
									<Show when={props.repoName(toast)}>{(name) => <span class={styles.repo}>{name()}</span>}</Show>
									<span class={styles.title}>
										{toast.title}
										<Show when={(toast.count ?? 1) > 1}> ×{toast.count}</Show>
									</span>
								</span>
								{toast.message && <span class={styles.message}>{toast.message}</span>}
							</span>
							<span class={styles.actions}>
								<For each={toastActions(toast)}>
									{(action) => (
										<Button
											variant={action === toast.action ? "primary" : "secondary"}
											onClick={(event) => {
												event.stopPropagation();
												// A toast's own action always dismisses it; only the repo action reports failure.
												const navigated = action.onClick();
												if (action === toast.action || navigated !== false) toastsStore.remove(toast.id);
											}}
										>
											{action.label}
										</Button>
									)}
								</For>
							</span>
						</div>
					)}
				</For>
			</div>
		</Show>
	);
};
