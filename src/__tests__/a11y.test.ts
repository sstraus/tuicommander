import { describe, expect, it, vi } from "vitest";
import { onClickKeyDown } from "../utils/a11y";

function key(k: string): KeyboardEvent & { preventDefault: ReturnType<typeof vi.fn> } {
	return { key: k, preventDefault: vi.fn() } as unknown as KeyboardEvent & {
		preventDefault: ReturnType<typeof vi.fn>;
	};
}

describe("onClickKeyDown", () => {
	it("invokes the handler on Enter and prevents default", () => {
		const handler = vi.fn();
		const e = key("Enter");
		(onClickKeyDown(handler) as (e: KeyboardEvent) => void)(e);
		expect(handler).toHaveBeenCalledOnce();
		expect(e.preventDefault).toHaveBeenCalledOnce();
	});

	it("invokes the handler on Space and prevents default (suppress page scroll)", () => {
		const handler = vi.fn();
		const e = key(" ");
		(onClickKeyDown(handler) as (e: KeyboardEvent) => void)(e);
		expect(handler).toHaveBeenCalledOnce();
		expect(e.preventDefault).toHaveBeenCalledOnce();
	});

	it("ignores other keys and does not prevent default", () => {
		const handler = vi.fn();
		const e = key("Tab");
		(onClickKeyDown(handler) as (e: KeyboardEvent) => void)(e);
		expect(handler).not.toHaveBeenCalled();
		expect(e.preventDefault).not.toHaveBeenCalled();
	});

	it("forwards the triggering event so the handler can call stopPropagation()", () => {
		const e = key("Enter");
		let received: KeyboardEvent | undefined;
		(onClickKeyDown((ev) => (received = ev)) as (e: KeyboardEvent) => void)(e);
		expect(received).toBe(e);
	});
});
