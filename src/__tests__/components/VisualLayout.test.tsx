import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { cleanup, render, waitFor } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CommandPalette } from "../../components/CommandPalette/CommandPalette";
import { CreateWorktreeDialog } from "../../components/CreateWorktreeDialog/CreateWorktreeDialog";
import { FileBrowserPanel } from "../../components/FileBrowserPanel/FileBrowserPanel";
import { HelpPanel } from "../../components/HelpPanel/HelpPanel";
import { MarkdownPanel } from "../../components/MarkdownPanel/MarkdownPanel";
import { PromptDrawer } from "../../components/PromptDrawer/PromptDrawer";
import { CloseDialog } from "../../components/QuitDialog/CloseDialog";
import { StoriesDialog } from "../../components/StoriesDialog/StoriesDialog";
import { TaskField } from "../../components/Tasks/detail/TaskField";
import { roadmapDemo } from "../../components/Tasks/roadmap/demo";
import { TasksRoadmap } from "../../components/Tasks/roadmap/TasksRoadmap";
import { TunnelEditorModal } from "../../components/TunnelsPanel/TunnelEditorModal";
import { DiffViewer } from "../../components/ui/DiffViewer";
import { WorktreeManager } from "../../components/WorktreeManager/WorktreeManager";
import { BottomTabs } from "../../mobile/components/BottomTabs";
import { closeDialogStore } from "../../stores/closeDialog";
import { commandPaletteStore } from "../../stores/commandPalette";
import { promptLibraryStore } from "../../stores/promptLibrary";
import { worktreeManagerStore } from "../../stores/worktreeManager";

vi.mock("../../invoke", () => ({ invoke: vi.fn().mockResolvedValue(null) }));
vi.mock("../../stores/appLogger", () => ({
	appLogger: { error: vi.fn(), warn: vi.fn(), info: vi.fn(), debug: vi.fn() },
}));

const sheets: HTMLStyleElement[] = [];
function css(...paths: string[]) {
	for (const path of paths) {
		const sheet = document.createElement("style");
		sheet.textContent = readFileSync(resolve(process.cwd(), path), "utf8");
		document.head.append(sheet);
		sheets.push(sheet);
	}
}
function componentCss(name: string) {
	css(`src/components/${name}/${name}.module.css`);
}
const noop = () => {};
afterEach(() => {
	cleanup();
	for (const sheet of sheets.splice(0)) sheet.remove();
	delete document.documentElement.dataset.theme;
	document.documentElement.removeAttribute("style");
	commandPaletteStore.close();
	promptLibraryStore.closeDrawer();
	worktreeManagerStore.close();
	vi.restoreAllMocks();
});

describe("visual review layout regressions", () => {
	it("catches dark native checkboxes on a light close dialog", () => {
		css("src/global.css");
		document.documentElement.dataset.theme = "vscode-light";
		closeDialogStore.present({ kind: "ask", sessions: 3, background_available: true });
		const { getByRole } = render(() => <CloseDialog />);
		expect(getByRole("checkbox")).toBeTruthy();
		expect(getComputedStyle(document.body).colorScheme).toBe("light");
	});

	it("catches palette hints forced beyond the narrow footer", () => {
		componentCss("CommandPalette");
		commandPaletteStore.open();
		const { container } = render(() => <CommandPalette actions={[]} />);
		expect(getComputedStyle(container.querySelector(".footer")!).flexWrap).toBe("wrap");
		expect(getComputedStyle(container.querySelector(".footerHint")!).whiteSpace).toBe("nowrap");
	});

	it("catches branch placeholders clipped without ellipsis", () => {
		componentCss("CreateWorktreeDialog");
		const { getByRole } = render(() => (
			<CreateWorktreeDialog
				visible
				suggestedName=""
				existingBranches={[]}
				worktreeBranches={[]}
				worktreesDir="/workspace"
				onClose={noop}
				onCreate={noop}
			/>
		));
		expect(getComputedStyle(getByRole("textbox")).textOverflow).toBe("ellipsis");
	});

	it("catches empty diffs touching the left edge", () => {
		css("src/styles.css");
		const { getByText } = render(() => <DiffViewer diff="" />);
		expect(parseFloat(getComputedStyle(getByText("No changes")).paddingLeft)).toBeGreaterThanOrEqual(8);
	});

	it.each(["Files", "Markdown"])("catches detached %s panels retaining half-window sizing", (name) => {
		componentCss(name === "Files" ? "FileBrowserPanel" : "MarkdownPanel");
		const { container } = render(() =>
			name === "Files" ? (
				<FileBrowserPanel visible repoPath={null} mode="detached" onFileOpen={noop} onClose={noop} />
			) : (
				<MarkdownPanel visible repoPath={null} mode="detached" onClose={noop} />
			),
		);
		const panel = getComputedStyle(container.querySelector(".panel")!);
		expect(panel.width).toBe("100%");
		expect(panel.maxWidth).toBe("100%");
		expect(parseFloat(panel.minWidth)).toBe(0);
	});

	it("catches Help metadata left aligned above a centered version", () => {
		componentCss("HelpPanel");
		const { getByText } = render(() => <HelpPanel visible onClose={noop} />);
		const footer = getComputedStyle(getByText(/MIT License/).parentElement!);
		expect(footer.textAlign).toBe("center");
		expect(footer.gap).toBe("8px");
	});

	it("catches mobile tabs painted dark over the light theme", () => {
		css("src/mobile/components/BottomTabs.module.css");
		document.documentElement.style.setProperty("--bg-secondary", "rgb(243, 243, 243)");
		const { getByRole } = render(() => <BottomTabs aiChatAvailable={true} active="sessions" onSelect={noop} />);
		expect(getComputedStyle(getByRole("navigation")).backgroundColor).toBe("rgb(243, 243, 243)");
	});

	it("catches dark diff gutters surviving a light application palette", () => {
		css("src/styles.css");
		vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue({
			measureText: () => ({ width: 8 }),
		} as unknown as CanvasRenderingContext2D);
		document.documentElement.style.setProperty("--bg-secondary", "rgb(243, 243, 243)");
		const diff = "diff --git a/a.ts b/a.ts\n--- a/a.ts\n+++ b/a.ts\n@@ -1 +1 @@\n-old\n+new\n";
		const { container } = render(() => <DiffViewer diff={diff} />);
		const root = container.querySelector(".diff-style-root")!;
		expect(root).toBeTruthy();
		expect(getComputedStyle(root).getPropertyValue("--diff-plain-lineNumber--")).toBe("rgb(243, 243, 243)");
	});

	it("catches prompt actions competing with hints for one footer line", () => {
		componentCss("PromptDrawer");
		promptLibraryStore.openDrawer();
		const { container } = render(() => <PromptDrawer />);
		expect(getComputedStyle(container.querySelector(".footer")!).flexWrap).toBe("wrap");
		expect(getComputedStyle(container.querySelector(".footer button")!).whiteSpace).toBe("nowrap");
	});

	it("catches Stories Retry wrapping one letter at a time beside errors", async () => {
		componentCss("StoriesDialog");
		const { findByRole } = render(() => <StoriesDialog project="/visual-audit" onClose={noop} />);
		const retry = await findByRole("button", { name: "Retry" });
		expect(getComputedStyle(retry).flexShrink).toBe("0");
		expect(getComputedStyle(retry).whiteSpace).toBe("nowrap");
	});

	it("catches task labels bottom aligned according to control height", () => {
		css("src/components/Tasks/detail/TasksDetail.module.css");
		const schema = {
			id: "priority",
			name: "Priority",
			kind: "enum" as const,
			required: false,
			editable: true,
			sortable: false,
			groupable: false,
			filterable: false,
			options: [],
		};
		const { container } = render(() => (
			<TaskField schema={schema} value={null} disabled={false} canEdit onSave={async () => true} />
		));
		expect(getComputedStyle(container.querySelector(".field")!).alignItems).toBe("flex-start");
	});

	it("catches roadmap state names cut off without an ellipsis", async () => {
		css("src/components/Tasks/roadmap/roadmap.module.css");
		vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(600);
		vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(1000);
		const { container } = render(() => (
			<TasksRoadmap provider={roadmapDemo()} initialDate={new Date(2026, 9, 1)} onOpen={noop} />
		));
		await waitFor(() => expect(container.querySelector(".bar > span")).toBeTruthy());
		const label = getComputedStyle(container.querySelector(".bar > span")!);
		expect(label.display).toBe("block");
		expect(label.textOverflow).toBe("ellipsis");
	});

	it("catches tunnel sibling fields receiving unequal top margins", () => {
		css("src/components/SettingsPanel/Settings.module.css", "src/components/TunnelsPanel/TunnelEditorModal.module.css");
		const { container } = render(() => <TunnelEditorModal onClose={noop} />);
		const host = container.querySelector('[data-setting-id="tunnels.profiles.{id}.host"]')!;
		const port = container.querySelector('[data-setting-id="tunnels.profiles.{id}.port"]')!;
		const top = (element: Element) => getComputedStyle(element.parentElement!).marginTop || "0px";
		expect(top(port)).toBe(top(host));
	});

	it("catches worktree filters flex shrinking below control height", () => {
		componentCss("WorktreeManager");
		css("src/components/ui/FormControls.module.css");
		worktreeManagerStore.open();
		const { getByPlaceholderText } = render(() => <WorktreeManager />);
		const input = getComputedStyle(getByPlaceholderText("Filter worktrees…"));
		expect(input.flexShrink).toBe("0");
		// Story 2316 density contract: desktop toolbar controls are 22px, not 36px form controls.
		expect(parseFloat(input.height)).toBe(22);
	});
});
