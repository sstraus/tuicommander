import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { afterEach, expect, it, vi } from "vitest";
import { VariableInputDialog } from "../../components/SmartPromptsDropdown/VariableInputDialog";

afterEach(cleanup);
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
