import { type Component, createEffect, createSignal, onCleanup, Show } from "solid-js";
import { registerModal } from "../../stores/modalStack";
import { validateBranchName } from "../RenameBranchDialog/RenameBranchDialog";
import d from "../shared/dialog.module.css";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";

export interface CreateBranchDialogProps {
	visible: boolean;
	/** Optional start point (branch/ref the new branch is based on). */
	startPoint?: string | null;
	onClose: () => void;
	onCreate: (name: string, checkout: boolean) => Promise<void>;
}

export const CreateBranchDialog: Component<CreateBranchDialogProps> = (props) => {
	const [name, setName] = createSignal("");
	const [checkout, setCheckout] = createSignal(true);
	const [error, setError] = createSignal<string | null>(null);
	const [isCreating, setIsCreating] = createSignal(false);
	let inputRef: HTMLInputElement | undefined;

	// Reset + focus on open.
	createEffect(() => {
		if (props.visible) {
			setName("");
			setCheckout(true);
			setError(null);
			setIsCreating(false);
			setTimeout(() => inputRef?.focus(), 0);
		}
	});

	// Enter to confirm, Escape to cancel.
	createEffect(() => {
		if (!props.visible) return;

		// Escape-to-close is handled centrally (stores/modalStack): registering routes
		// Escape to props.onClose AND stops it reaching the terminal underneath.
		registerModal(props.onClose);

		const handleKeydown = (e: KeyboardEvent) => {
			if (e.key === "Enter" && !isCreating()) {
				e.preventDefault();
				void handleCreate();
			}
		};
		document.addEventListener("keydown", handleKeydown);
		onCleanup(() => document.removeEventListener("keydown", handleKeydown));
	});

	const handleCreate = async () => {
		const trimmed = name().trim();
		const validationError = validateBranchName(trimmed);
		if (validationError) {
			setError(validationError);
			return;
		}
		setIsCreating(true);
		setError(null);
		try {
			await props.onCreate(trimmed, checkout());
			props.onClose();
		} catch (err) {
			setError(err instanceof Error ? err.message : String(err));
			setIsCreating(false);
		}
	};

	return (
		<Show when={props.visible}>
			<div class={d.overlay} onClick={props.onClose}>
				<div class={d.popover} onClick={(e) => e.stopPropagation()}>
					<div class={d.header}>
						<span class={d.headerIcon}>Y</span>
						<h4>Create Branch</h4>
					</div>
					<div class={d.body}>
						<Input
							ref={inputRef}
							type="text"
							value={name()}
							onInput={(e) => {
								setName(e.currentTarget.value);
								if (error()) setError(null);
							}}
							placeholder="New branch name"
							disabled={isCreating()}
						/>
						<Show when={props.startPoint}>
							<p class={d.subtitle}>from {props.startPoint}</p>
						</Show>
						<label
							style={{
								display: "flex",
								"align-items": "center",
								gap: "var(--space-2)",
								"font-size": "var(--font-sm)",
								color: "var(--fg-muted)",
								"margin-top": "var(--space-2)",
							}}
						>
							<input
								type="checkbox"
								checked={checkout()}
								onChange={(e) => setCheckout(e.currentTarget.checked)}
								disabled={isCreating()}
							/>
							Switch to the new branch
						</label>
						{error() && <p class={d.error}>{error()}</p>}
					</div>
					<div class={d.actions}>
						<Button class={d.cancelBtn} onClick={props.onClose} disabled={isCreating()}>
							Cancel
						</Button>
						<Button
							variant="primary"
							class={d.primaryBtn}
							onClick={handleCreate}
							disabled={isCreating() || !name().trim()}
						>
							{isCreating() ? "Creating…" : "Create"}
						</Button>
					</div>
				</div>
			</div>
		</Show>
	);
};

export default CreateBranchDialog;
