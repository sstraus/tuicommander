import { fireEvent, render } from "@solidjs/testing-library";
import { expect, it, vi } from "vitest";
import TipOfTheDay from "../../components/TipOfTheDay/TipOfTheDay";
import { TIPS } from "../../data/tips";

// Catches: replacing navigation arrows drops their owner callbacks or reverses boundary wrapping.
it("keeps previous and next tip navigation wrapping at the list boundary", () => {
	const random = vi.spyOn(Math, "random").mockReturnValue(0);
	try {
		const view = render(() => <TipOfTheDay />);
		expect(view.getByText(TIPS[0].feature)).toBeTruthy();
		fireEvent.click(view.getByRole("button", { name: "Previous tip" }));
		expect(view.getByText(TIPS.at(-1)!.feature)).toBeTruthy();
		fireEvent.click(view.getByRole("button", { name: "Next tip" }));
		expect(view.getByText(TIPS[0].feature)).toBeTruthy();
	} finally {
		random.mockRestore();
	}
});
