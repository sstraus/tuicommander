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

  it("sends command via sendCommand (Ctrl-U+text then Enter) on click", async () => {
    // agentType forces the Ctrl-U-prefix branch regardless of host platform
    // detection in the test environment (sendCommand skips Ctrl-U on native
    // Windows shells only when no agent is detected).
    const { container } = render(() => (
      <SuggestChips sessionId="s1" items={["Run tests"]} agentType="claude" />
    ));
    const button = container.querySelector("button")!;
    await fireEvent.click(button);
    // Flush the second await in sendCommand (writeFn("\r"))
    await new Promise((r) => setTimeout(r, 0));
    expect(rpc).toHaveBeenCalledTimes(2);
    expect(rpc).toHaveBeenNthCalledWith(1, "write_pty", { sessionId: "s1", data: "\x15Run tests" });
    expect(rpc).toHaveBeenNthCalledWith(2, "write_pty", { sessionId: "s1", data: "\r" });
  });

  it("renders nothing when items is empty", () => {
    const { container } = render(() => (
      <SuggestChips sessionId="s1" items={[]} />
    ));
    const buttons = container.querySelectorAll("button");
    expect(buttons.length).toBe(0);
  });
});
