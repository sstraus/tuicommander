import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, expect, it, vi } from "vitest";
import { CreateBranchDialog } from "../../components/CreateBranchDialog/CreateBranchDialog";
import { PromptDialog } from "../../components/PromptDialog/PromptDialog";
import { WhatsNewDialog } from "../../components/WhatsNewDialog/WhatsNewDialog";

afterEach(cleanup);

// Catches: shared field/button migration drops the typed branch or checkout selection.
it("submits the trimmed branch with the selected checkout state", async () => {
	const onCreate = vi.fn().mockResolvedValue(undefined),
		onClose = vi.fn();
	const view = render(() => <CreateBranchDialog visible onClose={onClose} onCreate={onCreate} />);
	fireEvent.input(view.getByPlaceholderText("New branch name"), { target: { value: "  feature/control  " } });
	fireEvent.click(view.getByLabelText("Switch to the new branch"));
	fireEvent.click(view.getByText("Create", { selector: "button" }));
	await waitFor(() => expect(onCreate).toHaveBeenCalledWith("feature/control", false));
	expect(onClose).toHaveBeenCalledTimes(1);
});

// Catches: shared controls submit a blank prompt or lose trimming and close behavior.
it("keeps blank prompt disabled and submits the trimmed text", () => {
	const onConfirm = vi.fn(),
		onClose = vi.fn();
	const view = render(() => (
		<PromptDialog visible title="Name" placeholder="Draft name" onConfirm={onConfirm} onClose={onClose} />
	));
	const confirm = view.getByText("OK", { selector: "button" }) as HTMLButtonElement;
	expect(confirm.disabled).toBe(true);
	fireEvent.input(view.getByPlaceholderText("Draft name"), { target: { value: "  next draft  " } });
	fireEvent.click(confirm);
	expect(onConfirm).toHaveBeenCalledWith("next draft");
	expect(onClose).toHaveBeenCalledTimes(1);
});

// Catches: replacing the release-notes action loses its dismissal callback.
it("dismisses release notes through the shared action", () => {
	const onClose = vi.fn();
	const view = render(() => (
		<WhatsNewDialog visible version="fixture" highlights={[]} contributions={[]} onClose={onClose} />
	));
	fireEvent.click(view.getByText("Got it"));
	expect(onClose).toHaveBeenCalledTimes(1);
});
