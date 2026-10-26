import { Button } from "../ui/Button";
import { Input } from "../ui/Input";
import { Select } from "../ui/Select";
import { Switch as CheckboxSwitch } from "../ui/Switch";
/**
 * The two questions an agent can block on, and the only answers it will take.
 *
 * A permission request is answered with one of the option ids the agent
 * published — never with an Allow/Deny pair invented here, which would answer a
 * question nobody asked. An elicitation is drawn only in `form` mode: the Rust
 * client answers `cancel` to every other mode before it reaches a host, so a
 * form drawn for anything else would be a form for a mode this client never
 * advertised.
 */

import { type Component, createSignal, For, Match, Show, Switch } from "solid-js";
import type {
	AcpCreateElicitationRequest,
	AcpHostRequestId,
	AcpPendingInteraction,
	AcpPermissionOption,
	AcpRequestPermissionRequest,
} from "../../types/acp";
import { cx } from "../../utils";
import s from "./AIChatPanel.module.css";

/** One field of an elicitation form, as far as a panel can draw it. */
export interface ElicitationField {
	name: string;
	label: string;
	type: "string" | "number" | "boolean" | "enum";
	choices: string[];
	required: boolean;
}

/**
 * Read the fields out of a requested schema.
 *
 * Deliberately shallow: a nested object or an array has no single control to
 * draw, and guessing one would collect a value the agent cannot read back. An
 * unreadable schema yields no fields, which renders as a message with accept
 * and decline — the agent still gets an answer.
 */
export function elicitationFields(schema: unknown): ElicitationField[] {
	if (!schema || typeof schema !== "object") return [];
	const properties = (schema as { properties?: unknown }).properties;
	if (!properties || typeof properties !== "object") return [];
	const required = new Set(
		Array.isArray((schema as { required?: unknown }).required)
			? ((schema as { required: unknown[] }).required.filter((name) => typeof name === "string") as string[])
			: [],
	);
	return Object.entries(properties as Record<string, unknown>).flatMap(([name, raw]) => {
		if (!raw || typeof raw !== "object") return [];
		const property = raw as { type?: unknown; title?: unknown; description?: unknown; enum?: unknown };
		const choices = Array.isArray(property.enum) ? property.enum.map(String) : [];
		const declared = typeof property.type === "string" ? property.type : "string";
		if (
			choices.length === 0 &&
			declared !== "string" &&
			declared !== "number" &&
			declared !== "integer" &&
			declared !== "boolean"
		) {
			return [];
		}
		const type: ElicitationField["type"] =
			choices.length > 0 ? "enum" : declared === "boolean" ? "boolean" : declared === "string" ? "string" : "number";
		return [
			{
				name,
				label: typeof property.title === "string" ? property.title : name,
				type,
				choices,
				required: required.has(name),
			},
		];
	});
}

const PERMISSION_CLASS: Record<AcpPermissionOption["kind"], string> = {
	allow_once: s.approveBtn,
	allow_always: s.alwaysAllowBtn,
	reject_once: s.denyBtn,
	reject_always: s.denyBtn,
};

/** What the agent wants to do, when it said. */
function permissionTitle(request: AcpRequestPermissionRequest): string {
	const call = request.toolCall;
	if (call && typeof call === "object" && typeof (call as { title?: unknown }).title === "string") {
		return (call as { title: string }).title;
	}
	return "The agent is asking for permission.";
}

const PermissionCard: Component<{
	requestId: AcpHostRequestId;
	request: AcpRequestPermissionRequest;
	onAnswer: (requestId: AcpHostRequestId, optionId: string) => void;
	onDismiss: (requestId: AcpHostRequestId) => void;
}> = (props) => (
	<div class={s.approvalCard}>
		<div class={s.approvalText}>{permissionTitle(props.request)}</div>
		<div class={s.approvalActions}>
			{/* The agent's own options, in the agent's own order. */}
			<For each={props.request.options}>
				{(option) => (
					<Button
						type="button"
						class={cx(s.approvalBtn, PERMISSION_CLASS[option.kind])}
						onClick={() => props.onAnswer(props.requestId, option.optionId)}
					>
						{option.name}
					</Button>
				)}
			</For>
			<Button type="button" class={s.approvalBtn} onClick={() => props.onDismiss(props.requestId)}>
				Dismiss
			</Button>
		</div>
	</div>
);

const ElicitationCard: Component<{
	requestId: AcpHostRequestId;
	request: AcpCreateElicitationRequest;
	onAccept: (requestId: AcpHostRequestId, content: Record<string, unknown>) => void;
	onDecline: (requestId: AcpHostRequestId) => void;
	onCancel: (requestId: AcpHostRequestId) => void;
}> = (props) => {
	const [values, setValues] = createSignal<Record<string, unknown>>({});
	const fields = () => elicitationFields(props.request.requestedSchema);
	const quickChoice = () => {
		const schema = props.request.requestedSchema;
		const properties = schema && typeof schema === "object" ? (schema as { properties?: unknown }).properties : null;
		if (!properties || typeof properties !== "object" || Object.keys(properties).length !== 1) return null;
		const all = fields();
		return all.length === 1 && all[0].type === "enum" && all[0].choices.length > 0 && all[0].choices.length <= 3
			? all[0]
			: null;
	};
	const set = (name: string, value: unknown) => setValues({ ...values(), [name]: value });

	return (
		<div class={s.approvalCard}>
			<div class={s.approvalText}>{props.request.message}</div>
			<For each={quickChoice() ? [] : fields()}>
				{(field) => (
					<label class={s.formField}>
						<span class={s.formLabel}>
							{field.label}
							{field.required ? " *" : ""}
						</span>
						<Switch>
							<Match when={field.type === "enum"}>
								<Select
									class={s.formInput}
									onChange={(event) => set(field.name, event.currentTarget.value)}
									value={String(values()[field.name] ?? "")}
								>
									<option value="" />
									<For each={field.choices}>{(choice) => <option value={choice}>{choice}</option>}</For>
								</Select>
							</Match>
							<Match when={field.type === "boolean"}>
								<CheckboxSwitch
									role="checkbox"
									checked={Boolean(values()[field.name])}
									onChange={(event) => set(field.name, event.currentTarget.checked)}
								/>
							</Match>
							<Match when={field.type === "number"}>
								<Input
									class={s.formInput}
									type="number"
									onInput={(event) => set(field.name, event.currentTarget.valueAsNumber)}
								/>
							</Match>
							<Match when={field.type === "string"}>
								<Input
									class={s.formInput}
									type="text"
									onInput={(event) => set(field.name, event.currentTarget.value)}
								/>
							</Match>
						</Switch>
					</label>
				)}
			</For>
			<div class={s.approvalActions}>
				<For each={quickChoice()?.choices ?? []}>
					{(choice) => (
						<Button
							type="button"
							class={s.approvalBtn}
							onClick={() => props.onAccept(props.requestId, { [quickChoice()!.name]: choice })}
						>
							{choice === "trusted" ? "Trust" : choice === "untrusted" ? "Don't trust" : choice}
						</Button>
					)}
				</For>
				<Show when={!quickChoice()}>
					<Button
						type="button"
						class={cx(s.approvalBtn, s.approveBtn)}
						onClick={() => props.onAccept(props.requestId, values())}
					>
						Submit
					</Button>
					<Button type="button" class={cx(s.approvalBtn, s.denyBtn)} onClick={() => props.onDecline(props.requestId)}>
						Decline
					</Button>
				</Show>
				<Button type="button" class={s.approvalBtn} onClick={() => props.onCancel(props.requestId)}>
					Cancel
				</Button>
			</div>
		</div>
	);
};

export interface InteractionsProps {
	interactions: () => AcpPendingInteraction[];
	onPermission: (requestId: AcpHostRequestId, optionId: string) => void;
	onPermissionDismissed: (requestId: AcpHostRequestId) => void;
	onElicitationAccepted: (requestId: AcpHostRequestId, content: Record<string, unknown>) => void;
	onElicitationDeclined: (requestId: AcpHostRequestId) => void;
	onElicitationCancelled: (requestId: AcpHostRequestId) => void;
}

export const Interactions: Component<InteractionsProps> = (props) => (
	<For each={props.interactions()}>
		{(interaction) => (
			<Switch>
				<Match when={interaction.kind === "permission" && interaction}>
					{(permission) => (
						<PermissionCard
							requestId={permission().requestId}
							request={permission().request as AcpRequestPermissionRequest}
							onAnswer={props.onPermission}
							onDismiss={props.onPermissionDismissed}
						/>
					)}
				</Match>
				{/* `form` is the only mode this client advertised, so it is the only
				    one drawn. Anything else is answered `cancel` in Rust and must
				    never reach a person as a form they cannot fill in. */}
				<Match when={interaction.kind === "elicitation" && interaction.request.mode === "form" && interaction}>
					{(elicitation) => (
						<ElicitationCard
							requestId={elicitation().requestId}
							request={elicitation().request as AcpCreateElicitationRequest}
							onAccept={props.onElicitationAccepted}
							onDecline={props.onElicitationDeclined}
							onCancel={props.onElicitationCancelled}
						/>
					)}
				</Match>
			</Switch>
		)}
	</For>
);
