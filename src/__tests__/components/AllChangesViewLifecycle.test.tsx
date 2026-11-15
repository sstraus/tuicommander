import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { AllChangesView } from "../../components/DiffTab/AllChangesView";
import { repositoriesStore } from "../../stores/repositories";
import { mockInvoke } from "../mocks/tauri";
import "../mocks/tauri";

const { getDiff, getInfo, listBaseRefOptions } = vi.hoisted(() => ({
	getDiff: vi.fn(),
	getInfo: vi.fn(),
	listBaseRefOptions: vi.fn(),
}));
vi.mock("../../hooks/useRepository", () => ({
	useRepository: () => ({ getDiff, getInfo, listBaseRefOptions }),
}));
vi.mock("../../components/shared/DiffFileList", () => ({
	DiffFileList: (props: { header: unknown; files: { path: string }[] }) => (
		<div>
			{props.header as never}
			{props.files.map((file) => (
				<section>{file.path}</section>
			))}
		</div>
	),
}));

const fileDiff = "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1,2 @@\n a\n+b\n";
const entry = (hash: string) => ({
	hash: hash.repeat(20),
	parents: [],
	refs: [],
	author_name: "A",
	author_date: "2026-11-07T00:00:00Z",
	subject: `subject ${hash}`,
});

let log: ReturnType<typeof entry>[];

beforeEach(() => {
	vi.clearAllMocks();
	log = ["c0", "c1", "c2"].map(entry);
	getDiff.mockResolvedValue(fileDiff);
	getInfo.mockResolvedValue({ branch: "feature" });
	listBaseRefOptions.mockResolvedValue([{ name: "main", kind: "local", is_default: true }]);
	mockInvoke.mockImplementation(async (cmd: string) => {
		if (cmd === "get_branch_base") return null;
		if (cmd === "get_commit_log") return log;
		return undefined;
	});
});
afterEach(cleanup);

// Catches a commit selection stored as a list index: a commit made while the
// view is open prepends to the log, and the selection then shows another commit.
it("keeps showing the selected commit after a newer commit arrives", async () => {
	const repoPath = "/lifecycle-1";
	const view = render(() => <AllChangesView repoPath={repoPath} />);
	await waitFor(() => expect(getDiff).toHaveBeenCalled());
	fireEvent.click(view.getByRole("tab", { name: "Commit / range" }));
	await waitFor(() => expect(view.getByText("subject c1")).toBeInTheDocument());
	fireEvent.click(view.getByText("subject c1"));
	await waitFor(() => expect(getDiff).toHaveBeenLastCalledWith(repoPath, entry("c1").hash));

	log = [entry("c9"), ...log];
	getDiff.mockClear();
	repositoriesStore.bumpRevision(repoPath);
	await waitFor(() => expect(view.getByText("subject c9")).toBeInTheDocument());
	await waitFor(() => expect(getDiff).toHaveBeenCalled());
	const shown = getDiff.mock.calls.map((call) => call[1]);
	expect(shown).not.toContain(entry("c0").hash);
	expect(shown.at(-1)).toBe(entry("c1").hash);
});
