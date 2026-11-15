import { batch, createSignal } from "solid-js";

/** Workflows space (Boss Q6=A, 2026-11-12): neutral definitions, opened without a
 *  selected plan or story. */
const [visible, setVisible] = createSignal(false);
const [project, setProject] = createSignal<string | null>(null);

function open(path: string | null | undefined): void {
	if (!path) return;
	batch(() => {
		setProject(path);
		setVisible(true);
	});
}

export const workflowsUi = {
	visible,
	project,
	open,
	close(): void {
		setVisible(false);
	},
};
