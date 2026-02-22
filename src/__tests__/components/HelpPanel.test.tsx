import { describe, it, expect, vi } from "vitest";
import { render, fireEvent } from "@solidjs/testing-library";
import { HelpPanel } from "../../components/HelpPanel/HelpPanel";

vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: vi.fn().mockResolvedValue(undefined),
}));

describe("HelpPanel", () => {
  const defaultProps = {
    visible: true,
    onClose: vi.fn(),
    onOpenShortcuts: vi.fn(),
  };

  it("renders nothing when not visible", () => {
    const { container } = render(() => (
      <HelpPanel visible={false} onClose={() => {}} onOpenShortcuts={() => {}} />
    ));
    const overlay = container.querySelector(".overlay");
    expect(overlay).toBeNull();
  });

  it("renders help panel when visible", () => {
    const { container } = render(() => (
      <HelpPanel {...defaultProps} />
    ));
    const overlay = container.querySelector(".overlay");
    expect(overlay).not.toBeNull();
    const heading = container.querySelector("h2");
    expect(heading).not.toBeNull();
    expect(heading!.textContent).toBe("Help");
  });

  it("shows project links", () => {
    const { container } = render(() => (
      <HelpPanel {...defaultProps} />
    ));
    const buttons = container.querySelectorAll("button");
    const buttonTexts = Array.from(buttons).map((b) => b.textContent?.trim());
    expect(buttonTexts).toContain("GitHub Project");
    expect(buttonTexts).toContain("Documentation");
    expect(buttonTexts).toContain("Report an Issue");
  });

  it("shows keyboard shortcuts button", () => {
    const { container } = render(() => (
      <HelpPanel {...defaultProps} />
    ));
    const buttons = container.querySelectorAll("button");
    const buttonTexts = Array.from(buttons).map((b) => b.textContent?.trim());
    expect(buttonTexts).toContain("Keyboard Shortcuts");
  });

  it("calls onOpenShortcuts and onClose when keyboard shortcuts button is clicked", () => {
    const onClose = vi.fn();
    const onOpenShortcuts = vi.fn();
    const { container } = render(() => (
      <HelpPanel visible={true} onClose={onClose} onOpenShortcuts={onOpenShortcuts} />
    ));
    const buttons = Array.from(container.querySelectorAll("button"));
    const shortcutsBtn = buttons.find((b) => b.textContent?.trim() === "Keyboard Shortcuts");
    expect(shortcutsBtn).not.toBeNull();
    fireEvent.click(shortcutsBtn!);
    expect(onClose).toHaveBeenCalledOnce();
    expect(onOpenShortcuts).toHaveBeenCalledOnce();
  });

  it("calls onClose when close button is clicked", () => {
    const handleClose = vi.fn();
    const { container } = render(() => (
      <HelpPanel visible={true} onClose={handleClose} onOpenShortcuts={() => {}} />
    ));
    const closeBtn = container.querySelector(".close")!;
    fireEvent.click(closeBtn);
    expect(handleClose).toHaveBeenCalledOnce();
  });

  it("displays the app version", () => {
    const { container } = render(() => (
      <HelpPanel {...defaultProps} />
    ));
    expect(container.textContent).toContain("Version");
  });
});
