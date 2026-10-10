import { render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { DirEntry } from "../../../types/fs";

const { mockInvoke } = vi.hoisted(() => ({ mockInvoke: vi.fn() }));

vi.mock("../../../invoke", async (importOriginal) => {
	const actual = await importOriginal<typeof import("../../../invoke")>();
	return { ...actual, invoke: mockInvoke, listen: () => Promise.resolve(() => {}) };
});

const { FileBrowserPanel } = await import("../../../components/FileBrowserPanel/FileBrowserPanel");

const entry = (name: string, isDir: boolean, path = name): DirEntry => ({
	name,
	path,
	is_dir: isDir,
	size: 0,
	modified_at: 0,
	git_status: "",
	is_ignored: false,
});

let listing: DirEntry[] = [];

beforeEach(() => {
	mockInvoke
		.mockReset()
		.mockImplementation((cmd: string) => Promise.resolve(cmd === "list_directory" ? listing : undefined));
});

afterEach(async () => {
	vi.unstubAllGlobals();
	document.body.innerHTML = "";
	await new Promise((resolve) => setTimeout(resolve, 50));
});

const pointer = (type: string, pointerId: number, x: number, y: number) =>
	new PointerEvent(type, {
		bubbles: true,
		cancelable: true,
		button: 0,
		pointerId,
		pointerType: "touch",
		clientX: x,
		clientY: y,
	});

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));
const renameCalls = () => mockInvoke.mock.calls.filter(([cmd]) => cmd === "rename_path");

const mount = async () => {
	const { container, queryByText } = render(() => (
		<FileBrowserPanel visible={true} repoPath="/repo" onClose={() => {}} onFileOpen={() => {}} />
	));
	await waitFor(() => expect(queryByText(listing[0].name)).not.toBeNull());
	const rows = Array.from(container.querySelectorAll(".entry")) as HTMLElement[];
	return (name: string) => rows.find((r) => r.textContent?.includes(name)) as HTMLElement;
};

/** Hold on `src` at (50,50), move through `path`, release at the last point over `hitAt(x, y)`. */
const holdDrag = async (
	src: HTMLElement,
	path: Array<[number, number]>,
	hitAt: (x: number, y: number) => Element | null,
) => {
	const original = document.elementFromPoint;
	document.elementFromPoint = hitAt;
	try {
		src.dispatchEvent(pointer("pointerdown", 1, 50, 50));
		await sleep(450);
		for (const [x, y] of path) document.dispatchEvent(pointer("pointermove", 1, x, y));
		const [lx, ly] = path.at(-1)! ?? [50, 50];
		document.dispatchEvent(pointer("pointerup", 1, lx, ly));
		await sleep(20);
	} finally {
		document.elementFromPoint = original;
	}
};

describe("FileBrowserPanel touch hold slop (#1329-a31a, round 4 critic)", () => {
	// Catches: the new guards blocking every hold drop (inverted contains check, or `moved`
	// never set) — a deliberate long drag onto another folder must still move the file.
	it("a drag well beyond the slop onto another folder row moves the file", async () => {
		listing = [entry("other", true), entry("a.txt", false, "docs/a.txt")];
		const row = await mount();
		await holdDrag(row("a.txt"), [[50, 80]], () => row("other"));
		expect(renameCalls().map(([, args]) => (args as { to: string }).to)).toEqual(["other/a.txt"]);
	});

	// Catches: slop boundary off by one — 10 px of displacement is still jitter (strict `>`),
	// matching the pre-arm bail-out in initMouseDrag; 11 px is a drag.
	it("10 px of displacement is jitter, 11 px is a drag", async () => {
		listing = [entry("other", true), entry("a.txt", false, "docs/a.txt")];
		const row = await mount();
		await holdDrag(row("a.txt"), [[60, 50]], () => row("other"));
		expect(renameCalls()).toHaveLength(0);

		await holdDrag(row("a.txt"), [[61, 50]], () => row("other"));
		expect(renameCalls()).toHaveLength(1);
	});

	// Catches: `moved` being sticky with no re-check at the release point — a finger that
	// wandered far and came back over its own row would drop the file on the panel root.
	it("a drag that returns to the source row before release moves nothing", async () => {
		listing = [entry("other", true), entry("a.txt", false, "docs/a.txt")];
		const row = await mount();
		await holdDrag(
			row("a.txt"),
			[
				[50, 120],
				[52, 51],
			],
			() => row("a.txt"),
		);
		expect(renameCalls()).toHaveLength(0);
	});

	// Catches: elementFromPoint returning null (finger released outside the viewport) throwing
	// inside Node.contains or leaving the drag state (source opacity) behind.
	it("a release over nothing neither throws nor moves, and the row is restored", async () => {
		listing = [entry("other", true), entry("a.txt", false, "docs/a.txt")];
		const row = await mount();
		await holdDrag(row("a.txt"), [[50, 400]], () => null);
		expect(renameCalls()).toHaveLength(0);
		expect(row("a.txt").style.opacity).toBe("");
	});
});
