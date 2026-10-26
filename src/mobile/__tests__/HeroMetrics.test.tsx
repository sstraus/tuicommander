import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";
import { HeroMetrics } from "../components/HeroMetrics";

afterEach(cleanup);

describe("HeroMetrics", () => {
	it("displays working session count", () => {
		const { container } = render(() => <HeroMetrics workingCount={5} idleCount={0} awaitingCount={0} />);
		const numbers = container.querySelectorAll("[data-testid='metric-number']");
		expect(numbers[0].textContent).toBe("5");
	});

	it("displays awaiting input count", () => {
		const { container } = render(() => <HeroMetrics workingCount={3} idleCount={1} awaitingCount={2} />);
		const numbers = container.querySelectorAll("[data-testid='metric-number']");
		expect(numbers[2].textContent).toBe("2");
	});

	it("displays zero counts correctly", () => {
		const { container } = render(() => <HeroMetrics workingCount={0} idleCount={0} awaitingCount={0} />);
		const numbers = container.querySelectorAll("[data-testid='metric-number']");
		expect(numbers[0].textContent).toBe("0");
		expect(numbers[1].textContent).toBe("0");
	});

	it("renders three metric cards", () => {
		const { container } = render(() => <HeroMetrics workingCount={1} idleCount={1} awaitingCount={1} />);
		const cards = container.querySelectorAll("[data-testid='metric-card']");
		expect(cards.length).toBe(3);
	});

	it("shows correct labels", () => {
		const { container } = render(() => <HeroMetrics workingCount={1} idleCount={1} awaitingCount={1} />);
		const labels = container.querySelectorAll("[data-testid='metric-label']");
		expect(labels[0].textContent).toBe("Working");
		expect(labels[1].textContent).toBe("Idle");
		expect(labels[2].textContent).toBe("Awaiting");
	});
});

// Catches: an empty awaiting tile remains actionable after header navigation moves.
it("disables the awaiting action when no session is waiting", async () => {
	const onAwaitingClick = vi.fn();
	const view = render(() => (
		<HeroMetrics workingCount={2} idleCount={1} awaitingCount={0} onAwaitingClick={onAwaitingClick} />
	));
	const tile = view.getByRole("button", { name: "0 Awaiting" }) as HTMLButtonElement;
	expect(tile.disabled).toBe(true);
	await fireEvent.click(tile);
	expect(onAwaitingClick).not.toHaveBeenCalled();
});
