import { createStore, produce } from "solid-js/store";

interface TabOrderingState {
	order: string[];
}

function createTabOrderingStore() {
	const [state, setState] = createStore<TabOrderingState>({ order: [] });

	function insert(id: string, afterId?: string): void {
		if (state.order.includes(id)) return;
		setState(
			produce((s) => {
				if (afterId) {
					const idx = s.order.indexOf(afterId);
					if (idx !== -1) {
						s.order.splice(idx + 1, 0, id);
						return;
					}
				}
				s.order.push(id);
			}),
		);
	}

	function remove(id: string): void {
		setState(
			produce((s) => {
				const idx = s.order.indexOf(id);
				if (idx !== -1) s.order.splice(idx, 1);
			}),
		);
	}

	function reorder(sourceId: string, targetId: string, side: "before" | "after"): void {
		if (sourceId === targetId) return;
		setState(
			produce((s) => {
				const src = s.order.indexOf(sourceId);
				const tgt = s.order.indexOf(targetId);
				if (src === -1 || tgt === -1) return;
				s.order.splice(src, 1);
				const newTgt = s.order.indexOf(targetId);
				s.order.splice(side === "before" ? newTgt : newTgt + 1, 0, sourceId);
			}),
		);
	}

	function getOrdered(visibleIds: Set<string>): string[] {
		const ordered = state.order.filter((id) => visibleIds.has(id));
		const orderedSet = new Set(ordered);
		const remaining = [...visibleIds].filter((id) => !orderedSet.has(id));
		return [...ordered, ...remaining];
	}

	function clear(): void {
		setState("order", []);
	}

	return { state, insert, remove, reorder, getOrdered, clear };
}

export const tabOrderingStore = createTabOrderingStore();
