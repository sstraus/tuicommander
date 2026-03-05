import { describe, it, expect, vi, afterEach } from "vitest";
import { render, fireEvent, cleanup } from "@solidjs/testing-library";
import { NewSessionSheet } from "../components/NewSessionSheet";

vi.mock("../../transport", () => ({
  rpc: vi.fn().mockResolvedValue({}),
}));

import { rpc } from "../../transport";

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

const REPOS = ["/home/user/project-a", "/home/user/project-b"];

describe("NewSessionSheet", () => {
  it("renders a button for each repo", () => {
    const { container } = render(() => (
      <NewSessionSheet repos={REPOS} onDismiss={() => {}} />
    ));
    const buttons = container.querySelectorAll("button");
    expect(buttons.length).toBe(2);
    expect(buttons[0].textContent).toContain("project-a");
    expect(buttons[1].textContent).toContain("project-b");
  });

  it("calls create_pty with selected repo cwd on click", async () => {
    const onDismiss = vi.fn();
    const { container } = render(() => (
      <NewSessionSheet repos={REPOS} onDismiss={onDismiss} />
    ));
    const buttons = container.querySelectorAll("button");
    await fireEvent.click(buttons[0]);
    expect(rpc).toHaveBeenCalledWith("create_pty", {
      config: { cwd: "/home/user/project-a" },
    });
    expect(onDismiss).toHaveBeenCalled();
  });

  it("calls onDismiss when backdrop is clicked", async () => {
    const onDismiss = vi.fn();
    const { container } = render(() => (
      <NewSessionSheet repos={REPOS} onDismiss={onDismiss} />
    ));
    const backdrop = container.firstElementChild as HTMLElement;
    await fireEvent.click(backdrop);
    expect(onDismiss).toHaveBeenCalled();
  });

  it("shows empty state when no repos", () => {
    const { container } = render(() => (
      <NewSessionSheet repos={[]} onDismiss={() => {}} />
    ));
    expect(container.textContent).toContain("No repositories configured");
  });
});
