import { Facet } from "@codemirror/state";

/** Submit the captured Compose editor through its normal send/cleanup path. */
export const composeSubmit = Facet.define<() => void, (() => void) | null>({
	combine: (handlers) => handlers[0] ?? null,
});
