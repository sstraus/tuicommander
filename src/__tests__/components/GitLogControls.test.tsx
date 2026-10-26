import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { expect, it, vi } from "vitest";

const h = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("../../invoke", () => ({ invoke: h.invoke }));

import { LogTab } from "../../components/GitPanel/LogTab";

it("catches log pagination losing the last hash or pending disabled state", async () => {
	const commits = Array.from({ length: 50 }, (_, i) => ({
		hash: `hash${i}`,
		subject: `Commit ${i}`,
		author_name: "Test",
		author_date: "2026-10-24T12:00:00Z",
		parents: [],
		refs: [],
	}));
	let finish!: (value: typeof commits) => void;
	h.invoke.mockImplementation((command: string, args: { after?: string }) => {
		if (command !== "get_commit_log") return Promise.resolve([]);
		return args.after
			? new Promise((resolve) => {
					finish = resolve;
				})
			: Promise.resolve(commits);
	});
	render(() => <LogTab repoPath="/repo" onOpenDiff={vi.fn()} />);
	fireEvent.click(await screen.findByRole("button", { name: "Load more" }));
	expect(h.invoke).toHaveBeenLastCalledWith("get_commit_log", { path: "/repo", count: 50, after: "hash49" });
	expect(screen.getByRole("button", { name: "Loading..." })).toBeDisabled();
	finish([]);
	await waitFor(() => expect(screen.queryByRole("button", { name: "Load more" })).not.toBeInTheDocument());
});
