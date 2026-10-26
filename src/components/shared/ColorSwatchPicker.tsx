import { type Component, For, Show } from "solid-js";
import { t } from "../../i18n";
import type { SettingId } from "../../services/settingsCatalogue";
import s from "../SettingsPanel/Settings.module.css";
import { ColorInput } from "../ui/ColorInput";
import { ColorSwatch } from "../ui/ColorSwatch";
import type { ColorPreset } from "./colorPresets";

export interface ColorSwatchPickerProps {
	settingId?: SettingId;
	color: string;
	presets: readonly ColorPreset[];
	onChange: (color: string) => void;
}

export const ColorSwatchPicker: Component<ColorSwatchPickerProps> = (props) => {
	const isCustomColor = () => props.color && !props.presets.some((preset) => preset.hex === props.color);

	return (
		<div class={s.groupColorPicker}>
			<For each={props.presets}>
				{(preset) => (
					<ColorSwatch
						data-setting-id={props.settingId}
						selected={props.color === preset.hex}
						swatchColor={preset.hex}
						aria-label={preset.name}
						onClick={() => props.onChange(preset.hex)}
						title={preset.name}
					/>
				)}
			</For>
			<ColorInput
				selected={!!isCustomColor()}
				data-setting-id={props.settingId}
				aria-label={t("groups.btn.customColor", "Custom color")}
				title={t("groups.btn.customColor", "Custom color")}
				value={props.color || "#999999"}
				onInput={(e) => props.onChange(e.currentTarget.value)}
			>
				<Show when={!isCustomColor()}>
					<span class={s.colorSwatchLabel}>&#x22EF;</span>
				</Show>
			</ColorInput>
			<ColorSwatch
				data-setting-id={props.settingId}
				selected={!props.color}
				aria-label={t("groups.btn.noColor", "No color")}
				onClick={() => props.onChange("")}
				title={t("groups.btn.noColor", "No color")}
			>
				&times;
			</ColorSwatch>
		</div>
	);
};
