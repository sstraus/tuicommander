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

import { ColorSwatchPicker } from "../../../components/shared/ColorSwatchPicker";
import { PRESET_COLORS } from "../../../components/SettingsPanel/tabs/AppearanceTab";

describe("ColorSwatchPicker", () => {
  let onChange: (color: string) => void;

  beforeEach(() => {
    onChange = vi.fn();
  });

  it("renders 8 preset swatches + 1 custom + 1 clear", () => {
    const { container } = render(() => <ColorSwatchPicker color="" onChange={onChange} />);
    const swatches = container.querySelectorAll(".colorSwatch");
    // 8 presets + 1 custom (label) + 1 clear
    expect(swatches.length).toBe(10);
  });

  it("clicking a preset swatch calls onChange with its hex", () => {
    const { container } = render(() => <ColorSwatchPicker color="" onChange={onChange} />);
    const firstSwatch = container.querySelector(".colorSwatch")!;
    fireEvent.click(firstSwatch);
    expect(onChange).toHaveBeenCalledWith(PRESET_COLORS[0].hex);
  });

  it("clicking clear calls onChange with empty string", () => {
    const { container } = render(() => <ColorSwatchPicker color="#4A9EFF" onChange={onChange} />);
    const clearBtn = container.querySelector(".colorSwatchClear")!;
    fireEvent.click(clearBtn);
    expect(onChange).toHaveBeenCalledWith("");
  });

  it("marks preset swatch as active when color matches", () => {
    const { container } = render(() => <ColorSwatchPicker color="#4A9EFF" onChange={onChange} />);
    const firstSwatch = container.querySelector(".colorSwatch")!;
    expect(firstSwatch.classList.contains("active")).toBe(true);
  });

  it("marks clear as active when no color set", () => {
    const { container } = render(() => <ColorSwatchPicker color="" onChange={onChange} />);
    const clearBtn = container.querySelector(".colorSwatchClear")!;
    expect(clearBtn.classList.contains("active")).toBe(true);
  });

  it("marks custom swatch as active for non-preset color", () => {
    const { container } = render(() => <ColorSwatchPicker color="#123456" onChange={onChange} />);
    const customSwatch = container.querySelector(".colorSwatchCustom")!;
    expect(customSwatch.classList.contains("active")).toBe(true);
  });

  it("custom color input fires onChange with picked value", () => {
    const { container } = render(() => <ColorSwatchPicker color="" onChange={onChange} />);
    const colorInput = container.querySelector("input[type='color']")! as HTMLInputElement;
    fireEvent.input(colorInput, { target: { value: "#abcdef" } });
    expect(onChange).toHaveBeenCalledWith("#abcdef");
  });
});
