import { type Component, For, type JSX, Show } from "solid-js";
import {
	type SettingId,
	settingDescriptor,
	settingHint,
	settingLabel,
	settingOptions,
} from "../../services/settingsCatalogue";
import { Input } from "../ui/Input";
import { Select } from "../ui/Select";
import { Slider } from "../ui/Slider";
import { Switch } from "../ui/Switch";
import { ContextualHelp } from "./ContextualHelp";
import s from "./Settings.module.css";
import { SettingRow } from "./SettingsPrimitives";

export { SettingAccount, type SettingAccountOption, type SettingAccountProps } from "./fields/SettingAccount";
export { SettingButton, type SettingButtonProps } from "./fields/SettingButton";
export { SettingList, type SettingListProps, type SettingListRow } from "./fields/SettingList";
export { SettingMultiSelect, type SettingMultiSelectProps } from "./fields/SettingMultiSelect";
export { SettingStatus, type SettingStatusProps } from "./fields/SettingStatus";

export const SettingToggle: Component<{
	checked: boolean;
	onChange: (checked: boolean) => void;
	label: string;
	settingId?: SettingId;
	disabled?: boolean;
	"data-non-setting"?: string;
	"data-setting-exclusion"?: string;
	row?: boolean;
	hint?: string;
	hintStyle?: JSX.CSSProperties;
}> = (props) => (
	<Show
		when={props.row}
		fallback={
			<div class={s.group}>
				<div class={s.toggle}>
					<Switch
						role="checkbox"
						aria-label={settingLabel(props.settingId, props.label)}
						disabled={props.disabled}
						data-setting-id={props.settingId}
						data-non-setting={props["data-non-setting"]}
						data-setting-exclusion={props["data-setting-exclusion"]}
						checked={props.checked}
						onChange={(e) => props.onChange(e.currentTarget.checked)}
					/>
					<span>{settingLabel(props.settingId, props.label)}</span>
					<ContextualHelp
						label={settingLabel(props.settingId, props.label)}
						hint={settingHint(props.settingId, props.hint)}
					/>
				</div>
				<Show when={settingHint(props.settingId, props.hint)}>
					<p class={s.hint} style={props.hintStyle}>
						{settingHint(props.settingId, props.hint)}
					</p>
				</Show>
			</div>
		}
	>
		<SettingRow
			settingId={props.settingId}
			label={settingLabel(props.settingId, props.label)}
			hint={settingHint(props.settingId, props.hint)}
		>
			{(id, descriptionId) => (
				<Switch
					disabled={props.disabled}
					data-setting-id={props.settingId}
					data-non-setting={props["data-non-setting"]}
					data-setting-exclusion={props["data-setting-exclusion"]}
					id={id}
					aria-describedby={descriptionId()}
					role="switch"
					checked={props.checked}
					onChange={(e) => props.onChange(e.currentTarget.checked)}
				/>
			)}
		</SettingRow>
	</Show>
);

export const SettingSelect: Component<{
	label: string;
	settingId?: SettingId;
	disabled?: boolean;
	"data-non-setting"?: string;
	"data-setting-exclusion"?: string;
	row?: boolean;
	value: string;
	onChange: (value: string) => void;
	options: { value: string; label: string }[];
	/** Runtime availability can narrow, but never extend, the owner schema. */
	unavailableValues?: readonly string[];
	hint?: string;
	hintStyle?: JSX.CSSProperties;
}> = (props) => {
	const options = () => {
		const choices =
			props.settingId && settingDescriptor(props.settingId).schema.enum
				? settingOptions(
						props.settingId,
						Object.fromEntries(props.options.map((option) => [option.value, option.label])),
					)
				: props.options;
		return choices.filter((option) => !props.unavailableValues?.includes(option.value));
	};
	return (
		<Show
			when={props.row}
			fallback={
				<div class={s.group}>
					<div class={s.helpLabel}>
						<label>{settingLabel(props.settingId, props.label)}</label>
						<ContextualHelp
							label={settingLabel(props.settingId, props.label)}
							hint={settingHint(props.settingId, props.hint)}
						/>
					</div>
					<Select
						disabled={props.disabled}
						data-setting-id={props.settingId}
						data-non-setting={props["data-non-setting"]}
						data-setting-exclusion={props["data-setting-exclusion"]}
						value={props.value}
						onChange={(e) => props.onChange(e.currentTarget.value)}
					>
						<For each={options()}>{(opt) => <option value={opt.value}>{opt.label}</option>}</For>
					</Select>
					<Show when={settingHint(props.settingId, props.hint)}>
						<p class={s.hint} style={props.hintStyle}>
							{settingHint(props.settingId, props.hint)}
						</p>
					</Show>
				</div>
			}
		>
			<SettingRow
				settingId={props.settingId}
				label={settingLabel(props.settingId, props.label)}
				hint={settingHint(props.settingId, props.hint)}
			>
				{(id, descriptionId) => (
					<Select
						disabled={props.disabled}
						data-setting-id={props.settingId}
						data-non-setting={props["data-non-setting"]}
						data-setting-exclusion={props["data-setting-exclusion"]}
						id={id}
						aria-describedby={descriptionId()}
						value={props.value}
						onChange={(e) => props.onChange(e.currentTarget.value)}
					>
						<For each={options()}>{(opt) => <option value={opt.value}>{opt.label}</option>}</For>
					</Select>
				)}
			</SettingRow>
		</Show>
	);
};

export const SettingSlider: Component<{
	label: string;
	settingId?: SettingId;
	disabled?: boolean;
	"data-non-setting"?: string;
	"data-setting-exclusion"?: string;
	row?: boolean;
	value: number;
	onChange: (value: number) => void;
	/** Fires once when the drag is released (DOM `change`), e.g. to play a preview at the committed value */
	onCommit?: (value: number) => void;
	min: number;
	max: number;
	step?: number;
	suffix?: string;
	formatValue?: (value: number) => string;
	hint?: string;
}> = (props) => (
	<Show
		when={props.row}
		fallback={
			<div class={s.group}>
				<div class={s.helpLabel}>
					<label>{settingLabel(props.settingId, props.label)}</label>
					<ContextualHelp
						label={settingLabel(props.settingId, props.label)}
						hint={settingHint(props.settingId, props.hint)}
					/>
				</div>
				<div class={s.slider}>
					<Slider
						disabled={props.disabled}
						data-setting-id={props.settingId}
						data-non-setting={props["data-non-setting"]}
						data-setting-exclusion={props["data-setting-exclusion"]}
						min={props.min}
						max={props.max}
						step={props.step}
						value={props.value}
						onInput={(e) => props.onChange(parseInt(e.currentTarget.value, 10))}
						onChange={(e) => props.onCommit?.(parseInt(e.currentTarget.value, 10))}
					/>
					<span>{props.formatValue ? props.formatValue(props.value) : `${props.value}${props.suffix ?? ""}`}</span>
				</div>
				<Show when={settingHint(props.settingId, props.hint)}>
					<p class={s.hint}>{settingHint(props.settingId, props.hint)}</p>
				</Show>
			</div>
		}
	>
		<SettingRow
			settingId={props.settingId}
			label={settingLabel(props.settingId, props.label)}
			hint={settingHint(props.settingId, props.hint)}
		>
			{(id, descriptionId) => (
				<div class={s.slider}>
					<Slider
						disabled={props.disabled}
						data-setting-id={props.settingId}
						data-non-setting={props["data-non-setting"]}
						data-setting-exclusion={props["data-setting-exclusion"]}
						id={id}
						aria-describedby={descriptionId()}
						min={props.min}
						max={props.max}
						step={props.step}
						value={props.value}
						onInput={(e) => props.onChange(parseInt(e.currentTarget.value, 10))}
						onChange={(e) => props.onCommit?.(parseInt(e.currentTarget.value, 10))}
					/>
					<span>{props.formatValue ? props.formatValue(props.value) : `${props.value}${props.suffix ?? ""}`}</span>
				</div>
			)}
		</SettingRow>
	</Show>
);

export const SettingInput: Component<{
	label: string;
	settingId?: SettingId;
	disabled?: boolean;
	"data-non-setting"?: string;
	"data-setting-exclusion"?: string;
	row?: boolean;
	value: string;
	onInput: (value: string) => unknown;
	/** Fires when the field loses focus, for checks that would reject a half-typed value. */
	onBlur?: () => void;
	placeholder?: string;
	hint?: string;
	type?: "text" | "password" | "number";
	min?: number;
	max?: number;
	step?: number;
}> = (props) => (
	<Show
		when={props.row}
		fallback={
			<div class={s.group}>
				<div class={s.helpLabel}>
					<label>{settingLabel(props.settingId, props.label)}</label>
					<ContextualHelp
						label={settingLabel(props.settingId, props.label)}
						hint={settingHint(props.settingId, props.hint)}
					/>
				</div>
				<Input
					disabled={props.disabled}
					data-setting-id={props.settingId}
					data-non-setting={props["data-non-setting"]}
					data-setting-exclusion={props["data-setting-exclusion"]}
					type={props.type ?? "text"}
					min={props.min}
					max={props.max}
					step={props.step}
					value={props.value}
					onInput={(e) => {
						if (props.onInput(e.currentTarget.value) === false) e.currentTarget.value = props.value;
					}}
					onBlur={() => props.onBlur?.()}
					placeholder={props.placeholder}
				/>
				<Show when={settingHint(props.settingId, props.hint)}>
					<p class={s.hint}>{settingHint(props.settingId, props.hint)}</p>
				</Show>
			</div>
		}
	>
		<SettingRow
			settingId={props.settingId}
			label={settingLabel(props.settingId, props.label)}
			hint={settingHint(props.settingId, props.hint)}
		>
			{(id, descriptionId) => (
				<Input
					disabled={props.disabled}
					data-setting-id={props.settingId}
					data-non-setting={props["data-non-setting"]}
					data-setting-exclusion={props["data-setting-exclusion"]}
					id={id}
					aria-describedby={descriptionId()}
					type={props.type ?? "text"}
					min={props.min}
					max={props.max}
					step={props.step}
					value={props.value}
					onInput={(e) => {
						if (props.onInput(e.currentTarget.value) === false) e.currentTarget.value = props.value;
					}}
					onBlur={() => props.onBlur?.()}
					placeholder={props.placeholder}
				/>
			)}
		</SettingRow>
	</Show>
);
