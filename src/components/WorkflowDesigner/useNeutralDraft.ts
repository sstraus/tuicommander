import { createEffect, createSignal, on, onCleanup } from "solid-js";
import { repositoriesStore } from "../../stores/repositories";
import {
	blankDefinition,
	type Catalog,
	type Diagnostic,
	type EditorMetadata,
	type NeutralDefinition,
	type NeutralDraft,
	type NeutralTemplate,
	neutralClient,
	plainMessage,
} from "./neutralClient";

export function useNeutralDraft(project: () => string) {
	const call = neutralClient(project);
	const [catalog, setCatalog] = createSignal<Catalog>({ activities: [], metanodes: [] });
	const [drafts, setDrafts] = createSignal<NeutralDraft[]>([]);
	const [templates, setTemplates] = createSignal<NeutralTemplate[]>([]);
	const [draft, setDraft] = createSignal<NeutralDraft>();
	const [definition, setDefinition] = createSignal<NeutralDefinition>(blankDefinition());
	const [editor, setEditor] = createSignal<EditorMetadata>({ labels: {}, presets: {} });
	const [name, setName] = createSignal("Untitled workflow");
	const [dirty, setDirty] = createSignal(false);
	const [busy, setBusy] = createSignal(false);
	const [error, setError] = createSignal("");
	const [catalogError, setCatalogError] = createSignal("");
	const [diagnostics, setDiagnostics] = createSignal<Diagnostic[]>([]);
	let generation = 0;
	let refreshPending = false;
	let refreshSequence = 0;
	function choose(next: NeutralDraft) {
		if (dirty()) {
			setError("Save the current draft before switching.");
			return;
		}
		setDraft(next);
		setDefinition(structuredClone(next.definition));
		setName(next.name);
		setEditor(next.editor);
		setDiagnostics([]);
	}
	async function perform(work: () => Promise<void>) {
		if (busy()) return;
		const current = generation;
		setBusy(true);
		setError("");
		try {
			await work();
		} catch (cause) {
			if (current === generation) setError(plainMessage(cause));
		} finally {
			if (current === generation) {
				setBusy(false);
				if (refreshPending) {
					refreshPending = false;
					void refresh().catch((cause) => setError(plainMessage(cause)));
				}
			}
		}
	}
	async function refresh() {
		const request = ++refreshSequence;
		const current = generation;
		const [entries, list, builtIn] = await Promise.allSettled([
			call({ action: "catalog" }, "catalog"),
			call({ action: "list_drafts" }, "drafts"),
			call({ action: "templates" }, "templates"),
		]);
		if (current !== generation || request !== refreshSequence) return;
		if (entries.status === "fulfilled") {
			setCatalog(entries.value);
			setCatalogError("");
		} else {
			setCatalog({ activities: [], metanodes: [] });
			setCatalogError(plainMessage(entries.reason));
		}
		if (builtIn.status === "fulfilled") setTemplates(builtIn.value);
		if (list.status === "fulfilled") setDrafts(list.value);
		else throw list.reason;
	}
	onCleanup(() => {
		generation++;
	});
	createEffect(
		on(
			() => [project(), repositoriesStore.getRevision(project())],
			() => {
				// Refresh catalog/list metadata without replacing unsaved definition state.
				if (busy()) refreshPending = true;
				else void perform(refresh);
			},
		),
	);

	function newDraft() {
		if (dirty()) {
			setError("Save the current draft before creating another.");
			return;
		}
		setDraft(undefined);
		setDefinition(blankDefinition());
		setName("Untitled workflow");
		setEditor({ labels: {}, presets: {} });
		setDiagnostics([]);
	}
	/** Open a built-in template as a new unsaved draft; Save creates it. */
	function fromTemplate(template: NeutralTemplate) {
		if (dirty()) {
			setError("Save the current draft before creating another.");
			return;
		}
		setDraft(undefined);
		setDefinition(structuredClone(template.definition));
		setName(template.name);
		setEditor(structuredClone(template.editor));
		setDiagnostics([]);
		setDirty(true);
	}
	function edit(next: NeutralDefinition) {
		setDefinition(next);
		setDirty(true);
		setDiagnostics([]);
	}
	async function save() {
		const current = draft();
		const saved = await call(
			current
				? {
						action: "update_draft",
						id: current.id,
						expected_revision: current.revision,
						name: name(),
						definition: definition(),
						editor: editor(),
					}
				: { action: "create_draft", name: name(), definition: definition(), editor: editor() },
			"draft",
		);
		setDirty(false);
		choose(saved);
		await refresh();
	}
	async function validate() {
		const result = await call({ action: "validate", definition: definition() }, "validation");
		setDiagnostics(result.diagnostics);
		if (!result.valid) setError("Resolve invalid links and inputs before publication.");
		return result.valid;
	}
	async function publish() {
		const current = draft();
		if (!current || dirty() || !(await validate())) return;
		const published = await call(
			{ action: "publish", id: current.id, expected_revision: current.revision },
			"publication",
		);
		setDraft({ ...current, latest_published_revision: published.revision });
		await refresh();
	}
	return {
		call,
		catalog,
		drafts,
		templates,
		fromTemplate,
		draft,
		definition,
		editor,
		editEditor: (next: EditorMetadata) => {
			setEditor(next);
			setDirty(true);
		},
		name,
		dirty,
		busy,
		error,
		catalogError,
		diagnostics,
		setName,
		setDirty,
		setError,
		newDraft,
		choose,
		edit,
		perform,
		refresh,
		save,
		validate,
		publish,
	};
}
