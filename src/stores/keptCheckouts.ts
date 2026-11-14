import { createStore, produce } from "solid-js/store";

/** Checkouts the user answered Keep for, in memory only. The sidebar reads this to show Reset Keep. */
function createKeptCheckoutsStore() {
	const [kept, setKept] = createStore<Record<string, number>>({});
	let resetHandler: ((repoPath: string, prNumber: number) => void) | null = null;

	const keyOf = (repoPath: string, branch: string) => `${repoPath}\u0000${branch}`;

	return {
		isKept: (repoPath: string, branch: string): boolean => keyOf(repoPath, branch) in kept,
		add(repoPath: string, branch: string, prNumber: number): void {
			setKept(keyOf(repoPath, branch), prNumber);
		},
		remove(repoPath: string, branch: string): void {
			setKept(produce((m) => delete m[keyOf(repoPath, branch)]));
		},
		clear(): void {
			setKept(
				produce((m) => {
					for (const k of Object.keys(m)) delete m[k];
				}),
			);
		},
		setResetHandler(fn: ((repoPath: string, prNumber: number) => void) | null): void {
			resetHandler = fn;
		},
		/** Reset Keep: the next PR notification for this checkout is processed again. */
		reset(repoPath: string, branch: string): void {
			const pr = kept[keyOf(repoPath, branch)];
			if (pr === undefined) return;
			resetHandler?.(repoPath, pr);
		},
	};
}

export const keptCheckoutsStore = createKeptCheckoutsStore();
