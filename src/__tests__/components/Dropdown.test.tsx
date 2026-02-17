import { describe, it, expect, vi } from "vitest";
import { render, fireEvent } from "@solidjs/testing-library";
import { Dropdown } from "../../components/ui/Dropdown";
import type { DropdownItem } from "../../components/ui/Dropdown";

const sampleItems: DropdownItem[] = [
  { id: "a", label: "Alpha" },
  { id: "b", label: "Beta" },
  { id: "c", label: "Gamma", disabled: true },
];

describe("Dropdown", () => {
  it("renders nothing when visible is false", () => {
    const { container } = render(() => (
      <Dropdown
        items={sampleItems}
        visible={false}
        onSelect={() => {}}
        onClose={() => {}}
      />
    ));
    const dropdown = container.querySelector(".dropdown");
    expect(dropdown).toBeNull();
  });

  it("renders items when visible is true", () => {
    const { container } = render(() => (
      <Dropdown
        items={sampleItems}
        visible={true}
        onSelect={() => {}}
        onClose={() => {}}
      />
    ));
    const items = container.querySelectorAll(".dropdown-item");
    expect(items.length).toBe(3);
    expect(items[0].querySelector(".dropdown-item-label")!.textContent).toBe("Alpha");
    expect(items[1].querySelector(".dropdown-item-label")!.textContent).toBe("Beta");
  });

  it("applies selected class to selected item", () => {
    const { container } = render(() => (
      <Dropdown
        items={sampleItems}
        selected="b"
        visible={true}
        onSelect={() => {}}
        onClose={() => {}}
      />
    ));
    const items = container.querySelectorAll(".dropdown-item");
    expect(items[0].classList.contains("selected")).toBe(false);
    expect(items[1].classList.contains("selected")).toBe(true);
  });

  it("calls onSelect when non-disabled item is clicked", () => {
    const handleSelect = vi.fn();
    const { container } = render(() => (
      <Dropdown
        items={sampleItems}
        visible={true}
        onSelect={handleSelect}
        onClose={() => {}}
      />
    ));
    const items = container.querySelectorAll(".dropdown-item");
    fireEvent.click(items[0]);
    expect(handleSelect).toHaveBeenCalledWith("a");
  });

  it("does not call onSelect for disabled items", () => {
    const handleSelect = vi.fn();
    const { container } = render(() => (
      <Dropdown
        items={sampleItems}
        visible={true}
        onSelect={handleSelect}
        onClose={() => {}}
      />
    ));
    const items = container.querySelectorAll(".dropdown-item");
    fireEvent.click(items[2]); // disabled item
    expect(handleSelect).not.toHaveBeenCalled();
  });

  it("applies disabled class to disabled items", () => {
    const { container } = render(() => (
      <Dropdown
        items={sampleItems}
        visible={true}
        onSelect={() => {}}
        onClose={() => {}}
      />
    ));
    const items = container.querySelectorAll(".dropdown-item");
    expect(items[2].classList.contains("disabled")).toBe(true);
  });

  it("closes on Escape key", () => {
    const handleClose = vi.fn();
    render(() => (
      <Dropdown
        items={sampleItems}
        visible={true}
        onSelect={() => {}}
        onClose={handleClose}
      />
    ));
    fireEvent.keyDown(document, { key: "Escape" });
    expect(handleClose).toHaveBeenCalled();
  });

  it("closes on click outside", async () => {
    const handleClose = vi.fn();
    render(() => (
      <div>
        <div data-testid="outside">Outside</div>
        <Dropdown
          items={sampleItems}
          visible={true}
          onSelect={() => {}}
          onClose={handleClose}
        />
      </div>
    ));
    // The click-outside handler is delayed by setTimeout(0)
    await vi.waitFor(() => {
      fireEvent.click(document.body);
      expect(handleClose).toHaveBeenCalled();
    });
  });

  it("renders with top position class", () => {
    const { container } = render(() => (
      <Dropdown
        items={sampleItems}
        visible={true}
        onSelect={() => {}}
        onClose={() => {}}
        position="top"
      />
    ));
    const dropdown = container.querySelector(".dropdown");
    expect(dropdown!.classList.contains("dropdown-top")).toBe(true);
  });

  it("renders items with icons", () => {
    const itemsWithIcon: DropdownItem[] = [
      { id: "a", label: "Alpha", icon: (<span>icon</span>) as any },
    ];
    const { container } = render(() => (
      <Dropdown
        items={itemsWithIcon}
        visible={true}
        onSelect={() => {}}
        onClose={() => {}}
      />
    ));
    const icon = container.querySelector(".dropdown-item-icon");
    expect(icon).not.toBeNull();
  });

  it("renders dividers", () => {
    const itemsWithDivider: DropdownItem[] = [
      { id: "a", label: "Alpha" },
      { id: "div", label: "", divider: true },
      { id: "b", label: "Beta" },
    ];
    const { container } = render(() => (
      <Dropdown
        items={itemsWithDivider}
        visible={true}
        onSelect={() => {}}
        onClose={() => {}}
      />
    ));
    const dividers = container.querySelectorAll(".dropdown-divider");
    expect(dividers.length).toBe(1);
  });
});
