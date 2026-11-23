/** Local wall-clock `hh:mm` of an epoch-millisecond instant. */
export function clockTime(time: number): string {
	const date = new Date(time);
	return `${String(date.getHours()).padStart(2, "0")}:${String(date.getMinutes()).padStart(2, "0")}`;
}
