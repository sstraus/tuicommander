import { fireEvent, render } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";

// Mock useRepository hook to avoid Tauri invoke calls
vi.mock("../../hooks/useRepository", () => ({
	useRepository: () => ({
		getInfo: vi.fn(),
		getDiff: vi.fn(),
		getDiffStats: vi.fn(),
		openInApp: vi.fn(),
		renameBranch: vi.fn(),
		removeWorktree: vi.fn(),
		createWorktree: vi.fn(),
		getWorktreePaths: vi.fn(),
		listMarkdownFiles: vi.fn().mockResolvedValue([]),
		readFile: vi.fn().mockResolvedValue(""),
		getChangedFiles: vi.fn().mockResolvedValue([]),
		getFileDiff: vi.fn().mockResolvedValue(""),
	}),
}));

// Mock Tauri APIs
vi.mock("@tauri-apps/api/core", () => ({
	invoke: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("@tauri-apps/api/event", () => ({
	listen: vi.fn().mockResolvedValue(vi.fn()),
	emit: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("@tauri-apps/api/window", () => ({
	getCurrentWindow: vi.fn(() => ({
		listen: vi.fn().mockResolvedValue(vi.fn()),
		setTitle: vi.fn().mockResolvedValue(undefined),
	})),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
	open: vi.fn().mockResolvedValue(null),
}));

vi.mock("@tauri-apps/plugin-opener", () => ({
	openUrl: vi.fn().mockResolvedValue(undefined),
}));

import { MarkdownPanel } from "../../components/MarkdownPanel/MarkdownPanel";

describe("MarkdownPanel", () => {
	it("renders markdown panel structure", () => {
		const { container } = render(() => <MarkdownPanel visible={true} repoPath="/test/repo" onClose={() => {}} />);
		const panel = container.querySelector(".panel");
		expect(panel).not.toBeNull();
	});

	it("renders panel title 'Markdown Files'", () => {
		const { container } = render(() => <MarkdownPanel visible={true} repoPath="/test/repo" onClose={() => {}} />);
		const title = container.querySelector(".title");
		expect(title).not.toBeNull();
		expect(title!.textContent).toBe("Markdown Files");
	});

	it("renders close button", () => {
		const { container } = render(() => <MarkdownPanel visible={true} repoPath="/test/repo" onClose={() => {}} />);
		// Located by accessible title, not a styling class — the close control is an
		// SVG button in PanelWindowControls (no `.close` styling hook).
		const closeBtn = container.querySelector('[title="Close"]');
		expect(closeBtn).not.toBeNull();
	});

	it("calls onClose when close button is clicked", () => {
		const handleClose = vi.fn();
		const { container } = render(() => <MarkdownPanel visible={true} repoPath="/test/repo" onClose={handleClose} />);
		const closeBtn = container.querySelector('[title="Close"]')!;
		fireEvent.click(closeBtn);
		expect(handleClose).toHaveBeenCalledOnce();
	});

	it("applies hidden class when not visible", () => {
		const { container } = render(() => <MarkdownPanel visible={false} repoPath={null} onClose={() => {}} />);
		const panel = container.querySelector(".panel");
		expect(panel!.classList.contains("hidden")).toBe(true);
	});

	it("has panel-content container", () => {
		const { container } = render(() => <MarkdownPanel visible={true} repoPath="/test/repo" onClose={() => {}} />);
		const content = container.querySelector(".content");
		expect(content).not.toBeNull();
	});

	it("defaults to filename search mode", () => {
		const { container } = render(() => <MarkdownPanel visible={true} repoPath="/test/repo" onClose={() => {}} />);
		const input = container.querySelector(".searchInput") as HTMLInputElement;
		expect(input).not.toBeNull();
		expect(input.placeholder).toContain("Filter");
		const toggle = container.querySelector("button[aria-pressed]");
		expect(toggle).not.toBeNull();
		expect(toggle).toHaveAttribute("aria-pressed", "false");
	});

	it("toggles to content search mode, switching placeholder and active state", () => {
		const { container } = render(() => <MarkdownPanel visible={true} repoPath="/test/repo" onClose={() => {}} />);
		const toggle = container.querySelector("button[aria-pressed]")!;
		fireEvent.click(toggle);

		const input = container.querySelector(".searchInput") as HTMLInputElement;
		expect(input.placeholder).toContain("contents");
		expect(toggle).toHaveAttribute("aria-pressed", "true");

		// Toggling back restores filename mode.
		fireEvent.click(toggle);
		expect(input.placeholder).toContain("Filter");
		expect(toggle).toHaveAttribute("aria-pressed", "false");
	});
});

it("catches query clear losing the draft or sort selection failing to close the menu", () => {
	const view = render(() => <MarkdownPanel visible repoPath={null} onClose={() => {}} />);
	const input = view.getByRole("textbox") as HTMLInputElement;
	fireEvent.input(input, { target: { value: "*.md" } });
	fireEvent.click(view.getByRole("button", { name: "Clear search" }));
	expect(input.value).toBe("");
	fireEvent.click(view.getByRole("button", { name: "Sort by folder" }));
	fireEvent.click(view.getByText("Date"));
	expect(view.getByRole("button", { name: "Sort by date" })).toBeInTheDocument();
	expect(view.queryByTestId("dropdown")).toBeNull();
});
