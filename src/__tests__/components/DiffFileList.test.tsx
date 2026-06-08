import { render } from "@solidjs/testing-library";
import { describe, expect, it } from "vitest";
import { DiffFileList, sectionToRawDiff } from "../../components/shared/DiffFileList";
import { parseDiffFiles } from "../../components/ui/DiffViewer";

describe("sectionToRawDiff", () => {
	it("reconstructs a file section's raw diff from its parsed lines", () => {
		const raw = "diff --git a/f b/f\n@@ -1 +1 @@\n-old\n+new";
		const [section] = parseDiffFiles(raw);
		expect(sectionToRawDiff(section)).toBe(raw);
	});
});

describe("DiffFileList", () => {
	// The virtualizer can't measure rows in happy-dom (zero layout), so we assert
	// the always-present chrome rather than per-file row counts.
	it("renders the provided header above the list", () => {
		const files = parseDiffFiles("diff --git a/a.ts b/a.ts\n@@ -1 +1 @@\n-x\n+y");
		const { getByText } = render(() => <DiffFileList files={files} mode="unified" header={<div>HEADER</div>} />);
		expect(getByText("HEADER")).toBeTruthy();
	});

	it("mounts without throwing when there are no files", () => {
		const { container } = render(() => <DiffFileList files={[]} mode="unified" />);
		expect(container).toBeTruthy();
	});
});
