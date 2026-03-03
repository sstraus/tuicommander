import { describe, it, expect, vi, afterEach } from "vitest";
import { render, fireEvent, cleanup } from "@solidjs/testing-library";
import { SuggestChips } from "../components/SuggestChips";

vi.mock("../../transport", () => ({
  rpc: vi.fn().mockResolvedValue(undefined),
}));

import { rpc } from "../../transport";

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("SuggestChips", () => {
  it("renders a chip for each item", () => {
    const { container } = render(() => (
      <SuggestChips sessionId="s1" items={["Run tests", "Review diff", "Deploy"]} />
    ));
    const buttons = container.querySelectorAll("button");
    expect(buttons.length).toBe(3);
    expect(buttons[0].textContent).toBe("Run tests");
    expect(buttons[1].textContent).toBe("Review diff");
    expect(buttons[2].textContent).toBe("Deploy");
  });

  it("calls write_pty with item text + newline on click", async () => {
    const { container } = render(() => (
      <SuggestChips sessionId="s1" items={["Run tests"]} />
    ));
    const button = container.querySelector("button")!;
    await fireEvent.click(button);
    expect(rpc).toHaveBeenCalledWith("write_pty", { sessionId: "s1", data: "Run tests\n" });
  });

  it("renders nothing when items is empty", () => {
    const { container } = render(() => (
      <SuggestChips sessionId="s1" items={[]} />
    ));
    const buttons = container.querySelectorAll("button");
    expect(buttons.length).toBe(0);
  });
});
