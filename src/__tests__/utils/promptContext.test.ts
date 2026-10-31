import { beforeEach, describe, expect, it, vi } from "vitest";
import type { SavedPrompt } from "../../stores/promptLibrary";
import type { BranchPrStatus } from "../../types";

const mocks = vi.hoisted(() => ({
	getSmartByPlacement: vi.fn(),
	markAsUsed: vi.fn(),
	toast: vi.fn(),
	error: vi.fn(),
}));

vi.mock("../../i18n", () => ({ t: (_key: string, fallback: string) => fallback }));
vi.mock("../../stores/promptLibrary", () => ({
	promptLibraryStore: {
		getSmartByPlacement: mocks.getSmartByPlacement,
		markAsUsed: mocks.markAsUsed,
	},
}));
vi.mock("../../stores/toasts", () => ({ toastsStore: { add: mocks.toast } }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { error: mocks.error } }));

import {
	fileContextSmartMenuItem,
	fileContextVariables,
	prContextVariables,
	runWithFeedback,
} from "../../utils/promptContext";

const prompt: SavedPrompt = {
	id: "review-file",
	name: "Review file",
	content: "Review {file_rel_path}",
	category: "custom",
	isFavorite: false,
	createdAt: 1,
	updatedAt: 1,
};

describe("promptContext helpers", () => {
	// Catches: failures before mode dispatch still stringify structured errors.
	it("shows structured dispatch rejections in the feedback toast", async () => {
		mocks.toast.mockClear();
		const failure = { code: "context_error", message: "Cannot resolve prompt context" };
		await runWithFeedback(prompt, async () => {
			throw failure;
		});
		expect(mocks.toast).toHaveBeenCalledWith("Review file", "context_error: Cannot resolve prompt context", "error");
		expect(mocks.error).toHaveBeenCalledWith("prompts", 'Failed to execute "Review file"', failure);
	});

	beforeEach(() => {
		mocks.getSmartByPlacement.mockReset();
		mocks.markAsUsed.mockReset();
		mocks.toast.mockReset();
		mocks.error.mockReset();
	});

	it("builds PR variables, preserving zero counts and omitting unavailable optional context", () => {
		const pr = {
			number: 42,
			title: "Fix parser",
			url: "https://example.test/pr/42",
			state: "OPEN",
			mergeable: "MERGEABLE",
			review_decision: "",
			checks: { passed: 0, failed: 1, pending: 2 },
			author: "octo",
			labels: [{ name: "bug" }, { name: "urgent" }],
			additions: 0,
			deletions: 0,
		} as BranchPrStatus;

		expect(prContextVariables(pr)).toEqual({
			pr_number: "42",
			pr_title: "Fix parser",
			pr_url: "https://example.test/pr/42",
			pr_state: "OPEN",
			merge_status: "MERGEABLE",
			review_decision: "",
			pr_checks: "0 passed, 1 failed, 2 pending",
			pr_author: "octo",
			pr_labels: "bug, urgent",
			pr_additions: "0",
			pr_deletions: "0",
		});
	});

	it("derives file variables without treating a path with a shared prefix as repo-relative", () => {
		expect(fileContextVariables({ absPath: "/work/repo-other/.env", repoRoot: "/work/repo", isDir: true })).toEqual({
			file_path: "/work/repo-other/.env",
			file_rel_path: "/work/repo-other/.env",
			file_name: ".env",
			file_ext: "",
			file_dir: "/work/repo-other",
			file_is_dir: "true",
		});
	});

	it("omits the menu when no file-context prompts are registered", () => {
		mocks.getSmartByPlacement.mockReturnValue([]);

		expect(
			fileContextSmartMenuItem({ absPath: "/work/repo/src/main.ts" }, { executeSmartPrompt: vi.fn() } as never),
		).toBeNull();
		expect(mocks.getSmartByPlacement).toHaveBeenCalledWith("file-context");
	});

	it("marks the prompt used and reports an unsuccessful execution through a warning toast", async () => {
		mocks.getSmartByPlacement.mockReturnValue([prompt]);
		const executeSmartPrompt = vi.fn().mockResolvedValue({ ok: false, reason: "No idle terminal" });
		const menu = fileContextSmartMenuItem(
			{ absPath: "/work/repo/src/main.ts", repoRoot: "/work/repo" },
			{ executeSmartPrompt } as never,
			{ separator: true },
		);

		expect(menu).toMatchObject({ label: "Smart Prompts", separator: true });
		menu?.children?.[0].action();
		await vi.waitFor(() => expect(mocks.toast).toHaveBeenCalledWith("Review file", "No idle terminal", "warn"));
		expect(mocks.markAsUsed).toHaveBeenCalledWith("review-file");
		expect(executeSmartPrompt).toHaveBeenCalledWith(prompt, {
			file_path: "/work/repo/src/main.ts",
			file_rel_path: "src/main.ts",
			file_name: "main.ts",
			file_ext: ".ts",
			file_dir: "/work/repo/src",
			file_is_dir: "false",
		});
	});

	it("contains an execution rejection, logs it, and surfaces an error toast", async () => {
		mocks.getSmartByPlacement.mockReturnValue([prompt]);
		const failure = new Error("agent unavailable");
		const menu = fileContextSmartMenuItem({ absPath: "/work/repo/file.txt" }, {
			executeSmartPrompt: vi.fn().mockRejectedValue(failure),
		} as never);

		menu?.children?.[0].action();
		await vi.waitFor(() =>
			expect(mocks.toast).toHaveBeenCalledWith("Review file", "Error: agent unavailable", "error"),
		);
		expect(mocks.error).toHaveBeenCalledWith("prompts", 'Failed to execute "Review file"', failure);
	});
});
