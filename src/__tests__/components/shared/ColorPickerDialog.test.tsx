import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@solidjs/testing-library";
import "../../mocks/tauri";

vi.mock("../../../stores/settings", () => ({
  settingsStore: { state: {} },
}));

vi.mock("../../../stores/ui", () => ({
  uiStore: { state: {} },
}));

vi.mock("../../../stores/repositories", () => ({
  repositoriesStore: { state: { groups: {}, groupOrder: [] } },
}));

vi.mock("../../../themes", () => ({
  THEME_NAMES: {},
}));

import { ColorPickerDialog } from "../../../components/shared/ColorPickerDialog";
import { PRESET_COLORS } from "../../../components/SettingsPanel/tabs/AppearanceTab";

describe("ColorPickerDialog", () => {
  let onClose: () => void;
  let onConfirm: (color: string) => void;

  beforeEach(() => {
    onClose = vi.fn();
    onConfirm = vi.fn();
  });

  it("does not render when visible is false", () => {
    const { container } = render(() => (
      <ColorPickerDialog
        visible={false}
        title="Group Color"
        currentColor=""
        onClose={onClose}
        onConfirm={onConfirm}
      />
    ));
    expect(container.querySelector(".overlay")).toBeNull();
  });

  it("renders overlay and popover when visible", () => {
    const { container } = render(() => (
      <ColorPickerDialog
        visible={true}
        title="Group Color"
        currentColor=""
        onClose={onClose}
        onConfirm={onConfirm}
      />
    ));
    expect(container.querySelector(".overlay")).not.toBeNull();
    expect(container.querySelector(".popover")).not.toBeNull();
  });

  it("renders title in header", () => {
    const { container } = render(() => (
      <ColorPickerDialog
        visible={true}
        title="Group Color"
        currentColor=""
        onClose={onClose}
        onConfirm={onConfirm}
      />
    ));
    const header = container.querySelector("h4");
    expect(header?.textContent).toBe("Group Color");
  });

  it("contains color swatches", () => {
    const { container } = render(() => (
      <ColorPickerDialog
        visible={true}
        title="Group Color"
        currentColor=""
        onClose={onClose}
        onConfirm={onConfirm}
      />
    ));
    const swatches = container.querySelectorAll(".colorSwatch");
    expect(swatches.length).toBe(10);
  });

  it("clicking a preset swatch calls onConfirm and onClose", () => {
    const { container } = render(() => (
      <ColorPickerDialog
        visible={true}
        title="Group Color"
        currentColor=""
        onClose={onClose}
        onConfirm={onConfirm}
      />
    ));
    const firstSwatch = container.querySelector(".colorSwatch")!;
    fireEvent.click(firstSwatch);
    expect(onConfirm).toHaveBeenCalledWith(PRESET_COLORS[0].hex);
    expect(onClose).toHaveBeenCalled();
  });

  it("clicking clear calls onConfirm with empty string and closes", () => {
    const { container } = render(() => (
      <ColorPickerDialog
        visible={true}
        title="Group Color"
        currentColor="#4A9EFF"
        onClose={onClose}
        onConfirm={onConfirm}
      />
    ));
    const clearBtn = container.querySelector(".colorSwatchClear")!;
    fireEvent.click(clearBtn);
    expect(onConfirm).toHaveBeenCalledWith("");
    expect(onClose).toHaveBeenCalled();
  });

  it("clicking overlay backdrop calls onClose", () => {
    const { container } = render(() => (
      <ColorPickerDialog
        visible={true}
        title="Group Color"
        currentColor=""
        onClose={onClose}
        onConfirm={onConfirm}
      />
    ));
    const overlay = container.querySelector(".overlay")!;
    fireEvent.click(overlay);
    expect(onClose).toHaveBeenCalled();
  });

  it("Escape key calls onClose", () => {
    render(() => (
      <ColorPickerDialog
        visible={true}
        title="Group Color"
        currentColor=""
        onClose={onClose}
        onConfirm={onConfirm}
      />
    ));
    fireEvent.keyDown(document, { key: "Escape" });
    expect(onClose).toHaveBeenCalled();
  });

  it("Cancel button calls onClose", () => {
    const { container } = render(() => (
      <ColorPickerDialog
        visible={true}
        title="Group Color"
        currentColor=""
        onClose={onClose}
        onConfirm={onConfirm}
      />
    ));
    const cancelBtn = container.querySelector(".cancelBtn")!;
    fireEvent.click(cancelBtn);
    expect(onClose).toHaveBeenCalled();
  });
});
