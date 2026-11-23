import { fireEvent } from "@solidjs/testing-library";

const triggers = (container: HTMLElement) =>
	Array.from(container.querySelectorAll<HTMLButtonElement>('button[aria-label="More actions"]'));

const openItem = (trigger: HTMLButtonElement, label: string) => {
	for (const other of document.querySelectorAll<HTMLButtonElement>('button[aria-label="More actions"]')) {
		if (other !== trigger && other.getAttribute("aria-expanded") === "true") fireEvent.click(other);
	}
	if (trigger.getAttribute("aria-expanded") !== "true") fireEvent.click(trigger);
	const item = Array.from(document.querySelectorAll<HTMLElement>('[role="menuitem"]')).find(
		(el) => el.textContent?.trim() === label,
	);
	return item;
};

/** Rows (in order) whose `⋯` menu offers an item with this label. Leaves every menu closed. */
export function rowsOfferingMenuItem(container: HTMLElement, label: string): number[] {
	return triggers(container).flatMap((trigger, row) => {
		const found = openItem(trigger, label) !== undefined;
		fireEvent.click(trigger);
		return found ? [row] : [];
	});
}

/** Opens the `⋯` menu of the given row and returns the item with this label; the menu stays open. */
export function menuItem(container: HTMLElement, label: string, row = 0): HTMLElement | undefined {
	const trigger = triggers(container)[row];
	return trigger && openItem(trigger, label);
}

/** Selects a `⋯` menu item of the given row. Throws when the row does not offer it. */
export function selectMenuItem(container: HTMLElement, label: string, row = 0): void {
	const item = menuItem(container, label, row);
	if (!item) throw new Error(`row ${row} offers no "${label}" menu item`);
	fireEvent.click(item);
}
