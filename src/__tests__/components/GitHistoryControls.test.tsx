import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { expect, it, vi } from "vitest";

const h = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("../../invoke", () => ({ invoke: h.invoke }));

import { HistoryTab } from "../../components/GitPanel/HistoryTab";

it("catches history pagination losing the last hash or pending disabled state", async () => {
	const commits = Array.from({ length: 50 }, (_, i) => ({
		hash: `hash${i}`,
		subject: `Commit ${i}`,
		author_name: "Test",
		author_date: "2026-10-24T12:00:00Z",
		parents: [],
		refs: [],
	}));
	let finish!: (value: typeof commits) => void;
	h.invoke.mockResolvedValueOnce(commits).mockImplementationOnce(
		() =>
			new Promise((resolve) => {
				finish = resolve;
			}),
	);
	render(() => <HistoryTab repoPath="/repo" filePath="file.ts" onOpenDiff={vi.fn()} />);
	fireEvent.click(await screen.findByRole("button", { name: "Load more" }));
	expect(h.invoke).toHaveBeenLastCalledWith("get_file_history", {
		path: "/repo",
		file: "file.ts",
		count: 50,
		after: "hash49",
	});
	expect(screen.getByRole("button", { name: "Loading..." })).toBeDisabled();
	finish([]);
	await waitFor(() => expect(screen.queryByRole("button", { name: "Load more" })).not.toBeInTheDocument());
});
