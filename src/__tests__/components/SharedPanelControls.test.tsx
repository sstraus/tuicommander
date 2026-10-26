import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { afterEach, expect, it, vi } from "vitest";
import { OutlinePanel } from "../../components/OutlinePanel/OutlinePanel";
import { ReferencesPanel } from "../../components/ReferencesPanel/ReferencesPanel";

afterEach(cleanup);
// Catches: primitive replacement leaves a visible close action without its owner callback.
it.each([
	["outline", OutlinePanel],
	["references", ReferencesPanel],
] as const)("keeps the %s close action bound to its owner", (_name, Panel) => {
	const onClose = vi.fn();
	const view = render(() => <Panel visible onClose={onClose} />);
	fireEvent.click(view.getByTitle("Close"));
	expect(onClose).toHaveBeenCalledTimes(1);
});
