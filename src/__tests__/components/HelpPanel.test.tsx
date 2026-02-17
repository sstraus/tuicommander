import { describe, it, expect, vi } from "vitest";
import { render, fireEvent } from "@solidjs/testing-library";
import { HelpPanel } from "../../components/HelpPanel/HelpPanel";

describe("HelpPanel", () => {
  it("renders nothing when not visible", () => {
    const { container } = render(() => (
      <HelpPanel visible={false} onClose={() => {}} />
    ));
    const overlay = container.querySelector(".help-overlay");
    expect(overlay).toBeNull();
  });

  it("renders help panel when visible", () => {
    const { container } = render(() => (
      <HelpPanel visible={true} onClose={() => {}} />
    ));
    const overlay = container.querySelector(".help-overlay");
    expect(overlay).not.toBeNull();
    const heading = container.querySelector("h2");
    expect(heading).not.toBeNull();
    expect(heading!.textContent).toBe("Keyboard Shortcuts");
  });

  it("renders sections with titles", () => {
    const { container } = render(() => (
      <HelpPanel visible={true} onClose={() => {}} />
    ));
    const sections = container.querySelectorAll(".help-section-title");
    expect(sections.length).toBeGreaterThan(0);
    // Should have Terminal, Zoom, Panels, Git, Sidebar sections
    const titles = Array.from(sections).map((s) => s.textContent);
    expect(titles).toContain("Terminal");
    expect(titles).toContain("Zoom");
    expect(titles).toContain("Panels");
    expect(titles).toContain("Git");
  });

  it("renders keyboard shortcuts in tables", () => {
    const { container } = render(() => (
      <HelpPanel visible={true} onClose={() => {}} />
    ));
    const kbds = container.querySelectorAll("kbd");
    expect(kbds.length).toBeGreaterThan(0);
  });

  it("has a search input", () => {
    const { container } = render(() => (
      <HelpPanel visible={true} onClose={() => {}} />
    ));
    const input = container.querySelector("input[type='text']");
    expect(input).not.toBeNull();
    expect(input!.getAttribute("placeholder")).toBe("Search shortcuts...");
  });

  it("filters shortcuts by search text", async () => {
    const { container } = render(() => (
      <HelpPanel visible={true} onClose={() => {}} />
    ));
    const input = container.querySelector("input[type='text']") as HTMLInputElement;

    // Type a search term that matches only zoom shortcuts
    fireEvent.input(input, { target: { value: "zoom" } });

    const sections = container.querySelectorAll(".help-section-title");
    const titles = Array.from(sections).map((s) => s.textContent);
    expect(titles).toContain("Zoom");
    // Other sections should be filtered out (no Terminal shortcuts match "zoom")
    expect(titles).not.toContain("Terminal");
  });

  it("shows empty message when no shortcuts match", async () => {
    const { container } = render(() => (
      <HelpPanel visible={true} onClose={() => {}} />
    ));
    const input = container.querySelector("input[type='text']") as HTMLInputElement;

    fireEvent.input(input, { target: { value: "xyznonexistent" } });

    const empty = container.querySelector(".help-empty");
    expect(empty).not.toBeNull();
    expect(empty!.textContent).toBe("No shortcuts match your search");
  });

  it("calls onClose when close button is clicked", () => {
    const handleClose = vi.fn();
    const { container } = render(() => (
      <HelpPanel visible={true} onClose={handleClose} />
    ));
    const closeBtn = container.querySelector(".help-close")!;
    fireEvent.click(closeBtn);
    expect(handleClose).toHaveBeenCalledOnce();
  });
});
