// @vitest-environment jsdom

import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CommentOverlay } from "../../components/MarkdownTab/CommentOverlay";

afterEach(() => cleanup());

describe("CommentOverlay block gutter", () => {
	// Catches: an item-relative action covering markers/checkboxes as list indentation changes.
	it("keeps the list hover action outside the outer list at every nesting depth", () => {
		const host = document.createElement("div");
		const content = document.createElement("div");
		host.append(content);
		document.body.append(host);
		const rect = (left: number, top: number) =>
			({
				left,
				right: 600,
				top,
				bottom: top + 20,
				width: 600 - left,
				height: 20,
				x: left,
				y: top,
				toJSON() {},
			}) as DOMRect;
		const list = document.createElement("ol");
		list.getBoundingClientRect = () => rect(100, 20);
		content.append(list);
		const items: HTMLElement[] = [];
		let parent: HTMLElement = list;
		for (let depth = 0; depth < 3; depth++) {
			const item = document.createElement("li");
			item.dataset.commentSourceStart = String(depth * 10);
			item.dataset.commentSourceEnd = String(depth * 10 + 8);
			item.textContent = "Task";
			item.getBoundingClientRect = () => rect(140 + depth * 40, 20 + depth * 30);
			parent.append(item);
			items.push(item);
			parent = document.createElement("ul");
			item.append(parent);
		}
		render(() => <CommentOverlay contentRef={content} onSave={vi.fn()} onDelete={vi.fn()} />);
		for (const item of items) {
			const box = item.getBoundingClientRect();
			fireEvent.mouseMove(host, { clientX: box.left - 20, clientY: box.top + 5 });
			const button = document.body.querySelector<HTMLButtonElement>('[aria-label="Comment on this block"]')!;
			expect.soft(parseFloat(button.style.left)).toBeLessThan(100);
			expect.soft(button.style.transform).toBe("translateX(-100%)");
			// Moving from the item to the shared gutter must not dismiss the action.
			fireEvent.mouseMove(host, { clientX: 98, clientY: box.top + 5 });
			expect(document.body.querySelector('[aria-label="Comment on this block"]')).not.toBeNull();
		}
	});

	// Catches: a list-only placement fix also moving paragraph/heading actions.
	it.each(["p", "h2"])("retains the current %s hover action position", (tag) => {
		const host = document.createElement("div");
		const content = document.createElement("div");
		const block = document.createElement(tag);
		block.dataset.commentSourceStart = "0";
		block.dataset.commentSourceEnd = "5";
		block.getBoundingClientRect = () => ({ left: 100, right: 500, top: 20, bottom: 50 }) as DOMRect;
		content.append(block);
		host.append(content);
		document.body.append(host);
		render(() => <CommentOverlay contentRef={content} onSave={vi.fn()} onDelete={vi.fn()} />);
		fireEvent.mouseMove(host, { clientX: 80, clientY: 25 });
		const button = document.body.querySelector<HTMLButtonElement>('[aria-label="Comment on this block"]')!;
		expect(button.style.left).toBe("70px");
		expect(button.style.top).toBe("22px");
	});

	// Catches: a parent's saved comment swallowing clicks on an independently commentable child.
	it("does not open a parent item comment when its nested item is clicked", () => {
		const content = document.createElement("div");
		const parent = document.createElement("li");
		parent.className = "tweak-block-highlight";
		parent.dataset.tweakId = "c_parent";
		parent.dataset.tweakComment = "Parent note";
		parent.dataset.tweakAt = "2026-09-27T12:00:00.000Z";
		const child = document.createElement("li");
		child.textContent = "child";
		parent.append(child);
		content.append(parent);
		document.body.append(content);
		render(() => <CommentOverlay contentRef={content} onSave={vi.fn()} onDelete={vi.fn()} />);
		fireEvent.click(child);
		expect(document.body.querySelector("textarea")).toBeNull();
	});
	// Catches: an overlapping parent <li> stealing the nested item's gutter hover.
	it("saves a nested bullet rather than its parent when hovering the nested gutter", () => {
		const scrollHost = document.createElement("div");
		const content = document.createElement("div");
		const list = document.createElement("ul");
		const parent = document.createElement("li");
		parent.textContent = "parent";
		parent.dataset.commentSourceStart = "0";
		parent.dataset.commentSourceEnd = "8";
		const nested = document.createElement("ul");
		const child = document.createElement("li");
		child.textContent = "child";
		child.dataset.commentSourceStart = "11";
		child.dataset.commentSourceEnd = "20";
		nested.append(child);
		parent.append(nested);
		list.append(parent);
		content.append(list);
		scrollHost.append(content);
		document.body.append(scrollHost);
		parent.getBoundingClientRect = () =>
			({ left: 100, right: 500, top: 20, bottom: 100, width: 400, height: 80, x: 100, y: 20, toJSON() {} }) as DOMRect;
		child.getBoundingClientRect = () =>
			({ left: 120, right: 500, top: 60, bottom: 80, width: 380, height: 20, x: 120, y: 60, toJSON() {} }) as DOMRect;
		const onSaveBlock = vi.fn();
		render(() => (
			<CommentOverlay
				contentRef={content}
				onSave={vi.fn()}
				onSaveBlock={onSaveBlock}
				blockSource={() => "  - child"}
				onDelete={vi.fn()}
			/>
		));
		fireEvent.mouseMove(scrollHost, { clientX: 96, clientY: 65 });
		fireEvent.mouseDown(document.body.querySelector('[aria-label="Comment on this block"]')!);
		fireEvent.input(document.body.querySelector("textarea")!, { target: { value: "Fix child" } });
		fireEvent.click(Array.from(document.body.querySelectorAll("button")).find((el) => el.textContent === "Save")!);
		expect(onSaveBlock).toHaveBeenCalledWith(expect.objectContaining({ comment: "Fix child" }), { start: 11, end: 20 });
	});
	it("does not open the block comment when an interactive checkbox inside it is clicked", () => {
		const scrollHost = document.createElement("div");
		const content = document.createElement("div");
		const list = document.createElement("ul");
		list.className = "tweak-block-highlight";
		list.dataset.tweakId = "c_tasks";
		list.dataset.tweakComment = "Review these tasks";
		list.dataset.tweakAt = "2026-09-23T08:00:00.000Z";
		const checkbox = document.createElement("input");
		checkbox.type = "checkbox";
		list.append(checkbox);
		content.append(list);
		scrollHost.append(content);
		document.body.append(scrollHost);

		render(() => <CommentOverlay contentRef={content} onSave={vi.fn()} onDelete={vi.fn()} />);
		fireEvent.click(checkbox);

		expect(document.body.querySelector("textarea")).toBeNull();
	});

	// A drag-select ends with a click on the same element. Opening the view
	// popover then would make text inside a commented block unselectable.
	it("does not open the comment popover when the click ends a text selection", () => {
		const content = document.createElement("div");
		const paragraph = document.createElement("p");
		paragraph.className = "tweak-block-highlight";
		paragraph.dataset.tweakId = "c_para";
		paragraph.dataset.tweakComment = "Tighten this";
		paragraph.dataset.tweakAt = "2026-09-23T08:00:00.000Z";
		paragraph.textContent = "Some commented paragraph text.";
		content.append(paragraph);
		document.body.append(content);

		render(() => <CommentOverlay contentRef={content} onSave={vi.fn()} onDelete={vi.fn()} />);
		const range = document.createRange();
		range.setStart(paragraph.firstChild!, 5);
		range.setEnd(paragraph.firstChild!, 14);
		window.getSelection()!.removeAllRanges();
		window.getSelection()!.addRange(range);
		fireEvent.click(paragraph);
		expect(document.body.querySelector("textarea")).toBeNull();

		window.getSelection()!.removeAllRanges();
		fireEvent.click(paragraph);
		expect(document.body.querySelector("textarea")).not.toBeNull();
	});

	// Top-level blocks stack in document order, so a gutter hover on a long
	// document must not read the layout of every block on each mousemove.
	it("finds the hovered block with a logarithmic number of layout reads", () => {
		const scrollHost = document.createElement("div");
		const content = document.createElement("div");
		let layoutReads = 0;
		for (let i = 0; i < 2000; i++) {
			const block = document.createElement("p");
			block.textContent = `Block ${i}`;
			block.dataset.commentSourceStart = String(i * 10);
			block.dataset.commentSourceEnd = String(i * 10 + 7);
			block.getBoundingClientRect = () => {
				layoutReads++;
				const top = i * 20;
				return {
					left: 100,
					right: 500,
					top,
					bottom: top + 18,
					width: 400,
					height: 18,
					x: 100,
					y: top,
					toJSON() {},
				} as DOMRect;
			};
			content.append(block);
		}
		scrollHost.append(content);
		document.body.append(scrollHost);

		const onSaveBlock = vi.fn();
		render(() => (
			<CommentOverlay
				contentRef={content}
				onSave={vi.fn()}
				onSaveBlock={onSaveBlock}
				blockSource={() => "raw"}
				onDelete={vi.fn()}
			/>
		));
		fireEvent.mouseMove(scrollHost, { clientX: 76, clientY: 1500 * 20 + 5 });

		expect(layoutReads).toBeLessThan(30);
		fireEvent.mouseDown(document.body.querySelector('[aria-label="Comment on this block"]')!);
		fireEvent.input(document.body.querySelector("textarea")!, { target: { value: "c" } });
		fireEvent.click(Array.from(document.body.querySelectorAll("button")).find((el) => el.textContent === "Save")!);
		expect(onSaveBlock).toHaveBeenCalledWith(expect.anything(), { start: 15000, end: 15007 });
	});

	it("opens a comment for the exact source range when the gutter icon is clicked", () => {
		const scrollHost = document.createElement("div");
		const content = document.createElement("div");
		const paragraph = document.createElement("p");
		paragraph.textContent = "Rendered formatted paragraph.";
		paragraph.dataset.commentSourceStart = "18";
		paragraph.dataset.commentSourceEnd = "61";
		content.append(paragraph);
		scrollHost.append(content);
		document.body.append(scrollHost);
		paragraph.getBoundingClientRect = () =>
			({ left: 100, right: 500, top: 80, bottom: 120, width: 400, height: 40, x: 100, y: 80, toJSON() {} }) as DOMRect;
		scrollHost.getBoundingClientRect = () =>
			({ left: 60, right: 600, top: 20, bottom: 500, width: 540, height: 480, x: 60, y: 20, toJSON() {} }) as DOMRect;

		const onSaveBlock = vi.fn();
		let source = `${"x".repeat(18)}Rendered **formatted** paragraph.`;
		const blockSource = (range: { start: number; end: number }) => source.slice(range.start, range.end);
		render(() => (
			<CommentOverlay
				contentRef={content}
				onSave={vi.fn()}
				onSaveBlock={onSaveBlock}
				blockSource={blockSource}
				onDelete={vi.fn()}
			/>
		));

		fireEvent.mouseMove(scrollHost, { clientX: 76, clientY: 96 });
		const button = document.body.querySelector('[aria-label="Comment on this block"]') as HTMLButtonElement;
		expect(button).not.toBeNull();
		fireEvent.mouseDown(button);
		fireEvent.input(document.body.querySelector("textarea")!, { target: { value: "Clarify this block" } });
		// The file changes while the user types: the save must carry the source
		// seen when the popover opened, so the caller can detect the move.
		source = `An agent edit. ${source}`;
		fireEvent.click(Array.from(document.body.querySelectorAll("button")).find((el) => el.textContent === "Save")!);

		expect(onSaveBlock).toHaveBeenCalledWith(
			expect.objectContaining({ comment: "Clarify this block", highlighted: "Rendered **formatted** paragraph." }),
			{ start: 18, end: 61 },
		);
	});
});

describe("CommentOverlay edit actions", () => {
	function mountComment() {
		const content = document.createElement("div");
		const highlight = document.createElement("span");
		highlight.className = "tweak-highlight";
		highlight.textContent = "Selected text";
		highlight.dataset.tweakId = "c_edit";
		highlight.dataset.tweakComment = "Original note";
		highlight.dataset.tweakAt = "2026-09-27T12:00:00.000Z";
		content.append(highlight);
		document.body.append(content);
		const onSave = vi.fn().mockResolvedValue(false);
		const onDelete = vi.fn().mockResolvedValue(false);
		render(() => <CommentOverlay contentRef={content} onSave={onSave} onDelete={onDelete} />);
		window.getSelection()!.removeAllRanges();
		fireEvent.click(highlight);
		const textarea = document.body.querySelector("textarea")!;
		const button = (name: string) =>
			Array.from(document.body.querySelectorAll("button")).find((el) => el.textContent === name)!;
		return { textarea, button, onSave, onDelete };
	}

	// Catches: a primitive replacement dropping disabled forwarding or the textarea input handler.
	it("disables blank save and restores it when the draft changes", () => {
		const { textarea, button, onSave } = mountComment();
		fireEvent.input(textarea, { target: { value: "   " } });
		expect(button("Save").disabled).toBe(true);
		button("Save").click();
		expect(onSave).not.toHaveBeenCalled();
		fireEvent.input(textarea, { target: { value: "Clarify" } });
		expect(button("Save").disabled).toBe(false);
		fireEvent.click(button("Cancel"));
		expect(document.body.querySelector("textarea")).toBeNull();
		expect(onSave).not.toHaveBeenCalled();
	});

	// Catches: shared controls swallowing bubbled shortcuts or closing rejected saves/deletes.
	it("keeps a rejected edit/delete open and preserves keyboard save and Escape", async () => {
		const { textarea, button, onSave, onDelete } = mountComment();
		fireEvent.input(textarea, { target: { value: "Clarify" } });
		fireEvent.keyDown(textarea, { key: "Enter", ctrlKey: true });
		await Promise.resolve();
		expect(onSave).toHaveBeenCalledWith(
			{ id: "c_edit", highlighted: "Selected text", comment: "Clarify", createdAt: "2026-09-27T12:00:00.000Z" },
			0,
		);
		expect(document.body.querySelector("textarea")).toBe(textarea);
		fireEvent.click(button("Delete"));
		await Promise.resolve();
		expect(onDelete).toHaveBeenCalledWith("c_edit");
		expect(document.body.querySelector("textarea")).toBe(textarea);
		fireEvent.keyDown(textarea, { key: "Escape" });
		expect(document.body.querySelector("textarea")).toBeNull();
	});
});
