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
  };

  it("renders nothing when not visible", () => {
    const { container } = render(() => (
      <HelpPanel visible={false} onClose={() => {}} />
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

  it("shows inline shortcuts when keyboard shortcuts button is clicked", () => {
    const { container } = render(() => (
      <HelpPanel {...defaultProps} />
    ));
    const buttons = Array.from(container.querySelectorAll("button"));
    const shortcutsBtn = buttons.find((b) => b.textContent?.trim() === "Keyboard Shortcuts");
    expect(shortcutsBtn).not.toBeNull();
    fireEvent.click(shortcutsBtn!);
    // After clicking, heading should change to "Keyboard Shortcuts"
    const heading = container.querySelector("h2");
    expect(heading!.textContent).toBe("Keyboard Shortcuts");
    // And shortcut sections should render (kbd elements from KeyboardShortcutsTab)
    const kbds = container.querySelectorAll("kbd");
    expect(kbds.length).toBeGreaterThan(0);
  });

  it("calls onClose when close button is clicked", () => {
    const handleClose = vi.fn();
    const { container } = render(() => (
      <HelpPanel visible={true} onClose={handleClose} />
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

  it("displays license and credits", () => {
    const { container } = render(() => (
      <HelpPanel {...defaultProps} />
    ));
    expect(container.textContent).toContain("MIT License");
    expect(container.textContent).toContain("Tauri 2");
    expect(container.textContent).toContain("SolidJS");
  });
});
