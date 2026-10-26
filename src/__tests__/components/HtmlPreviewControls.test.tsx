import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { afterEach, expect, it, vi } from "vitest";
import { HtmlPreviewTab } from "../../components/HtmlPreviewTab/HtmlPreviewTab";
import { editorTabsStore } from "../../stores/editorTabs";
import type { HtmlPreviewTab as PreviewData } from "../../stores/mdTabs";
import { openLocalPath } from "../../utils/openUrl";

vi.mock("../../hooks/useRepository", () => ({
	useRepository: () => ({ readFile: vi.fn().mockResolvedValue("fixture") }),
}));
vi.mock("../../utils/openUrl", () => ({ handleOpenUrl: vi.fn(), openLocalPath: vi.fn() }));
afterEach(() => {
	cleanup();
	vi.restoreAllMocks();
});
// Catches: primitive migration drops the preview's edit or external-open callbacks.
it("keeps preview edit and external-open actions bound to the original file", () => {
	const add = vi.spyOn(editorTabsStore, "add").mockReturnValue("fixture");
	const tab = {
		id: "preview-controls",
		type: "html-preview",
		repoPath: "/repo",
		filePath: "notes.txt",
		fileName: "notes.txt",
	} as PreviewData;
	const view = render(() => <HtmlPreviewTab tab={tab} />);
	fireEvent.click(view.getByTitle("Edit source"));
	expect(add).toHaveBeenCalledWith("/repo", "notes.txt");
	fireEvent.click(view.getByTitle("Open externally"));
	expect(openLocalPath).toHaveBeenCalledWith("/repo/notes.txt");
});
