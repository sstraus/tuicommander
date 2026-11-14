import { createEffect, createMemo, createSignal, on, onCleanup } from "solid-js";
import { repositoriesStore } from "../../stores/repositories";
import type { ObjectSchema } from "./catalogTypes";
import { type NeutralPublication, neutralClient, plainMessage } from "./neutralClient";
import type { NeutralTriggerBinding, TriggerBindingDraft, TriggerEvent, TriggerPreview } from "./triggerTypes";

export function useTriggerBinding(project: () => string, publications: () => NeutralPublication[]) {
	const call = neutralClient(project);
	const [bindings, setBindings] = createSignal<NeutralTriggerBinding[]>([]);
	const initial = (): TriggerBindingDraft => ({
		id: "",
		publication: { project: project(), id: "", revision: 0 },
		source: "github",
		kind: "issue",
		resource_prefix: "",
		events: ["opened"],
		filters: {},
		ignored_actors: [],
		mapping: { inputs: {}, subject: {} },
		limits: { max_activations: 100, max_concurrency: 1, max_duration_ms: 3600000 },
	});
	const [draft, setDraft] = createSignal<TriggerBindingDraft>(initial());
	const [schema, setSchema] = createSignal<ObjectSchema>({ fields: {} });
	const [revision, setRevision] = createSignal(0);
	const [enabled, setEnabled] = createSignal(false);
	const [dirty, setDirty] = createSignal(true);
	const [preview, setPreview] = createSignal<TriggerPreview>();
	const [acknowledged, setAcknowledged] = createSignal(false);
	const [sample, setSample] = createSignal("");
	const [busy, setBusy] = createSignal(false);
	const [error, setError] = createSignal("");
	let alive = true;
	let epoch = 0;
	let refreshSequence = 0;
	let schemaSequence = 0;
	let previewSequence = 0;
	onCleanup(() => {
		alive = false;
		schemaSequence++;
	});
	function invalidate() {
		previewSequence++;
		setPreview(undefined);
		setAcknowledged(false);
	}
	function edit(next: TriggerBindingDraft) {
		setDraft(next);
		setDirty(true);
		invalidate();
	}
	function choose(binding: NeutralTriggerBinding) {
		setDraft(structuredClone(binding.draft));
		setRevision(binding.revision);
		setEnabled(binding.enabled);
		setDirty(false);
		invalidate();
	}
	async function refresh() {
		const current = epoch;
		const request = ++refreshSequence;
		const next = await call({ action: "trigger_bindings" }, "trigger_bindings");
		if (!alive || current !== epoch || request !== refreshSequence) return;
		setBindings(next);
		invalidate();
		const binding = next.find((binding) => binding.draft.id === draft().id);
		if (binding) {
			setRevision(binding.revision);
			setEnabled(binding.enabled);
		}
	}
	async function perform(work: () => Promise<void>) {
		if (busy()) return;
		const current = epoch;
		setBusy(true);
		setError("");
		try {
			await work();
		} catch (cause) {
			if (alive && current === epoch) setError(plainMessage(cause));
		} finally {
			if (alive && current === epoch) setBusy(false);
		}
	}
	createEffect(
		on(project, () => {
			epoch++;
			setBusy(false);
			setDraft(initial());
			setRevision(0);
			setEnabled(false);
			setDirty(true);
			invalidate();
		}),
	);
	createEffect(
		on(
			() => [project(), repositoriesStore.getRevision(project())],
			() => {
				invalidate();
				void refresh().catch((cause) => {
					if (alive) setError(plainMessage(cause));
				});
			},
		),
	);
	const sourceKey = createMemo(() => `${project()}\0${draft().source}\0${draft().kind}`);
	createEffect(
		on(sourceKey, () => {
			const sequence = ++schemaSequence;
			setSchema({ fields: {} });
			void call({ action: "trigger_schema", source: draft().source, kind: draft().kind }, "schema")
				.then((value) => {
					if (alive && sequence === schemaSequence) setSchema(value);
				})
				.catch((cause) => {
					if (alive && sequence === schemaSequence) setError(plainMessage(cause));
				});
		}),
	);
	const publication = () =>
		publications().find(
			(value) => value.id === draft().publication.id && value.revision === draft().publication.revision,
		);
	async function mutate(work: () => Promise<NeutralTriggerBinding>) {
		const current = epoch;
		let failure: unknown;
		try {
			const saved = await work();
			if (alive && current === epoch) choose(saved);
		} catch (cause) {
			failure = cause;
		} finally {
			// Subscription configuration can fail AFTER commit. Always re-read durable metadata.
			if (alive && current === epoch) {
				try {
					await refresh();
				} catch (cause) {
					failure ??= cause;
				}
			}
		}
		if (failure) throw failure;
	}
	return {
		bindings,
		draft,
		revision,
		enabled,
		dirty,
		preview,
		schema,
		publication,
		busy,
		error,
		acknowledged,
		sample,
		edit,
		choose,
		setAcknowledged,
		perform,
		refresh,
		newBinding: () => {
			setDraft(initial());
			setRevision(0);
			setEnabled(false);
			setDirty(true);
			setSample("");
			invalidate();
		},
		setSample: (value: string) => {
			setSample(value);
			invalidate();
		},
		previewMapping: () =>
			perform(async () => {
				invalidate();
				const current = epoch;
				const request = previewSequence;
				const value = await call(
					{ action: "preview_trigger_mapping", draft: draft(), snapshot: JSON.parse(sample()) as TriggerEvent },
					"trigger_preview",
				);
				if (alive && current === epoch && request === previewSequence) setPreview(value);
			}),
		save: () =>
			perform(() =>
				mutate(() =>
					call({ action: "save_trigger_binding", expected_revision: revision(), draft: draft() }, "trigger_binding"),
				),
			),
		toggle: () =>
			perform(async () => {
				if (dirty() || (!enabled() && (!preview() || !acknowledged()))) return;
				await mutate(() =>
					call(
						{
							action: "enable_trigger_binding",
							id: draft().id,
							expected_revision: revision(),
							enabled: !enabled(),
							acknowledged_overlaps: preview()?.overlaps ?? {},
						},
						"trigger_binding",
					),
				);
			}),
	};
}
