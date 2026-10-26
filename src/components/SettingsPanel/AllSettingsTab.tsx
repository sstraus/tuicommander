import { type Component, createEffect, createMemo, createSignal, For, onCleanup, Show } from "solid-js";
import { settingsCatalogue } from "../../generated/settingsCatalogue";
import { invoke } from "../../invoke";
import { hasCatalogueEditor, type SettingId, settingHint, settingLabel } from "../../services/settingsCatalogue";
import { settingsRefresh } from "../../services/settingsRefresh";
import { repositoriesStore } from "../../stores/repositories";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";
import { Select } from "../ui/Select";
import { SettingList } from "./fields/SettingList";
import { SettingInput, SettingSelect, SettingToggle } from "./SettingFields";
import s from "./Settings.module.css";
import { SectionCard, SettingPageHeader } from "./SettingsPrimitives";

type Field = (typeof settingsCatalogue.fields)[SettingId];
interface Failure {
	code: string;
	nextAction: string;
	persisted: boolean;
	proposalId?: string;
}
interface Snapshot {
	revision: string;
	values: Record<string, { stored: unknown; effective: unknown; source: string }>;
	error?: Failure;
}
interface Receipt {
	revision?: string;
	persisted?: boolean;
	applied?: boolean;
	error?: Failure;
}

/** One real owner field at a time: no opaque JSON, no implicit discovery or multi-store patch. */
export const CatalogueFieldEditor: Component<{ field: Field; path: string; repository?: string; profile?: string }> = (
	props,
) => {
	const [revision, setRevision] = createSignal<string>();
	const [value, setValue] = createSignal<unknown>();
	const [dirty, setDirty] = createSignal(false);
	const [busy, setBusy] = createSignal(false);
	const [notice, setNotice] = createSignal("");
	const [proposal, setProposal] = createSignal<{ id: string; request: Record<string, unknown> }>();
	let disposed = false;
	let version = 0;
	const field = () => props.field;
	const schema = () => field().schema as Record<string, unknown>;
	const types = () => (Array.isArray(schema().type) ? (schema().type as string[]) : [schema().type as string]);
	const target = () =>
		field().owner === "repository"
			? { kind: "repository", id: props.repository }
			: field().store === "ego" && props.profile
				? { kind: "profile", id: props.profile }
				: { kind: field().owner === "instance" ? "instance" : "machine" };
	const read = async () =>
		invoke<Snapshot>("settings_action", {
			request: { action: "get", store: field().store, target: target(), paths: [props.path] },
		});
	function adopt(snapshot: Snapshot): void {
		if (disposed) return;
		if (snapshot.error) {
			setNotice(`${snapshot.error.code}. Next action: ${snapshot.error.nextAction}.`);
			return;
		}
		// Retain the original CAS token with dirty intent: an external update must not
		// silently authorize overwriting it. Discard/reload is explicit.
		if (dirty()) {
			setNotice("This setting changed elsewhere. Your draft is kept; reload before saving again.");
			return;
		}
		const stored = snapshot.values[props.path];
		if (!stored) return;
		setRevision(snapshot.revision);
		setValue(() => stored.stored);
		setNotice(`Source: ${stored.source}.`);
	}
	onCleanup(settingsRefresh.register({ section: field().store, read, apply: adopt }));
	onCleanup(() => {
		disposed = true;
		version++;
	});
	function edit(next: unknown): void {
		version++;
		setValue(() => next);
		setDirty(true);
		setProposal(undefined);
	}
	async function reload(): Promise<void> {
		if (busy()) return;
		setBusy(true);
		const current = version;
		try {
			const snapshot = await read();
			if (!disposed && current === version) {
				if (!snapshot.error) {
					setDirty(false);
					setProposal(undefined);
				}
				adopt(snapshot);
			}
		} catch {
			if (!disposed) setNotice("Owner read unavailable. Retry without discarding your draft.");
		} finally {
			if (!disposed) setBusy(false);
		}
	}
	async function save(reset = false, approved = false): Promise<void> {
		if (busy() || !revision()) return;
		const pending = proposal();
		const request =
			approved && pending
				? { ...pending.request, approval: pending.id, confirm: true }
				: {
						action: reset ? "reset" : "set",
						store: field().store,
						target: target(),
						path: props.path,
						expectedRevision: revision(),
						...(reset ? {} : { value: value() }),
					};
		setBusy(true);
		const current = version;
		try {
			const reply = await invoke<Receipt>("settings_action", { request });
			if (disposed || current !== version) return;
			setProposal(undefined);
			if (reply.error) {
				const error = reply.error;
				// A committed phase cannot be safely replayed with this editor's old snapshot.
				if (error.persisted) setRevision(undefined);
				if (error.code === "confirmation_required" && error.proposalId) setProposal({ id: error.proposalId, request });
				setNotice(
					`${error.code}. ${error.persisted ? "Persisted; do not replay. Read back to recover." : `Next action: ${error.nextAction}.`}`,
				);
				return;
			}
			setDirty(false);
			adopt(await read());
			if (reply.applied === false) setNotice("Saved. Runtime application is pending; read back before retrying.");
		} catch {
			if (!disposed) {
				setRevision(undefined);
				setProposal(undefined);
				setNotice("Outcome unavailable. Read back before retrying; do not blindly replay.");
			}
		} finally {
			if (!disposed) setBusy(false);
		}
	}
	const options = () =>
		(schema().enum as unknown[] | undefined)?.map((option) => ({
			value: option === null ? "" : String(option),
			label: option === null ? "Default / inherit" : String(option),
		}));
	const label = () => settingLabel(field().id as SettingId, field().label);
	const hint = () =>
		settingHint(field().id as SettingId, `${String(schema().description ?? field().effect)} Apply: ${field().apply}.`);
	return (
		<div>
			<Show when={revision()} fallback={<p class={s.hint}>Read this owner setting to edit it.</p>}>
				<Show
					when={options()}
					fallback={
						<>
							<Show when={types().includes("boolean")}>
								<SettingToggle
									settingId={field().id as SettingId}
									row
									label={label()}
									hint={hint()}
									checked={value() === true}
									disabled={busy()}
									onChange={edit}
								/>
							</Show>
							<Show when={types().includes("string") || types().includes("integer") || types().includes("number")}>
								<SettingInput
									settingId={field().id as SettingId}
									row
									label={label()}
									hint={hint()}
									type={types().includes("string") ? "text" : "number"}
									min={schema().minimum as number | undefined}
									max={schema().maximum as number | undefined}
									step={types().includes("integer") ? 1 : undefined}
									value={value() == null ? "" : String(value())}
									disabled={busy()}
									onInput={(text) => {
										if (types().includes("string")) {
											edit(text);
											return true;
										}
										const number = Number(text);
										if (
											!text.trim() ||
											!Number.isFinite(number) ||
											(types().includes("integer") && !Number.isSafeInteger(number))
										)
											return false;
										edit(number);
										return true;
									}}
								/>
							</Show>
							<Show when={types().includes("array")}>
								<SettingList
									settingId={field().id as SettingId}
									label={label()}
									hint={hint()}
									disabled={busy()}
									columns={[{ id: "value", label: "Value" }]}
									rows={(Array.isArray(value()) ? (value() as string[]) : []).map((entry, index) => ({
										id: String(index),
										values: { value: entry },
									}))}
									onAdd={() => ({
										id: String(Array.isArray(value()) ? (value() as string[]).length : 0),
										values: { value: "" },
									})}
									onChange={(rows) => edit(rows.map((row) => row.values.value))}
								/>
							</Show>
						</>
					}
				>
					<SettingSelect
						settingId={field().id as SettingId}
						row
						label={label()}
						hint={hint()}
						disabled={busy()}
						value={value() === null ? "" : String(value())}
						options={options() ?? []}
						onChange={(encoded) =>
							edit((schema().enum as unknown[]).find((option) => (option === null ? "" : String(option)) === encoded))
						}
					/>
				</Show>
			</Show>
			<div class={s.catalogueActions} role="group" aria-label="Setting actions">
				<Show when={revision()}>
					<Show when={field().nullable && !options()}>
						<Button
							data-setting-id={field().id as SettingId}
							class={s.testBtn}
							disabled={busy()}
							onClick={() => edit(null)}
						>
							Use null / inherit
						</Button>
					</Show>
					<Button
						data-setting-id={field().id as SettingId}
						class={s.testBtn}
						disabled={busy() || !dirty()}
						onClick={() => void save()}
					>
						Save setting
					</Button>
					<Show when={(field().operations as readonly string[]).includes("reset")}>
						<Button
							data-setting-id={field().id as SettingId}
							class={s.testBtn}
							disabled={busy()}
							onClick={() => void save(true)}
						>
							Reset to owner default
						</Button>
					</Show>
				</Show>
				<Button
					data-non-setting="Read the owner snapshot; optionally discard this editor draft"
					class={s.testBtn}
					disabled={busy()}
					onClick={() => void reload()}
				>
					{dirty() ? "Discard draft and reload" : "Reload owner"}
				</Button>
				<Show when={proposal()}>
					<Button
						data-setting-id={field().id as SettingId}
						class={s.testBtn}
						disabled={busy()}
						onClick={() => void save(false, true)}
					>
						Ask for confirmation
					</Button>
				</Show>
			</div>
			<p class={s.hint} role="status">
				{notice()}
			</p>
		</div>
	);
};

export const AllSettingsTab: Component<{ initialSetting?: string }> = (props) => {
	const fields = Object.values(settingsCatalogue.fields).filter(hasCatalogueEditor);
	const [selected, setSelected] = createSignal<SettingId>(
		fields.find((field) => field.id === props.initialSetting)?.id ?? fields[0].id,
	);
	const [keys, setKeys] = createSignal<Record<number, string>>({});
	createEffect(() => {
		const requested = fields.find((field) => field.id === props.initialSetting);
		if (requested) {
			setSelected(requested.id);
			setKeys({});
		}
	});
	const [repository, setRepository] = createSignal("");
	const [profile, setProfile] = createSignal("");
	const field = () => fields.find((field) => field.id === selected()) ?? fields[0];
	const parts = () => field().path.split("/");
	const path = createMemo(() =>
		parts()
			.map((part, index) =>
				part.startsWith("{") ? (keys()[index] ?? "").replaceAll("~", "~0").replaceAll("/", "~1") : part,
			)
			.join("/"),
	);
	const ready = () =>
		parts().every((part, index) => !part.startsWith("{") || !!keys()[index]?.trim()) &&
		(field().owner !== "repository" || !!repository());
	return (
		<div class={`${s.section} ${s.applicationPage}`}>
			<SettingPageHeader
				title="All settings"
				breadcrumb="Application / Expert"
				description="Settings without a dedicated page."
				scope="Current instance / machine"
			/>
			<SectionCard title="Catalogue inspector">
				<label class={`${s.group} ${s.catalogueLabel}`}>
					Setting
					<Select
						data-non-setting="Select an owner field to inspect"
						value={selected()}
						onChange={(event) => {
							setSelected(event.currentTarget.value as SettingId);
							setKeys({});
						}}
					>
						<For each={fields}>
							{(field) => (
								<option value={field.id}>
									{field.store}: {settingLabel(field.id, field.label)} ({field.path})
								</option>
							)}
						</For>
					</Select>
				</label>
				<For
					each={parts()
						.map((part, index) => ({ part, index }))
						.filter(({ part }) => part.startsWith("{"))}
				>
					{({ index }) => (
						<label class={`${s.group} ${s.catalogueLabel}`}>
							Record
							<Input
								data-non-setting="Bind a catalogue path to an existing stable record key"
								value={keys()[index] ?? ""}
								onInput={(event) => setKeys({ ...keys(), [index]: event.currentTarget.value })}
							/>
						</label>
					)}
				</For>
				<Show when={field().owner === "repository"}>
					<label class={`${s.group} ${s.catalogueLabel}`}>
						Registered repository
						<Select
							data-non-setting="Select the repository settings owner"
							value={repository()}
							onChange={(event) => setRepository(event.currentTarget.value)}
						>
							<option value="">Select repository</option>
							<For each={repositoriesStore.getAllReposOrdered()}>
								{(repo) => <option value={repo.path}>{repo.displayName || repo.path}</option>}
							</For>
						</Select>
					</label>
				</Show>
				<Show when={field().store === "ego" && field().owner === "profile"}>
					<label class={`${s.group} ${s.catalogueLabel}`}>
						Ego profile (empty uses machine)
						<Input
							data-non-setting="Select an existing Ego profile owner"
							value={profile()}
							onInput={(event) => setProfile(event.currentTarget.value)}
						/>
					</label>
				</Show>
				<Show when={ready() && `${selected()}\0${path()}\0${repository()}\0${profile()}`} keyed>
					{(_identity) => {
						const ownerField = field();
						const ownerPath = path();
						const ownerRepo = repository();
						const ownerProfile = profile();
						return (
							<CatalogueFieldEditor field={ownerField} path={ownerPath} repository={ownerRepo} profile={ownerProfile} />
						);
					}}
				</Show>
			</SectionCard>
		</div>
	);
};
