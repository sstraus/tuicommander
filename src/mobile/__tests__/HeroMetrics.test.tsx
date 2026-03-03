import { describe, it, expect, afterEach } from "vitest";
import { render, cleanup } from "@solidjs/testing-library";
import { HeroMetrics } from "../components/HeroMetrics";

afterEach(cleanup);

describe("HeroMetrics", () => {
  it("displays active session count", () => {
    const { container } = render(() => (
      <HeroMetrics activeCount={5} awaitingCount={0} />
    ));
    const numbers = container.querySelectorAll("[data-testid='metric-number']");
    expect(numbers[0].textContent).toBe("5");
  });

  it("displays awaiting input count", () => {
    const { container } = render(() => (
      <HeroMetrics activeCount={3} awaitingCount={2} />
    ));
    const numbers = container.querySelectorAll("[data-testid='metric-number']");
    expect(numbers[1].textContent).toBe("2");
  });

  it("displays zero counts correctly", () => {
    const { container } = render(() => (
      <HeroMetrics activeCount={0} awaitingCount={0} />
    ));
    const numbers = container.querySelectorAll("[data-testid='metric-number']");
    expect(numbers[0].textContent).toBe("0");
    expect(numbers[1].textContent).toBe("0");
  });

  it("renders two metric cards", () => {
    const { container } = render(() => (
      <HeroMetrics activeCount={1} awaitingCount={1} />
    ));
    const cards = container.querySelectorAll("[data-testid='metric-card']");
    expect(cards.length).toBe(2);
  });

  it("shows correct labels", () => {
    const { container } = render(() => (
      <HeroMetrics activeCount={1} awaitingCount={1} />
    ));
    const labels = container.querySelectorAll("[data-testid='metric-label']");
    expect(labels[0].textContent).toBe("Active");
    expect(labels[1].textContent).toBe("Awaiting");
  });
});
