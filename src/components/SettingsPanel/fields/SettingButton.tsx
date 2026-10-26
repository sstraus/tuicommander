import type { Component } from "solid-js";
import { Button } from "../../ui/Button";
import s from "./fields.module.css";
import { SettingField, type SettingFieldProps } from "./SettingField";

export interface SettingButtonProps extends SettingFieldProps {
	value: string;
	onClick: () => void;
}

export const SettingButton: Component<SettingButtonProps> = (props) => (
	<SettingField label={props.label} hint={props.hint} row={props.row} error={props.error} help={props.help}>
		{(id, _hintId, descriptionId) => (
			<Button
				id={id}
				type="button"
				class={s.button}
				aria-label={props.value}
				aria-describedby={descriptionId()}
				disabled={props.disabled}
				onClick={props.onClick}
			>
				{props.value}
			</Button>
		)}
	</SettingField>
);
