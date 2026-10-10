import { createSignal } from "solid-js";
import type {
	AutomationAdapter,
	AutomationDefinition,
	AutomationListItem,
	AutomationRun,
	DefinitionSchedulePreview,
	SchedulePreset,
	SchedulePreview,
} from "../components/AutomationsDialog/contract";
import { randomId } from "../utils/randomId";

const [visible, setVisible] = createSignal(false);
export const automationsUi = { visible, open: () => setVisible(true), close: () => setVisible(false) };

/** Dialog-scoped state. The adapter owns validation, schedule interpretation and launch. */
export function createAutomationsStore(adapter: AutomationAdapter) {
	const [items, setItems] = createSignal<AutomationListItem[]>([]);
	const [draft, setDraft] = createSignal<AutomationDefinition>();
	const [isNew, setIsNew] = createSignal(false);
	const [loading, setLoading] = createSignal(false);
	const [busy, setBusy] = createSignal(false);
	const [error, setError] = createSignal("");
	const [notice, setNotice] = createSignal("");
	const [runs, setRuns] = createSignal<AutomationRun[]>([]);
	const [historyLoading, setHistoryLoading] = createSignal(false);
	const [schedulePreview, setSchedulePreview] = createSignal<SchedulePreview | DefinitionSchedulePreview>();
	const [previewError, setPreviewError] = createSignal("");
	const [previewLoading, setPreviewLoading] = createSignal(false);
	let listEpoch = 0,
		previewEpoch = 0,
		selectionEpoch = 0,
		historyEpoch = 0;
	const message = (cause: unknown) => (cause instanceof Error ? cause.message : String(cause));
	const invalidatePreview = () => {
		previewEpoch++;
		setSchedulePreview(undefined);
		setPreviewError("");
		setPreviewLoading(false);
	};
	function select(value: AutomationDefinition, creating = false) {
		selectionEpoch++;
		historyEpoch++;
		invalidatePreview();
		setDraft(structuredClone(value));
		setIsNew(creating);
		setRuns([]);
		setHistoryLoading(false);
		setError("");
		setNotice("");
	}
	function edit(patch: Partial<AutomationDefinition>) {
		setDraft((value) => (value ? { ...value, ...patch } : value));
		if ("cron" in patch || "timezone" in patch || "once_local" in patch) invalidatePreview();
	}
	async function refresh() {
		const epoch = ++listEpoch;
		setLoading(true);
		setError("");
		try {
			const values = await adapter.list();
			if (epoch === listEpoch) setItems(values);
		} catch (cause) {
			if (epoch === listEpoch) setError(message(cause));
		} finally {
			if (epoch === listEpoch) setLoading(false);
		}
	}
	async function mutate(operation: () => Promise<void>) {
		if (busy()) return;
		setBusy(true);
		setError("");
		setNotice("");
		try {
			await operation();
		} catch (cause) {
			setError(message(cause));
		} finally {
			setBusy(false);
		}
	}
	async function preview() {
		const value = draft();
		if (!value) return;
		const epoch = ++previewEpoch;
		setPreviewLoading(true);
		setPreviewError("");
		try {
			const reply =
				value.once_local != null
					? await adapter.previewDefinition(structuredClone(value))
					: await adapter.preview(value.cron, value.timezone);
			if (epoch !== previewEpoch) return;
			setSchedulePreview(reply);
			// Only fill an omitted creation zone with the backend's resolved local zone.
			if (!value.timezone) setDraft((current) => (current ? { ...current, timezone: reply.timezone } : current));
		} catch (cause) {
			if (epoch === previewEpoch) {
				setSchedulePreview(undefined);
				setPreviewError(message(cause));
			}
		} finally {
			if (epoch === previewEpoch) setPreviewLoading(false);
		}
	}
	async function loadHistory() {
		const value = draft();
		if (!value || isNew()) return;
		const epoch = ++historyEpoch;
		setHistoryLoading(true);
		try {
			const reply = await adapter.history(value.id);
			if (epoch === historyEpoch) setRuns(reply);
		} catch (cause) {
			if (epoch === historyEpoch) setError(message(cause));
		} finally {
			if (epoch === historyEpoch) setHistoryLoading(false);
		}
	}
	return {
		items,
		draft,
		isNew,
		loading,
		busy,
		error,
		notice,
		runs,
		historyLoading,
		schedulePreview,
		previewError,
		previewLoading,
		select,
		edit,
		refresh,
		preview,
		loadHistory,
		create() {
			select(
				{
					id: randomId("automation"),
					name: "",
					prompt: "",
					repository: "",
					run_config: "",
					workspace: { mode: "existing" },
					cron: "0 9 * * *",
					timezone: "",
					enabled: true,
					grace_secs: 43200,
					overlap: "skip",
					max_duration_secs: 3600,
					precheck: null,
				},
				true,
			);
		},
		save: () =>
			mutate(async () => {
				const value = draft();
				if (!value) return;
				const epoch = selectionEpoch;
				const saved = await adapter.save(structuredClone(value), isNew());
				if (epoch === selectionEpoch) {
					select(saved);
					setNotice("Saved");
				}
				await refresh();
			}),
		setEnabled: (enabled: boolean) =>
			mutate(async () => {
				const value = draft();
				if (!value || isNew()) return;
				const epoch = selectionEpoch;
				await adapter.setEnabled(value.id, enabled);
				if (epoch === selectionEpoch) edit({ enabled });
				await refresh();
			}),
		remove: () =>
			mutate(async () => {
				const value = draft();
				if (!value || isNew()) return;
				const epoch = selectionEpoch;
				await adapter.remove(value.id);
				if (epoch === selectionEpoch) {
					selectionEpoch++;
					historyEpoch++;
					invalidatePreview();
					setDraft(undefined);
					setRuns([]);
				}
				await refresh();
			}),
		runNow: () =>
			mutate(async () => {
				const value = draft();
				if (!value || isNew()) return;
				const reply = await adapter.runNow(value.id);
				setNotice(`${reply.status}${reply.reason ? `: ${reply.reason}` : ""}`);
				await refresh();
				await loadHistory();
			}),
		applyPreset: (preset: SchedulePreset) =>
			mutate(async () => {
				const epoch = selectionEpoch;
				const previewAtStart = previewEpoch;
				const reply = await adapter.preset(preset);
				if (epoch !== selectionEpoch || previewAtStart !== previewEpoch) return;
				edit({ cron: reply.cron, once_local: null });
				await preview();
			}),
		dispose() {
			listEpoch++;
			selectionEpoch++;
			historyEpoch++;
			previewEpoch++;
		},
	};
}
export type AutomationsStore = ReturnType<typeof createAutomationsStore>;
