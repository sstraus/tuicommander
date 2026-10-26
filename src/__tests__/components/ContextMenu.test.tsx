import { fireEvent, render } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { describe, expect, it, vi } from "vitest";
import type { ContextMenuItem } from "../../components/ContextMenu/ContextMenu";
import { ContextMenu, createContextMenu } from "../../components/ContextMenu/ContextMenu";
import { testInScope, testInScopeAsync } from "../helpers/store";

const sampleItems: ContextMenuItem[] = [
	{ label: "Copy", shortcut: "\u2318C", action: vi.fn() },
	{ label: "Paste", shortcut: "\u2318V", action: vi.fn() },
	{ label: "Delete", action: vi.fn(), disabled: true },
];

describe("ContextMenu", () => {
	it("renders nothing when not visible", () => {
		const { container } = render(() => (
			<ContextMenu items={sampleItems} x={100} y={200} visible={false} onClose={() => {}} />
		));
		const menu = container.querySelector(".menu");
		expect(menu).toBeNull();
	});

	it("renders menu items when visible", () => {
		const { container } = render(() => (
			<ContextMenu items={sampleItems} x={100} y={200} visible={true} onClose={() => {}} />
		));
		const items = container.querySelectorAll(".item");
		expect(items.length).toBe(3);
	});

	it("renders labels correctly", () => {
		const { container } = render(() => (
			<ContextMenu items={sampleItems} x={0} y={0} visible={true} onClose={() => {}} />
		));
		const labels = container.querySelectorAll(".label");
		expect(labels[0].textContent).toBe("Copy");
		expect(labels[1].textContent).toBe("Paste");
		expect(labels[2].textContent).toBe("Delete");
	});

	it("renders shortcuts when provided", () => {
		const { container } = render(() => (
			<ContextMenu items={sampleItems} x={0} y={0} visible={true} onClose={() => {}} />
		));
		const shortcuts = container.querySelectorAll(".shortcut");
		expect(shortcuts.length).toBe(2); // Copy and Paste have shortcuts
		expect(shortcuts[0].textContent).toBe("\u2318C");
	});

	it("fires action and closes on item click", () => {
		const action = vi.fn();
		const handleClose = vi.fn();
		const items: ContextMenuItem[] = [{ label: "Run", action }];
		const { container } = render(() => <ContextMenu items={items} x={0} y={0} visible={true} onClose={handleClose} />);
		const btn = container.querySelector(".item")!;
		fireEvent.click(btn);
		expect(action).toHaveBeenCalledOnce();
		expect(handleClose).toHaveBeenCalledOnce();
	});

	it("does not fire action on disabled item click", () => {
		const action = vi.fn();
		const items: ContextMenuItem[] = [{ label: "Disabled", action, disabled: true }];
		const { container } = render(() => <ContextMenu items={items} x={0} y={0} visible={true} onClose={() => {}} />);
		const btn = container.querySelector(".item")!;
		fireEvent.click(btn);
		expect(action).not.toHaveBeenCalled();
	});

	it("renders the item AND a trailing divider when a real item sets separator", () => {
		const items: ContextMenuItem[] = [
			{ label: "Above", action: vi.fn(), separator: true },
			{ label: "Below", action: vi.fn() },
		];
		const { container } = render(() => <ContextMenu items={items} x={0} y={0} visible={true} onClose={() => {}} />);
		// `separator` on a real item is a trailing-divider MODIFIER — "Above"
		// must still render its button, not be replaced by the divider.
		const labels = Array.from(container.querySelectorAll(".label")).map((el) => el.textContent);
		expect(labels).toEqual(["Above", "Below"]);
		expect(container.querySelectorAll(".separator").length).toBe(1);
	});

	it("separator renders only a divider — no selectable button row", () => {
		const items: ContextMenuItem[] = [
			{ label: "Top", action: vi.fn() },
			{ label: "", action: vi.fn(), separator: true },
			{ label: "Bottom", action: vi.fn() },
		];
		const { container } = render(() => <ContextMenu items={items} x={0} y={0} visible={true} onClose={() => {}} />);
		// Two real items only — the separator must NOT produce a hoverable .item button.
		expect(container.querySelectorAll(".item").length).toBe(2);
		expect(container.querySelectorAll(".itemWrap").length).toBe(2);
		expect(container.querySelectorAll(".separator").length).toBe(1);
	});

	it("a real item with separator renders its label plus a trailing divider", () => {
		// Regression guard: commit 2f032261 treated `separator` as exclusive
		// (divider OR item), silently dropping every FileBrowser item that
		// requested a trailing divider — New File, Paste, Delete, Add to
		// .gitignore all vanished. A non-empty label must ALWAYS render.
		const items: ContextMenuItem[] = [
			{ label: "Delete", action: vi.fn(), separator: true },
			{ label: "Reveal", action: vi.fn() },
		];
		const { container } = render(() => <ContextMenu items={items} x={0} y={0} visible={true} onClose={() => {}} />);
		const labels = Array.from(container.querySelectorAll(".label")).map((el) => el.textContent);
		expect(labels).toEqual(["Delete", "Reveal"]);
		expect(container.querySelectorAll(".separator").length).toBe(1);
	});

	it("suppresses a trailing separator on the LAST item (no dangling divider)", () => {
		// Every context menu renders through this component; a modifier separator
		// on the final item must NOT paint a divider with nothing after it.
		const items: ContextMenuItem[] = [
			{ label: "First", action: vi.fn() },
			{ label: "Last", action: vi.fn(), separator: true },
		];
		const { container } = render(() => <ContextMenu items={items} x={0} y={0} visible={true} onClose={() => {}} />);
		const labels = Array.from(container.querySelectorAll(".label")).map((el) => el.textContent);
		expect(labels).toEqual(["First", "Last"]);
		expect(container.querySelectorAll(".separator").length).toBe(0);
	});

	it("suppresses a pure separator row when it is the LAST item", () => {
		const items: ContextMenuItem[] = [
			{ label: "Only", action: vi.fn() },
			{ label: "", action: vi.fn(), separator: true },
		];
		const { container } = render(() => <ContextMenu items={items} x={0} y={0} visible={true} onClose={() => {}} />);
		expect(container.querySelectorAll(".item").length).toBe(1);
		expect(container.querySelectorAll(".separator").length).toBe(0);
	});

	it("closes on Escape key", () => {
		const handleClose = vi.fn();
		render(() => <ContextMenu items={sampleItems} x={0} y={0} visible={true} onClose={handleClose} />);
		fireEvent.keyDown(document, { key: "Escape" });
		expect(handleClose).toHaveBeenCalled();
	});

	it("closes on click outside menu", () => {
		const handleClose = vi.fn();
		render(() => (
			<div>
				<div data-testid="outside">Outside</div>
				<ContextMenu items={sampleItems} x={0} y={0} visible={true} onClose={handleClose} />
			</div>
		));
		fireEvent.mouseDown(document.body);
		expect(handleClose).toHaveBeenCalled();
	});

	// Catches: fadeIn's scale(0.95) underestimates a tall popup's final box.
	it("clamps the full layout box at the bottom edge during the opening animation", async () => {
		const [visible, setVisible] = createSignal(false);
		const rect = vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
			width: 190,
			height: 712.5,
			left: 0,
			top: 0,
			right: 190,
			bottom: 712.5,
			x: 0,
			y: 0,
			toJSON: () => ({}),
		});
		const height = vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(750);
		const width = vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(200);
		try {
			const { container } = render(() => (
				<ContextMenu
					items={sampleItems}
					x={window.innerWidth - 2}
					y={window.innerHeight - 2}
					visible={visible()}
					onClose={() => {}}
				/>
			));
			setVisible(true);
			await new Promise((resolve) => requestAnimationFrame(resolve));
			const menu = container.querySelector(".menu") as HTMLElement;
			expect(Number.parseFloat(menu.style.top) + 750).toBeLessThanOrEqual(window.innerHeight - 8);
			expect(Number.parseFloat(menu.style.left) + 200).toBeLessThanOrEqual(window.innerWidth - 8);
			expect(menu.style.opacity).toBe("1");
		} finally {
			rect.mockRestore();
			height.mockRestore();
			width.mockRestore();
		}
	});

	it("reclamps an already-open menu when another right click moves its anchor", async () => {
		// Catches: x/y update inline styles but fail to schedule the viewport clamp.
		const [position, setPosition] = createSignal({ x: 20, y: 20 });
		const { container } = render(() => (
			<ContextMenu items={sampleItems} x={position().x} y={position().y} visible={true} onClose={() => {}} />
		));
		await new Promise((resolve) => requestAnimationFrame(resolve));
		setPosition({ x: window.innerWidth - 2, y: window.innerHeight - 2 });
		await new Promise((resolve) => requestAnimationFrame(resolve));
		const menu = container.querySelector(".menu") as HTMLElement;
		expect(Number.parseFloat(menu.style.left) + 180).toBeLessThanOrEqual(window.innerWidth - 8);
		expect(Number.parseFloat(menu.style.top) + sampleItems.length * 36 + 8).toBeLessThanOrEqual(window.innerHeight - 8);
	});

	it("clamps position to viewport bounds", async () => {
		// Set a small viewport
		Object.defineProperty(window, "innerWidth", { value: 200, writable: true, configurable: true });
		Object.defineProperty(window, "innerHeight", { value: 100, writable: true, configurable: true });

		const { container } = render(() => (
			<ContextMenu items={[{ label: "Test", action: vi.fn() }]} x={190} y={90} visible={true} onClose={() => {}} />
		));
		// Position is adjusted after a requestAnimationFrame
		await new Promise((r) => requestAnimationFrame(r));
		const menu = container.querySelector(".menu") as HTMLElement;
		// x should be clamped: 200 - 180 - 8 = 12
		expect(parseInt(menu.style.left, 10)).toBeLessThan(190);
		// y should be clamped (menu grows upward from click point)
		expect(parseInt(menu.style.top, 10)).toBeLessThan(90);

		// Restore
		Object.defineProperty(window, "innerWidth", { value: 1024, writable: true, configurable: true });
		Object.defineProperty(window, "innerHeight", { value: 768, writable: true, configurable: true });
	});

	it("constrains an oversized menu to the visible viewport", async () => {
		Object.defineProperty(window, "innerWidth", { value: 120, writable: true, configurable: true });
		Object.defineProperty(window, "innerHeight", { value: 80, writable: true, configurable: true });

		try {
			const items: ContextMenuItem[] = Array.from({ length: 20 }, (_, i) => ({
				label: `Item ${i}`,
				action: vi.fn(),
			}));
			const { container } = render(() => (
				<ContextMenu items={items} x={110} y={70} visible={true} onClose={() => {}} />
			));

			await new Promise((r) => requestAnimationFrame(r));
			const menu = container.querySelector(".menu") as HTMLElement;

			expect(menu.style.left).toBe("8px");
			expect(menu.style.top).toBe("8px");
			expect(menu.style.maxWidth).toBe("104px");
			expect(menu.style.maxHeight).toBe("64px");
		} finally {
			Object.defineProperty(window, "innerWidth", { value: 1024, writable: true, configurable: true });
			Object.defineProperty(window, "innerHeight", { value: 768, writable: true, configurable: true });
		}
	});

	it("positions menu at x,y coordinates", () => {
		const { container } = render(() => (
			<ContextMenu items={[{ label: "Test", action: vi.fn() }]} x={150} y={250} visible={true} onClose={() => {}} />
		));
		const menu = container.querySelector(".menu") as HTMLElement;
		expect(menu.style.left).toBe("150px");
		expect(menu.style.top).toBe("250px");
	});
});

describe("ContextMenu submenus", () => {
	it("items with children render submenu arrow indicator", () => {
		const items: ContextMenuItem[] = [
			{ label: "Normal", action: vi.fn() },
			{
				label: "Move to Group",
				action: vi.fn(),
				children: [
					{ label: "Work", action: vi.fn() },
					{ label: "Personal", action: vi.fn() },
				],
			},
		];
		const { container } = render(() => <ContextMenu items={items} x={0} y={0} visible={true} onClose={() => {}} />);
		const arrows = container.querySelectorAll(".arrow");
		expect(arrows.length).toBe(1);
	});

	it("hovering parent shows submenu", async () => {
		const items: ContextMenuItem[] = [
			{
				label: "Move to Group",
				action: vi.fn(),
				children: [{ label: "Work", action: vi.fn() }],
			},
		];
		const { container } = render(() => <ContextMenu items={items} x={0} y={0} visible={true} onClose={() => {}} />);
		const parentWrap = container.querySelector(".itemWrap")!;
		fireEvent.mouseEnter(parentWrap);
		const submenu = container.querySelector(".submenu");
		expect(submenu).not.toBeNull();
	});

	it("clicking submenu item fires action and closes all menus", () => {
		const childAction = vi.fn();
		const handleClose = vi.fn();
		const items: ContextMenuItem[] = [
			{
				label: "Move to Group",
				action: vi.fn(),
				children: [{ label: "Work", action: childAction }],
			},
		];
		const { container } = render(() => <ContextMenu items={items} x={0} y={0} visible={true} onClose={handleClose} />);
		// Show submenu
		const parentWrap = container.querySelector(".itemWrap")!;
		fireEvent.mouseEnter(parentWrap);
		// Click submenu item
		const submenuItem = container.querySelector(".submenu .item")!;
		fireEvent.click(submenuItem);
		expect(childAction).toHaveBeenCalledOnce();
		expect(handleClose).toHaveBeenCalledOnce();
	});

	it("disabled item with children does not open submenu on hover", () => {
		const items: ContextMenuItem[] = [
			{
				label: "Agents",
				action: vi.fn(),
				disabled: true,
				children: [{ label: "Claude Code", action: vi.fn() }],
			},
		];
		const { container } = render(() => <ContextMenu items={items} x={0} y={0} visible={true} onClose={() => {}} />);
		const parentWrap = container.querySelector(".itemWrap")!;
		fireEvent.mouseEnter(parentWrap);
		const submenu = container.querySelector(".submenu");
		expect(submenu).toBeNull();
	});

	it("clicking parent item with children does not fire parent action", () => {
		const parentAction = vi.fn();
		const items: ContextMenuItem[] = [
			{
				label: "Move to Group",
				action: parentAction,
				children: [{ label: "Work", action: vi.fn() }],
			},
		];
		const { container } = render(() => <ContextMenu items={items} x={0} y={0} visible={true} onClose={() => {}} />);
		const parentItem = container.querySelector(".item")!;
		fireEvent.click(parentItem);
		expect(parentAction).not.toHaveBeenCalled();
	});

	it("constrains oversized submenus to the visible viewport", async () => {
		Object.defineProperty(window, "innerWidth", { value: 180, writable: true, configurable: true });
		Object.defineProperty(window, "innerHeight", { value: 100, writable: true, configurable: true });
		const originalRect = HTMLElement.prototype.getBoundingClientRect;
		try {
			HTMLElement.prototype.getBoundingClientRect = function () {
				const el = this as HTMLElement;
				if (el.className.includes("itemWrap")) {
					return {
						left: 140,
						right: 170,
						top: 80,
						bottom: 104,
						width: 30,
						height: 24,
						x: 140,
						y: 80,
						toJSON: () => ({}),
					} as DOMRect;
				}
				if (el.className.includes("submenu")) {
					return {
						left: 0,
						right: 220,
						top: 0,
						bottom: 220,
						width: 220,
						height: 220,
						x: 0,
						y: 0,
						toJSON: () => ({}),
					} as DOMRect;
				}
				return originalRect.call(this);
			};

			const items: ContextMenuItem[] = [
				{
					label: "Move to Group",
					action: vi.fn(),
					children: Array.from({ length: 20 }, (_, i) => ({ label: `Group ${i}`, action: vi.fn() })),
				},
			];
			const { container } = render(() => <ContextMenu items={items} x={0} y={0} visible={true} onClose={() => {}} />);
			fireEvent.mouseEnter(container.querySelector(".itemWrap")!);
			await new Promise((r) => requestAnimationFrame(r));

			const submenu = container.querySelector(".submenu") as HTMLElement;
			expect(submenu.style.left).toBe("8px");
			expect(submenu.style.top).toBe("8px");
			expect(submenu.style.maxWidth).toBe("164px");
			expect(submenu.style.maxHeight).toBe("84px");
		} finally {
			HTMLElement.prototype.getBoundingClientRect = originalRect;
			Object.defineProperty(window, "innerWidth", { value: 1024, writable: true, configurable: true });
			Object.defineProperty(window, "innerHeight", { value: 768, writable: true, configurable: true });
		}
	});
});

describe("createContextMenu", () => {
	it("initializes with visible=false", () => {
		testInScope(() => {
			const menu = createContextMenu();
			expect(menu.visible()).toBe(false);
			expect(menu.position().x).toBe(0);
			expect(menu.position().y).toBe(0);
		});
	});

	it("open sets visible and position from mouse event", () => {
		testInScope(() => {
			const menu = createContextMenu();
			const mockEvent = {
				preventDefault: vi.fn(),
				clientX: 300,
				clientY: 400,
			} as unknown as MouseEvent;

			menu.open(mockEvent);

			expect(menu.visible()).toBe(true);
			expect(menu.position().x).toBe(300);
			expect(menu.position().y).toBe(400);
			expect(mockEvent.preventDefault).toHaveBeenCalled();
		});
	});

	it("openAt sets visible and position from coordinates", () => {
		testInScope(() => {
			const menu = createContextMenu();
			menu.openAt(250, 350);

			expect(menu.visible()).toBe(true);
			expect(menu.position().x).toBe(250);
			expect(menu.position().y).toBe(350);
		});
	});

	it("close sets visible to false", async () => {
		testInScope(() => {
			const menu = createContextMenu();
			const mockEvent = {
				preventDefault: vi.fn(),
				clientX: 100,
				clientY: 100,
			} as unknown as MouseEvent;

			menu.open(mockEvent);
			expect(menu.visible()).toBe(true);

			menu.close();
			expect(menu.visible()).toBe(false);
		});
		// Flush happy-dom's setImmediate-based requestAnimationFrame from close()
		await new Promise((r) => setImmediate(r));
	});

	it("close restores focus to the element that was focused when the menu opened", async () => {
		const opener = document.createElement("button");
		document.body.appendChild(opener);
		try {
			opener.focus();
			// Async scope: disposing it would cancel close()'s pending frame.
			await testInScopeAsync(async () => {
				const menu = createContextMenu();
				menu.openAt(10, 10);
				opener.blur();
				menu.close();
				await new Promise((r) => setImmediate(r));
			});
			expect(document.activeElement).toBe(opener);
		} finally {
			opener.remove();
		}
	});

	it("close does NOT steal focus from a dialog opened by the clicked menu item", async () => {
		const opener = document.createElement("button");
		const dialogInput = document.createElement("input");
		document.body.append(opener, dialogInput);
		try {
			opener.focus();
			await testInScopeAsync(async () => {
				const menu = createContextMenu();
				menu.openAt(10, 10);
				// The item's action opens a dialog that focuses its own input.
				dialogInput.focus();
				menu.close();
				await new Promise((r) => setImmediate(r));
			});
			expect(document.activeElement).toBe(dialogInput);
		} finally {
			opener.remove();
			dialogInput.remove();
		}
	});
});
