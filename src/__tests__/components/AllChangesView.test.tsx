import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { AllChangesView } from "../../components/DiffTab/AllChangesView";
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

// A list boundary: the real virtualized list is covered by its own tests and needs layout.
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
const commits = ["c0", "c1", "c2"].map((hash) => ({
	hash: hash.repeat(20),
	parents: [],
	refs: [],
	author_name: "A",
	author_date: "2026-11-07T00:00:00Z",
	subject: `subject ${hash}`,
}));

let storedBase: string | null;
let repoCounter = 0;
let repoPath = "";

beforeEach(() => {
	vi.clearAllMocks();
	// A fresh path per test: the session-only base map is keyed by repository.
	repoPath = `/fixture-${++repoCounter}`;
	storedBase = null;
	getDiff.mockResolvedValue(fileDiff);
	getInfo.mockResolvedValue({ branch: "feature" });
	listBaseRefOptions.mockResolvedValue([
		{ name: "main", kind: "local", is_default: true },
		{ name: "develop", kind: "local", is_default: false },
	]);
	mockInvoke.mockImplementation(async (cmd: string) => {
		if (cmd === "get_branch_base") return storedBase;
		if (cmd === "get_commit_log") return commits;
		return undefined;
	});
});
afterEach(cleanup);

const scopes = () => getDiff.mock.calls.map((call) => call[1]);

it("catches a feature branch opening on uncommitted, ignoring the stored base, or including uncommitted by default", async () => {
	storedBase = "develop";
	const view = render(() => <AllChangesView repoPath={repoPath} />);
	await waitFor(() => expect(scopes()).toContain("base:develop"));
	expect(view.getByRole("tab", { name: "Branch vs base" })).toHaveAttribute("aria-selected", "true");
	expect(view.getByRole("checkbox", { name: "Include uncommitted" })).not.toBeChecked();
	expect(scopes()).not.toContain("head");
});

it("catches the base branch opening on an empty branch diff, or uncommitted read as two overlapping diffs", async () => {
	getInfo.mockResolvedValue({ branch: "main" });
	const view = render(() => <AllChangesView repoPath={repoPath} />);
	await waitFor(() => expect(scopes()).toEqual(["head"]));
	expect(view.getByRole("tab", { name: "Uncommitted" })).toHaveAttribute("aria-selected", "true");
	// One section for a.txt, not a staged copy plus an unstaged copy.
	await waitFor(() => expect(view.getByText("1 files", { exact: false })).toBeInTheDocument());
});

it("catches Include uncommitted not reaching the backend scope", async () => {
	const view = render(() => <AllChangesView repoPath={repoPath} />);
	await waitFor(() => expect(scopes()).toContain("base:main"));
	fireEvent.click(view.getByRole("checkbox", { name: "Include uncommitted" }));
	await waitFor(() => expect(scopes()).toContain("base-uncommitted:main"));
});

it("catches a base chosen in the selector being lost on reopen or written to git config", async () => {
	const first = render(() => <AllChangesView repoPath={repoPath} />);
	await waitFor(() => expect(scopes()).toContain("base:main"));
	fireEvent.change(first.getByRole("combobox", { name: "Base" }), { target: { value: "develop" } });
	await waitFor(() => expect(scopes()).toContain("base:develop"));
	cleanup();
	getDiff.mockClear();
	render(() => <AllChangesView repoPath={repoPath} />);
	await waitFor(() => expect(scopes()).toEqual(["base:develop"]));
	const commands = new Set(mockInvoke.mock.calls.map((call) => call[0]));
	expect([...commands].sort()).toEqual(["get_branch_base"]);
});

it("catches a commit range that drops its oldest commit or swaps its ends", async () => {
	const view = render(() => <AllChangesView repoPath={repoPath} />);
	await waitFor(() => expect(scopes()).toContain("base:main"));
	fireEvent.click(view.getByRole("tab", { name: "Commit / range" }));
	await waitFor(() => expect(view.getByText("subject c0")).toBeInTheDocument());
	fireEvent.click(view.getByText("subject c0"));
	await waitFor(() => expect(scopes()).toContain(commits[0].hash));
	fireEvent.click(view.getByText("subject c2"), { shiftKey: true });
	await waitFor(() => expect(scopes()).toContain(`range:${commits[2].hash}..${commits[0].hash}`));
});
