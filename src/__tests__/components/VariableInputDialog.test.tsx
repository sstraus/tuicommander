import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { afterEach, expect, it, vi } from "vitest";
import { VariableInputDialog } from "../../components/SmartPromptsDropdown/VariableInputDialog";

import { nextAnimationFrame } from "../helpers/animationFrame";

afterEach(async () => {
	// The control/submit assertions finish before the initial focus frame.
	await nextAnimationFrame();
	cleanup();
});
// Catches: shared inputs stop forwarding values or Run loses native submit semantics.
it("keeps suggestions, whitespace validation and submitted values through shared controls", () => {
	const onSubmit = vi.fn();
	const view = render(() => (
		<VariableInputDialog
			variables={["goal", "scope"]}
			suggestions={{ goal: "Review" }}
			promptName="Review changes"
			onSubmit={onSubmit}
			onCancel={() => {}}
		/>
	));
	const goal = view.getByRole("textbox", { name: "{goal}" }) as HTMLInputElement;
	const scope = view.getByRole("textbox", { name: "{scope}" });
	const run = view.getByRole("button", { name: "Run" }) as HTMLButtonElement;
	expect(goal.value).toBe("Review");
	expect(run.disabled).toBe(true);
	fireEvent.input(scope, { target: { value: "  " } });
	fireEvent.submit(view.container.querySelector("form")!);
	expect(onSubmit).not.toHaveBeenCalled();
	expect(run.disabled).toBe(true);
	fireEvent.input(scope, { target: { value: "Current branch" } });
	expect(run.disabled).toBe(false);
	expect(run.type).toBe("submit");
	fireEvent.submit(view.container.querySelector("form")!);
	expect(onSubmit).toHaveBeenCalledExactlyOnceWith({ goal: "Review", scope: "Current branch" });
});
// Catches: the initial focus frame outlives the dialog, so a pending timer survives test teardown.
it("cancels the initial focus frame when the dialog unmounts first", async () => {
	const requestFrame = window.requestAnimationFrame.bind(window);
	let ranAfterUnmount = false;
	let unmounted = false;
	const spy = vi.spyOn(window, "requestAnimationFrame").mockImplementationOnce((callback) =>
		requestFrame((time) => {
			ranAfterUnmount = unmounted;
			callback(time);
		}),
	);
	const view = render(() => (
		<VariableInputDialog variables={["goal"]} promptName="Prompt" onSubmit={() => {}} onCancel={() => {}} />
	));
	spy.mockRestore();
	view.unmount();
	unmounted = true;
	await nextAnimationFrame();
	expect(ranAfterUnmount).toBe(false);
});
// Catches: the first-field ref never binds through the shared Input, so opening the dialog leaves focus on body.
it("focuses the first input when the dialog opens", async () => {
	const view = render(() => (
		<VariableInputDialog variables={["goal", "scope"]} promptName="Prompt" onSubmit={() => {}} onCancel={() => {}} />
	));
	await nextAnimationFrame();
	expect(document.activeElement).toBe(view.getByRole("textbox", { name: "{goal}" }));
});
// Catches: the migrated Cancel button submits the form or Escape loses its owner.
it("keeps Cancel and Escape separate from submission", () => {
	const onCancel = vi.fn(),
		onSubmit = vi.fn();
	const view = render(() => (
		<VariableInputDialog
			variables={["goal"]}
			suggestions={{ goal: "Review" }}
			promptName="Prompt"
			onSubmit={onSubmit}
			onCancel={onCancel}
		/>
	));
	const cancel = view.getByRole("button", { name: "Cancel" }) as HTMLButtonElement;
	expect(cancel.type).toBe("button");
	fireEvent.click(cancel);
	fireEvent.keyDown(document, { key: "Escape" });
	expect(onCancel).toHaveBeenCalledTimes(2);
	expect(onSubmit).not.toHaveBeenCalled();
});
