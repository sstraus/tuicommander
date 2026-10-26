import { type Component, For, Show } from "solid-js";
import { Button } from "../../ui/Button";
import { Input } from "../../ui/Input";
import s from "./fields.module.css";
import { SettingField, type SettingFieldProps } from "./SettingField";

export interface SettingListRow {
	id: string;
	values: Record<string, string>;
}

export interface SettingListProps extends SettingFieldProps {
	rows: readonly SettingListRow[];
	columns: readonly { id: string; label: string; placeholder?: string }[];
	onChange: (rows: SettingListRow[]) => void;
	/** The caller supplies a stable ID and default values for a new row. */
	onAdd?: () => SettingListRow;
}

export const SettingList: Component<SettingListProps> = (props) => (
	<SettingField
		settingId={props.settingId}
		data-setting-id={props["data-setting-id"]}
		label={props.label}
		hint={props.hint}
		row={props.row}
		error={props.error}
		help={props.help}
		actions={props.actions}
	>
		{(id, _hintId, descriptionId) => (
			<div id={id} role="group" aria-label={props.label} aria-describedby={descriptionId()} class={s.list}>
				<For each={props.rows.map((row) => row.id)}>
					{(rowId, index) => (
						<div class={s.row} role="group" aria-label={`Row ${rowId}`}>
							<For each={props.columns}>
								{(column) => (
									<label class={s.cell}>
										<span>{column.label}</span>
										<Input
											data-setting-id={props.settingId ?? props["data-setting-id"]}
											type="text"
											value={props.rows[index()]?.values[column.id] ?? ""}
											placeholder={column.placeholder}
											disabled={props.disabled}
											onInput={(e) => {
												const value = e.currentTarget.value;
												props.onChange(
													props.rows.map((current) =>
														current.id === rowId
															? { ...current, values: { ...current.values, [column.id]: value } }
															: current,
													),
												);
											}}
										/>
									</label>
								)}
							</For>
							<Button
								data-setting-id={props.settingId ?? props["data-setting-id"]}
								type="button"
								class={s.button}
								disabled={props.disabled}
								aria-label={`Remove row ${rowId}`}
								onClick={() => props.onChange(props.rows.filter((current) => current.id !== rowId))}
							>
								Remove
							</Button>
						</div>
					)}
				</For>
				<Show when={props.onAdd}>
					{(add) => (
						<Button
							data-setting-id={props.settingId ?? props["data-setting-id"]}
							type="button"
							class={s.button}
							disabled={props.disabled}
							onClick={() => props.onChange([...props.rows, add()()])}
						>
							Add row
						</Button>
					)}
				</Show>
			</div>
		)}
	</SettingField>
);
