import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { DiffTab } from "../../components/DiffTab/DiffTab";
import { uiStore } from "../../stores/ui";
import { mockInvoke } from "../mocks/tauri";
import "../mocks/tauri";

const { getFileDiff } = vi.hoisted(() => ({ getFileDiff: vi.fn() }));
vi.mock("../../hooks/useRepository", () => ({ useRepository: () => ({ getFileDiff }) }));
vi.mock("../../hooks/usePty", () => ({ usePty: () => ({ sendCommand: vi.fn() }) }));
// A renderer boundary with the row attributes consumed by DiffTab; real renderer is mounted in browser evidence.
vi.mock("../../components/ui/DiffViewer", () => ({
	DiffViewer: (props: { contentRef: (el: HTMLElement) => void }) => (
		<div ref={props.contentRef}>
			<table>
				<tbody>
					<tr>
						<td class="diff-line-hunk-content">hunk</td>
					</tr>
					<tr>
						<td data-line-new-num="1">added</td>
					</tr>
				</tbody>
			</table>
		</div>
	),
}));
const diff = "diff --git a/file.txt b/file.txt\n--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-old\n+new\n";
beforeEach(() => {
	vi.clearAllMocks();
	getFileDiff.mockResolvedValue(diff);
	uiStore.setDiffViewMode("split");
	uiStore._testCancelPendingSave();
});
afterEach(() => {
	cleanup();
	uiStore._testCancelPendingSave();
});

it("catches view icon migration losing mode changes or one-sided disabled state", async () => {
	getFileDiff.mockResolvedValue(
		"diff --git a/file.txt b/file.txt\nnew file mode 100644\n--- /dev/null\n+++ b/file.txt\n@@ -0,0 +1 @@\n+new\n",
	);
	const view = render(() => <DiffTab tabId="controls" repoPath="/fixture" filePath="file.txt" />);
	await waitFor(() => expect(view.getByRole("button", { name: "Side-by-side" })).toBeDisabled());
	expect(view.getByRole("button", { name: "All files" })).toBeDisabled();
	expect(view.getByRole("button", { name: "Inline" })).toHaveAttribute("aria-pressed", "true");
	cleanup();
	getFileDiff.mockResolvedValue(diff);
	const normal = render(() => <DiffTab tabId="controls" repoPath="/fixture" filePath="file.txt" />);
	await waitFor(() => expect(normal.getByRole("button", { name: "Side-by-side" })).not.toBeDisabled());
	fireEvent.click(normal.getByRole("button", { name: "Inline" }));
	expect(uiStore.state.diffViewMode).toBe("unified");
});

it("catches comment controls losing disabled Send, Escape cancellation or selection clear", async () => {
	const view = render(() => <DiffTab tabId="controls" repoPath="/fixture" filePath="file.txt" />);
	await waitFor(() => expect(view.getByRole("button", { name: "Discard hunk" })).toBeInTheDocument());
	fireEvent.mouseDown(view.getByText("added"), { button: 0 });
	fireEvent.mouseUp(document);
	fireEvent.click(view.getByRole("button", { name: "Comment" }));
	await waitFor(() => expect(document.activeElement).toBe(view.getByRole("textbox")));
	expect(view.getByRole("button", { name: "Send" })).toBeDisabled();
	fireEvent.input(view.getByRole("textbox"), { target: { value: "review" } });
	expect(view.getByRole("button", { name: "Send" })).not.toBeDisabled();
	fireEvent.keyDown(view.getByRole("textbox"), { key: "Escape" });
	expect(view.queryByRole("textbox")).toBeNull();
	fireEvent.click(view.getByRole("button", { name: "Clear selection" }));
	expect(view.queryByRole("button", { name: "Comment" })).toBeNull();
});

it("catches hunk icon migration breaking delegated confirmation and reverse patch payload", async () => {
	const view = render(() => <DiffTab tabId="controls" repoPath="/fixture" filePath="file.txt" />);
	await waitFor(() => expect(view.getByRole("button", { name: "Discard hunk" })).toBeInTheDocument());
	fireEvent.click(view.getByRole("button", { name: "Discard hunk" }));
	fireEvent.click(view.getByRole("button", { name: "Discard" }));
	await waitFor(() =>
		expect(mockInvoke).toHaveBeenCalledWith("git_apply_reverse_patch", {
			path: "/fixture",
			patch: expect.stringContaining("@@ -1 +1 @@"),
			scope: undefined,
		}),
	);
});
