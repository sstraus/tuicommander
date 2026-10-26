import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { expect, it, vi } from "vitest";

const h = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("../../invoke", () => ({ invoke: h.invoke }));

import { ChangesTab } from "../../components/GitPanel/ChangesTab";

const status = {
	staged: [{ path: "staged.ts", status: "M", additions: 1, deletions: 0 }],
	unstaged: [{ path: "edited.ts", status: "M", additions: 1, deletions: 0 }],
	untracked: [],
};

it("catches shared commit editor losing generated drafts, keyboard commit or pending disable", async () => {
	let finish!: (value: string) => void;
	h.invoke.mockImplementation((command: string) =>
		command === "get_working_tree_status"
			? Promise.resolve(status)
			: command === "git_commit"
				? new Promise((resolve) => {
						finish = resolve;
					})
				: Promise.resolve([]),
	);
	render(() => <ChangesTab repoPath="/changes-editor" onOpenDiff={vi.fn()} />);
	await screen.findByText("staged.ts");
	const editor = screen.getByPlaceholderText("Commit message...");
	expect(screen.getByTitle("Commit staged changes")).toBeDisabled();
	window.dispatchEvent(new CustomEvent("smart-prompt:commit-message", { detail: "  Generated\nmultiline draft  " }));
	expect(editor).toHaveValue("Generated\nmultiline draft");
	fireEvent.keyDown(editor, { key: "Enter", ctrlKey: true });
	expect(h.invoke).toHaveBeenCalledWith("git_commit", {
		path: "/changes-editor",
		message: "Generated\nmultiline draft",
		amend: null,
	});
	expect(editor).toBeDisabled();
	expect(screen.getByTitle("Commit staged changes")).toBeDisabled();
	finish("hash");
	await waitFor(() => expect(editor).toHaveValue(""));
	expect(editor).toBeEnabled();
});

it("catches shared filter clear or stage actions clearing selection through row bubbling", async () => {
	h.invoke.mockReset();
	h.invoke.mockImplementation(async (command: string) => (command === "get_working_tree_status" ? status : []));
	const onOpenDiff = vi.fn();
	render(() => <ChangesTab repoPath="/changes-filter" onOpenDiff={onOpenDiff} />);
	await screen.findByText("edited.ts");
	const filter = screen.getByPlaceholderText("Filter... (*, ** wildcards)");
	fireEvent.input(filter, { target: { value: "staged" } });
	expect(screen.queryByText("edited.ts")).not.toBeInTheDocument();
	fireEvent.click(screen.getByRole("button", { name: "Clear file filter" }));
	expect(filter).toHaveValue("");
	expect(screen.getByText("edited.ts")).toBeInTheDocument();
	fireEvent.click(screen.getByTitle("Stage"));
	await waitFor(() =>
		expect(h.invoke).toHaveBeenCalledWith("git_stage_files", { path: "/changes-filter", files: ["edited.ts"] }),
	);
	expect(onOpenDiff).not.toHaveBeenCalled();
});
