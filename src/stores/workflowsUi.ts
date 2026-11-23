import { batch, createSignal } from "solid-js";

/** Workflows space (Boss Q6=A, 2026-11-12): neutral definitions, opened without a
 *  selected plan or story. */
const [visible, setVisible] = createSignal(false);
const [project, setProject] = createSignal<string | null>(null);
/** A run to inspect once the Designer shows the project; consumed by the inspection view. */
const [run, setRun] = createSignal<string | null>(null);

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
	run,
	open,
	/** Open the project's Designer on the inspection of one run. */
	openRun(path: string, runId: string): void {
		batch(() => {
			setRun(runId);
			open(path);
		});
	},
	/** Read the pending run once; later opens start without a selected run. */
	takeRun(): string | null {
		const id = run();
		if (id) setRun(null);
		return id;
	},
	close(): void {
		setVisible(false);
	},
};
