import { describe, it, expect } from "vitest";
import { render, fireEvent } from "@solidjs/testing-library";
import { KeyboardShortcutsTab } from "../../components/SettingsPanel/tabs/KeyboardShortcutsTab";

describe("KeyboardShortcutsTab", () => {
  it("renders the heading", () => {
    const { container } = render(() => <KeyboardShortcutsTab />);
    const heading = container.querySelector("h3");
    expect(heading).not.toBeNull();
    expect(heading!.textContent).toBe("Keyboard Shortcuts");
  });

  it("has a search input", () => {
    const { container } = render(() => <KeyboardShortcutsTab />);
    const input = container.querySelector("input[type='text']");
    expect(input).not.toBeNull();
    expect(input!.getAttribute("placeholder")).toBe("Search shortcuts...");
  });

  it("renders shortcut sections with labels", () => {
    const { container } = render(() => <KeyboardShortcutsTab />);
    const labels = container.querySelectorAll("label");
    const labelTexts = Array.from(labels).map((l) => l.textContent);
    expect(labelTexts).toContain("Terminal");
    expect(labelTexts).toContain("Zoom");
    expect(labelTexts).toContain("Panels");
    expect(labelTexts).toContain("Git");
  });

  it("renders keyboard shortcuts in kbd elements", () => {
    const { container } = render(() => <KeyboardShortcutsTab />);
    const kbds = container.querySelectorAll("kbd");
    expect(kbds.length).toBeGreaterThan(0);
  });

  it("filters shortcuts by search text", async () => {
    const { container } = render(() => <KeyboardShortcutsTab />);
    const input = container.querySelector("input[type='text']") as HTMLInputElement;

    fireEvent.input(input, { target: { value: "zoom" } });

    const labels = container.querySelectorAll("label");
    const labelTexts = Array.from(labels).map((l) => l.textContent);
    expect(labelTexts).toContain("Zoom");
    expect(labelTexts).not.toContain("Terminal");
  });

  it("shows empty message when no shortcuts match", async () => {
    const { container } = render(() => <KeyboardShortcutsTab />);
    const input = container.querySelector("input[type='text']") as HTMLInputElement;

    fireEvent.input(input, { target: { value: "xyznonexistent" } });

    expect(container.textContent).toContain("No shortcuts match your search");
  });
});
