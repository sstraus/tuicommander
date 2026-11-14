/** Saved review window; disk reloads invalidate it, successful local saves extend it. */
export function createMarkdownReviewBaseline() {
	let baseline: string | null = null;
	let latest: string | undefined;
	let pending = false;
	let epoch = 0;
	return {
		load(text: string) {
			if (text !== latest) {
				baseline = null;
				epoch++;
			}
			latest = text;
		},
		save(before: string, after: string) {
			if (!pending && latest === before) baseline = before;
			else if (latest !== before) baseline = null;
			latest = after;
			pending = true;
		},
		capture() {
			return { baseline, epoch };
		},
		acknowledge(text: string, capturedEpoch: number) {
			pending = latest !== text;
			baseline = pending && epoch === capturedEpoch ? text : null;
		},
	};
}
