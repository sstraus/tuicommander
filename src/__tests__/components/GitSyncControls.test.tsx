import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { expect, it, vi } from "vitest";

const h = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("../../invoke", () => ({ invoke: h.invoke }));

import { SyncRow } from "../../components/GitPanel/SyncRow";

it("catches sync primitive adoption losing operation payload, pending disable or feedback", async () => {
	let finish!: (value: unknown) => void;
	h.invoke.mockImplementation(
		() =>
			new Promise((resolve) => {
				finish = resolve;
			}),
	);
	render(() => (
		<SyncRow
			repoPath="/repo"
			status={{
				branch: "main",
				upstream: "origin/main",
				ahead: 1,
				behind: 2,
				stash_count: 0,
				staged: [],
				unstaged: [],
				untracked: [],
			}}
		/>
	));
	fireEvent.click(screen.getByRole("button", { name: "Fetch" }));
	expect(h.invoke).toHaveBeenCalledWith("run_git_command", { path: "/repo", args: ["fetch", "--all"] });
	for (const button of screen.getAllByRole("button")) expect(button).toBeDisabled();
	expect(screen.getByRole("button", { name: "Fetch" })).toHaveAttribute("aria-busy", "true");
	finish({ success: true, stdout: "Already up to date.\n", stderr: "", exit_code: 0 });
	await screen.findByText("Already up to date.");
	await waitFor(() => expect(screen.getByRole("button", { name: "Push" })).toBeEnabled());
});
