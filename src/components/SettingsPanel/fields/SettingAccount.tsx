import { type Component, createResource, createSignal, For, Show } from "solid-js";
import { invoke } from "../../../invoke";
import { Button } from "../../ui/Button";
import { Select } from "../../ui/Select";
import s from "./fields.module.css";
import { SettingField, type SettingFieldProps } from "./SettingField";

/** Safe projection of A1 AccountView. Never accept or display a credential. */
export interface SettingAccountOption {
	id: string;
	provider: string;
	label: string;
}

export interface SettingAccountProps extends SettingFieldProps {
	provider: string;
	value: string;
	onInput: (value: string) => unknown;
	/** Opens the native A2c flow. Resolves after save or cancel. No token crosses this callback. */
	onAdd: (provider: string) => Promise<void>;
	listAccounts?: () => Promise<readonly SettingAccountOption[]>;
}

const listAccounts = () => invoke<SettingAccountOption[]>("account_action", { request: { action: "list" } });

export const SettingAccount: Component<SettingAccountProps> = (props) => {
	const [adding, setAdding] = createSignal(false);
	const [addFailed, setAddFailed] = createSignal(false);
	const [accounts, { refetch }] = createResource(
		() => props.provider,
		async () => (props.listAccounts ?? listAccounts)(),
	);
	// Keep the error guard and resource read in one computation. JSX ternaries
	// passed to a component can become independent memos that read a failed resource.
	const selectedAccount = () => (accounts.error ? "" : accounts()?.length ? props.value : "");
	const add = async () => {
		setAdding(true);
		setAddFailed(false);
		try {
			await props.onAdd(props.provider);
			await refetch();
		} catch {
			// Native/transport errors can contain sensitive input. Render only a fixed message.
			setAddFailed(true);
		} finally {
			setAdding(false);
		}
	};
	return (
		<SettingField label={props.label} hint={props.hint}>
			{(id, hintId) => (
				<div class={s.list}>
					<div class={s.row}>
						<Select
							data-setting-id={props.settingId ?? props["data-setting-id"]}
							id={id}
							value={selectedAccount()}
							aria-describedby={hintId}
							disabled={props.disabled || accounts.loading || adding()}
							onChange={(e) => props.onInput(e.currentTarget.value)}
						>
							<option value="">Choose an account</option>
							<For each={accounts.error ? [] : accounts()?.filter((account) => account.provider === props.provider)}>
								{(account) => <option value={account.id}>{account.label}</option>}
							</For>
						</Select>
						<Button type="button" class={s.button} disabled={props.disabled || adding()} onClick={() => void add()}>
							Add account
						</Button>
					</div>
					<Show when={accounts.error || addFailed()}>
						<div role="alert">Could not load or add accounts. Try again.</div>
						<Button
							type="button"
							class={s.button}
							disabled={accounts.loading || adding()}
							onClick={() => {
								void refetch();
							}}
						>
							Retry
						</Button>
					</Show>
				</div>
			)}
		</SettingField>
	);
};
