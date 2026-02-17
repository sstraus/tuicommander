import { describe, it, expect, vi } from "vitest";
import { render, fireEvent } from "@solidjs/testing-library";
import { StatusBadge, BranchBadge, PrBadge, CiBadge } from "../../components/ui/StatusBadge";

describe("StatusBadge", () => {
  it("renders label text", () => {
    const { container } = render(() => (
      <StatusBadge label="Active" />
    ));
    const badge = container.querySelector(".status-badge");
    expect(badge).not.toBeNull();
    expect(badge!.textContent).toBe("Active");
  });

  it("applies variant class", () => {
    const { container } = render(() => (
      <StatusBadge label="OK" variant="success" />
    ));
    const badge = container.querySelector(".status-badge");
    expect(badge!.classList.contains("success")).toBe(true);
  });

  it("defaults to 'default' variant when none specified", () => {
    const { container } = render(() => (
      <StatusBadge label="Default" />
    ));
    const badge = container.querySelector(".status-badge");
    expect(badge!.classList.contains("default")).toBe(true);
  });

  it("shows pointer cursor when onClick is provided", () => {
    const { container } = render(() => (
      <StatusBadge label="Clickable" onClick={() => {}} />
    ));
    const badge = container.querySelector(".status-badge") as HTMLElement;
    expect(badge.style.cursor).toBe("pointer");
  });

  it("shows default cursor when no onClick", () => {
    const { container } = render(() => (
      <StatusBadge label="Static" />
    ));
    const badge = container.querySelector(".status-badge") as HTMLElement;
    expect(badge.style.cursor).toBe("default");
  });

  it("fires click handler", () => {
    const handleClick = vi.fn();
    const { container } = render(() => (
      <StatusBadge label="Click me" onClick={handleClick} />
    ));
    fireEvent.click(container.querySelector(".status-badge")!);
    expect(handleClick).toHaveBeenCalledOnce();
  });
});

describe("BranchBadge", () => {
  it("renders branch name", () => {
    const { container } = render(() => (
      <BranchBadge branch="main" ahead={0} behind={0} />
    ));
    const badge = container.querySelector(".status-badge");
    expect(badge!.textContent).toContain("main");
  });

  it("shows ahead indicator", () => {
    const { container } = render(() => (
      <BranchBadge branch="feature" ahead={3} behind={0} />
    ));
    const badge = container.querySelector(".status-badge");
    expect(badge!.textContent).toContain("\u21913");
    expect(badge!.classList.contains("info")).toBe(true);
  });

  it("shows behind indicator", () => {
    const { container } = render(() => (
      <BranchBadge branch="feature" ahead={0} behind={2} />
    ));
    const badge = container.querySelector(".status-badge");
    expect(badge!.textContent).toContain("\u21932");
  });

  it("shows both ahead and behind", () => {
    const { container } = render(() => (
      <BranchBadge branch="feature" ahead={1} behind={5} />
    ));
    const badge = container.querySelector(".status-badge");
    const text = badge!.textContent!;
    expect(text).toContain("\u21911");
    expect(text).toContain("\u21935");
  });

  it("uses branch variant when not ahead", () => {
    const { container } = render(() => (
      <BranchBadge branch="main" ahead={0} behind={0} />
    ));
    const badge = container.querySelector(".status-badge");
    expect(badge!.classList.contains("branch")).toBe(true);
  });
});

describe("PrBadge", () => {
  it("renders PR number", () => {
    const { container } = render(() => (
      <PrBadge number={42} title="Fix bug" state="open" />
    ));
    const badge = container.querySelector(".status-badge");
    expect(badge!.textContent).toBe("PR #42");
  });

  it("uses merged variant for merged state", () => {
    const { container } = render(() => (
      <PrBadge number={1} title="Merged PR" state="merged" />
    ));
    const badge = container.querySelector(".status-badge");
    expect(badge!.classList.contains("merged")).toBe(true);
  });

  it("uses merged variant for MERGED state (uppercase)", () => {
    const { container } = render(() => (
      <PrBadge number={1} title="Merged PR" state="MERGED" />
    ));
    const badge = container.querySelector(".status-badge");
    expect(badge!.classList.contains("merged")).toBe(true);
  });

  it("uses closed variant for closed state", () => {
    const { container } = render(() => (
      <PrBadge number={1} title="Closed PR" state="closed" />
    ));
    const badge = container.querySelector(".status-badge");
    expect(badge!.classList.contains("closed")).toBe(true);
  });

  it("uses pr variant for open state", () => {
    const { container } = render(() => (
      <PrBadge number={1} title="Open PR" state="open" />
    ));
    const badge = container.querySelector(".status-badge");
    expect(badge!.classList.contains("pr")).toBe(true);
  });

  it("shows warning variant when mergeStateStatus is BEHIND", () => {
    const { container } = render(() => (
      <PrBadge number={1} title="Behind PR" state="open" mergeStateStatus="BEHIND" />
    ));
    const badge = container.querySelector(".status-badge");
    expect(badge!.classList.contains("warning")).toBe(true);
  });

  it("shows error variant when mergeable is CONFLICTING", () => {
    const { container } = render(() => (
      <PrBadge number={1} title="Conflict PR" state="open" mergeable="CONFLICTING" />
    ));
    const badge = container.querySelector(".status-badge");
    expect(badge!.classList.contains("error")).toBe(true);
  });

  it("shows warning variant when mergeStateStatus is BLOCKED", () => {
    const { container } = render(() => (
      <PrBadge number={1} title="Blocked PR" state="open" mergeStateStatus="BLOCKED" />
    ));
    const badge = container.querySelector(".status-badge");
    expect(badge!.classList.contains("warning")).toBe(true);
  });

  it("keeps pr variant when mergeStateStatus is CLEAN", () => {
    const { container } = render(() => (
      <PrBadge number={1} title="Clean PR" state="open" mergeStateStatus="CLEAN" />
    ));
    const badge = container.querySelector(".status-badge");
    expect(badge!.classList.contains("pr")).toBe(true);
  });
});

describe("CiBadge", () => {
  it("shows 'CI passed' for success conclusion", () => {
    const { container } = render(() => (
      <CiBadge status="completed" conclusion="success" workflowName="CI" />
    ));
    const badge = container.querySelector(".status-badge");
    expect(badge!.textContent).toBe("CI passed");
    expect(badge!.classList.contains("success")).toBe(true);
  });

  it("shows 'CI failed' for failure conclusion", () => {
    const { container } = render(() => (
      <CiBadge status="completed" conclusion="failure" workflowName="CI" />
    ));
    const badge = container.querySelector(".status-badge");
    expect(badge!.textContent).toBe("CI failed");
    expect(badge!.classList.contains("error")).toBe(true);
  });

  it("shows 'CI pending' when status is pending and no conclusion", () => {
    const { container } = render(() => (
      <CiBadge status="pending" conclusion={null} workflowName="Build" />
    ));
    const badge = container.querySelector(".status-badge");
    expect(badge!.textContent).toBe("CI pending");
    expect(badge!.classList.contains("warning")).toBe(true);
  });

  it("sets title to workflow name", () => {
    const { container } = render(() => (
      <CiBadge status="completed" conclusion="success" workflowName="My Workflow" />
    ));
    const badge = container.querySelector(".status-badge");
    expect(badge!.getAttribute("title")).toBe("My Workflow");
  });
});
