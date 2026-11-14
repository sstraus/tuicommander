/** Structural equality for plain JSON-shaped data: RPC payloads and the row
 *  objects built from them. Store proxies compare by their current values. */
export function sameData(a: unknown, b: unknown): boolean {
	if (a === b) return true;
	if (typeof a !== "object" || typeof b !== "object" || a === null || b === null) return false;
	if (Array.isArray(a) !== Array.isArray(b)) return false;
	const ra = a as Record<string, unknown>;
	const rb = b as Record<string, unknown>;
	const keys = Object.keys(ra);
	if (keys.length !== Object.keys(rb).length) return false;
	return keys.every((key) => key in rb && sameData(ra[key], rb[key]));
}

/** A polled list arrives as fresh objects on every tick, and a reference-keyed
 *  `<For>` remounts every row whose object changed identity — so an identical
 *  refresh tore down and rebuilt the whole list. Hand the previous rows back in:
 *  a row whose data is unchanged keeps its previous object, and when nothing at
 *  all moved `prev` itself is returned so a signal or store write is a no-op. */
export function reconcileById<T, K>(next: readonly T[], prev: readonly T[] | undefined, key: (row: T) => K): T[] {
	const prevByKey = prev ? new Map(prev.map((row) => [key(row), row])) : undefined;
	const rows = next.map((row) => {
		const old = prevByKey?.get(key(row));
		return old !== undefined && sameData(old, row) ? old : row;
	});
	// Positional, not just per-key: two reused rows that swapped places are still
	// a changed list, and handing back `prev` would silently keep the old order.
	if (prev && prev.length === rows.length && rows.every((row, i) => row === prev[i])) {
		return prev as T[];
	}
	return rows;
}
