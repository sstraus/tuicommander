import { describe, it, expect, vi } from "vitest";
import { render, fireEvent } from "@solidjs/testing-library";
import { CiRing } from "../../components/ui/CiRing";

describe("CiRing", () => {
  it("renders nothing when all counts are 0", () => {
    const { container } = render(() => (
      <CiRing passed={0} failed={0} pending={0} />
    ));
    expect(container.querySelector("svg")).toBeNull();
  });

  it("renders an SVG when there are checks", () => {
    const { container } = render(() => (
      <CiRing passed={3} failed={0} pending={0} />
    ));
    const svg = container.querySelector("svg");
    expect(svg).not.toBeNull();
    expect(svg!.getAttribute("width")).toBe("16");
    expect(svg!.getAttribute("height")).toBe("16");
  });

  it("renders green-only ring for all passed", () => {
    const { container } = render(() => (
      <CiRing passed={5} failed={0} pending={0} />
    ));
    const circles = container.querySelectorAll("circle");
    // Background circle + one arc circle for passed
    const passedCircle = Array.from(circles).find((c) =>
      c.getAttribute("stroke")?.includes("#3fb950") || c.classList.contains("ci-ring-passed")
    );
    expect(passedCircle).toBeDefined();
  });

  it("renders red-only ring for all failed", () => {
    const { container } = render(() => (
      <CiRing passed={0} failed={3} pending={0} />
    ));
    const circles = container.querySelectorAll("circle");
    const failedCircle = Array.from(circles).find((c) =>
      c.getAttribute("stroke")?.includes("#f85149") || c.classList.contains("ci-ring-failed")
    );
    expect(failedCircle).toBeDefined();
  });

  it("renders proportional arcs for mixed state", () => {
    const { container } = render(() => (
      <CiRing passed={2} failed={1} pending={1} />
    ));
    // Should have arcs for each non-zero state
    const circles = container.querySelectorAll("circle");
    // At minimum: background + some arc circles
    expect(circles.length).toBeGreaterThanOrEqual(2);
  });

  it("fires onClick handler", () => {
    const onClick = vi.fn();
    const { container } = render(() => (
      <CiRing passed={1} failed={0} pending={0} onClick={onClick} />
    ));
    const svg = container.querySelector("svg")!;
    fireEvent.click(svg);
    expect(onClick).toHaveBeenCalledOnce();
  });

  it("has pointer cursor when onClick provided", () => {
    const { container } = render(() => (
      <CiRing passed={1} failed={0} pending={0} onClick={() => {}} />
    ));
    const svg = container.querySelector("svg")!;
    expect(svg.style.cursor).toBe("pointer");
  });

  it("does not have pointer cursor when no onClick", () => {
    const { container } = render(() => (
      <CiRing passed={1} failed={0} pending={0} />
    ));
    const svg = container.querySelector("svg")!;
    expect(svg.style.cursor).not.toBe("pointer");
  });

  it("renders pending-only ring", () => {
    const { container } = render(() => (
      <CiRing passed={0} failed={0} pending={4} />
    ));
    const svg = container.querySelector("svg");
    expect(svg).not.toBeNull();
    const circles = container.querySelectorAll("circle");
    const pendingCircle = Array.from(circles).find((c) =>
      c.getAttribute("stroke")?.includes("#d29922") || c.classList.contains("ci-ring-pending")
    );
    expect(pendingCircle).toBeDefined();
  });
});
