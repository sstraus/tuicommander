import { fireEvent, render } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import "../../mocks/tauri";

vi.mock("../../../stores/settings", () => ({
	settingsStore: { state: {} },
	FONT_FAMILIES: {},
}));

vi.mock("../../../stores/ui", () => ({
	uiStore: { state: {} },
}));

vi.mock("../../../stores/repositories", () => ({
	repositoriesStore: { state: { groups: {}, groupOrder: [] } },
}));

vi.mock("../../../themes", () => ({
	getThemeNames: () => ({}),
}));

import { PRESET_COLORS } from "../../../components/SettingsPanel/tabs/AppearanceTab";
import { ColorSwatchPicker } from "../../../components/shared/ColorSwatchPicker";
import { DEFAULT_COLOR_PRESETS } from "../../../components/shared/colorPresets";

describe("ColorSwatchPicker", () => {
	let onChange: (color: string) => void;

	beforeEach(() => {
		onChange = vi.fn();
	});

	it("renders 8 preset swatches + 1 custom + 1 clear", () => {
		const { container } = render(() => (
			<ColorSwatchPicker color="" presets={DEFAULT_COLOR_PRESETS} onChange={onChange} />
		));
		const swatches = container.querySelectorAll('[data-form-control="swatch"], [data-form-control="color"]');
		// 8 presets + 1 custom (label) + 1 clear
		expect(swatches.length).toBe(10);
	});

	it("clicking a preset swatch calls onChange with its hex", () => {
		const { container } = render(() => (
			<ColorSwatchPicker color="" presets={DEFAULT_COLOR_PRESETS} onChange={onChange} />
		));
		const firstSwatch = container.querySelector('[data-form-control="swatch"]')!;
		fireEvent.click(firstSwatch);
		expect(onChange).toHaveBeenCalledWith(PRESET_COLORS[0].hex);
	});

	it("clicking clear calls onChange with empty string", () => {
		const { container } = render(() => (
			<ColorSwatchPicker color="#4A9EFF" presets={DEFAULT_COLOR_PRESETS} onChange={onChange} />
		));
		const clearBtn = container.querySelector('[data-form-control="swatch"][data-clear]')!;
		fireEvent.click(clearBtn);
		expect(onChange).toHaveBeenCalledWith("");
	});

	it("marks preset swatch as active when color matches", () => {
		const { container } = render(() => (
			<ColorSwatchPicker color="#4A9EFF" presets={DEFAULT_COLOR_PRESETS} onChange={onChange} />
		));
		const firstSwatch = container.querySelector('[data-form-control="swatch"]')!;
		expect(firstSwatch).toHaveAttribute("aria-pressed", "true");
	});

	it("marks clear as active when no color set", () => {
		const { container } = render(() => (
			<ColorSwatchPicker color="" presets={DEFAULT_COLOR_PRESETS} onChange={onChange} />
		));
		const clearBtn = container.querySelector('[data-form-control="swatch"][data-clear]')!;
		expect(clearBtn).toHaveAttribute("aria-pressed", "true");
	});

	it("marks custom swatch as active for non-preset color", () => {
		const { container } = render(() => (
			<ColorSwatchPicker color="#123456" presets={DEFAULT_COLOR_PRESETS} onChange={onChange} />
		));
		const customSwatch = container.querySelector('[data-form-control="color"]')!;
		expect(customSwatch).toHaveAttribute("data-selected");
	});

	it("custom color input fires onChange with picked value", () => {
		const { container } = render(() => (
			<ColorSwatchPicker color="" presets={DEFAULT_COLOR_PRESETS} onChange={onChange} />
		));
		const colorInput = container.querySelector("input[type='color']")! as HTMLInputElement;
		fireEvent.input(colorInput, { target: { value: "#abcdef" } });
		expect(onChange).toHaveBeenCalledWith("#abcdef");
	});
});
